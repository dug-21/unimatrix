# Test Plan: observation-record

**Component**: `crates/unimatrix-core/src/observation.rs`
**AC Coverage**: AC-01
**Risk Coverage**: R-03 (fixture gap), R-13 (HookType constants misuse)

---

## Unit Test Expectations

### Location: `crates/unimatrix-core/src/observation.rs` (inline `#[cfg(test)]` module)

### T-OR-01: Field presence — event_type and source_domain replace hook

```rust
// test_observation_record_has_event_type_and_source_domain
// Arrange: construct ObservationRecord with event_type and source_domain
// Act: access both fields
// Assert: fields exist and hold the provided string values
// Assert: there is no field named `hook` on the struct (compile-time proof)
```

If `hook: HookType` still exists, this test will not compile. The test itself IS the assertion.

### T-OR-02: Hook type constants are &str, not enum variants

```rust
// test_hook_type_constants_are_str
// Assert: hook_type::PRETOOLUSE == "PreToolUse"
// Assert: hook_type::POSTTOOLUSE == "PostToolUse"
// Assert: hook_type::SUBAGENTSTART == "SubagentStart"
// Assert: hook_type::SUBAGENTSTOPPED == "SubagentStop"
// Assert: the type of each constant is &str (compile-time — they should not be HookType)
```

### T-OR-03: Serialization round-trip

```rust
// test_observation_record_serde_round_trip
// Arrange: ObservationRecord { ts: 1_000_000, event_type: "PostToolUse",
//          source_domain: "claude-code", session_id: "s1", tool: None,
//          input: None, response_size: None, response_snippet: None }
// Act: serde_json::to_value(&record) then serde_json::from_value(v)
// Assert: deserialized record == original
// Assert: serialized JSON has keys "event_type" and "source_domain"
// Assert: serialized JSON does NOT have key "hook"
```

### T-OR-04: All existing fields preserved

```rust
// test_observation_record_all_fields_present
// Assert: ts, session_id, tool, input, response_size, response_snippet fields accessible
// This is a compile-time structural check — if any field was accidentally removed
// during the HookType refactor, this test will fail to compile.
```

---

## Static Verification (AC-01, R-13)

These are grep-based checks run after Wave 3 completes. They are part of the Wave 3
compilation gate checklist, not `cargo test`:

1. `grep -r "hook: HookType" crates/` — assert zero matches.
2. `grep -r "HookType::" crates/` — assert zero matches outside `hook_type` module itself.
3. `grep -r "use.*HookType" crates/` — assert any remaining imports reference `hook_type` module, not an enum type.

---

## Integration Test Expectations

`ObservationRecord` is a shared type. Its correctness in cross-crate contexts is
validated by the detection-rules and metrics-extension test plans.

---

## Edge Cases

- `source_domain = ""` (empty string): not rejected at the struct level; rejection is
  at ingest (see ingest-security). The struct itself accepts any String.
- `event_type` with Unicode characters: must serialize/deserialize correctly per T-OR-03.
- `tool = None` and `input = None`: valid state for non-tool-use events.

---

## R-03 Obligation

After Wave 4, ALL `ObservationRecord` construction sites in test files must supply
both `event_type` (non-empty) and `source_domain` (non-empty). The `make_search_obs`
helper in `extraction_pipeline.rs` must be updated to use `event_type: String` and
`source_domain: String` instead of `hook: HookType`. Grep check:

```bash
grep -rn 'ObservationRecord {' crates/unimatrix-observe/tests/
# Manually verify: every block includes event_type and source_domain with non-empty values
grep -rn 'source_domain: ""' crates/
# Must return zero matches for test files
```
