#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_markdown_links_repro() {
        let input = r#"
Some text here.

[1]: #maincontent
[2]: https://www.cncf.io/accessibility-statement/
[3]: https://events.linuxfoundation.org/kubecon-cloudnativecon-europe/?utm_sourc
e=cncf&utm_medium=subpage&utm_campaign=18269725-KubeCon-EU-2026&utm_content=hell
o-bar
[4]: /
"#;
        let cleaned = clean_markdown_links(input);
        println!("Cleaned output:\n{}", cleaned);

        assert!(!cleaned.contains("[1]: #maincontent"));
        assert!(!cleaned.contains("[3]: https://events"));
    }
}
