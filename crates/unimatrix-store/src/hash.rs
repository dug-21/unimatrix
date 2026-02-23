use sha2::{Digest, Sha256};

/// Compute SHA-256 hash of entry content.
///
/// Format matches `prepare_text(title, content, ": ")` from unimatrix-embed.
/// Returns lowercase hex string (64 chars).
pub fn compute_content_hash(title: &str, content: &str) -> String {
    let text = match (title.is_empty(), content.is_empty()) {
        (true, true) => String::new(),
        (true, false) => content.to_string(),
        (false, true) => title.to_string(),
        (false, false) => format!("{title}: {content}"),
    };
    let hash = Sha256::digest(text.as_bytes());
    format!("{hash:x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_hash_known_value() {
        let hash = compute_content_hash("Test", "Content");
        let expected = format!("{:x}", Sha256::digest(b"Test: Content"));
        assert_eq!(hash, expected);
    }

    #[test]
    fn test_content_hash_empty_title() {
        let hash = compute_content_hash("", "Content");
        let expected = format!("{:x}", Sha256::digest(b"Content"));
        assert_eq!(hash, expected);
    }

    #[test]
    fn test_content_hash_empty_content() {
        let hash = compute_content_hash("Title", "");
        let expected = format!("{:x}", Sha256::digest(b"Title"));
        assert_eq!(hash, expected);
    }

    #[test]
    fn test_content_hash_both_empty() {
        let hash = compute_content_hash("", "");
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_content_hash_unicode() {
        let hash = compute_content_hash("\u{4e16}\u{754c}", "\u{1f510} secure");
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        // Determinism
        let hash2 = compute_content_hash("\u{4e16}\u{754c}", "\u{1f510} secure");
        assert_eq!(hash, hash2);
    }

    #[test]
    fn test_content_hash_determinism() {
        let first = compute_content_hash("Same", "Input");
        for _ in 0..100 {
            assert_eq!(compute_content_hash("Same", "Input"), first);
        }
    }

    #[test]
    fn test_content_hash_format() {
        let hash = compute_content_hash("Any", "Value");
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        // Lowercase
        assert!(hash.chars().filter(|c| c.is_alphabetic()).all(|c| c.is_lowercase()));
    }
}
