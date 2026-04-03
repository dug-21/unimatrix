# Test Plan Overview: crt-043 Behavioral Signal Infrastructure

## Feature Summary

crt-043 adds two write-path-only signal columns to SQLite schema v20 → v21:
- `goal_embedding BLOB` on `cycle_events` (embedded by fire-and-forget spawn on CycleStart)
- `phase TEXT` on `observations` (pre-captured before spawn_blocking at all four write sites)

No retrieval paths, MCP tool responses, or search ranking logic is changed.

---

## Test Strategy

### Unit Tests (unimatrix-store)

Location: `crates/unimatrix-store/tests/migration_v20_v21.rs` (new file, pattern from `migration_v19_v20.rs`)

Covers:
- Schema migration correctness (v20 fixture → v21 columns present, schema_version = 21)
- Migration idempotency (v21 DB re-opened, no error)
- bincode round-trip encode→decode for `encode_goal_embedding` / `decode_goal_embedding`
- Negative decode test (malformed blob → DecodeError, not panic)
- `update_cycle_start_goal_embedding` no-op contract (non-existent cycle_id → Ok, zero rows)

Location: `crates/unimatrix-store/src/embedding.rs` (or inline in `db.rs`) — unit tests live alongside the helpers per project convention.

### Unit Tests (unimatrix-server)

Location: `crates/unimatrix-server/src/uds/listener.rs` (existing test module) and/or a new `tests/` file.

Covers:
- Phase capture at all four write sites (read-back from DB via raw SQL)
- Pre-capture timing contract (phase captured before spawn_blocking)
- Empty/absent goal → no embed task, no warn, goal_embedding NULL
- Embed service unavailable → warn emitted, cycle start not blocked, goal_embedding NULL
- Embed service error → same outcome as unavailable
- handle_cycle_event returns in < 5ms under slow embed stub (NFR-01 timing contract)
- `context_cycle` MCP response text unchanged (regression)

### Integration-Level Tests

The write path is exercised through the UDS listener. The migration is exercised through `SqlxStore::open()`. Neither is visible through the MCP JSON-RPC interface (no retrieval, no tool response format change), so the integration harness adds limited but non-zero value:

1. Smoke suite: regression gate on overall server stability after schema change
2. Lifecycle suite: restart persistence — after schema v21, server survives restart with existing data intact
3. Tools suite: `context_cycle` tool still accepts type=start/phase_end/stop and returns correct format (AC-06 regression)

---

## Risk-to-Test Mapping

| Risk ID | Priority | Test Location | Test Name(s) | Coverage |
|---------|----------|--------------|--------------|----------|
| R-01 | Critical | server unit | `test_goal_embedding_written_after_cycle_start` | Integration await + DB read-back |
| R-01 | Critical | server unit | `test_goal_embedding_concurrent_cycle_starts` | Concurrent 20-cycle stress (may be `#[ignore]`) |
| R-02 | High | store unit | `test_encode_decode_goal_embedding_round_trip` | Exact float equality |
| R-02 | High | store unit | `test_decode_goal_embedding_malformed_bytes` | DecodeError returned |
| R-02 | High | store unit | `test_encode_decode_matches_raw_bincode` | Cross-call consistency |
| R-03 | High | server unit | `test_phase_captured_record_event_site` | RecordEvent path DB read-back |
| R-03 | High | server unit | `test_phase_captured_rework_candidate_site` | rework_candidate path DB read-back |
| R-03 | High | server unit | `test_phase_captured_record_events_batch_site` | RecordEvents batch DB read-back |
| R-03 | High | server unit | `test_phase_captured_context_search_site` | ContextSearch path DB read-back |
| R-04 | High | server unit | `test_phase_capture_timing_pre_spawn` | Phase at capture time, not write time |
| R-05 | High | store unit | `test_v20_to_v21_both_columns_present` | Real v20 fixture, pragma_table_info |
| R-05 | High | store unit | `test_v20_to_v21_partial_apply_recovery` | Pre-existing goal_embedding column |
| R-06 | Med | store unit | `test_v21_migration_idempotent` | Re-open v21, no error, version = 21 |
| R-07 | High | code review | (static assertion in plan) | Embed via ml_inference_pool |
| R-08 | Med | store unit | `test_update_goal_embedding_nonexistent_cycle_id` | Ok(()) on zero rows affected |
| R-09 | Low | server unit | `test_no_embed_task_on_empty_goal` | No stub calls, no warn, NULL blob |
| R-09 | Low | server unit | `test_no_embed_task_on_absent_goal` | Same assertions, goal=None |
| R-10 | High | server unit | `test_goal_embedding_unavailable_service_warn` | tracing::warn! captured, NULL blob |
| R-10 | High | server unit | `test_goal_embedding_error_during_embed` | Error path, warn captured |
| R-10 | High | server unit | `test_handle_cycle_event_returns_before_embedding` | < 5ms latency |
| R-11 | High | store unit | `test_encode_decode_goal_embedding_round_trip` | (same as R-02, covers both) |
| R-12 | Low | server unit | `test_context_cycle_response_text_unchanged` | Byte-for-byte MCP response check |
| R-13 | Med | delivery note | (written decision required before PR) | Composite index decision |

