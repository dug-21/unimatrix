# crt-028: listener.rs Pseudocode — GH #354 Source Field Allowlist

## Purpose

Fix GH #354: the `source` field on `HookRequest::ContextSearch` is written verbatim to
the `hook TEXT NOT NULL` column in the observations table without validation. Replace
the inline expression with an allowlist-validated helper (ADR-004 crt-028).

This is a minimal, single-site change. No other logic in `listener.rs` is touched by
crt-028.

---

## File: `crates/unimatrix-server/src/uds/listener.rs`

---

## New Function: `sanitize_observation_source`

Add as a private `fn` in listener.rs, grouped with the existing `sanitize_session_id`
and `sanitize_metadata_field` helpers (approximately after line 77 in the current file,
or at a logical grouping point near those helpers):

```
/// Allowlist-validate the `source` field before writing to the observations hook column.
///
/// Valid values (compile-time exhaustive set, ADR-004 crt-028):
///   "UserPromptSubmit" — default; UserPromptSubmit hook arm (source: None)
///   "SubagentStart"    — SubagentStart hook arm
///
/// Any other value (including None, empty string, and arbitrarily long strings)
/// falls back to "UserPromptSubmit".
///
/// SOLE WRITE GATE: This function is the only place that validates the `source`
/// field before it is written to the observations `hook` column. Future code
/// that constructs ObservationRow for ContextSearch-derived observations MUST
/// call this function. Do not add a second write site that bypasses this helper
/// (GH #354, SR-05 — guards against schema pollution and future injection risk).
///
/// To add a new source type: add a new arm to the match below.
fn sanitize_observation_source(source: Option<&str>) -> String {
    match source {
        Some("UserPromptSubmit") => "UserPromptSubmit".to_string(),
        Some("SubagentStart")    => "SubagentStart".to_string(),
        _                        => "UserPromptSubmit".to_string(),
    }
}
```

---

## Replacement Site in `dispatch_request`

### Location

`dispatch_request`, the `HookRequest::ContextSearch` arm, inside the
`if let Some(ref sid) = session_id` block where `ObservationRow` is constructed.

The current code (approximately line 812-813):

```rust
// ADR-001 crt-027: use source field, default to "UserPromptSubmit"
hook: source.as_deref().unwrap_or("UserPromptSubmit").to_string(),
```

### Replacement

```rust
// GH #354: allowlist-validated; see sanitize_observation_source (ADR-004 crt-028)
hook: sanitize_observation_source(source.as_deref()),
```

The `ObservationRow` construction block in full context becomes:

```
let obs = ObservationRow {
    session_id: sid.clone(),
    ts_millis: (unix_now_secs() as i64).saturating_mul(1000),
    // GH #354: allowlist-validated; see sanitize_observation_source (ADR-004 crt-028)
    hook: sanitize_observation_source(source.as_deref()),
    tool: None,
    input: Some(truncated_input),
    response_size: None,
    response_snippet: None,
    topic_signal: topic_signal.clone(),
};
```

No other fields in `ObservationRow` are changed.

---

## Context: Why Only This One Site

The `source` field on `HookRequest::ContextSearch` is the only field that uses an
unchecked value for the `hook` column in the `ContextSearch` arm. The other
`ObservationRow` construction sites (in `extract_observation_fields` for batch events
and in `handle_compact_payload`) use `event.event_type` values, which come from internal
code paths controlled by the server. Only the UDS-wire `source` field from an external
process requires allowlist validation.

---

## Error Handling

`sanitize_observation_source` is total (no failure path). The match is exhaustive:
the wildcard arm `_` covers all unrecognized values including None, empty string, and
adversarially long strings. The return type is `String` — no `Option`, no `Result`.

---

## Key Test Scenarios

### R-07 (High): All six allowlist cases (AC-11)

Unit test `sanitize_observation_source_all_cases`:

1. `sanitize_observation_source(Some("UserPromptSubmit"))` == `"UserPromptSubmit"`
2. `sanitize_observation_source(Some("SubagentStart"))` == `"SubagentStart"`
3. `sanitize_observation_source(None)` == `"UserPromptSubmit"`
4. `sanitize_observation_source(Some("unknown"))` == `"UserPromptSubmit"`
5. `sanitize_observation_source(Some(""))` == `"UserPromptSubmit"`
6. `sanitize_observation_source(Some("UserPromptSubmitXXXXXXXXX"))` == `"UserPromptSubmit"`

All six cases correspond directly to ADR-004 and AC-11.

### Integration: end-to-end source field write path

Test name: `context_search_source_sanitized_in_observation`

Steps:
1. Call `dispatch_request` with `HookRequest::ContextSearch { source: Some("Injected\nEvil"), ... }`
2. Fetch the observation row written to the store
3. Assert `observation.hook == "UserPromptSubmit"` (not the injected value)

This verifies the end-to-end write path including the single call site replacement.

### No second write site regression

The doc comment on `sanitize_observation_source` is the primary guard. Code review
should verify via grep that `source.as_deref().unwrap_or` does not appear in
`listener.rs` after the change (the old pattern is replaced, not duplicated).
