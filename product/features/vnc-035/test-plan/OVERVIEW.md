# Test Plan Overview — vnc-035 `context_correct` Outgoing-Edge Carry-Forward

> Operationalizes RISK-TEST-STRATEGY.md (R-01..R-11) and ACCEPTANCE-MAP.md (AC-01..AC-11
> + 5 design-mandated checks) into per-component test plans. Test infra is **cumulative**
> (NFR-06): extend the existing vnc-017 redirect-loop inline test module in `tools.rs`
> (`mod tests` ~line 9626, imports symbols by path) and the vnc-015 edge fixtures — do NOT
> scaffold isolated harnesses.

## Test Strategy

Three layers, mapped to component boundaries:

| Layer | Where | Covers |
|-------|-------|--------|
| **Store unit** | `unimatrix-store` query module tests (extend `read.rs`-style tests) | `query_outgoing_edges` eligibility predicate (R-03, AC-04 unit half) |
| **Carry-loop unit** | `unimatrix-server/src/mcp/tools.rs` `mod tests` (sibling of `run_redirect_loop` tests) | `run_carry_forward_loop` count contract, warn-and-continue, Contradicts, fault injection (R-01, R-02, R-05, R-08, R-11) |
| **Handler integration** | `tools.rs` `mod tests` end-to-end `context_correct` calls | pipeline order, ack envelope, composition, shed, no-ceiling (R-04, R-07, R-10; AC-01..AC-03, AC-05, AC-08..AC-11) |

The carry-forward loop is a `pub(super)` sibling of `run_redirect_loop`, so its unit tests
import it by path exactly as the redirect tests do:
`use crate::mcp::tools::{run_carry_forward_loop, CarrySummary};`. Mirror the existing
`open_store_and_insert_active` / `insert_edge` helpers — do not duplicate them.

## Risk → Test Mapping (R-01..R-11)

| Risk | Priority | Primary test(s) | Component plan |
|------|----------|-----------------|----------------|
| **R-01** | **Critical** | **`test_carry_forward_continues_on_edge_copy_failure`** (mandatory, by name) + `test_carry_query_err_returns_empty_summary` + `test_correction_committed_before_carry` | run_carry_forward_loop.md |
| **R-02** | High | `test_carry_count_idempotent_repass`, `test_carry_count_keys_off_true_only`, `test_edges_carried_omitted_when_zero`, `test_edges_carried_no_content` | run_carry_forward_loop.md, context_correct_handler.md |
| **R-03** | High | `test_query_outgoing_excludes_derived_classes`, `test_carry_integration_excludes_derived` + single-source SQL grep guard | query_outgoing_edges.md |
| **R-04** | High | `test_pipeline_order_8b_before_8b_prime`, `test_repassed_edge_conflicts_in_8b_prime` | context_correct_handler.md |
| **R-05** | Medium | `test_carry_contradicts_both_directions_exactly_once`, `test_carry_redirect_contradicts_converge`, `test_carry_contradicts_counts_one` | run_carry_forward_loop.md |
| **R-06** | Medium | `test_self_referential_edge_rejected_at_write`, `test_carry_redirect_no_double_process_on_self_loop` | run_carry_forward_loop.md |
| **R-07** | Medium | `test_carried_edge_visible_depth1_immediately`, `test_carried_edge_bfs_path_after_tick` | context_correct_handler.md |
| **R-08** | Medium | `test_carry_loop_owns_write_loop_count_exact` (count test doubles as the guard) | run_carry_forward_loop.md |
| **R-09** | Low | Plan-time finding: `idx_graph_edges_source_id` **exists** (db.rs:969, migration.rs:367). No functional test; note in report. | query_outgoing_edges.md |
| **R-10** | Low | `test_shed_carried_edge_against_new_id`, `test_shed_against_deprecated_original_rejected` | context_correct_handler.md |
| **R-11** | Low | `test_carried_edge_created_at_is_now_not_source` | run_carry_forward_loop.md |

## ⚠️ Mandatory test — verified by name at Gate 3b

