# Component 1 — Shared verbosity primitives (`mcp/response/verbosity.rs`, NEW)

## Purpose

Single source (ADR-001 SR-03) for the tool-agnostic pieces of the two-axis contract: the
verbosity enum + parser, the 256-byte preview cap, and the UTF-8-safe preview builder. Every
adopter imports these; nothing is re-declared or re-literalled downstream (C-9). vnc-044's only
adopter is `context_graph`, but this module is deliberately tool-neutral so later migrations
reuse it unchanged.

## Module wiring

- New file `crates/unimatrix-server/src/mcp/response/verbosity.rs`.
- In `mcp/response/mod.rs` add `pub mod verbosity;` (alongside the existing `pub mod edges;`
  / `pub mod status;`). Do NOT re-export its items into `response::*` — graph code references
  them by full path (`crate::mcp::response::verbosity::…`) to keep the shared enum surface of
  `response/mod.rs` untouched (C-4/SR-06).
- Imports: `use crate::error::ServerError;`

## Constants

```
pub const CONTENT_PREVIEW_BYTES: usize = 256;
```

This is the only definition of 256 in the graph path (C-9 / R-12). `content_preview` and any
cap reference use this symbol.

## Types

```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detail { Summary, Full }
```

## Functions

### `parse_detail`

```
pub fn parse_detail(detail: &Option<String>) -> Result<Detail, ServerError>
```

Pseudocode (mirrors the established `parse_format` idiom — case-insensitive via `to_lowercase`,
`None` → the default):

```
match detail {
    None => Ok(Detail::Summary),                      // DEFAULT = summary (ADR-001 §3, FR-3, AC-05)
    Some(d) => match d.to_lowercase().as_str() {
        "summary" => Ok(Detail::Summary),
        "full"    => Ok(Detail::Full),
        _ => Err(ServerError::InvalidInput {
            field:  "detail".to_string(),
            reason: "must be summary or full".to_string(),
        }),
    },
}
```

- `ServerError::InvalidInput` maps to `ERROR_INVALID_PARAMS` via the existing
  `From<ServerError> for ErrorData` (error.rs:251) — callers use `?`/`.map_err(Into::into)`.
- Returning `ServerError` (not `ErrorData`) keeps this primitive tool-agnostic; the graph
  resolver adapts it (component 3).

### `content_preview`

```
pub fn content_preview(content: &str) -> (String, bool)   // (preview, truncated)
```

**Exact idiom — do NOT substitute.** Not `&s[..256]` (panics on a non-boundary — request-
triggered DoS, R-01/#3706), not nightly `str::floor_char_boundary`, not `.chars().take(N)`
(char count cannot enforce a *byte* cap — R-01/#4350):

```
pub fn content_preview(content: &str) -> (String, bool) {
    if content.len() <= CONTENT_PREVIEW_BYTES {
        return (content.to_string(), false);          // whole content fits; NOT truncated
    }
    let mut end = CONTENT_PREVIEW_BYTES;
    while end > 0 && !content.is_char_boundary(end) {  // codebase char-boundary floor
        end -= 1;
    }
    (content[..end].to_string(), true)                 // truncated == content.len() > 256
}
```

Contract nuances (load-bearing):
- `content.len()` is **bytes** (Rust `str::len`), matching the byte cap.
- `truncated` is `content.len() > CONTENT_PREVIEW_BYTES` — decoupled from where `end` floored.
  The trap (R-02): 257-byte ASCII floors `end` to exactly 256, yet `truncated` MUST be `true`
  because the length compare — not `end != 256` — decides it. The `if content.len() <= CAP`
  early-return + the unconditional `true` on the else branch encode this correctly; never
  derive the flag from `end`.
- **No ellipsis** — no `…`/marker appended. `content_truncated` is the only truncation signal.
- Exactly-256 bytes → first branch → `(whole, false)`. Empty `""` → first branch →
  `("", false)`.
- Result is always valid UTF-8 (slice ends on a char boundary; `end` reaches 0 in the
  degenerate case, yielding `""`).

## Data flow

- Input: a node's full `content: &str` (from `EntryRecord.content`, still read by
  `fetch_nodes_batch` — SR-01; wire-size win only, not a DB-read win).
- Output: `(String, bool)` consumed by `node_summary` (component 2).

## Error handling

- `parse_detail` is the only fallible function → `ServerError::InvalidInput` on an unknown
  string. `content_preview` is total (never panics, never errors) — that totality is the R-01
  DoS mitigation.

## Key test scenarios (hints; full plan in test-plan/verbosity.md)

- `content_preview` boundary table (R-01/AC-03b), every case asserts valid UTF-8 + no `…`:
  1. `""` → `("", false)`
  2. `<256` ASCII → `(whole, false)`
  3. exactly 256 → `(whole, false)`
  4. 257 ASCII → 256-byte prefix, `true`  ← the R-02 false-negative trap
  5. multibyte codepoint (2/3/4-byte) straddling byte 256 → floors **below** 256 on a char
     boundary, valid UTF-8, `true` (build via `char::from_u32`, not bare literals — #4769)
  6. byte 256 landing exactly on a boundary between two multibyte chars → floors to 256, `true`
- `content_truncated == (content.len() > CONTENT_PREVIEW_BYTES)` asserted directly,
  independent of the flooring index (R-02).
- `parse_detail`: `None`→Summary; `"summary"`/`"SUMMARY"`→Summary; `"full"`→Full;
  `"bogus"`→`Err` (`ERROR_INVALID_PARAMS` after conversion).
- Preview-length assertions reference `CONTENT_PREVIEW_BYTES` symbolically, never a bare `256`
  (R-12).

## Constraints honored

- C-9 / SR-03: `256` single-sourced here as `CONTENT_PREVIEW_BYTES`.
- SR-06 / C-4: `response/mod.rs` shared `ResponseFormat`/`parse_format` untouched; this is a
  sibling module, not an edit to the shared enum.