---

## Cross-Component Test Dependencies

- **schema-migration tests** depend on the `create_v20_database` fixture builder (analogous to `create_v19_database` in `migration_v19_v20.rs`). This fixture must be created as part of the migration test file — no pre-existing v20 `.db` file exists.
- **goal-embedding tests** depend on `update_cycle_start_goal_embedding` being callable (store method) and `encode_goal_embedding` / `decode_goal_embedding` being available in the store crate.
- **phase-capture tests** depend on `insert_observation` and `insert_observations_batch` accepting `phase` in `ObservationRow`. Tests in the server crate use an in-process store instance, not the MCP layer.

---

## Integration Harness Plan

### Suite Selection

crt-043 touches schema + storage changes and UDS listener write paths. Per the suite selection table:

| Feature touch point | Suite(s) to run |
|---------------------|-----------------|
| Schema or storage changes | `lifecycle` (restart persistence), `volume` |
| Any change at all | `smoke` (mandatory minimum gate) |
| `context_cycle` tool (no response change) | `tools` (regression: correct format accepted and returned) |

**Mandatory gate:** `pytest -m smoke` must pass.

**Recommended suites:** `lifecycle`, `tools`, `smoke`.

Volume and security suites are not directly impacted and may be skipped in Stage 3c (regression gate only via smoke).

### Existing Suite Coverage vs. crt-043 Behavior

| Behavior | Existing Coverage | Gap? |
|----------|------------------|------|
| `context_cycle start/phase_end/stop` accepted | `test_lifecycle.py::test_cycle_lifecycle_full_flow` | No gap — covers tool acceptance |
| Server restart preserves data | `test_lifecycle.py` restart persistence tests | No gap for existing columns |
| Schema v21 restart persistence | None explicitly | New scenario: server must survive restart after v21 migration |
| `goal_embedding` written and non-NULL | None | Gap — not visible through MCP interface |
| `phase` column on observations | None | Gap — not visible through MCP interface |

### New Integration Tests to Write

The goal_embedding and phase columns are internal write-path infrastructure with no MCP-visible retrieval. The integration harness cannot directly assert column presence. However, one new lifecycle-level scenario is valuable:

**New test: `test_lifecycle.py::test_cycle_start_goal_does_not_block_response`**

- Use `server` fixture (fresh DB)
- Call `context_cycle(type=start, session_id=..., goal="design a test pipeline")`
- Assert: response received within 2s wall clock (embedding is fire-and-forget, not blocking)
- Assert: response content matches expected format (AC-06 regression)
- Rationale: the only MCP-visible behavior from the goal embedding path is that the response is not delayed. This validates NFR-01 through the actual MCP interface.

**No new tests** are needed in tools, security, confidence, contradiction, or edge_cases suites — crt-043 does not change any tool's input/output contract, security surface, or scoring logic visible at the MCP layer.

The composite index decision (R-13) has no harness test — it is a delivery-agent written decision.

---

## Non-Negotiable Tests (Must Exist Before Gate 3b)

1. `test_v20_to_v21_both_columns_present` — real v20 fixture via `create_v20_database()`, both columns verified via `pragma_table_info` (R-05, AC-01, AC-07)
2. `test_encode_decode_goal_embedding_round_trip` — exact float equality (R-02, R-11, AC-14)
3. All four `test_phase_captured_*_site` tests — read-back from DB (R-03, AC-09, AC-10)
4. `test_goal_embedding_unavailable_service_warn` — warn captured, NULL blob, not blocked (R-10, AC-04a)
5. `test_no_embed_task_on_empty_goal` and `test_no_embed_task_on_absent_goal` (R-09, AC-04b)
6. `test_v21_migration_idempotent` (R-06, AC-11)

---

## Fixture Database Status

**No v20 fixture `.db` file exists.** The established pattern in this codebase (see `migration_v19_v20.rs`) is to create the prior-version database programmatically using a `create_v{N}_database(path)` async builder function that executes DDL and seeds counters at version N. A `create_v20_database` function must be written as part of `migration_v20_v21.rs`. The v20 schema is fully specified by the v19 DDL (already in `migration_v19_v20.rs`) plus the v19→v20 changes (bidirectional S1/S2/S8 back-fill is data-only, no new columns). The cycle_events table as of v20 has: `id, cycle_id, seq, event_type, phase, outcome, next_phase, timestamp, goal` — no `goal_embedding` column yet.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — attempted; Unimatrix MCP server was disconnected at agent spawn time. Fell back to reading ADR files directly from product/features/crt-043/architecture/. ADR entry IDs #4067, #4068, #4069 referenced by number from IMPLEMENTATION-BRIEF.md.
- Stored: nothing novel to store — the migration fixture builder pattern is already established in `migration_v19_v20.rs` and the fire-and-forget test pattern is documented in existing entries (#735, #771).
