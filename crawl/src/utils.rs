use tracing_subscriber;

pub fn init_logging() {
    tracing_subscriber::fmt::init();
}

pub fn clean_markdown_links(markdown: &str) -> String {
    // 1. Remove reference lines at the bottom.
    // This regex matches `[n]: ...` and any subsequent lines that do NOT start with `[` (which would be the next reference).
    // This handles cases where html2text wraps long URLs across multiple lines.
    let ref_line_regex = regex::Regex::new(r"(?m)^\[\d+\]:.*(?:\n[^\[\r\n].*)*").unwrap();
    let cleaned_text = ref_line_regex.replace_all(markdown, "");

    // 2. Remove [n] markers from text, e.g. "some text [1]" -> "some text"
    // Regex: \[ \d+ \]
    let marker_regex = regex::Regex::new(r"\[\d+\]").unwrap();
    let cleaned_text = marker_regex.replace_all(&cleaned_text, "");

    // 3. Remove excess newlines that might be left behind
    let whitespace_regex = regex::Regex::new(r"\n{3,}").unwrap();
    let final_text = whitespace_regex.replace_all(&cleaned_text, "\n\n");

    final_text.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_markdown_links() {
        let input = r#"
This is a paragraph with a link [1] and another [2].

[1]: https://www.typescriptlang.org/docs/handbook/2/basic-types.html
[2]: http://example.com
"#;
        let expected = r#"
This is a paragraph with a link  and another .
"#;
        assert_eq!(clean_markdown_links(input), expected.trim());

        let input2 = "Hello [1] World.\n\n[1]: http://test.com";
        let output2 = clean_markdown_links(input2);

        assert_eq!(output2, "Hello  World.");
    }

    #[test]
    fn test_clean_markdown_links_wrapped() {
        let input = r#"
Some text with [3] link.

[3]: https://events.linuxfoundation.org/kubecon-cloudnativecon-europe/?utm_sourc
e=cncf&utm_medium=subpage&utm_campaign=18269725-KubeCon-EU-2026&utm_content=hell
o-bar
[4]: /
"#;
        let cleaned = clean_markdown_links(input);
        println!("Cleaned output:\n{}", cleaned);

        assert!(!cleaned.contains("utm_sourc"));
        assert!(!cleaned.contains("e=cncf"));
        assert!(!cleaned.contains("o-bar"));
        assert_eq!(cleaned.trim(), "Some text with  link.");
    }
}
