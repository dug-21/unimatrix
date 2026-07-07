# vnc-045 Test Plan — OVERVIEW (`context_tag`, mechanism only)

> Rooted in RISK-TEST-STRATEGY.md (8 risks, 0 Critical, 3 High). Scope REDUCED — `protected_tags` DEFERRED. No test may require a validator, allow-list, config type, `min_trust_level`, or cadence guard (ADR-003/005/006/007 DEFERRED — VOIDED-BY-DEFERRAL).

## Test-Seam Constraint (#5468 — binding, shapes the whole plan)

The `context_tag` `#[tool]` handler is **NOT unit-constructible** (no `RequestContext<RoleServer>` constructor in unit scope). Orchestration + audit proofs land at directly-constructible seams; end-to-end route/format proofs land in Stage-3c integration.

| Seam | What is proven | Constructibility |
|------|----------------|------------------|
| Store primitive (`add_tag`/`remove_tag`/`replace_tag`) | atomicity, rollback, invariance, LIKE-escape, edge cases | `SqlxStore::open` over temp DB — direct |
| `StoreTagService::tag` | orchestration order, throttle, lifecycle guard, value-opacity, one-tx→one-event | `make_server()` (server.rs:1323) — direct |
| `audit_log` read-back | complete metadata shape, `prior_value` rule, non-`{}`, single-event, serde form | raw `SELECT ... FROM audit_log` + 50ms settle — direct |
| Extracted pure helpers (namespace derivation; lifecycle-guard decision) | R-06 boundary table, guard decision | free `pub(crate)` fns per pattern #5389 — direct |
| Integration route (MCP) | tool discovery, Write-gate rejection, action roundtrip, read-freshness, quarantine refusal, rate limit | infra-001 harness over the binary |

**Precedent to mirror:** `server_edge_cleanup_audit_tests.rs` (crt-058) — helper + real audit emit + `audit_log` read-back with `audit_settle()` (50ms). Same non-constructible-handler situation. Reuse its `audit_settle()` and raw-SELECT read-back shape. **Do NOT instantiate the `#[tool]` fn.**

## Test Strategy Layers

- **Unit (store crate)** — `add_tag`/`remove_tag`/`replace_tag` transactionality, rollback, invariance, injection/LIKE-escape, idempotency edges.
- **Unit/seam (server crate over `make_server()`)** — `StoreTagService::tag` orchestration + `audit_log` read-back for the full audit contract; extracted namespace-derivation + lifecycle-guard helpers.
- **Integration (infra-001)** — route/format: tool registered, Write enforced, add/remove/replace visible to next read (read-freshness), quarantine refused, throttle fires.

## Risk → Test Mapping

| Risk | Pri | Primary seam | Test-plan file | Key scenarios |
|------|-----|--------------|----------------|----------------|
| R-01 invariance + read-freshness | High | store primitive + integration | store-tag-primitive.md, (read-freshness → OVERVIEW integration) | 5 learning cols + hash chain + edges + id byte-identical pre/post for add/remove/replace; no supersession; tag-filtered read reflects add→present/remove→absent |
| R-02 replace atomicity + colon-less degrade | High | store primitive | store-tag-primitive.md | one-tx DELETE+INSERT; inject mid-tx INSERT failure → prior survives, no zero-tag window; colon-less→add; no-prior→pure insert |
| R-03 audit completeness (retrofit-hard) | High | `audit_log` read-back | store-tag-service.md | full field set; `prior_value` mandatory on remove/replace, null on add; never `"{}"`; exactly one event/mutation; namespace derived-never-validated; `Outcome` variant-string; `session_id` before `spawn`; 50ms settle |
| R-04 value-opacity | Low-Med | service seam + grep | store-tag-service.md, context-tag-handler.md | `delivery:proven`/`delivery:anythingelse`/`foo` ALL accepted (NO rejection-path test); no `ProtectedTagsConfig`/validator shipped; `validate_outcome_tags` not invoked |
| R-05 lifecycle guards | Med | service seam / extracted guard | store-tag-service.md | quarantined→refuse (all 3 actions); deprecated→ALLOW free-form; active→proceed |
| R-06 namespace derivation | Med | pure helper table | context-tag-handler.md | first-colon positional split; colon-terminated; colon-less→null; multi-colon; mid-string colon; empty→invalid_params |
| R-07 live-control wiring | Low-Med | service seam + audit read-back | store-tag-service.md, audit-op-list.md | `check_write_rate` throttles (RateLimited, UdsSession-exempt); `'context_tag'` counted by `audit_write_count_since` |
| R-08 injection / over-broad DELETE | Low-Med | store primitive | store-tag-primitive.md | metachar `tag` stored/matched literally (bound params); derived namespace with `%`/`_` rejected-or-LIKE-escaped → no sibling over-match |

