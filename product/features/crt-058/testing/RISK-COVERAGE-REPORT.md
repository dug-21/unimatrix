# Risk Coverage Report: crt-058 — Eager Agent-Authored Edge Cleanup at `context_deprecate`

Stage 3c execution. Unit layer (`cargo test -p unimatrix-server`), full-workspace
LINK smoke (#878 guard), integration smoke gate, and the feature-specific MCP
integration tests (the handler-only halves Gate 3b deferred here). All new
assertions are state/parse-based, never call-count or bare-substring (SR-04).

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Subset test blind spot: successor-less fixtures never exercise the real break case (eager on a successor-bearing entry) | Unit `background::tests::test_deprecate_eager_subset_of_tick_and_exactly_agent_edges` (both real fns), `..::test_successor_bearing_edge_repointed_by_tick_but_eager_would_destroy`; **Integration `test_correct_successor_never_invokes_eager_cleanup`** (real `context_correct` → no `edge_cleanup` audit + inbound agent edge survives) | PASS | Full |
| R-02 | Eager/tick predicate drift (widen eager / narrow tick) | Unit subset test invokes BOTH real fns; `edge_write::delete_agent_tests::test_helper_predicate_and_pool_are_locked` (verbatim WHERE/RETURNING pin); fixture-identity assert in subset test | PASS | Full |
| R-03 | Post-commit marshaling error → irreversible delete, no audit | Unit `..::test_delete_returning_is_single_statement_capture` (single atomic `DELETE…RETURNING`/`fetch_all`); `..::test_count_source_of_truth_is_tuples_len_not_rows_affected` | PASS | Full (design-closed: single statement) |
| R-04 | Zero-case rendering (RESOLVED: `Some(0)`→`0`, ADR-004) | Unit `mcp::response::tests` `Some(0)` matrix + `Some(0)`≠`None` discriminator; **Integration `test_deprecate_zero_agent_edges_renders_literal_0`, `test_deprecate_entry_with_no_edges_succeeds`** (Json `edges_removed == 0`, key present) | PASS | Full |
| R-05 | Per-format count drop (one format silently drops the thread) | Unit response per-format matrix (`Some(n)`/`Some(0)`/`None`) + quarantine/restore byte-identity; **Integration** parses the Json `edges_removed` integer (`test_deprecate_reports_edges_removed_count`) | PASS | Full |
| R-06 | Unguarded helper — safety is call-site-only | Unit predicate/pool lock pin; **Integration `test_correct_successor_never_invokes_eager_cleanup`** doubles as the misuse guard (successor path never reaches the helper) | PASS | Full |
| R-07 | Concurrent-tick count under-report / zero-row tolerance | Unit `..::test_delete_agent_edges_empty_match_returns_ok_empty`, `..::test_delete_agent_edges_no_edges_at_all_returns_ok_empty` | PASS | Full |
| R-08 | Double audit-event confusion (flip vs cleanup) | Unit `server::edge_cleanup_audit_tests::test_flip_and_cleanup_are_two_distinct_records`; **Integration** all audit assertions filter `operation == 'context_deprecate.edge_cleanup'` | PASS | Full |
| R-09 | Provenance enumeration drift (`source='agent'`) | Unit `..::test_delete_agent_edges_only_removes_agent_source` (per-source matrix); **Integration** machine (`co_access`) edge survives deprecation | PASS | Full |
| R-10 | Self-loop / high-degree arithmetic | Unit `..::test_self_loop_agent_edge_removed_and_counted_once`, `..::test_high_degree_entry_all_agent_edges_removed`, `edge_cleanup_audit_tests::test_high_degree_audit_metadata_carries_all_tuples` | PASS | Full |
| R-11 | Idempotency / ordering regression | Unit `..::test_shared_edge_removed_by_first_deprecation`; **Integration `test_redeprecate_idempotent_no_second_cleanup_audit`** (2nd call omits advisory, no delete, no 2nd audit), `test_deprecate_removes_agent_edges_and_audits` (synchronous absence on return) | PASS | Full |

## Test Results

### Unit Tests (`cargo test -p unimatrix-server`)
- Total: 4650
- Passed: 4650
- Failed: 0
- crt-058-specific (per Gate 3b, confirmed present + green): 34 — helper DB (10, `edge_write::delete_agent_tests`), audit-emit (7, `server::edge_cleanup_audit_tests`), subset/negative invariant (2, `background::tests`), response formatter matrix + backward-compat, param-position.

### Full-Workspace LINK Smoke (#878 regression guard)
- `product/test/infra-002/check-workspace-link-smoke.sh` → PASS (exit 0). Full-workspace `--no-run` link completed at configured parallelism; #878 link-OOM invariant holds.

### Integration Tests (infra-001 MCP harness, release binary)
- Smoke gate (`pytest -m smoke`, MANDATORY): 28 passed, 0 failed.
- New crt-058 feature tests (7): all PASS.
  - `test_tools.py::test_deprecate_reports_edges_removed_count` (AC-02/04 wire, parsed int)
  - `test_tools.py::test_deprecate_zero_agent_edges_renders_literal_0` (AC-05 wire)
  - `test_lifecycle.py::test_deprecate_removes_agent_edges_and_audits` (AC-01/03/09/11 full chain)
  - `test_lifecycle.py::test_correct_successor_never_invokes_eager_cleanup` (AC-10 chokepoint-exclusion, real `context_correct`)
  - `test_lifecycle.py::test_redeprecate_idempotent_no_second_cleanup_audit` (AC-07 real re-deprecation)
  - `test_lifecycle.py::test_deprecate_eager_failure_is_non_fatal` (AC-06 injected failure via BEFORE DELETE trigger, real handler)
  - `test_edge_cases.py::test_deprecate_entry_with_no_edges_succeeds` (AC-05 edge case)
- Touched-surface regression: `test_tools.py` (deprecate/correct/quarantine/restore/edges_removed) — 32 passed, 1 xfailed (pre-existing GH#405, unrelated), 0 failed. `test_protocol.py` — 13 passed. `test_edge_cases.py` — 24 passed, 1 xfailed (pre-existing, unrelated), 0 failed (25 items, incl. the new no-edge test).

Fixtures: `server` (fresh DB) for all new tests. Agent edges seeded via the
cumulative direct-SQL pattern (test_get_edges `_seed_edges`). Audit + graph_edges
read back via a second SQLite connection (WAL, checkpointed).

## Gaps

None. Every risk R-01…R-11 has a passing behavioral test. Two items warrant a note:

- **AC-06 backstop tail (tick sweeps the residual):** the injected-failure test proves
  the non-fatal contract at the wire (success + `warn`+id + advisory omitted + agent
  edges REMAIN). Demonstrating the *tick* then sweeping them is intentionally NOT a live
  30s tick wait (flaky, > per-test budget); it is proven instead by the Rust
  `test_deprecate_eager_subset_of_tick_and_exactly_agent_edges` invariant (the real
  `run_orphaned_edge_compaction` removes exactly the two agent edges on the non-Active
  endpoint). Split is deliberate and non-flaky.
- **R-03 post-commit atomicity / R-06 single-caller:** unit-level (single-statement
  capture pin; predicate/pool lock). No distinct MCP-visible effect — correctly not
  duplicated at the integration layer (per test-plan OVERVIEW).

## GH Issues / xfail

- No new GH Issues filed — no pre-existing failures surfaced by the crt-058 runs.
- One pre-existing xfail encountered and left untouched: `test_tools.py::test_deprecated_visible_in_search_with_lower_confidence` — `@pytest.mark.xfail(reason="Pre-existing: GH#405 …")`. Unrelated to crt-058 (deprecated-confidence background-scoring timing).
- No integration tests deleted or commented out.

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_deprecate_removes_agent_edges_and_audits` — inbound+outbound agent edges absent synchronously; unit `test_delete_agent_edges_for_entry_removes_inbound_and_outbound_returns_ok` |
| AC-02 | PASS | `test_deprecate_reports_edges_removed_count` — parsed Json `edges_removed == 2` |
| AC-03 | PASS | `test_deprecate_removes_agent_edges_and_audits` — one `edge_cleanup` audit, `target_ids=[E]`, count+`#E` in detail; unit `edge_cleanup_audit_tests::test_edge_cleanup_audit_record_content` |
| AC-04 | PASS | per-source: machine (`co_access`) edge survives, only `agent` removed (unit `test_delete_agent_edges_only_removes_agent_source`); per-format: Json integer parsed |
| AC-05 | PASS | `test_deprecate_zero_agent_edges_renders_literal_0`, `test_deprecate_entry_with_no_edges_succeeds` — `edges_removed` key present == 0; unit `Some(0)` matrix |
| AC-06 | PASS | `test_deprecate_eager_failure_is_non_fatal` — success + entry Deprecated + `warn`("eager edge cleanup failed") w/ id + advisory omitted (None ≠ Some(0)) + edges remain; backstop by subset invariant |
| AC-07 | PASS | `test_redeprecate_idempotent_no_second_cleanup_audit` — 2nd call omits advisory, fresh edge survives, no 2nd `edge_cleanup` audit |
| AC-08 | PASS | Gate 3b: tick (`run_orphaned_edge_compaction`) unchanged, `write_pool_server()` on `graph_edges`, no migration; unit predicate/pool lock pin |
| AC-09 | PASS | `test_deprecate_removes_agent_edges_and_audits` — agent edges queried absent immediately on return (no sleep) |
| AC-10 | PASS | Unit `test_deprecate_eager_subset_of_tick_and_exactly_agent_edges` (R⊆T AND R==2 agent edges, both real fns) + Integration `test_correct_successor_never_invokes_eager_cleanup` (chokepoint-exclusion via real handler) + predicate pin |
| AC-11 | PASS | `test_deprecate_removes_agent_edges_and_audits` — metadata is a 2-tuple JSON array, set-equal to the seeded edges; unit `test_edge_cleanup_audit_metadata_tuple_set_equality`, `..._not_sentinel_on_nonempty` |
