# Agent Report: crt-058-agent-6-tester (Stage 3c — Test Execution)

## Outcome
All gates green. Unit + link smoke + integration smoke + feature integration tests pass.

## Executed
- **Unit** (`cargo test -p unimatrix-server`, hardened form): 4650 passed, 0 failed. 34 crt-058 tests present + green (helper 10, audit-emit 7, subset/negative 2, formatter matrix + backward-compat).
- **LINK smoke** (`infra-002/check-workspace-link-smoke.sh`, #878 guard): PASS (exit 0).
- **Integration smoke** (`pytest -m smoke`, MANDATORY gate): 28 passed, 0 failed.
- **New crt-058 integration tests (7): all PASS** — added to `test_tools.py` (2), `test_lifecycle.py` (4), `test_edge_cases.py` (1).
- **Touched-surface regression**: `test_tools.py` deprecate/correct/quarantine/restore = 32 passed + 1 pre-existing xfail (GH#405); `test_protocol.py` 13 passed; `test_edge_cases.py` 24 passed + 1 xfail.

## New integration tests (the Gate-3b handler-only deferrals + wire ACs)
Driven through the REAL `#[tool]` handlers over MCP JSON-RPC (not unit-constructible):
1. `test_correct_successor_never_invokes_eager_cleanup` — AC-10 chokepoint-exclusion via real `context_correct`: no `context_deprecate.edge_cleanup` audit for the original; the inbound agent edge survives (vnc-017 repoints it to the successor — NOT eagerly deleted).
2. `test_deprecate_eager_failure_is_non_fatal` — AC-06 injected failure via a `BEFORE DELETE … RAISE(ABORT)` trigger on `graph_edges WHEN OLD.source='agent'` (no server fault-injection seam exists; the trigger forces the eager `DELETE…RETURNING` to Err). Asserts: deprecation success, entry Deprecated, `warn`("eager edge cleanup failed") carrying the id, advisory OMITTED (None, distinct from `Some(0)`), agent edges remain, no cleanup audit.
3. `test_redeprecate_idempotent_no_second_cleanup_audit` — AC-07 via real re-deprecation: 2nd call omits advisory, fresh edge survives, no 2nd cleanup audit.
4. `test_deprecate_removes_agent_edges_and_audits` — AC-01/03/09/11 full chain: synchronous both-direction removal, machine edge survives, one `edge_cleanup` audit with 2-tuple metadata set-equal to the seed.
5. `test_deprecate_reports_edges_removed_count` — AC-02/04 wire: parsed Json integer `edges_removed == 2`.
6. `test_deprecate_zero_agent_edges_renders_literal_0` + `test_deprecate_entry_with_no_edges_succeeds` — AC-05: `edges_removed` key present == 0.

Wire assertions parse the structured Json field / read `graph_edges` + `audit_log` via a second SQLite connection — never substring/call-count (SR-04).

## One fix during execution (not a product bug)
`test_correct_successor_never_invokes_eager_cleanup` initially asserted the inbound edge still pointed at the original. The vnc-017 auto-redirect repoints it to the successor synchronously (`redirected=1`). Relaxed the survival assertion to "exactly one agent edge from the source persists" — the crt-058 guarantee is only that the eager helper never destroyed it; the redirect target is vnc-017's concern. Chokepoint proof (no `edge_cleanup` audit) held throughout.

## Deliverables
- `product/features/crt-058/testing/RISK-COVERAGE-REPORT.md`
- Integration tests appended to `product/test/infra-001/suites/{test_tools,test_lifecycle,test_edge_cases}.py`

## GH Issues
None filed — no pre-existing failures surfaced. No tests deleted/commented. The one xfail encountered (GH#405) is pre-existing and left as-is.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — surfaced #3806/#3386 (implementors omit handler-specific + edge-case integration tests → Gate 3b/3c FAIL), #5460 (ADR-003 subset invariant), #2758 (Gate 3c must grep every non-negotiable test name). Applied: implemented all deferred handler-only halves rather than deferring further.
- Stored: entry #5470 "Force a non-unit-constructible handler's swallowed-failure path via a SQLite BEFORE DELETE trigger" via context_store (topic: testing, category: pattern) — reusable technique for driving a Rust non-fatal DB-error branch through the pure MCP wire when no server fault-injection seam exists.
