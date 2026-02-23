## ADR-004: SHA-256 Content Hash

### Context

nxs-004 adds a `content_hash` field to EntryRecord for tamper detection and integrity verification. The hash must cover the entry's meaningful content. Options:

1. SHA-256 of content field only
2. SHA-256 of `"{title}: {content}"` (matching the embedding text format)
3. SHA-256 of all fields (full record hash)
4. BLAKE3 for speed

Option 1 misses title changes. Option 3 would change on every metadata update (status change, access count increment), making the hash useless for content-specific integrity checks. Option 4 adds a niche dependency for marginal speed benefit at Unimatrix scale. Option 2 covers the semantic content (same text that gets embedded) and is stable under metadata-only updates.

### Decision

Use SHA-256 of `"{title}: {content}"` via the `sha2` crate.

The format `"{title}: {content}"` matches `prepare_text(title, content, ": ")` from unimatrix-embed. This means:
- The hash covers the same text that gets embedded as a vector
- An entry's content hash and its embedding are derived from identical input text
- This alignment is valuable for future integrity checks (crt-003 embedding consistency)

Implementation:
```rust
use sha2::{Sha256, Digest};

pub fn compute_content_hash(title: &str, content: &str) -> String {
    let text = if title.is_empty() && content.is_empty() {
        String::new()
    } else if title.is_empty() {
        content.to_string()
    } else if content.is_empty() {
        title.to_string()
    } else {
        format!("{title}: {content}")
    };
    let hash = Sha256::digest(text.as_bytes());
    format!("{hash:x}")
}
```

Output is lowercase hex (64 characters for SHA-256). Stored as `String` on EntryRecord.

### Consequences

- **Easier**: Content integrity verification is trivial -- recompute hash from title+content and compare.
- **Easier**: Hash stability -- metadata-only updates (status, access_count, confidence) don't change the hash.
- **Easier**: Embedding consistency checks (crt-003) can verify hash against embedding input.
- **Harder**: Title or content changes always produce a new hash (this is intentional -- the `previous_hash` field preserves the chain).
- **Neutral**: SHA-256 is not the fastest hash, but at Unimatrix scale (entries with text content) it's sub-microsecond per entry. Not a bottleneck.
