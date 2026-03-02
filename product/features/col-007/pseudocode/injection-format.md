# Pseudocode: injection-format

## Purpose

Format a ranked list of `EntryPayload` entries as structured plain text within a byte budget. Output is printed to stdout by the hook process and injected into Claude's context window.

## Constants

```
const MAX_INJECTION_BYTES: usize = 1400;  // ~350 tokens at 4 bytes/token
```

## New Function: format_injection()

```
fn format_injection(entries: &[EntryPayload], max_bytes: usize) -> Option<String>:
    if entries.is_empty():
        return None

    let mut output = String::new()
    let header = "--- Unimatrix Context ---\n"
    output.push_str(header)

    let mut entries_added = 0

    for entry in entries:
        // Format a single entry block
        let block = format_entry_block(entry)

        // Check if adding this block would exceed the budget
        let projected_len = output.len() + block.len()
        if projected_len <= max_bytes:
            output.push_str(&block)
            entries_added += 1
        else:
            // Check remaining budget
            let remaining = max_bytes - output.len()
            if remaining < 100:
                // Too small for meaningful content -- stop
                break

            // Truncate block to fit
            let truncated = truncate_utf8(&block, remaining)
            output.push_str(&truncated)
            entries_added += 1
            break

    if entries_added == 0:
        return None

    Some(output)
```

## Helper: format_entry_block()

```
fn format_entry_block(entry: &EntryPayload) -> String:
    // Format: title line, metadata line, content, blank separator
    let confidence_pct = (entry.confidence * 100.0) as u32
    format!(
        "[{title}] ({category}, {confidence}% confidence)\n{content}\n<!-- id:{id} sim:{similarity:.2} -->\n\n",
        title = entry.title,
        category = entry.category,
        confidence = confidence_pct,
        content = entry.content,
        id = entry.id,
        similarity = entry.similarity,
    )
```

The entry ID and similarity are included in an HTML comment for downstream tracing (col-008/col-009 will parse these). HTML comments are invisible to Claude's reasoning but preserved in the context.

## Helper: truncate_utf8()

```
fn truncate_utf8(s: &str, max_bytes: usize) -> &str:
    if s.len() <= max_bytes:
        return s

    // Find the largest byte index <= max_bytes that is a valid char boundary
    let mut end = max_bytes
    while end > 0 && !s.is_char_boundary(end):
        end -= 1

    &s[..end]
```

This is critical for R-03 (multi-byte UTF-8 safety). Rust's `str::is_char_boundary()` ensures we never split a multi-byte character.

## Error Handling

- Empty entries slice: return None (silent skip)
- All entries too large for even the header + 100 bytes: return None
- Entry with content containing control characters: preserved as-is (FR-03.7 says no control characters, but entry content from the knowledge base is trusted; sanitization is a store-level concern)

## Key Test Scenarios

1. Single entry fits within budget: full output returned
2. Multiple entries fit within budget: all included in rank order
3. Entry exceeds remaining budget: truncated at UTF-8 boundary
4. Entry would leave < 100 bytes remaining: omitted entirely
5. Empty entries: returns None
6. CJK content (3 bytes/char): byte count correct, no split characters
7. Emoji content (4 bytes/char): byte count correct, no split characters
8. Output never exceeds max_bytes
9. Output is always valid UTF-8
10. Header line present: "--- Unimatrix Context ---"
11. Each entry includes title, category, confidence percentage, content
12. Entry IDs present in HTML comment metadata
13. Entries appear in input order (rank order from server)
