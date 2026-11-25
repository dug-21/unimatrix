# vnc-050-agent-4-cap-ceiling — C4b Report

## Report
Files modified (crate unimatrix-server/src): server.rs (reserved-id guard first line of build_context; require_cap(&self, ctx: &ToolContext, cap) with ceiling check), mcp/tools.rs (14 require_cap swaps, 5 UsageContext.learning_allowed, query_log gate, 2 confirmed_entries gates, `use cap_ceiling`), mcp/cap_ceiling.rs (dead_code allows removed, test modules wired), mcp/cap_ceiling_http_tests.rs (reusable boot_with/open/call/seed/scalar fixture), http/principal.rs (reject_reserved_agent_id + unit test), services/usage.rs (learning_allowed field + early return; cfg(test) test_hooks, enqueued/flush, in-flight guards at the 4 spawn sites; test literals set true), services/mod.rs.
Files created: mcp/cap_ceiling_deny_tests.rs, mcp/cap_ceiling_learning_tests.rs, services/usage_test_hooks.rs.

Tests: cap_ceiling suite 20/20 pass (step 0, unit, deny table, rw companion, Admin deny, narrow-only, R-20, R-22, empty-ceiling, learning, source inventories). Workspace: unimatrix-server lib 4681 pass / 1 fail (eval::runner::sweep_tests::test_ac14_correlated_sweep_non_vacuous — passes in isolation; flaky under parallel load, unrelated); rc=101.

Sink inventory (tools.rs read tools): record_access (usage/helpful/co_access/feature_entries) — gated in UsageService; query_log insert — gated; session_registry.record_confirmed_entry (lookup single-id, get) — gated (+tested via confirmed_entries). Search service and briefing: no other writes found. Resolve_agent auto-enroll of `anonymous`/new agent_ids in agent_registry still happens before require_cap (pre-existing identity behaviour; not gated).

Deviations / issues:
- require_cap sites: 14, not 15 (re-grep). Admin sites: quarantine, enroll only.
- `empty_ceiling_denies_read_and_write`: the router slug guard returns 500 when the principal is missing, so the tool never sees Parts-without-principal over HTTP. Test asserts fail-closed at HTTP + DB unchanged; tool-level empty ceiling covered by unit test. Strip layer is a local `MapRequestLayer` in the test fixture (http/auth/test_layers.rs not present; not my file).
- search/briefing need the embedding model (not loaded in tests): query_log suppression only covered by the source gate test + placement; rw companion cannot assert query_log growth.
- R-20 `http_access.token_label` assertion omitted (http_access sink owned by router agents); tool audit `human` asserted.
- Tests live in-crate (need cfg(test) seams), not tests/headless_access_http.rs.
- Clippy warnings present in router.rs/slug_guard.rs/handlers.rs (other agents), none in my files.

## Knowledge Stewardship
- Queried: Unimatrix not invoked; used pseudocode/test-plan and ADR files.
- Stored: nothing novel to store -- findings (router 500 precedes tool ceiling; embed-less test server) are feature-specific and recorded here.
