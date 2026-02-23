# Pseudocode: content-hash

## Purpose
Implement SHA-256 content hash computation matching the embedding pipeline text format.

## New File: crates/unimatrix-store/src/hash.rs

```
use sha2::{Sha256, Digest};

/// Compute SHA-256 hash of entry content.
/// Format matches prepare_text(title, content, ": ") from unimatrix-embed.
/// Returns lowercase hex string (64 chars).
pub(crate) fn compute_content_hash(title: &str, content: &str) -> String {
    let text = match (title.is_empty(), content.is_empty()) {
        (true, true) => String::new(),
        (true, false) => content.to_string(),
        (false, true) => title.to_string(),
        (false, false) => format!("{title}: {content}"),
    };
    let hash = Sha256::digest(text.as_bytes());
    format!("{hash:x}")
}
```

## Modified File: crates/unimatrix-store/src/lib.rs

Add `mod hash;` to module declarations.

## Error Handling
SHA-256 digest is infallible. No error paths.

## Key Test Scenarios
- Known value: compute_content_hash("Test", "Content") == sha256("Test: Content")
- Empty title: hash of content only
- Empty content: hash of title only
- Both empty: hash of ""
- Unicode content: deterministic hash
- Output is 64 lowercase hex characters
- Same input always produces same output
