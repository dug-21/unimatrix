# C6: Text Module -- Test Plan

## Tests

```
test_prepare_text_both_present:
    result = prepare_text("JWT", "Validate exp", ": ")
    ASSERT result == "JWT: Validate exp"

test_prepare_text_empty_title:
    result = prepare_text("", "content only", ": ")
    ASSERT result == "content only"

test_prepare_text_empty_content:
    result = prepare_text("title only", "", ": ")
    ASSERT result == "title only"

test_prepare_text_both_empty:
    result = prepare_text("", "", ": ")
    ASSERT result == ""

test_prepare_text_custom_separator:
    result = prepare_text("title", "content", " - ")
    ASSERT result == "title - content"

test_prepare_text_empty_separator:
    result = prepare_text("title", "content", "")
    ASSERT result == "titlecontent"

test_prepare_text_title_contains_separator:
    result = prepare_text("key: value", "content", ": ")
    ASSERT result == "key: value: content"

test_prepare_text_unicode:
    result = prepare_text("Auth", "Validate tokens", ": ")
    ASSERT result == "Auth: Validate tokens"

test_prepare_text_long_content:
    title = "Short Title"
    content = "a".repeat(10000)
    result = prepare_text(title, &content, ": ")
    ASSERT result.starts_with("Short Title: ")
    ASSERT result.len() == 12 + 10000 + 2  // title + separator + content

test_embed_entry_calls_provider:
    // Integration: verify embed_entry uses prepare_text + provider.embed
    provider = MockProvider::new(384)
    result = embed_entry(&provider, "Auth", "Use JWT")
    ASSERT result.is_ok()
    embedding = result.unwrap()
    ASSERT embedding.len() == 384

    // Verify same result as manual prepare + embed
    text = prepare_text("Auth", "Use JWT", ": ")
    manual = provider.embed(&text).unwrap()
    ASSERT embedding == manual

test_embed_entries_batch:
    provider = MockProvider::new(384)
    entries = vec![
        ("Title1".to_string(), "Content1".to_string()),
        ("Title2".to_string(), "Content2".to_string()),
    ]
    result = embed_entries(&provider, &entries)
    ASSERT result.is_ok()
    embeddings = result.unwrap()
    ASSERT embeddings.len() == 2
    ASSERT embeddings[0].len() == 384
    ASSERT embeddings[1].len() == 384

test_embed_entries_empty_list:
    provider = MockProvider::new(384)
    entries: Vec<(String, String)> = vec![]
    result = embed_entries(&provider, &entries)
    ASSERT result.is_ok()
    ASSERT result.unwrap().is_empty()

test_embed_entry_empty_fields:
    provider = MockProvider::new(384)
    result = embed_entry(&provider, "", "")
    ASSERT result.is_ok()
    ASSERT result.unwrap().len() == 384
```

## Risks Covered

- R-08: Title+content concatenation edge cases (AC-06).
- AC-07: embed_entry produces same result as prepare_text + embed.
- AC-06: Empty title, empty content, both empty, custom separator.