**`test_carry_forward_continues_on_edge_copy_failure`** (AC-07 / R-01 / SR-01 / lesson #4473).
Fully specified by name with 4 assertions in **run_carry_forward_loop.md**. Warn-and-continue
on a side-effect failure produces **no behavioral signal** — vnc-017's identical AC was
omitted and FAILed Gate 3b. This test MUST be present by name; Gate 3b checks by name, not
by inferring from passing happy-path tests.

### Fault-injection seam (proven precedent)

vnc-017 already established the seam this test extends: rename `graph_edges` to
`graph_edges_broken`, then `CREATE VIEW graph_edges AS SELECT ... FROM graph_edges_broken`.
SELECTs (`query_outgoing_edges`) succeed via the view; DML (`write_graph_edge`
INSERT) fails because SQLite rejects writes to a plain view. See
`test_redirect_loop_correction_succeeds_when_redirect_fails` (tools.rs:10197) — copy its
structure. **Caveat:** that seam fails *every* write, not one mid-loop. To assert
"edges copied **before** the failure persist" (assertion 3), the loop must be driven so at
least one edge writes successfully before the seam engages — seed ≥2 eligible edges and
either (a) write the first via a non-view path then engage the view, or (b) expose a
counted fault-injection seam in `run_carry_forward_loop` that fails the Nth write only.
The implementation MUST expose whichever seam makes assertion 3 observable (brief constraint).

## Integration Harness Plan

This feature touches **store/retrieval behavior** and **server tool logic** (per the suite
selection table). The Rust unit/integration tests above are the primary gate; the infra-001
Python MCP harness provides black-box confirmation through the JSON-RPC interface.

### infra-001 suites to run (Stage 3c)

| Suite | Why | New tests needed? |
|-------|-----|-------------------|
| `smoke` | Mandatory minimum gate (any change) | No |
| `tools` | `context_correct` is a server tool; carry-forward changes its response envelope (`edges_carried`) | **Yes** — see below |
| `lifecycle` | Correction → re-read is a multi-step flow; carried edges must be visible on the new entry through MCP | **Yes** — see below |
| `protocol` | Response envelope shape changes (new optional field) | No (existing handshake/compliance tests suffice) |

`confidence`, `contradiction`, `security`, `volume`, `edge_cases` are **not** required:
carry-forward introduces no new external input (Security Risks section confirms parameterized
binds, static predicate), no confidence-formula change, and no contradiction-detection change
(the `Contradicts` work is graph-edge bidirectionality, not detection).

### New integration tests to add (Stage 3c)

Behavior visible **only** through the MCP interface (not unit-testable):

1. **`suites/test_tools.py::test_correct_response_includes_edges_carried`** — `store` fixture.
   Store entry A, declare an outgoing edge, `context_correct(A→B)` with `edges` omitted;
   assert the response JSON contains `edges_carried` = expected N. (AC-11a, through MCP.)
2. **`suites/test_tools.py::test_correct_omits_edges_carried_when_zero`** — `store` fixture.
   Correct an entry with no eligible outgoing edges; assert `edges_carried` key **absent**
   from the response envelope (not `0`). (AC-11b, through MCP.)
3. **`suites/test_lifecycle.py::test_correction_carries_outgoing_edges_visible_on_new_entry`**
   — `store` fixture. Store A with outgoing edge to X; correct A→B; query B's edges through
   MCP; assert the carried edge appears on B and not on A. (AC-01/AC-02, through MCP.)

Use the `store` fixture (fresh DB, no leakage) for all three. Naming follows
`test_{tool_or_concept}_{behavior}`. Do **not** add a `confidence`/`security` suite test —
no gap exists there.

### Tick/drain discipline for BFS path-mode (lesson #4526, R-07)

Carried edges are visible to **DB-backed depth-1 reads immediately**, but to **BFS
path-mode/subgraph only after the next graph tick**. Any path-mode assertion (Rust or
infra-001) MUST tick/drain first. The new infra-001 lifecycle test above asserts edge
presence via a **depth-1 DB-backed read** (immediate) — it does NOT use path-mode, so no
tick is required there. The Rust `test_carried_edge_bfs_path_after_tick` is the single
path-mode assertion and explicitly ticks/drains before asserting, with an in-test comment
that pre-tick invisibility is expected (#4526), not a carry-forward defect. Do not add a
path-mode assertion without a preceding tick — that flake gets mis-filed as a carry bug.

## Failure Triage Reminder (Stage 3c)

Per USAGE-PROTOCOL.md: an infra-001 failure **caused by this feature** → fix code, re-run,
document. **Pre-existing/unrelated** → do NOT fix; file a GH Issue and `@pytest.mark.xfail`
with the issue number. **Bad assertion** → fix the test, document. Never fix unrelated
integration failures in this PR.

## AC → Test Plan Index

| AC | Verification | Component plan |
|----|--------------|----------------|
| AC-01 | integration carry-by-default | context_correct_handler.md |
| AC-02 | integration attach-to-new-id | context_correct_handler.md |
| AC-03 | integration Advances→vision_root regression | context_correct_handler.md |
| AC-04 | unit (predicate) + integration (mix) | query_outgoing_edges.md + context_correct_handler.md |
| AC-05 | integration shed via new id + negative deprecated | context_correct_handler.md |
| AC-06 | unit Contradicts bidirectional | run_carry_forward_loop.md |
| **AC-07** | **`test_carry_forward_continues_on_edge_copy_failure`** | **run_carry_forward_loop.md** |
| AC-08 | integration idempotent/additive/changed-target | context_correct_handler.md |
| AC-09 | integration >50 no-ceiling | context_correct_handler.md |
| AC-10 | file-check + grep (docs) | docs_cleanup.md |
| AC-11 | integration ack envelope (+ infra-001 MCP mirror) | context_correct_handler.md |
| Carried metadata (R-11) | unit created_at=now | run_carry_forward_loop.md |
| Pipeline order (R-04) | integration order assertion | context_correct_handler.md |
| Eligibility single-source (R-03) | grep guard | query_outgoing_edges.md |
| `source_id` index (R-09) | plan-time confirmed present | query_outgoing_edges.md |
| Tick staleness (R-07) | depth-1 immediate + path post-tick | context_correct_handler.md |

## Knowledge Stewardship
- Queried: `context_briefing` + `context_search` + `context_get` on ADRs #4983-#4988, lessons
  #4473 (warn-continue failure-path silently omitted — root of R-01), #4526 (tick staleness,
  R-07), patterns #4041 (rows-affected bool, R-02), #4459 (Contradicts source-validation, R-05).
  All applied. Plan-time codebase finding: `idx_graph_edges_source_id` exists (R-09 resolved);
  vnc-017 table-rename-to-view fault-injection seam (tools.rs:10197) reused for AC-07.
- Stored: nothing novel at plan stage — the warn-continue "verify by name" lesson is already
  #4473; the fault-injection seam pattern is captured in the vnc-017 test itself. Any new
  carry-loop-specific test helper discovered in Stage 3c should be stored then via
  /uni-store-pattern.
