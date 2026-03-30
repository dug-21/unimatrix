# Test Plan Overview: crt-035 — Bidirectional CoAccess Edges + Bootstrap-Era Back-fill

## Overall Test Strategy

crt-035 is a precision SQL change with a well-defined blast radius. No new public API surfaces
are added and no MCP tool behavior changes. The test strategy is therefore:

1. **Unit tests** (dominant tier) — the feature's correctness is directly testable at the
   SQL/Rust function level. The tick, migration, and PPR integration path each have dedicated
   test files that prove their contract without requiring the full MCP stack.
2. **Integration smoke gate** (mandatory minimum) — confirms the compiled binary still
   starts and the MCP handshake succeeds after the schema bump to v19.
3. **No new infra-001 suite tests required** — all affected behavior is unit-testable. The
   existing `lifecycle` and `tools` suites exercise graph retrieval at the MCP level; they
   are run as smoke validation but require no new tests.

---

## Risk-to-Test Mapping

| Risk ID | Priority | Test(s) | File(s) |
|---------|----------|---------|---------|
| R-02 | Critical | GATE-3B-01 grep + T-BLR-08 update | tick.md |
| R-08 | Critical | GATE-3B-02 grep + all T-BLR count assertions | tick.md |
| R-01 | High | GATE-3B-03 EXPLAIN QUERY PLAN + MIG-U-03/04 multi-row | migration.md |
| R-03 | High | OQ-01 closed in spec (count=2 confirmed) + T-BLR-08 | tick.md |
| R-07 | High | GATE-3B-04 SqlxStore grep + AC-12 test structure | ac12-test.md |
| R-04 | Med | MIG-U weight=0.0 sub-case inside MIG-U-03 or MIG-U-04 | migration.md |
| R-05 | Med | T-NEW-02 convergence test (both stale directions updated) | tick.md |
| R-06 | Med | Coverage gap note — reverse insert assertion in `test_existing_edge_current_weight_no_update` | tick.md |
| R-09 | Med | MIG-U-06 idempotency (success re-run path) | migration.md |
| R-10 | Low | MIG-U-01 CURRENT_SCHEMA_VERSION == 19 | migration.md |

---

## Gate-3b Checklist

These four checks are non-negotiable before Stage 3b delivery is accepted.

### GATE-3B-01: "no duplicate" grep (R-02 — Critical)

```bash
grep -n '"no duplicate"' crates/unimatrix-server/src/services/co_access_promotion_tick_tests.rs
```

Must return zero matches. Any match means T-BLR-08 was not updated.

### GATE-3B-02: Odd count_co_access_edges grep (R-08 — Critical)

```bash
grep -n 'count_co_access_edges\|assert_eq!(count' \
  crates/unimatrix-server/src/services/co_access_promotion_tick_tests.rs
```

All numeric assertion values must be even (0, 2, 4, 6, 10...). Any odd value (1, 3, 5...)
indicates a missed blast-radius test update.

### GATE-3B-03: EXPLAIN QUERY PLAN on back-fill NOT EXISTS sub-join (R-01 — High)

Run against a tempfile SqlxStore with the v19 schema. Expected inner plan:
`SEARCH graph_edges rev USING INDEX sqlite_autoindex_graph_edges_1`

If the plan shows `SCAN graph_edges rev`, a composite index must be added to the migration
DDL before delivery. Document the output as a comment in `tests/migration_v18_to_v19.rs`.

### GATE-3B-04: SqlxStore in AC-12 test (R-07 — High)

```bash
grep -n 'SqlxStore\|open_test_store' \
  crates/unimatrix-server/src/services/typed_graph.rs | grep test_ppr_reverse
```

Must confirm the test opens a real store, not a bare `TypedRelationGraph::new()`.

---

## Component Test Plan Files

| Component | Test Plan File | Test Count |
|-----------|---------------|------------|
| `co_access_promotion_tick.rs` + `_tests.rs` | tick.md | 8 updates + 3 new = 11 changes |
| `migration.rs` + `migration_v18_to_v19.rs` | migration.md | 7 MIG-U cases |
| `typed_graph.rs` (AC-12) | ac12-test.md | 1 new tokio test |

---

## Cross-Component Test Dependencies

- The AC-12 test (`typed_graph.rs`) depends on `GRAPH_EDGES` being writable via the
  `SqlxStore` write pool — not on the tick or migration code directly. It is independent
  of the other components.
- MIG-U tests open a `SqlxStore` which triggers `run_main_migrations` — the same code
  path used in production. The v18 builder must match the exact schema produced by v17→v18
  migration (including all indexes, especially `UNIQUE(source_id, target_id, relation_type)`).
- GATE-3B-03 (EXPLAIN QUERY PLAN) can be run in the migration test file itself as a
  test case, or executed manually by the delivery agent and documented as a comment.

---

## Integration Harness Plan (infra-001)

### Feature-to-suite applicability

| Feature change | Suite(s) to run |
|---------------|----------------|
| Schema version bump 18→19 | smoke (mandatory), lifecycle, tools |
| No new MCP tools or parameters | protocol not required beyond smoke |
| No confidence/contradiction changes | confidence, contradiction not required |
| No security surface changes | security not required |

### Suites to execute in Stage 3c

1. **Smoke** (`-m smoke`) — mandatory gate. Covers binary startup, MCP handshake, tool
   discovery. Confirms the v19 schema migration does not break server open.
2. **lifecycle** — multi-step flows include store→search which exercises `TypedGraphState`
   rebuild. Confirms bidirectional edges do not break restart persistence or search results.
3. **tools** — exercises all 12 tools through JSON-RPC. Confirms no regressions in
   context_search, context_briefing, or context_status after the schema bump.

### No new infra-001 integration tests needed

All crt-035 behavior that requires verification is:
- SQL-level (tick writes, migration back-fill) — fully covered by unit tests.
- Data-only (no new tool parameters, no new MCP responses) — existing tool suite covers.
- `TypedGraphState` + PPR path — covered by AC-12 unit test using real SqlxStore.

The PPR scoring improvement (reverse edges now visible) is not directly assertable through
the MCP search interface without knowing which entries are in the graph; unit tests provide
deterministic coverage that integration tests cannot. No new infra-001 suite additions
are planned for crt-035.

---

## Schema Version Maintenance Note

The CURRENT_SCHEMA_VERSION bump from 18 to 19 requires updating one existing test in
`crates/unimatrix-server/src/services` that hardcodes the current schema version
(Pattern #2937). The delivery agent must search for version-number assertions and update
them. This is not a new test but a maintenance task on existing tests.
