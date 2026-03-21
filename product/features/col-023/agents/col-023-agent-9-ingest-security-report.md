# Agent Report: col-023-agent-9-ingest-security

**Wave**: 4 (final)
**Feature**: col-023 W1-5 Observation Pipeline Generalization
**Agent ID**: col-023-agent-9-ingest-security

---

## Summary

Wave 4 implemented all ingest-security requirements: removed `HookType` enum from the last three files in `unimatrix-server`, rewrote `parse_observation_rows()` with security bounds, added `json_depth()` helper, and wired `DomainPackRegistry` at server startup.

---

## Files Modified

- `crates/unimatrix-server/src/services/observation.rs`
- `crates/unimatrix-server/src/background.rs`
- `crates/unimatrix-server/src/uds/listener.rs`
- `crates/unimatrix-server/src/main.rs`
- `crates/unimatrix-server/src/mcp/tools.rs`
- `crates/unimatrix-server/src/services/status.rs`
- `crates/unimatrix-server/src/server.rs` (migration test version bump: 13→14)

---

## Implementation Notes

### SqlObservationSource — two constructors

Added `new(store, registry)` for full dependency injection, and `new_default(store)` as a convenience constructor that creates a built-in claude-code registry. All existing call sites (tools.rs, status.rs) use `new_default()`. The `new()` constructor is available for future wiring at a higher level if needed.

### parse_observation_rows() changes

- `HookType` match arm removed; all event_type strings pass through (FR-03.1, AC-11)
- `source_domain = "claude-code"` set unconditionally for hook-path records (FR-03.3)
- Payload size guard: `if s.len() > 65_536 { continue }` before JSON parse (ADR-007)
- `json_depth()` guard: applied after parse, skips on false return (ADR-007)
- `SubagentStart` special-case for plain-text input preserved

### json_depth() implementation

Recursive, O(n) walk with short-circuit at `current > max`. Depth 10 passes (current=10, 10>10 is false). Depth 11 rejects (current=11, 11>10 is true, return false immediately).

### main.rs wiring

`domain_pack_from_config()` helper converts `DomainPackConfig` → `DomainPack`. Applied in both `tokio_main_daemon` and `tokio_main_stdio` startup paths. The registry is built, its categories are registered into `CategoryAllowlist`, and the registry is held as `Arc` for the startup side-effects. The `_observation_registry` variable is prefixed with `_` since the registry isn't yet threaded into individual call sites (they use `new_default()` which creates its own built-in registry).

### Pre-existing test fix

`server::tests::test_migration_v7_to_v8_backfill` asserted schema version == 13. Wave 3 (schema-migration agent) added v13→v14 migration, so the assert needed to be updated to 14.

---

## Test Results

- `cargo check --workspace`: clean (zero errors, 9 pre-existing warnings)
- `cargo test --workspace`: **3112 passed, 0 failed**, 27 ignored

### New tests added (17)

All in `crates/unimatrix-server/src/services/observation.rs`:

| Test | Covers |
|------|--------|
| test_payload_size_boundary_exact_limit_passes | T-SEC-01, AC-06 |
| test_payload_size_one_byte_over_limit_rejects | T-SEC-02, AC-06 |
| test_payload_size_measured_in_bytes_not_chars | T-SEC-03, SEC-01 |
| test_payload_size_multibyte_utf8_boundary_passes | T-SEC-04, SEC-01 |
| test_nesting_depth_boundary_10_passes | T-SEC-05, AC-06 |
| test_nesting_depth_11_rejects | T-SEC-06, AC-06 |
| test_json_depth_no_stack_overflow_at_10_levels | T-SEC-07, SEC-02 |
| test_json_depth_short_circuits_above_max | T-SEC-08, ADR-007 |
| test_parse_rows_unknown_event_type_passthrough | T-SEC-12, AC-11, FR-03.1 |
| test_parse_rows_hook_path_always_claude_code | T-SEC-13, FR-03.3 |
| test_parse_rows_partial_batch_invalid_skipped | T-SEC-14, FM-02 |
| test_json_depth_empty_object_passes | edge case |
| test_json_depth_scalar_passes | edge case |
| test_json_depth_flat_array_passes | edge case |
| test_subagent_start_input_preserved_as_string | SubagentStart path |

Plus 2 json_depth helper tests (build_nested_json is shared infrastructure).

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` — no prior patterns for `SqlObservationSource` or ingest boundary in the knowledge base.
- Stored: pattern entry via `/uni-store-pattern` — see below.

### What to store

Two non-obvious gotchas discovered during implementation:

1. **`SqlObservationSource::new_default()` vs `new(store, registry)`**: All internal call sites use `new_default()` which creates a throwaway built-in registry per call. If the `DomainPackRegistry` needs to be shared or have TOML-configured packs visible during ingest, those callers must be updated to use `new(store, registry_arc)`. The `_observation_registry` in `main.rs` is not currently plumbed to individual request handlers — this is intentional for W1-5 since all events are hook-path and always resolve to `source_domain = "claude-code"`.

2. **Schema version bump cascades to server.rs migration tests**: Any agent that adds a schema migration must update `server::tests::test_migration_v7_to_v8_backfill` — it hardcodes the current schema version number. Failing to do so causes a test failure in a seemingly unrelated test.
