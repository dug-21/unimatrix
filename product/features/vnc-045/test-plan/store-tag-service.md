# Test Plan — `StoreTagService` (`unimatrix-server` services/store_tag.rs, new)

> `StoreTagService::tag(id, action, tag, namespace, audit_ctx, caller_id) -> Result<TagResult>`. Mirrors `store_correct.rs`: order = `check_write_rate` → store write → fire-and-forget audit. Covers R-03 (audit completeness — PRIMARY control), R-05 (lifecycle guards), R-04 (value-opacity), R-07 (throttle), and the R-02×R-03 one-tx→one-event integration seam.
>
> Seam: `make_server()` (server.rs:1323) yields a real `UnimatrixServer` (real store, gateway, audit sink) over a temp DB. Audit read-back via raw `SELECT operation, target_ids, agent_id, capability_used, metadata FROM audit_log WHERE operation='context_tag'` + a **50ms `audit_settle()`** before querying (reuse the `server_edge_cleanup_audit_tests.rs` crt-058 helper — fire-and-forget lands async, #4377). Handler NOT constructed (#5468).

## R-03 — Audit completeness (High — the primary, retrofit-hard control)

For each action, run `StoreTagService::tag`, `audit_settle().await`, then read back the `context_tag` row(s).

1. `test_audit_prior_value_mandatory_on_remove` — after `remove`, `metadata.prior_value` present and **non-null** (= removed tag). (FR-10, AC-04)
2. `test_audit_prior_value_mandatory_on_replace` — after a namespaced `replace` with a prior, `prior_value` = evicted prior, `new_value` = new tag. (FR-10, AC-04)
3. `test_audit_prior_value_null_on_add` — after plain `add`, `prior_value` null/absent, `new_value` carries the tag. (FR-10, AC-04)
4. `test_audit_metadata_never_sentinel` — emitted `metadata` is a well-formed `{action, namespace, tag, prior_value, new_value}` JSON object, **never `"{}"`** (#5468). On a (simulated) serialize error the code warns and SKIPS the event — assert no `"{}"` row emitted, not a corrupt record. (R-03, AC-04)
5. `test_audit_exactly_one_event_per_mutation` — a `replace` emits **exactly one** `operation="context_tag"` row (carrying prior+new together), not one per DELETE/INSERT half; add and remove each emit exactly one. (R-03, AC-04) — cross-links R-02 (one store tx ↔ one audit row).
6. `test_audit_namespace_derived_recorded_never_validated` — `metadata.namespace` = `delivery` for `delivery:x`, `null` for a colon-less tag; recorded verbatim, no rejection on any value. (R-03/R-06, AC-04)
7. `test_audit_outcome_serialized_as_variant_string` — `Outcome`/enum fields read back as variant strings (`"Success"`), NOT integers (`0`) (#4366). Assert on the string form. (R-03)
8. `test_audit_session_id_captured_before_spawn` — `session_id` is the value from `ctx.audit_ctx.session_id`, non-default/non-empty — proving it was captured **before** `tokio::spawn`, not filled by `::default()` after (#4388/#4389). (R-03)
9. `test_audit_field_completeness` — `operation="context_tag"`, `target_ids=[id]`, `agent_id`, `capability_used="write"`, `timestamp` all present. (FR-10)

**Coverage:** every action variant (add, remove, replace, colon-less degrade) has a read-back assertion for full field set + `prior_value` rule + non-`{}` + single-event + variant-string serde; settle handled so tests are deterministic.

## R-05 — Lifecycle guards (Med)

> Guard decision MUST be reachable at a directly-constructible seam. If pseudocode keeps it inline in the `#[tool]` fn it is untestable (#5468) — require extraction into a `pub(crate)` guard fn OR placement in `StoreTagService::tag` (pattern #5389). Tests below assume the reachable seam. **Open question flagged to Stage 3b: exact guard placement.**

1. `test_quarantined_entry_refused_all_actions` — quarantine an entry; assert add, remove, AND replace each refused with a lifecycle `invalid_params`; assert no `entry_tags` write and no audit row. (FR-11, AC-07)
2. `test_deprecated_entry_freeform_tag_allowed` — deprecate an entry; an arbitrary/free-form tag is **ALLOWED** and written (the easy-to-miss over-restriction case — NO "refuse protected tag on deprecated" rule ships). (FR-11, AC-07, SD-12)
3. `test_active_entry_mutations_proceed` — active entry: all valid mutations proceed (guard does not over-fire). (AC-07)

## R-04 — Value-opacity acceptance (Low-Med)

1. `test_value_opaque_acceptance_table` — table over the service seam: (a) `delivery:proven` accepted+written; (b) `delivery:anythingelse` accepted+written (NO rejection path exists); (c) free-form `foo` accepted+written; (d) same `Write` agent throughout — no `TrustLevel`/`agent_id` difference changes the outcome. (FR-07, AC-05) — **Do NOT write a rejection-path test; none ships.**
2. `test_not_conflated_with_validate_outcome_tags` — a tag equal to a reserved outcome key is written as-is; assert `StoreTagService`/`context_tag` does NOT invoke `validate_outcome_tags` (tools.rs:895-898) and its `context_store`-path behavior is unregressed. (R-04)
   > Static/grep proof that no `ProtectedTagsConfig`/`evaluate_protected_tag`/allow-list type shipped lives in context-tag-handler.md (AC-05 static half).

## R-07 — `check_write_rate` throttle (Low-Med)

1. `test_check_write_rate_throttles` — exceed the per-`CallerId` limit (60/3600s, gateway.rs:166) → the op returns `ServiceError::RateLimited`; assert no store write, no audit row past the limit. (FR-08, AC-06a)
2. `test_uds_session_exempt` — a `UdsSession` caller is exempt from the throttle (gateway.rs:60) — mutations proceed past the limit. (AC-06a)
3. `test_throttle_ordering` — `check_write_rate` fires **before** the store write (mirror store_correct.rs:29): a throttled call leaves the tag set unchanged (rejection-before-write leaves no trace). (Service parity, Failure Modes table.)

## Service Wiring Parity (integration seam — R-02×R-03)

1. `test_service_order_gate_throttle_write_audit` — assert the sequence: `check_write_rate` → store primitive → fire-and-forget audit (store_correct.rs:29/100). A reordering (audit before commit, or throttle after write) is a defect. A committed tag with no audit row, or two audit rows for one replace, is a seam bug. (Integration Risks)
2. `test_rejection_before_write_leaves_no_trace` — authz/lifecycle/malformed/throttle rejections leave NO `entry_tags` row and NO audit event; a post-commit audit-drop does NOT roll back the successful tag mutation (best-effort audit posture). (Failure Modes table)
