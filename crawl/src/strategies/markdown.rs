use crate::errors::CrawlError;

pub struct MarkdownGenerator;

impl Default for MarkdownGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkdownGenerator {
    pub fn new() -> Self {
        Self
    }

    pub fn generate(
        &self,
        html: &str,
        url: Option<&str>,
        magic_markdown: bool,
    ) -> Result<String, CrawlError> {
        let mut content = html.to_string();

        if magic_markdown {
            if let Some(u) = url {
                if let Ok(parsed_url) = url::Url::parse(u) {
                    let mutations = &mut content.as_bytes();
                    if let Ok(product) = readability::extractor::extract(mutations, &parsed_url) {
                        content = product.content;
                    }
                }
            }
        }

        // html2text conversion on the cleaned content
        let md = html2text::from_read(content.as_bytes(), 120);
        Ok(md)
    }
}