## Cross-Component Dependencies

- **R-02 × R-03 (one-tx → one-event):** one `replace_tag` transaction MUST correspond to exactly one `context_tag` audit row. Proven where the service wraps the primitive: store-tag-service.md asserts a replace yields exactly one audit event; store-tag-primitive.md asserts the primitive is one transaction. A mismatch (two rows, or committed tag with no audit) is an integration-seam defect.
- **Service parity with `store_correct`:** order MUST be gate(handler) → `check_write_rate`(service, store_correct.rs:29) → store write → fire-and-forget audit (store_correct.rs:100). Reordering (audit-before-commit, throttle-after-write) is a defect — asserted in store-tag-service.md.
- **Handler → helper extraction:** namespace derivation and the lifecycle-guard decision MUST be extracted as `pub(crate)` fns (pattern #5389) so they are reachable without a `RequestContext`. If pseudocode leaves them inline in the `#[tool]` fn, they are untestable — flagged as a hard requirement to Stage 3b.

## Integration Harness Plan (infra-001)

`context_tag` is a NEW MCP tool (12 → 13 tools). Suites that apply (per selection table — feature touches server tool logic, store/retrieval, security caps, storage persistence):

| Suite | Applies because | New tests to add |
|-------|-----------------|------------------|
| `protocol` | tool discovery / JSON-RPC compliance | `test_context_tag_in_tool_list` (tool advertised, schema has `id`/`action`/`tag`/`agent_id`/`format`) |
| `tools` | new tool, every param & response format | `test_context_tag_add_roundtrip`; `test_context_tag_remove`; `test_context_tag_replace_single_value` (one `delivery:*` remains); `test_context_tag_replace_colon_less_degrades_to_add`; `test_context_tag_invalid_action_rejected`; `test_context_tag_empty_tag_rejected`; `test_context_tag_value_opaque_freeform_accepted` |
| `lifecycle` | add→read / remove→read read-freshness; deprecated-allow; restart persistence | `test_context_tag_add_then_search_reflects` (present); `test_context_tag_remove_then_search_absent` (read-freshness, NFR-04); `test_context_tag_deprecated_entry_allowed`; `test_context_tag_persists_across_restart` |
| `security` | Write-cap enforcement, quarantine refusal, injection | `test_context_tag_requires_write_capability` (no-Write agent rejected); `test_context_tag_quarantined_entry_refused`; `test_context_tag_sql_metachar_tag_stored_literally` (R-08 — no injection, no sibling over-match observable via by-tag search) |
| `smoke` | MANDATORY minimum gate | mark `test_context_tag_add_roundtrip` + `test_context_tag_requires_write_capability` `@pytest.mark.smoke` |

**Gap the harness cannot cover:** audit-record completeness (`prior_value`, non-`{}`, single-event, serde form) — `audit_log` is not exposed through an MCP tool, so R-03 is proven at the unit read-back seam (store-tag-service.md), NOT in infra-001. Integration confirms only that the route accepts the call and the mutation is read-back-visible.

**Fixtures:** `server` (default fresh DB) for tools/roundtrip; `admin_server` for quarantine setup; `populated_server` unnecessary. Extend existing suites — do NOT add a new suite file (test infra is cumulative; a new suite would be an infra-enhancement GH Issue, not this PR).

## Knowledge Stewardship
- Queried: `context_briefing` + `context_search` (category=decision, topic=vnc-045) — surfaced #5610/#5608/#5609 (ADR-009/004/008 audit shape, atomic replace, authz posture), #5389 (extract `#[tool]` decision logic into `pub(crate)` seam fns — the non-constructibility workaround this plan is built on), #1369 (MCP 6-step handler pipeline), #1301 (params struct must include `agent_id`). Applied as cited. Confirmed via code: `server_edge_cleanup_audit_tests.rs` `audit_settle()`/read-back precedent; `make_server()` server.rs:1323; write.rs:78/161 entry_tags primitives; store_correct.rs:29/100 order.
- Stored: nothing novel at plan time — the reusable pattern (non-constructible-handler → seam-fn + audit read-back) is already #5389 + crt-058. If Stage 3c surfaces a reusable `entry_tags`-mutation or rollback-injection test fixture, the retro promotes it.
