# Agent Report: crt-045-agent-1-pseudocode

## Task

Produce pseudocode for all three crt-045 components:
1. `EvalServiceLayer` (layer.rs changes)
2. `ppr-expander-enabled.toml` (TOML content after fix)
3. `layer_tests.rs` (three-layer integration test + cycle-abort-safety test)

## Output Files

| File | Lines | Status |
|------|-------|--------|
| `product/features/crt-045/pseudocode/OVERVIEW.md` | ~90 | Complete |
| `product/features/crt-045/pseudocode/EvalServiceLayer.md` | ~175 | Complete |
| `product/features/crt-045/pseudocode/ppr-expander-enabled-toml.md` | ~105 | Complete |
| `product/features/crt-045/pseudocode/layer_tests.md` | ~270 | Complete |

## Components Covered

1. **EvalServiceLayer (`layer.rs`)** — Step 5b (rebuild call placement), Step 13b (write-back
   idiom with write-lock swap), new `pub(crate) typed_graph_handle()` accessor delegating to
   `self.inner.typed_graph_handle()`. All constraints verified against architecture.

2. **`ppr-expander-enabled.toml`** — Exact TOML content after fix: `distribution_change =
   false`, `mrr_floor = 0.2651`, `p_at_5_min = 0.1083`, explanatory comment. Delivery agent
   must verify field placement against `EvalProfile` struct deserialization before committing.

3. **`layer_tests.rs`** — Two new tests inside existing `mod layer_tests`:
   - `test_from_profile_typed_graph_rebuilt_after_construction`: three-layer assertion (handle
     state, graph connectivity via `find_terminal_active`, live `search()` call).
   - `test_from_profile_rebuild_error_degrades_gracefully`: cycle-abort-safety test using a
     Supersedes cycle inserted via raw SQL.
   New `seed_graph_snapshot()` helper pseudocoded; reuses existing `make_snapshot_db()` and
   `baseline_profile()` helpers.

## Source Reads Completed

- `product/features/crt-045/IMPLEMENTATION-BRIEF.md`
- `product/features/crt-045/architecture/ARCHITECTURE.md`
- `product/features/crt-045/specification/SPECIFICATION.md`
- `product/features/crt-045/RISK-TEST-STRATEGY.md`
- `architecture/ADR-001` through `ADR-005`
- `crates/unimatrix-server/src/eval/profile/layer.rs` (full)
- `crates/unimatrix-server/src/eval/profile/layer_tests.rs` (full)
- `crates/unimatrix-server/src/services/typed_graph.rs` (full)
- `crates/unimatrix-server/src/services/mod.rs` lines 280–460 (confirmed Arc::clone at :419,
  `typed_graph_handle()` at :297, `AuditContext`/`CallerId`/`AuditSource` definitions)
- `crates/unimatrix-server/src/services/search.rs` lines 256–320 and 548–570
  (`ServiceSearchParams` struct, `search()` signature)
- `crates/unimatrix-server/src/test_support.rs` lines 165–238 (confirmed AuditContext +
  CallerId construction pattern for test search calls)
- `product/research/ass-037/harness/profiles/ppr-expander-enabled.toml` (confirmed broken state)

## Open Questions / Flagged Gaps

1. **`find_terminal_active` visibility** — Used in Test 1 Layer 2. Must confirm it is
   accessible from `unimatrix-server` tests before the delivery agent uses it. If not
   accessible, the pseudocode specifies the fallback: direct `node_count()` / `edge_count()`
   assertions. This is flagged in `layer_tests.md` with the grep command to verify.

2. **`mrr_floor` / `p_at_5_min` field placement in TOML** — The pseudocode places these in
   the `[profile]` section. The delivery agent must verify field names match `EvalProfile`
   deserialization in `eval/profile/types.rs` before committing. A grep command is provided
   in `ppr-expander-enabled-toml.md`.

3. **Cycle detection via raw SQL Supersedes edges** — Test 2 inserts `relation_type='Supersedes'`
   edges via raw SQL to trigger `GraphError::CycleDetected`. If `build_typed_relation_graph()`
   does not model raw `graph_edges` Supersedes rows as cycle-detectable (e.g., if cycle
   detection only applies to the `supersedes` field on `EntryRecord`), the cycle test will
   fail to reach the degraded path. The pseudocode documents this risk and instructs the
   delivery agent to inspect `build_typed_relation_graph()` to verify cycle detection covers
   this path. An alternative (use `supersedes` field on entries) is noted.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_search` (pattern: eval service layer graph state rebuild)
  — found entry #4096 (EvalServiceLayer cold-start must-call rebuild pattern, directly applied),
  #2652 (EvalServiceLayer read-only wrapper pattern, confirms analytics suppression is correct),
  #2673 (VectorIndex load from sibling dir, confirms Step 5 vector path is correct).
- Queried: `mcp__unimatrix__context_search` (category: decision, topic: crt-045) — found
  #4099 (ADR-002 degraded mode), #4102 (ADR-005 distribution_change=false), #4101 (ADR-004
  pub(crate) accessor). All incorporated.
- Queried: `mcp__unimatrix__context_briefing` (task: pseudocode for rebuild/write-back/tests)
  — returned all 5 crt-045 ADRs (#4098, #4099, #4100, #4101, #4102), plus eval harness
  patterns #3610, #2652. All applied.
- Deviations from established patterns: none. The write-back idiom (`*guard = state` inside
  `handle.write().unwrap_or_else(|e| e.into_inner())`) matches the existing pattern in
  `typed_graph.rs:test_typed_graph_state_handle_write_lock_swap` exactly. The test structure
  matches `test_rebuild_excludes_quarantined_entries` in `typed_graph.rs` for store seeding
  and `test_reverse_coaccess_high_id_to_low_id_ppr_regression` for raw SQL edge insertion.
