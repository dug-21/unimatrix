use crate::error::Result;
use crate::provider::EmbeddingProvider;

/// Concatenate title and content with the given separator.
///
/// - Both non-empty: `"{title}{separator}{content}"`
/// - Title empty: content only (no separator prefix)
/// - Content empty: title only (no separator suffix)
/// - Both empty: empty string `""`
pub fn prepare_text(title: &str, content: &str, separator: &str) -> String {
    match (title.is_empty(), content.is_empty()) {
        (true, true) => String::new(),
        (true, false) => content.to_string(),
        (false, true) => title.to_string(),
        (false, false) => format!("{title}{separator}{content}"),
    }
}

/// Embed a single entry's text fields using the given provider.
///
/// Concatenates title and content with the given separator,
/// then calls `provider.embed()`.
pub fn embed_entry(
    provider: &dyn EmbeddingProvider,
    title: &str,
    content: &str,
    separator: &str,
) -> Result<Vec<f32>> {
    let text = prepare_text(title, content, separator);
    provider.embed(&text)
}

/// Embed a batch of entry text fields.
///
/// Each entry is a `(title, content)` pair. Concatenates each pair with
/// the given separator, then calls `provider.embed_batch()`.
/// Returns one embedding per entry in the same order.
pub fn embed_entries(
    provider: &dyn EmbeddingProvider,
    entries: &[(String, String)],
    separator: &str,
) -> Result<Vec<Vec<f32>>> {
    let texts: Vec<String> = entries
        .iter()
        .map(|(title, content)| prepare_text(title, content, separator))
        .collect();

    let refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
    provider.embed_batch(&refs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::MockProvider;

    #[test]
    fn test_prepare_text_both_present() {
        assert_eq!(prepare_text("JWT", "Validate exp", ": "), "JWT: Validate exp");
    }

    #[test]
    fn test_prepare_text_empty_title() {
        assert_eq!(prepare_text("", "content only", ": "), "content only");
    }

    #[test]
    fn test_prepare_text_empty_content() {
        assert_eq!(prepare_text("title only", "", ": "), "title only");
    }

    #[test]
    fn test_prepare_text_both_empty() {
        assert_eq!(prepare_text("", "", ": "), "");
    }

    #[test]
    fn test_prepare_text_custom_separator() {
        assert_eq!(prepare_text("title", "content", " - "), "title - content");
    }

    #[test]
    fn test_prepare_text_empty_separator() {
        assert_eq!(prepare_text("title", "content", ""), "titlecontent");
    }

    #[test]
    fn test_prepare_text_title_contains_separator() {
        assert_eq!(
            prepare_text("key: value", "content", ": "),
            "key: value: content"
        );
    }

    #[test]
    fn test_prepare_text_long_content() {
        let title = "Short Title";
        let content = "a".repeat(10000);
        let result = prepare_text(title, &content, ": ");
        assert!(result.starts_with("Short Title: "));
        assert_eq!(result.len(), 11 + 2 + 10000); // "Short Title" + ": " + content
    }

    #[test]
    fn test_embed_entry_calls_provider() {
        let provider = MockProvider::new(384);
        let result = embed_entry(&provider, "Auth", "Use JWT", ": ");
        assert!(result.is_ok());
        let embedding = result.unwrap();
        assert_eq!(embedding.len(), 384);

        // Verify same result as manual prepare + embed
        let text = prepare_text("Auth", "Use JWT", ": ");
        let manual = provider.embed(&text).unwrap();
        assert_eq!(embedding, manual);
    }

    #[test]
    fn test_embed_entry_custom_separator() {
        let provider = MockProvider::new(384);
        let with_default = embed_entry(&provider, "Auth", "Use JWT", ": ").unwrap();
        let with_custom = embed_entry(&provider, "Auth", "Use JWT", " - ").unwrap();
        // Different separators produce different concatenated text, so different embeddings
        assert_ne!(with_default, with_custom);
    }

    #[test]
    fn test_embed_entries_batch() {
        let provider = MockProvider::new(384);
        let entries = vec![
            ("Title1".to_string(), "Content1".to_string()),
            ("Title2".to_string(), "Content2".to_string()),
        ];
        let result = embed_entries(&provider, &entries, ": ");
        assert!(result.is_ok());
        let embeddings = result.unwrap();
        assert_eq!(embeddings.len(), 2);
        assert_eq!(embeddings[0].len(), 384);
        assert_eq!(embeddings[1].len(), 384);
    }

    #[test]
    fn test_embed_entries_empty_list() {
        let provider = MockProvider::new(384);
        let entries: Vec<(String, String)> = vec![];
        let result = embed_entries(&provider, &entries, ": ");
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_embed_entry_empty_fields() {
        let provider = MockProvider::new(384);
        let result = embed_entry(&provider, "", "", ": ");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 384);
    }
}
