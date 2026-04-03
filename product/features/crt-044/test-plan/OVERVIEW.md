# Test Plan Overview: crt-044
# Bidirectional S1/S2/S8 Edge Back-fill and graph_expand Security Comment

## Feature Summary

crt-044 makes S1 (tag co-occurrence), S2 (structural vocabulary), and S8 (co-access tick) graph
edges fully bidirectional by: (1) back-filling reverse edges for all existing rows via a v19→v20
schema migration, and (2) updating all three tick functions to write both directions going forward.
A secondary change adds a `// SECURITY:` comment at the `pub fn graph_expand` signature.

Three components map 1:1 to test plan files:

| Component | Test Plan | Verification Method |
|-----------|-----------|---------------------|
| `migration_v19_v20` | `migration_v19_v20.md` | Unit tests (existing migration test pattern) |
| `graph_enrichment_tick_s1_s2_s8` | `graph_enrichment_tick_s1_s2_s8.md` | Unit tests (extend existing tick test file) |
| `graph_expand_security_comment` | `graph_expand_security_comment.md` | Static grep only — no runtime test |

---

## Test Strategy

### Unit Tests (Primary)

All functional verification uses Rust unit/integration tests:

- **Migration tests**: Added to a new `crates/unimatrix-store/tests/migration_v19_v20.rs` file
  following the exact pattern established in `migration_v18_to_v19.rs`. Each test creates a v19-
  shaped SQLite database using a `create_v19_database()` helper, inserts fixture rows directly via
  `sqlx`, opens via `SqlxStore::open()` to trigger migration, then asserts results.

- **Tick tests**: Added to the existing
  `crates/unimatrix-server/src/services/graph_enrichment_tick_tests.rs` file. Tests use the
  established `seed_entry`, `seed_tag`, `fetch_edge`, and `count_edges_by_source` helpers already
  present in that file. Three new per-source bidirectionality tests + one steady-state false-return
  test + one pairs_written counter test.

- **Security comment verification**: Static grep check only. No runtime test. Verified in Stage 3c
  by running `grep '// SECURITY:' crates/unimatrix-engine/src/graph_expand.rs`.

### Integration Tests (infra-001)

See Integration Harness Plan section below.

---

## Risk-to-Test Mapping

| Risk ID | Priority | Description | Test File | Test Function(s) |
|---------|----------|-------------|-----------|------------------|
| R-01 | Critical | Migration Statement A uses wrong `relation_type` | `migration_v19_v20.rs` | `test_v19_to_v20_back_fills_s1_informs_edge`, `test_v19_to_v20_back_fills_s2_informs_edge`, `test_v19_to_v20_back_fills_s8_coaccess_edge` |
| R-02 | Critical | crt-043 ships first, consuming v20 | Pre-merge gate (manual) | Reviewer checks `CURRENT_SCHEMA_VERSION == 19` in base branch before merge |
| R-03 | Critical | One tick function omits second `write_graph_edge` call | `graph_enrichment_tick_tests.rs` | `test_s1_both_directions_written`, `test_s2_both_directions_written`, `test_s8_both_directions_written` |
| R-04 | High | `write_graph_edge` false return mishandled | `graph_enrichment_tick_tests.rs` | `test_s8_false_return_on_existing_reverse_no_warn_no_increment` |
| R-05 | High | `pairs_written` counter stays per-pair | `graph_enrichment_tick_tests.rs` | `test_s8_pairs_written_counter_per_edge_new_pair`, `test_s8_false_return_on_existing_reverse_no_warn_no_increment` |
| R-06 | Med | `co_access` edges accidentally back-filled | `migration_v19_v20.rs` | `test_v19_to_v20_excludes_excluded_sources` |
| R-07 | High | `nli` or `cosine_supports` edges accidentally back-filled | `migration_v19_v20.rs` | `test_v19_to_v20_excludes_excluded_sources` |
| R-08 | Low | Security comment becomes stale | `graph_expand_security_comment.md` | Static grep check (Stage 3c) |
| R-09 | High | Migration block outside transaction boundary | Code review + idempotency tests | `test_v19_to_v20_migration_idempotent_clean_state`, `test_v19_to_v20_migration_idempotent_with_preexisting_reverse` |
| R-10 | High | `CURRENT_SCHEMA_VERSION` not bumped | `migration_v19_v20.rs` | `test_current_schema_version_is_20`, `test_fresh_db_creates_schema_v20` |

---

## Cross-Component Test Dependencies

| Dependency | Nature |
|------------|--------|
| Migration back-fill enables tick steady-state | AC-13 tick test pre-inserts both edge directions to simulate post-migration state — this is the same state the migration produces |
| `write_graph_edge` return contract | Both migration idempotency and tick false-return tests rely on the same `INSERT OR IGNORE` / `UNIQUE` constraint behavior. Entry #4041 documents the three-case return contract that both tests validate |
| `CURRENT_SCHEMA_VERSION` bump | Migration tests that call `SqlxStore::open()` rely on the version constant being correct — R-10 test (`test_current_schema_version_is_20`) must be checked first in code review |

---

## Acceptance Criteria Coverage

