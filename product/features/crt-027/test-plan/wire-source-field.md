# Test Plan: wire-source-field (unimatrix-engine/src/wire.rs)

## Component

`crates/unimatrix-engine/src/wire.rs`

Changes: Add `#[serde(default)] source: Option<String>` to `HookRequest::ContextSearch`.

## Risks Covered

R-01 (source field backward compat and struct-literal compile surface), R-13 (HookRequest::Briefing not removed)

## ACs Covered

AC-05 (a — wire-level deserialization), AC-25

---

## Unit Test Expectations

All tests in `crates/unimatrix-engine/src/` or within `wire.rs`'s `#[cfg(test)]` block.

### Test: `context_search_source_absent_deserializes_to_none`
**Risk**: R-01 scenario 1
**Arrange**: JSON blob for `HookRequest::ContextSearch` that omits the `source` key entirely:
```json
{"type": "ContextSearch", "query": "design the hook", "session_id": null, "role": null, "task": null, "feature": null, "k": null, "max_tokens": null}
```
**Act**: `serde_json::from_str::<HookRequest>(&json)`
**Assert**:
- Deserialization succeeds (no error)
- `source == None`

### Test: `context_search_source_present_deserializes_correctly`
**Risk**: R-01 scenario 2
**Arrange**: JSON blob with `"source": "SubagentStart"`
**Act**: `serde_json::from_str::<HookRequest>(&json)`
**Assert**: `source == Some("SubagentStart".to_string())`

### Test: `context_search_source_none_round_trip`
**Risk**: R-01 scenario 3
**Arrange**: `HookRequest::ContextSearch { query: "test".to_string(), session_id: None, role: None, task: None, feature: None, k: None, max_tokens: None, source: None }`
**Act**: `serde_json::to_string(&req)` then `serde_json::from_str::<HookRequest>(&json)`
**Assert**: Round-tripped struct has `source == None` (not a parse error on re-read)

### Test: `context_search_source_subagentstart_round_trip`
**Arrange**: Same as above but `source: Some("SubagentStart".to_string())`
**Act**: Serialize then deserialize
**Assert**: `source == Some("SubagentStart".to_string())`

### Test: `hook_request_briefing_variant_still_present` (R-13)
**Assert**: `HookRequest::Briefing { .. }` variant compiles and can be pattern-matched.
This is a compile-time assertion — the test simply constructs a `Briefing` variant and
matches it without panicking. Confirms C-04: `HookRequest::Briefing` is NOT removed.
```rust
let req = HookRequest::Briefing { topic: "test".to_string(), session_id: None };
assert!(matches!(req, HookRequest::Briefing { .. }));
```

### Compile-Time Verification (AC-25)

Any existing test in `wire.rs`, `hook.rs`, or `listener.rs` that constructs
`HookRequest::ContextSearch` via struct literal must compile after the `source` field is
added. The requirement is:
- All struct-literal constructions add `source: None` OR
- Use `..Default::default()` spread if `Default` is derived (it is not currently, so explicit
  `source: None` is the path)

**Gate check**: `cargo build --release` with no `non_exhaustive` or `missing field` errors.

---

## Integration Test Expectations

No infra-001 integration tests target `wire.rs` directly — it is a pure serialization
layer. Integration-level coverage of `source` field propagation is provided by
`listener-dispatch.md` tests that submit complete `HookRequest` objects and assert on
the observation table `hook` column.

---

## Edge Cases

| Edge Case | Scenario | Expected |
|-----------|----------|----------|
| `source` key with value `null` in JSON | `{"source": null}` | `source == None` (serde default) |
| `source` key with empty string | `{"source": ""}` | `source == Some("")` — server uses `unwrap_or("UserPromptSubmit")`, empty string goes through; `dispatch_request` would tag observation as `""` — verify this behavior is acceptable or clamp to `"UserPromptSubmit"` |
| `source` key with unknown value | `{"source": "PreToolUse"}` | Accepted — stored as observation `hook` value; no validation at wire level |
