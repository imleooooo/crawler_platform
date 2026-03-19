use crate::errors::CrawlError;
use chromiumoxide::cdp::browser_protocol::page::AddScriptToEvaluateOnNewDocumentParams;
use chromiumoxide::Page;

pub struct StealthConfig {
    pub vendor: String,
    pub renderer: String,
    pub nav_platform: String,
    pub nav_languages: Vec<String>,
}

impl Default for StealthConfig {
    fn default() -> Self {
        Self {
            vendor: "Intel Inc.".to_string(),
            renderer: "Intel(R) Iris(R) Xe Graphics".to_string(),
            nav_platform: "Win32".to_string(),
            nav_languages: vec!["en-US".to_string(), "en".to_string()],
        }
    }
}

pub async fn apply_stealth(page: &Page, config: &StealthConfig) -> Result<(), CrawlError> {
    // 1. Remove webdriver
    let webdriver_script = "
        Object.defineProperty(navigator, 'webdriver', {
            get: () => undefined,
        });
    ";
    add_script(page, webdriver_script).await?;

    // 2. Mock Chrome runtime
    let chrome_script = "
        window.chrome = {
            runtime: {},
            app: {
                isInstalled: false,
                InstallState: { DISABLED: 'disabled', INSTALLED: 'installed', NOT_INSTALLED: 'not_installed' },
                RunningState: { CANNOT_RUN: 'cannot_run', READY_TO_RUN: 'ready_to_run', RUNNING: 'running' }
            },
            csi: () => {},
            loadTimes: () => {}
        };
    ";
    add_script(page, chrome_script).await?;

    // 3. Mock Plugins
    let plugins_script = "
        Object.defineProperty(navigator, 'plugins', {
            get: () => {
                var ChromiumPDFPlugin = {};
                ChromiumPDFPlugin.__proto__ = Plugin.prototype;
                var plugins = {
                    0: ChromiumPDFPlugin,
                    description: 'Portable Document Format',
                    filename: 'internal-pdf-viewer',
                    length: 1,
                    name: 'Chromium PDF Viewer',
                    __proto__: PluginArray.prototype,
                };
                return plugins;
            }
        });
    ";
    add_script(page, plugins_script).await?;

    // 4. Mock WebGL
    let webgl_script = format!(
        r#"
        const getParameter = WebGLRenderingContext.prototype.getParameter;
        WebGLRenderingContext.prototype.getParameter = function(parameter) {{
            // 37445: UNMASKED_VENDOR_WEBGL
            if (parameter === 37445) {{
                return '{}';
            }}
            // 37446: UNMASKED_RENDERER_WEBGL
            if (parameter === 37446) {{
                return '{}';
            }}
            return getParameter(parameter);
        }};
    "#,
        config.vendor, config.renderer
    );
    add_script(page, &webgl_script).await?;

    // 5. Mock Navigator Properties (Platform, Languages)
    // Note: User-Agent is handled by browser launch args, but platform should match it.
    let nav_script = format!(
        r#"
        Object.defineProperty(navigator, 'platform', {{
            get: () => '{}',
        }});
        Object.defineProperty(navigator, 'languages', {{
            get: () => {:?},
        }});
    "#,
        config.nav_platform, config.nav_languages
    );
    add_script(page, &nav_script).await?;

    // 6. Canvas Noise (Simple implementation)
    // This adds a very subtle noise to canvas readback to defeat naive fingerprinting hash
    let canvas_script = r#"
        (() => {
            const shift = {
                'r': Math.floor(Math.random() * 10) - 5,
                'g': Math.floor(Math.random() * 10) - 5,
                'b': Math.floor(Math.random() * 10) - 5,
                'a': Math.floor(Math.random() * 10) - 5
            };
            const originalToDataURL = HTMLCanvasElement.prototype.toDataURL;
            const originalGetContext = HTMLCanvasElement.prototype.getContext;

            // Proxy toDataURL
            HTMLCanvasElement.prototype.toDataURL = function() {
                 return originalToDataURL.apply(this, arguments);
            };

            // Proxy getContext to proxy getImageData
            HTMLCanvasElement.prototype.getContext = function(type, contextAttributes) {
                const context = originalGetContext.apply(this, arguments);
                if ((type === '2d' || type.startsWith('webgl')) && context) {
                     const originalGetImageData = context.getImageData;
                     if (originalGetImageData) {
                         context.getImageData = function(sx, sy, sw, sh) {
                             const imageData = originalGetImageData.apply(this, arguments);
                             // Minimal noise injection
                             // We just want to mess up the hash, not visible content
                             if (imageData.data.length > 0) {
                                 // Modify only one pixel or few
                                 for(let i=0; i < Math.min(imageData.data.length, 100); i+=4) {
                                     imageData.data[i] = Math.max(0, Math.min(255, imageData.data[i] + shift.r));
                                     imageData.data[i+1] = Math.max(0, Math.min(255, imageData.data[i+1] + shift.g));
                                     imageData.data[i+2] = Math.max(0, Math.min(255, imageData.data[i+2] + shift.b));
                                 }
                             }
                             return imageData;
                         };
                     }
                }
                return context;
            };
        })();
    "#;
    add_script(page, canvas_script).await?;

    Ok(())
}

async fn add_script(page: &Page, script: &str) -> Result<(), CrawlError> {
    let params = AddScriptToEvaluateOnNewDocumentParams::new(script.to_string());
    page.execute(params)
        .await
        .map_err(|e| CrawlError::Other(e.to_string()))?;
    Ok(())
}
