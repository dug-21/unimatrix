# Risk Coverage Report: vnc-042

`context_get` resolves superseded entries to their active terminal by default
(`follow_supersessions: Option<bool>`, handler-owned default = follow). Behavior LOCKED (GH #843).

**Stage 3c verdict: PASS.** All unit tests green; mandatory smoke gate green; all feature-relevant
integration suites green (0 failed). All 6 new vnc-042 integration tests pass. One blast-radius test
migration applied and FLAGGED (see §Blast Radius). No GH Issues filed — no pre-existing failures
discovered.

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Notice inside `format_single_entry` breaks byte-identity | `test_none_json_byte_identical_to_base_object` (canary, unchanged) · `test_with_note_stripped_equals_base_formatter` · ~15 shape tests · int `test_get_clean_passthrough_no_resolution_key` | PASS | Full |
| R-02 | serde-default footgun (silent default-OFF) | `test_get_params_follow_supersessions_{absent,true,false,no_quoted_scalar_coercion}` · **behavioral** `test_get_handler_field_absent_resolves_to_terminal` · int `test_get_default_resolves_deprecated_to_terminal` (field omitted through MCP) | PASS | Full |
| R-03 | `include_edges` keyed on requested vs resolved id | `test_get_handler_resolved_edges_keyed_on_terminal` · `test_get_handler_include_edges_false_skips_assembly` · int `test_get_resolved_edges_keyed_on_terminal` | PASS | Full |
| R-04 | Dead-end returns empty/silent; store-error swallowed | `test_get_handler_deadend_{orphaned,quarantined,over_50_hops,cycle,store_error}_*` · int `test_get_deadend_returns_requested_id_loud_flag` | PASS | Full |
| R-05 | Wrong `follow_to_current` copy / build breakage | BLD-01 build+clippy green (Gate 3b) · BLD-02 canonical call-site `crate::mcp::graph_read::follow_to_current` (tools.rs:528) — no `graph_read_supersession::`/`handle_current` call | PASS | Full |
| R-06 | `_with_note` drifts from base formatter shape | `test_with_note_body_matches_base_across_formats` · strip-and-compare across 3 formats | PASS | Full |
| R-07 | json `resolution` object leaks on clean passthrough | `test_json_clean_passthrough_has_no_resolution_key` + 3 non-clean presence tests · int `test_get_clean_passthrough_no_resolution_key` | PASS | Full |
| R-08 | Footer on NULL-`superseded_by` deprecated | `test_note_asstored_null_successor_wellformed_footer` (no panic, no `#null`/`#{}`) | PASS | Full |
| R-09 | Default-flip on non-code callers | `CONTEXT_GET_DESCRIPTION` documents follow default + `follow_supersessions=false` escape hatch (tools.rs:59) · tool-desc unit assert | PASS (proxy) | Partial — **behavioral coverage for non-code consumers impossible by design; FLAGGED for human** |
| R-10 | graph-vs-get naming/default divergence | Documented ADR-001; review-time awareness | ACCEPTED | None (not gated) |
| R-11 | JS `GetParams` schema parity | JS CI matrix (incl. Windows) only — additive field; **budget one post-PR CI round-trip** | DEFERRED | None locally (CI-only) |
| R-12 | Unresolved edge targets (NG-1) | Legible via resolution notice; not gated | ACCEPTED | None (not gated) |

## Test Results

### Unit Tests (`cargo test -p unimatrix-server`)
- **Library:** 4325 passed, 0 failed, 1 ignored.
- **All server test binaries (integration bins + doctests, run with serial link):** all `test result: ok`, 0 failed. `WS_TEST_RC=0`.
- **vnc-042-specific unit tests:** 32 (19 handler/params in `tools.rs`, 13 formatter in `response/entries.rs`) — all green within the library run.
- Note: an initial parallel-link run was OOM-killed (`ld terminated with signal 9`) on the separate
  integration-test binaries — a resource artifact, not a code defect. Re-running with
  `CARGO_BUILD_JOBS=1` (serial link) yields all-green.

### Integration Tests (infra-001 harness, over MCP JSON-RPC)
- **Smoke gate (`-m smoke`) — MANDATORY: 26 passed, 0 failed. PASS.**

Full feature-relevant suites (per OVERVIEW §4.1 suite selection):

| Suite | Passed | xfailed | xpassed | failed |
|-------|--------|---------|---------|--------|
| `test_tools.py` (5 chunks) | 195 | 1 | 0 | 0 |
| `test_protocol.py` | 13 | 0 | 0 | 0 |
| `test_get_edges.py` | 18 | 0 | 0 | 0 |
| `test_edge_cases.py` | 23 | 1 | 0 | 0 |
| `test_lifecycle.py` (2 chunks) | 80 | 6 | 1 | 0 |
| **Feature-suite total** | **329** | **8** | **1** | **0** |

Non-feature-touched suites (`confidence`, `contradiction`, `security`, `volume`, `adaptation`) were
run at smoke level per OVERVIEW §4.1 and verified free of deprecated-id `context_get` read-backs
(the only two `context_correct`+`context_get` pairings read the terminal / invalid ids — clean
passthrough, unaffected).

**New vnc-042 integration tests (OVERVIEW §4.3) — all 6 present and PASS:**
- `test_get_default_resolves_deprecated_to_terminal` (test_lifecycle.py, smoke) — AC-01/AC-06
- `test_get_clean_passthrough_no_resolution_key` (test_lifecycle.py, smoke) — AC-02/R-07
- `test_get_follow_false_returns_as_stored_with_footer` (test_lifecycle.py) — AC-03
- `test_get_deadend_returns_requested_id_loud_flag` (test_lifecycle.py, admin_server) — AC-04
- `test_get_resolved_edges_keyed_on_terminal` (test_get_edges.py) — R-03/AC-07
- `test_get_follow_supersessions_orthogonal_matrix` (test_tools.py) — AC-07

**xfail / xpass accounting (all pre-existing, none vnc-042-related, no GH Issue owed):**
- 8 xfailed: pre-existing `@pytest.mark.xfail` markers already in the suites (known-bug / CI-environment).
- 1 xpassed: `test_inferred_edge_count_unchanged_by_cosine_supports` — a pre-existing *environmental*
  xfail ("no embedding model in CI"); the embedding model IS available in this local env, so it passes.
  Not a vnc-042 signal; marker removal is a CI-environment decision, out of scope.

## Blast Radius — FLAG event (SR-02 / #5099)

**One harness test encoded the OLD `context_get` default and was migrated (not silently narrowed):**

- `suites/test_lifecycle.py::test_correct_leaves_supersedes_edges_unchanged` (vnc-017 AC-10).
  Its precondition read `context_get(id_s)` on a *deprecated* id expecting as-stored deprecated
  content (`status == "deprecated"`, `superseded_by == A`). Under the vnc-042 locked default this
  now resolves S→terminal A and returns `status == "active"` — assertion failed (`'active' == 'deprecated'`).
- **Cause:** vnc-042-induced (the intended contract change), not a code bug. Triage → the test's
  expectation became wrong under the locked contract (USAGE-PROTOCOL "test itself wrong" branch).
- **Fix applied (in-PR, intent-preserving):** added `follow_supersessions=False` to that single
  precondition read so it inspects S's OWN as-stored provenance. Test intent (Supersedes-edge
  exclusion) is unchanged; no assertion weakened. Re-run: 37 passed, 3 xfailed, 1 xpassed, 0 failed.
