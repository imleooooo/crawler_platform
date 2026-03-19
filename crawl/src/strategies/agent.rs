use crate::config::CrawlerRunConfig;
use crate::errors::CrawlError;
use chromiumoxide::Page;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::sleep;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentAction {
    pub action_type: String, // "click", "type", "scroll", "goto", "done", "fail"
    pub selector: Option<String>,
    pub text: Option<String>,
    pub url: Option<String>,
    pub reason: Option<String>,
}

pub struct Agent {
    client: Client,
}

impl Default for Agent {
    fn default() -> Self {
        Self::new()
    }
}

impl Agent {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    pub async fn run(&self, page: &Page, config: &CrawlerRunConfig) -> Result<String, CrawlError> {
        let max_steps = 10;
        let mut history = Vec::new();

        // Ensure we have API Key
        let api_key = config
            .api_key
            .as_deref()
            .ok_or(CrawlError::Other("Missing API Key for Agent".to_string()))?;
        let model = config.model.as_deref().unwrap_or("gpt-5.1"); // Default model
        let prompt = config.prompt.as_deref().unwrap_or("Extract content");

        println!("Agent started. Prompt: {}", prompt);

        for step in 0..max_steps {
            // 1. Get State (Simplified DOM or Screenshot if we had vision)
            // For now, let's get a simplified accessibility tree or just interactive elements?
            // To keep it simple speed-wise, let's dump the text content or generic HTML for now,
            // but ideally we want a condensed representation.
            // Let's assume we just feed it the HTML body for this lightweight version,
            // or maybe the accessibility tree if chromiumoxide supports it easily.
            // chromiumoxide has `page.get_accessibility_tree()`.

            // Fallback: Get body text
            let body_text = page
                .evaluate("document.body.innerText")
                .await
                .map_err(|e| CrawlError::JsError(e.to_string()))?
                .into_value::<String>()
                .map_err(|e| CrawlError::JsError(e.to_string()))?;

            // 2. Prepare Prompt
            let system_prompt = r#"
            You are a browser automation agent. You can interact with the page.
            Return a JSON object with the next action.
            Actions:
            - {"action_type": "goto", "url": "https://example.com", "reason": "..."}
            - {"action_type": "click", "selector": "css_selector", "reason": "..."}
            - {"action_type": "type", "selector": "css_selector", "text": "input_text", "reason": "..."}
            - {"action_type": "scroll", "reason": "..."}
            - {"action_type": "done", "text": "final result summary", "reason": "..."}
            - {"action_type": "fail", "reason": "..."}
            
            Focus on the user's goal.
            "#;

            let user_msg = format!(
                "Goal: {}\n\nCurrent Page Text:\n{:.2000}\n\nHistory: {:?}",
                prompt, body_text, history
            );

            // 3. Call LLM
            let response = self
                .call_llm(api_key, model, system_prompt, &user_msg)
                .await?;

            // 4. Parse Action
            let action: AgentAction = serde_json::from_str(&response).map_err(|e| {
                CrawlError::Other(format!(
                    "Failed to parse agent JSON: {}. Resp: {}",
                    e, response
                ))
            })?;

            println!("Step {}: Action {:?}", step, action);
            history.push(format!("Step {}: {:?}", step, action));

            // 5. Handle Control Flow Actions
            if action.action_type == "done" {
                return Ok(action.text.unwrap_or_default());
            }
            if action.action_type == "fail" {
                return Err(CrawlError::Other(format!(
                    "Agent gave up: {:?}",
                    action.reason
                )));
            }

            // 6. Execute Interaction Actions (Failures allowed)
            let execution_result = async {
                match action.action_type.as_str() {
                    "goto" => {
                        if let Some(url) = &action.url {
                            page.goto(url.as_str()).await
                                .map_err(|e| format!("Navigation failed: {}", e))?;
                            sleep(Duration::from_secs(3)).await;
                        }
                        Ok(())
                    },
                    "click" => {
                        if let Some(sel) = &action.selector {
                            let el = page.find_element(sel).await
                                .map_err(|e| format!("Element not found '{}': {}", sel, e))?;
                            el.click().await
                                .map_err(|e| format!("Click failed: {}", e))?;
                            sleep(Duration::from_secs(2)).await;
                        }
                        Ok(())
                    },
                    "type" => {
                        if let Some(sel) = &action.selector {
                            if let Some(txt) = &action.text {
                                let _el = page.find_element(sel).await
                                    .map_err(|e| format!("Input not found '{}': {}", sel, e))?;

                                let js = format!(
                                    "let el = document.querySelector('{}'); if(el) {{ el.value = '{}'; el.dispatchEvent(new Event('input', {{bubbles: true}})); }}",
                                    sel.replace("'", "\\'"),
                                    txt.replace("'", "\\'")
                                );
                                page.evaluate(js).await
                                    .map_err(|e| format!("JS type failed: {}", e))?;
                            }
                        }
                        Ok(())
                    },
                    "scroll" => {
                        page.evaluate("window.scrollBy(0, 500)").await
                            .map_err(|e| format!("Scroll failed: {}", e))?;
                        sleep(Duration::from_millis(500)).await;
                        Ok(())
                    },
                    _ => Err(format!("Unknown action: {}", action.action_type))
                }
            }.await;

            match execution_result {
                Ok(_) => {
                    history.push(format!(
                        "Step {}: Action {:?} Succeeded",
                        step, action.action_type
                    ));
                }
                Err(e) => {
                    println!("Step {}: Action Failed: {}", step, e);
                    history.push(format!("Step {}: Action Failed: {}", step, e));
                    // Check if this was a critical repeated failure? For now, just let LLM retry.
                }
            }
        }

        Err(CrawlError::Other(
            "Agent reached max steps without completion".to_string(),
        ))
    }

    async fn call_llm(
        &self,
        api_key: &str,
        model: &str,
        system: &str,
        user: &str,
    ) -> Result<String, CrawlError> {
        // OpenAI Chat Completion
        let url = "https://api.openai.com/v1/chat/completions";
        let body = serde_json::json!({
            "model": model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user}
            ],
            "response_format": { "type": "json_object" }
        });

        let resp = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| CrawlError::Other(format!("LLM Request failed: {}", e)))?;

        if !resp.status().is_success() {
            return Err(CrawlError::Other(format!(
                "LLM API Error: {}",
                resp.status()
            )));
        }

        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| CrawlError::Other(format!("Failed to parse LLM response: {}", e)))?;

        let content = json["choices"][0]["message"]["content"]
            .as_str()
            .ok_or(CrawlError::Other("No content in LLM response".to_string()))?;

        Ok(content.to_string())
    }
}
