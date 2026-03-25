## ADR-002: Separate `extract_error_field()` for PostToolUseFailure Payloads

### Context

`PostToolUse` payloads carry the tool outcome in `tool_response` (a JSON object). The existing
`extract_response_fields()` function in `listener.rs` reads `payload["tool_response"]`, serializes
it, measures its byte length, and truncates to 500 characters for the `response_snippet` column.

`PostToolUseFailure` payloads carry the outcome in `error` (a plain string). There is no
`tool_response` field. If `extract_response_fields()` were called on a failure payload it would
return `(None, None)` — the `tool_response` check would miss, and the legacy fallback would also
miss — producing a stored record with no `response_snippet` despite an error being present.

SR-01 (high severity, high likelihood) flags this as the primary risk: calling the wrong extractor
silently loses the error content without a test failure.

Two alternative designs were considered:

**Option A — extend `extract_response_fields()` with a field-name hint parameter:**
`extract_response_fields(payload, field: &str)` would read `payload[field]`. The caller passes
`"tool_response"` or `"error"` explicitly. This avoids a new function but adds a parameter to an
internal function used in multiple call sites, and mixes two different semantic shapes (object vs.
string) in one function body.

**Option B — add `extract_error_field()` as a sibling function:**
A new function reads `payload["error"]` as a plain string, applies the same 500-character truncation,
and returns `(None, Some(snippet))`. `response_size` is always `None` for failure events — error
messages are short and measuring their byte length provides no analytical value. The `"PostToolUseFailure"`
arm in `extract_observation_fields()` calls `extract_error_field()`; the existing `"PostToolUse"`
arm is unchanged.

### Decision

Adopt **Option B**: add a new `extract_error_field(payload: &serde_json::Value) -> (Option<i64>, Option<String>)`
function alongside `extract_response_fields()` in `listener.rs`.

Specification:
- Read `payload["error"].as_str()`
- If present and non-empty: return `(None, Some(snippet))` where snippet is truncated to 500 chars
  at a valid UTF-8 char boundary (same logic as `extract_response_fields`)
- If absent or null: return `(None, None)`
- `is_interrupt` field (optional bool on failure payloads) is ignored — it does not contribute to
  any stored field in col-027

The `"PostToolUseFailure"` arm in `extract_observation_fields()` calls `extract_error_field()`.
The existing call sites for `extract_response_fields()` are not modified.

### Consequences

**Easier:**
- The distinction between `error` (string) and `tool_response` (object) is explicit at the call
  site, not runtime-detected. A future reader knows exactly which extractor handles which event type.
- Tests for `extract_error_field()` use failure payloads with `"error": "some message"` — they
  cannot accidentally test the wrong shape.
- SR-01 is fully mitigated: there is no code path that would silently return `(None, None)` for a
  failure event with a populated `error` field.

**Harder / Watch for:**
- Two very similar truncation functions exist. If the truncation limit (500 chars) or logic changes,
  both must be updated. This is acceptable given the low frequency of such changes and the clarity
  gained by keeping them separate.
- `response_size` is always `None` for failure events. Consumers that aggregate `response_size`
  (e.g., `total_context_loaded_kb` in `metrics.rs`) already filter on `POSTTOOLUSE` by event_type,
  so they will not incorrectly include `PostToolUseFailure` records. This is correct by design.
