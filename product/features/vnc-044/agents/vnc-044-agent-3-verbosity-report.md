# Agent Report — vnc-044-agent-3-verbosity

## Task
Implement the shared verbosity primitives component (`mcp/response/verbosity.rs`, NEW) from
validated pseudocode: `CONTENT_PREVIEW_BYTES`, `Detail` enum, `parse_detail`, `content_preview`.
Register the module in `response/mod.rs`.

## Files Modified
- `crates/unimatrix-server/src/mcp/response/verbosity.rs` (new)
- `crates/unimatrix-server/src/mcp/response/mod.rs` (single `pub mod verbosity;` registration line)

## Implementation Notes
- `parse_detail` mirrors the established `parse_format` idiom (case-insensitive via
  `to_lowercase().as_str()`, `None` → `Detail::Summary` default). Unknown/empty →
  `ServerError::InvalidInput { field: "detail", ... }` → `ERROR_INVALID_PARAMS` on conversion.
- `content_preview` uses the mandated codebase char-boundary floor
  (`while end > 0 && !content.is_char_boundary(end) { end -= 1; }`) — not `&s[..256]`, not
  nightly `floor_char_boundary`, not `.chars().take()`.
- `truncated` is `content.len() > CONTENT_PREVIEW_BYTES`, structurally decoupled from the
  flooring index: the `if len <= CAP` early-return + unconditional `true` on the else branch
  encode the R-02 trap correctly (the flag is never derived from `end`).
- `256` single-sourced as `CONTENT_PREVIEW_BYTES`; every test asserts length symbolically
  against the constant (R-12), no bare `256` in the module body.
- No re-export into `response::*` — kept as a sibling module so the shared
  `ResponseFormat`/`parse_format` enum surface is untouched (C-4 / SR-06).

## Tests
`cargo test -p unimatrix-server --lib verbosity`: **24 passed, 0 failed.**

Coverage vs test plan:
- R-01 boundary table cases #1–#10 (empty / short / 255 / exactly-256 / 257-ASCII / 2-,3-,4-byte
  straddle / boundary-exact multibyte / all-multibyte-long) — each asserts valid UTF-8, no
  ellipsis, correct floor length, correct flag.
- R-01 deterministic fuzz `test_content_preview_never_panics_on_arbitrary_unicode` — BOM,
  combining marks, 4-byte emoji at every pad offset 250..=260, mixed adversarial cycle 0..300.
- R-02 `test_content_truncated_257_ascii_true` (non-negotiable false-negative trap) + empty/255/
  256/multibyte cases + `test_content_truncated_equals_byte_compare_invariant` over
  {0,1,255,256,257,300,1000}.
- R-10 empty-projection pair + no-marker/no-normalization.
- `parse_detail`: none-defaults-summary, summary, full, case-insensitive
  (`Summary`/`SUMMARY`/`Full`/`FULL`), unknown-rejected (`brief`/`bogus`/``).

Clippy clean for the crate lib (zero warnings). File formatted with `rustfmt` only (no
workspace-wide `cargo fmt`). File is 361 lines (< 500 limit).

## Issues / Blockers
None. No out-of-scope breakage observed.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search ('MCP response serialization UTF-8 char boundary
  content preview truncation', category=pattern) — no directly-applicable primitive-level
  pattern surfaced (top hits were markdown-formatter / MCP-dispatch patterns #3459, #3814,
  #3789; not relevant to the char-boundary floor). Relied on ADR-001 §6 + pseudocode's mandated
  idiom instead.
- Stored: nothing novel to store — the two load-bearing gotchas (the `&s[..256]` DoS panic and
  the R-02 truncated-flag decoupling) are already exhaustively documented in ADR-001 §6 and
  `pseudocode/verbosity.md`; storing a duplicate would only add recall noise. The implementation
  followed the ratified spec without discovering a new failure mode.