- This is precisely the SR-02 blast-radius hit the Risk Strategy anticipated ("IF one encoding the
  OLD default is found at delivery, migrating it is development work to be FLAGGED"). **FLAGGED here.**

## GH Issues Filed
None. No pre-existing/unrelated integration failures were discovered. The single failure was
feature-induced and fixed in-PR as a legitimate blast-radius test migration.

## Gaps
- **R-09 (accepted):** behavioral coverage for non-code durable-id consumers (memory files, edges,
  prior-session ids) is **impossible by design** — outside any harness. Covered only by the
  tool-description proxy (BLD-04). **Flagged for human** (LOCKED product bet, #843).
- **R-11 (CI-only):** JS edge-client `GetParams` parity for the additive field surfaces only in the
  JS CI matrix (incl. Windows), not in Linux-only local gates. **Budget one post-PR CI round-trip.**
- **R-10, R-12 (accepted):** documented, not gated — no coverage owed.

All gated risks (R-01..R-08) have Full coverage. No uncovered gated risk.

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | int `test_get_default_resolves_deprecated_to_terminal` (id==B, B's content) + unit `test_get_handler_default_deprecated_resolves_to_terminal` |
| AC-02 | PASS | hop notice `↻ … version #B` in `test_get_default_resolves…`; no-hop → `test_get_clean_passthrough_no_resolution_key` (no notice, no `resolution` key) |
| AC-03 | PASS | int `test_get_follow_false_returns_as_stored_with_footer` (id==A, deprecated, footer names #B) + unit `test_note_asstored_with_successor_appends_footer` |
| AC-04 | PASS | int `test_get_deadend_returns_requested_id_loud_flag` (non-empty, id==requested, `no_active_successor`) + unit dead-end suite (orphaned/quarantined/>50-hop/cycle/store-error) |
| AC-05 | PASS | grep: handler calls `crate::mcp::graph_read::follow_to_current` (tools.rs:528, via `resolve_effective_id`); no new CTE/walk; `graph_read_supersession::`/`handle_current` NOT called |
| AC-06 | PASS | **behavioral** unit `test_get_handler_field_absent_resolves_to_terminal` + int `test_get_default_resolves…` (field OMITTED through MCP ⇒ resolves) |
| AC-07 | PASS | int `test_get_follow_supersessions_orthogonal_matrix` (format × include_edges all resolve to B) + `test_get_resolved_edges_keyed_on_terminal` (edges keyed on `effective_id`) |
| AC-08 | PASS | unit `test_note_asstored_null_successor_wellformed_footer` (`superseded_by=NULL` ⇒ `deprecated; no recorded successor.`, no panic, no `#null`) |

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — surfaced #5058 (Stage-3c lastfailed-cache tests are
  not live failures — applied when triaging xfails), #5389 (behavioral unit tests for rmcp `#[tool]`
  handlers), #5388 (ADR-001 divergence). No results contradicted the plan.
- Stored: nothing novel — the blast-radius FLAG procedure (#5099) and store-layer-false-positive
  partitioning (#5383) already exist and governed this run; the `follow_supersessions=False`
  precondition-read migration is a direct application of the existing pattern, not a new one.
