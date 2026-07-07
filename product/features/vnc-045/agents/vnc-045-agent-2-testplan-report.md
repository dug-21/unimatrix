# Agent Report — vnc-045-agent-2-testplan (Stage 3a: Test Plan Design)

## Deliverables
- test-plan/OVERVIEW.md — strategy, seam table, R-01..R-08 mapping, integration harness plan
- test-plan/store-tag-primitive.md — R-01 invariance, R-02 atomicity/rollback/degrade, R-08 injection/LIKE, edges
- test-plan/audit-op-list.md — R-07 `audit_write_count_since` op-list inclusion
- test-plan/store-tag-service.md — R-03 audit completeness, R-05 lifecycle, R-04 value-opacity, R-07 throttle, wiring parity
- test-plan/context-tag-handler.md — R-06 namespace derivation (extracted pure fn), R-04 static/no-validator proof; NO handler unit tests (#5468)

## Key seam decisions
- Handler non-constructible (#5468) → proofs at store-primitive / `StoreTagService` (`make_server()`) / `audit_log` read-back seams; route/format → infra-001. Mirror crt-058 `server_edge_cleanup_audit_tests.rs` (`audit_settle()` 50ms + raw SELECT).
- Hard requirement flagged to Stage 3b: `derive_namespace` and the lifecycle-guard decision MUST be extracted as `pub(crate)` fns (pattern #5389) or they are untestable.
- No rejection-path / validator / `min_trust_level` / config-type tests (ADR-003/005/006/007 DEFERRED).

## Risk coverage
All 8 risks mapped; 3 High (R-01/R-02/R-03) comprehensive. R-03 audit completeness proven only at unit read-back seam (audit_log not MCP-exposed).

## Open questions
1. Lifecycle-guard placement — extracted `pub(crate)` fn vs `StoreTagService`-resident. Either is testable; inline-in-`#[tool]` is not. Decide in pseudocode.
2. R-08 reject-vs-LIKE-escape for `%`/`_` in derived namespace — implementer's choice; test asserts siblings survive under the chosen behavior.
3. Edge: duplicate-add / absent-remove defined behavior (idempotent vs error) — pin in pseudocode; test asserts whichever is chosen.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search (category=decision, topic=vnc-045) — surfaced #5610/#5608/#5609 (ADR-009/004/008), #5389 (extract `#[tool]` logic into `pub(crate)` seam fns — the workaround this plan rests on), #1369 (MCP 6-step pipeline), #1301 (params need `agent_id`). Confirmed via code: crt-058 audit read-back precedent, `make_server()` (server.rs:1323), write.rs:78/161 primitives, store_correct.rs:29/100 order.
- Stored: nothing novel — the non-constructible-handler → seam-fn + audit read-back pattern is already #5389 + crt-058. Retro may promote a reusable `entry_tags` rollback-injection fixture if Stage 3c produces one.
