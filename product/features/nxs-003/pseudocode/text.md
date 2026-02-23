# C6: Text Module -- Pseudocode

## Purpose

Title+content concatenation and convenience functions for embedding entries.

## File: `crates/unimatrix-embed/src/text.rs`

```
USE crate::provider::EmbeddingProvider
USE crate::error::Result

/// Concatenate title and content with separator.
/// - Both non-empty: "{title}{separator}{content}"
/// - Title empty: content only
/// - Content empty: title only
/// - Both empty: ""
pub fn prepare_text(title: &str, content: &str, separator: &str) -> String:
    MATCH (title.is_empty(), content.is_empty()):
        (true, true)   => String::new()
        (true, false)  => content.to_string()
        (false, true)  => title.to_string()
        (false, false) => format!("{title}{separator}{content}")

/// Embed a single entry (title + content) using the default separator ": ".
pub fn embed_entry(
    provider: &dyn EmbeddingProvider,
    title: &str,
    content: &str,
) -> Result<Vec<f32>>:
    text = prepare_text(title, content, ": ")
    provider.embed(&text)

/// Embed a batch of entries (title + content pairs) using the default separator ": ".
pub fn embed_entries(
    provider: &dyn EmbeddingProvider,
    entries: &[(String, String)],
) -> Result<Vec<Vec<f32>>>:
    texts: Vec<String> = entries.iter()
        .map(|(title, content)| prepare_text(title, content, ": "))
        .collect()

    refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect()
    provider.embed_batch(&refs)
```

## Design Notes

- `prepare_text` never includes the separator when either title or content is empty.
- The default separator ": " is hardcoded in `embed_entry`/`embed_entries`, matching EmbedConfig's default.
- `embed_entries` collects prepared texts then creates references for `embed_batch`, avoiding lifetime issues with temporary Strings.
- The `provider` parameter uses `&dyn EmbeddingProvider` for object-safe dispatch.
- R-08: Title+content concatenation edge cases are a medium risk.
