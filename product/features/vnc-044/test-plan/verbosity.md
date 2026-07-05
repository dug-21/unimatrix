# Test Plan — `response/verbosity.rs` (shared verbosity primitives)

> Component: `Detail` enum, `parse_detail`, `CONTENT_PREVIEW_BYTES = 256`, `content_preview()`.
> Owns the two **Critical** DoS-class risks R-01 (UTF-8 flooring) and R-02 (truncated flag). These are pure functions — cover them exhaustively as table-driven **unit** tests here; downstream projection tests assume them proven.
> Pseudocode: pseudocode/verbosity.md · AC-03b · R-01, R-02, R-10, R-12.

## Unit Test Expectations

All tests are `#[test]` in `verbosity.rs` `#[cfg(test)]`, Arrange/Act/Assert. Every returned preview MUST be asserted valid UTF-8 (it is a Rust `String`, so validity is structural — assert on `std::str::from_utf8` of the bytes only where a byte-level fixture is round-tripped; otherwise the type guarantees it).

### R-01 — `content_preview` UTF-8 char-boundary flooring (Critical, DoS)

`content_preview(&str) -> (String, bool)`. Table-driven. The mandated idiom is `while end > 0 && !content.is_char_boundary(end) { end -= 1; }` — **not** `&content[..256]`, **not** `.chars().take(N)`, **not** nightly `floor_char_boundary`.

| # | Case | Input | Assert preview | Assert flag |
|---|------|-------|----------------|-------------|
| 1 | empty | `""` | `== ""` | `false` |
| 2 | short ASCII | 10-byte ASCII | `== input` (whole) | `false` |
| 3 | boundary-below | 255-byte ASCII | `== input` (whole) | `false` |
| 4 | **exactly 256** | 256-byte ASCII | `== input` (whole, no truncation at the cap) | `false` |
| 5 | **257 ASCII** | 257-byte ASCII | `.len() == 256`, `== input[..256]` | `true` |
| 6 | **multibyte straddling 256 (2-byte)** | 255 ASCII + one 2-byte char (é, bytes 256-257) | `.len() == 255`, floors below 256 on a char boundary, valid UTF-8 | `true` |
| 7 | **multibyte straddling 256 (3-byte)** | 254 ASCII + one 3-byte char (byte 255-257) | floors to 254, valid UTF-8 | `true` |
| 8 | **multibyte straddling 256 (4-byte)** | 253 ASCII + one 4-byte char (emoji, bytes 254-257) | floors to 253, valid UTF-8 | `true` |
| 9 | boundary-exact multibyte | content where byte 256 lands exactly on a boundary between two multibyte chars | `.len() == 256`, valid | `true` |
| 10 | all-multibyte long | 200 × 2-byte chars (400 bytes) | floors to the last whole char ≤256 (≤256, even), valid | `true` |

- **No ellipsis:** assert `!preview.contains('…')` and `!preview.ends_with("...")` in every case.
- **Never panics:** none of these cases may panic. Construct straddle fixtures with explicit codepoints (`char::from_u32(0xE9)`, `'\u{1F600}'`) so the straddle byte is unambiguous (pattern #4769) — not opaque copy-pasted literals.

### R-01 property / fuzz (recommended, Security section)

`content` is attacker-influenceable → a property test over random multibyte strings:
```
proptest / loop over random Vec<char>: content_preview(&s) never panics,
  preview.len() <= 256, s.starts_with(&preview), preview is a char-boundary prefix.
```
Named test: `test_content_preview_never_panics_on_arbitrary_unicode`. This is the mitigation for the request-triggered-DoS (evidence #3706, #4350, #4863). If proptest is not wired in the crate, a deterministic loop over a curated adversarial set (BOM, combining marks, 4-byte emoji at every offset 253..259) suffices.

### R-02 — `content_truncated` byte-compare (Critical)

The flag MUST equal `content.len() > CONTENT_PREVIEW_BYTES`, **decoupled** from where the char-boundary floor landed.

| # | Case | Input len | Expected flag | Trap? |
|---|------|-----------|---------------|-------|
| 1 | empty | 0 | `false` | |
| 2 | 255B | 255 | `false` | |
| 3 | exactly 256B | 256 | `false` | |
| 4 | **257B ASCII (floors to exactly 256)** | 257 | `true` | **YES — false-negative trap** |
| 5 | multibyte >256 flooring to 254 | e.g. 258 | `true` | |

- Test `test_content_truncated_257_ascii_true` is **non-negotiable**: an implementation that derives the flag from `end != 256` returns `false` here and MUST fail this test.
- Add a direct invariant test: for a spread of lengths {0,1,255,256,257,300,1000}, `content_preview(s).1 == (s.len() > 256)`.

### R-10 — boundary/empty projection fidelity (primitive level)

- Empty content → `("", false)` (shared with R-01 #1; restate the intent for the projection consumer).
- Confirm `content_preview` does not allocate/append anything beyond the prefix (no marker, no normalization).

### R-12 — single-source constant

- `content_preview` and every length assertion reference `CONTENT_PREVIEW_BYTES`, never a bare `256`. A grep/code-review gate (documented in graph_read.md static gates) confirms no bare `256` literal in the graph path; the unit tests here reference the constant symbolically so a future cap change re-flows.

### `parse_detail` — verbosity axis parse

`parse_detail(&Option<String>) -> Result<Detail, ServerError>`:

| Input | Expected |
|-------|----------|
| `None` | `Ok(Detail::Summary)` (default = summary, FR-3) |
| `Some("summary")` | `Ok(Detail::Summary)` |
| `Some("full")` | `Ok(Detail::Full)` |
| `Some("Summary")` / `Some("SUMMARY")` | `Ok(Detail::Summary)` — case-INSENSITIVE accept (mirrors `response/mod.rs::parse_format`'s `f.to_lowercase().as_str()`) |
| `Some("Full")` / `Some("FULL")` | `Ok(Detail::Full)` — case-INSENSITIVE accept |
| `Some("brief")` / `Some("bogus")` / `Some("")` | `Err` → maps to `ERROR_INVALID_PARAMS` (genuinely-unknown value only) |

`test_parse_detail_none_defaults_summary` is the load-bearing default-flip assertion (AC-05 at the unit level). `test_parse_detail_case_insensitive` pins the ratified case-INSENSITIVE accept (`"Summary"`/`"SUMMARY"`/`"Full"`/`"FULL"` all accepted, mirroring `parse_format`); `test_parse_detail_unknown_rejected` pins that only a genuinely-unknown value (`"brief"`) errors.

## Integration Test Expectations

None owned directly by this component — `content_preview`/`parse_detail` have no independent MCP surface. Their end-to-end effect is observed through the projection (graph_read_projection.md) and resolver (graph_read.md) integration tests. The through-wire manifestation of R-01/R-02 is: a stored entry with multibyte content near 256 bytes, pulled via `context_graph(...,detail=summary)`, returns a valid-JSON `content_preview` and correct `content_truncated` without erroring (one such assertion belongs in the projection integration tests).

## Edge Cases Owned Here

- Empty content (R-01 #1, R-10).
- 255 / 256 / 257 byte boundaries (R-02).
- 2/3/4-byte codepoint straddling byte 256 (R-01 #6-8).
- Byte 256 exactly on a char boundary (R-01 #9).
- Arbitrary adversarial Unicode — no panic (Security).