| AC-ID | Test File | Test Function |
|-------|-----------|---------------|
| AC-01 | `migration_v19_v20.rs` | `test_v19_to_v20_s1_s2_count_parity_after_migration` |
| AC-02 | `migration_v19_v20.rs` | `test_v19_to_v20_s8_count_parity_after_migration` |
| AC-03 | `graph_enrichment_tick_tests.rs` | `test_s1_both_directions_written` |
| AC-04 | `graph_enrichment_tick_tests.rs` | `test_s2_both_directions_written` |
| AC-05 | `graph_enrichment_tick_tests.rs` | `test_s8_both_directions_written` + `test_s8_pairs_written_counter_per_edge_new_pair` |
| AC-06 | `migration_v19_v20.rs` | `test_current_schema_version_is_20` |
| AC-07 | `migration_v19_v20.rs` | `test_v19_to_v20_migration_idempotent_clean_state` |
| AC-08 | `graph_expand_security_comment.md` | Static grep: `grep '// SECURITY:' graph_expand.rs` |
| AC-09 | `migration_v19_v20.rs` | `test_v19_to_v20_back_fills_s1_informs_edge` + `test_v19_to_v20_back_fills_s8_coaccess_edge` |
| AC-10 | `graph_enrichment_tick_tests.rs` | `test_s1_both_directions_written`, `test_s2_both_directions_written`, `test_s8_both_directions_written` |
| AC-11 | Shell | `cargo test --workspace` exits 0 |
| AC-12 | PR review (manual) + `test_s8_pairs_written_counter_per_edge_new_pair` | Reviewer confirms PR description; test asserts `pairs_written == 2` |
| AC-13 | `graph_enrichment_tick_tests.rs` | `test_s8_false_return_on_existing_reverse_no_warn_no_increment` |
| AC-14 | `migration_v19_v20.rs` | `test_v19_to_v20_migration_idempotent_with_preexisting_reverse` |

---

## Integration Harness Plan (infra-001)

### Which Existing Suites Apply

crt-044 makes changes to schema (storage) and the graph_enrichment tick (internal services, not
MCP tools). The feature has no direct MCP tool interface — it is a background fix.

| Feature area | Suite | Rationale |
|-------------|-------|-----------|
| Schema change (v19→v20 migration) | `lifecycle` | Restart persistence test exercises migration on server open |
| Schema change (v19→v20 migration) | `volume` | Large-scale graph_edges population stress-tests the back-fill |
| Any change at all | `smoke` | **Mandatory gate** — minimum per-feature check |

Suites NOT applicable: `tools`, `protocol`, `security`, `confidence`, `contradiction`, `edge_cases`,
`adaptation` — crt-044 adds no new MCP tools, does not change tool signatures, and does not modify
security or confidence logic visible through the MCP interface.

### Suite Execution Plan (Stage 3c)

```bash
# Mandatory gate
python -m pytest suites/ -v -m smoke --timeout=60

# Schema/lifecycle suites
python -m pytest suites/test_lifecycle.py -v --timeout=60
```

The `volume` suite is optional — run if time permits, but the unit-level migration tests provide
the source-scoped back-fill correctness guarantees that the volume suite cannot provide at the
granularity needed (source field filtering is invisible through MCP).

### New Integration Tests Needed

No new integration tests are needed in infra-001 for crt-044.

Rationale:
- The critical correctness properties (per-source edge bidirectionality, idempotency, exclusion of
  nli/cosine_supports, pairs_written counter semantics) are all validated by direct SQL queries
  against the SQLite database in unit tests. These properties are invisible through the MCP JSON-RPC
  interface — there is no MCP tool that returns raw `GRAPH_EDGES` row counts or source field values.
- The smoke and lifecycle suites verify that the server starts up, migration runs without error, and
  the system remains operational — which is all the infra-001 harness can reasonably verify for this
  type of change.
- The `// SECURITY:` comment change has zero behavioral effect and requires no integration test.

If the crt-042 eval gate (`ppr_expander_enabled`) were toggled on in this feature, a `lifecycle` or
`tools` integration test validating graph_expand returns both directions would be warranted. That
gate remains off — crt-042 delivery team's post-eval decision.

---

## Fixture Strategy

### Migration Tests

New file `crates/unimatrix-store/tests/migration_v19_v20.rs` with a `create_v19_database()` helper
modeled on `create_v18_database()` in `migration_v18_to_v19.rs`. The v19 database shape is
identical to v18 shape plus the `cycle_review_index` table already added by v17→v18.

The v19 database helper must seed `schema_version = 19` in the counters table.

### Tick Tests

Extend `graph_enrichment_tick_tests.rs` using existing helpers:
- `seed_entry(store, id, status)` — entry seeding
- `seed_tag(store, entry_id, tag)` — tag seeding
- `fetch_edge(store, source_id, target_id, relation_type)` — direct GRAPH_EDGES query
- `count_edges_by_source(store, source)` — edge count by source
- `seed_audit_row` — for S8 co-access signal seeding

New helper needed: `count_edges_by_source_and_direction(store, source_id, target_id, source_field)`
may be useful but `fetch_edge` already covers this via `is_some()` check.

The `make_config_s8()` helper already exists for S8 config.

---

*Authored by crt-044-agent-2-testplan (claude-sonnet-4-6). Written 2026-04-03.*
