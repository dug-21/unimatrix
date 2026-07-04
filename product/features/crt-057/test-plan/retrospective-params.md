# Test Plan — `RetrospectiveParams` (param surface)

**File:** `unimatrix-server/src/mcp/tools.rs:~431`
**Risks:** R-12, R-09 (surface) · **ACs:** AC-01, AC-11 (deserialization side)

> Remove `include_transcript_candidates`; add `#[serde(default)] transcript: Option<TranscriptScope>`; keep
> `format` / `force`. The boolean param is GONE — a caller sending it must not silently succeed with old
> semantics.

---

## Deserialization
- `test_transcript_serde_default_omitted_is_none` — omitting `transcript` deserializes to `None` (lean
  default; backward-compatible with pre-crt-057 callers). (AC-01 support.)
- `test_transcript_present_deserializes_scope` — `transcript:{...}` deserializes to `Some(TranscriptScope)`
  with the supplied filters.
- `test_include_transcript_candidates_removed` — the `include_transcript_candidates` field no longer exists
  on `RetrospectiveParams` (source/compile assertion). A payload carrying it does not resurrect old behavior
  (either ignored as unknown, or rejected — assert whichever the serde config fixes).
- `test_force_default_none_is_false` — `force` omitted ≡ `false` (unchanged).
- `test_format_default_is_markdown` — `format` omitted ≡ `"markdown"`.

## Cross-references
- `format` value validation (`"summary"` dropped, unknown → `ERROR_INVALID_PARAMS`) is tested in
  `render-dispatch.md` (AC-11).
- `TranscriptScope` field semantics + `r#match` serde-rename are tested in `transcript-scope.md`.
