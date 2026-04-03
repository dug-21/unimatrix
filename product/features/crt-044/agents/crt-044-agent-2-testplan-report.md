# Agent Report: crt-044-agent-2-testplan
# Stage 3a — Test Plan Design

## Output Files

- `/workspaces/unimatrix/product/features/crt-044/test-plan/OVERVIEW.md`
- `/workspaces/unimatrix/product/features/crt-044/test-plan/migration_v19_v20.md`
- `/workspaces/unimatrix/product/features/crt-044/test-plan/graph_enrichment_tick_s1_s2_s8.md`
- `/workspaces/unimatrix/product/features/crt-044/test-plan/graph_expand_security_comment.md`

---

## Risk Coverage Summary

| Risk ID | Priority | Coverage |
|---------|----------|----------|
| R-01 | Critical | Full — 3 per-source migration tests (MIG-V20-U-03/04/05) |
| R-02 | Critical | Pre-merge gate (manual) — untestable in unit tests |
| R-03 | Critical | Full — 3 per-source tick tests (TICK-S1/S2/S8-U-10) |
| R-04 | High | Full — TICK-S8-U-12 |
| R-05 | High | Full — TICK-S8-U-11 + TICK-S8-U-12 |
| R-06 | Med | Full — MIG-V20-U-08 (exclusion test) |
| R-07 | High | Full — MIG-V20-U-08 (exclusion test) |
| R-08 | Low | Static grep only — accepted per ADR-003 |
| R-09 | High | Indirect — MIG-V20-U-09, MIG-V20-U-10 + code review |
| R-10 | High | Full — MIG-V20-U-01 (constant), MIG-V20-U-02 (fresh DB) |

---

## Test Inventory

### New test file: `crates/unimatrix-store/tests/migration_v19_v20.rs`
11 tests (1 non-async, 10 `#[tokio::test]`)

| Function | AC | Risks |
|----------|----|-------|
| `test_current_schema_version_is_20` | AC-06 | R-10 |
| `test_fresh_db_creates_schema_v20` | — | R-10 |
| `test_v19_to_v20_back_fills_s1_informs_edge` | AC-09 | R-01 |
| `test_v19_to_v20_back_fills_s2_informs_edge` | AC-09 | R-01 |
| `test_v19_to_v20_back_fills_s8_coaccess_edge` | AC-09 | R-01 |
| `test_v19_to_v20_s1_s2_count_parity_after_migration` | AC-01 | — |
| `test_v19_to_v20_s8_count_parity_after_migration` | AC-02 | — |
| `test_v19_to_v20_excludes_excluded_sources` | AC-09 | R-06, R-07 |
| `test_v19_to_v20_migration_idempotent_clean_state` | AC-07 | R-09 |
| `test_v19_to_v20_migration_idempotent_with_preexisting_reverse` | AC-14 | R-09 |
| `test_v19_to_v20_empty_graph_edges_is_noop` | — | edge case |

### Extension to: `crates/unimatrix-server/src/services/graph_enrichment_tick_tests.rs`
5 tests (all `#[tokio::test]`)

| Function | AC | Risks |
|----------|----|-------|
| `test_s1_both_directions_written` | AC-03, AC-10 | R-03 |
| `test_s2_both_directions_written` | AC-04, AC-10 | R-03 |
| `test_s8_both_directions_written` | AC-05, AC-10 | R-03 |
| `test_s8_pairs_written_counter_per_edge_new_pair` | AC-05, AC-12 | R-05 |
| `test_s8_false_return_on_existing_reverse_no_warn_no_increment` | AC-13 | R-04 |

### Static check: `crates/unimatrix-engine/src/graph_expand.rs`
1 grep check (Stage 3c) — AC-08.

---

## Integration Harness Plan

- **Mandatory gate**: `pytest -m smoke --timeout=60`
- **Recommended suite**: `pytest suites/test_lifecycle.py --timeout=60` (restart persistence validates migration runs on server open)
- **No new infra-001 tests needed**: all correctness properties (source field, row counts, idempotency, exclusion) are invisible through MCP interface and validated by direct SQL in unit tests.

---

## AC Verification Summary

All 14 AC-IDs from ACCEPTANCE-MAP.md have test coverage:

- AC-01 through AC-07, AC-09, AC-14: migration unit tests
- AC-03 through AC-05, AC-10, AC-12, AC-13: tick unit tests
- AC-08: static grep
- AC-11: `cargo test --workspace` exit 0 (Stage 3c gate)
- AC-12: also requires PR description reviewer confirmation (manual)

---

## Open Questions

None. All source documents are fully resolved (SPECIFICATION.md §Open Questions: OQ-1, OQ-2, OQ-3 all resolved).

One implementation note for Stage 3b: the `create_v19_database()` helper in `migration_v19_v20.rs`
must include the `cycle_review_index` table (added by v17→v18 migration) — it is part of the v19
schema shape. Copying `create_v18_database` from `migration_v18_to_v19.rs` and updating the
`schema_version` seed to 19 is the correct approach.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — 12 results. Entries #4078 (S8 gap pattern), #4080 (ADR-002 two-call tick pattern), #4066 (BFS test pairing), #4079 (ADR-001 migration strategy), #4081 (ADR-003 security comment) directly applicable.
- Queried: `context_search("crt-044 architectural decisions")` — retrieved all three crt-044 ADRs (#4079, #4080, #4081).
- Queried: `context_search("graph edge migration bidirectionality testing patterns")` — entries #4078, #4066, #4080 confirmed.
- Stored: entry #4082 "Per-source back-fill migration tests must be independent: one test per source value, not one combined count test" via `/uni-store-pattern` — novel pattern applicable to any future source-scoped back-fill migration, not crt-044-specific.

---

*Agent: crt-044-agent-2-testplan (claude-sonnet-4-6). Written 2026-04-03.*
