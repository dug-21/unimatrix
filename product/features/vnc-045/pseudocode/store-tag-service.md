# Component: `StoreTagService`

**Crate/file:** `unimatrix-server/src/services/store_tag.rs` (NEW). Mirrors `services/store_correct.rs`.
**ADRs:** ADR-004 (replace routing), ADR-008 (audit primary control), ADR-009 (audit shape).
**Risks:** R-02 (replace routing), R-03 (audit completeness/timing), R-05 (lifecycle relayed), R-07 (throttle).

## Purpose

Orchestration seam between the handler and the store. Sequences: `check_write_rate` (the one live
throttle) → the correct store primitive per action → fire-and-forget audit after commit. This is the
directly-constructible seam where orchestration + audit are tested (the `#[tool]` handler is NOT
unit-constructible, #5468).

## Struct + construction (mirror `StoreService`)

```
#[derive(Clone)]
pub(crate) struct StoreTagService {
    store:   Arc<Store>,          // unimatrix-store SqlxStore — provides add_tag/remove_tag/replace_tag
    gateway: Arc<SecurityGateway>,// provides check_write_rate
    audit:   Arc<AuditLog>,       // provides log_event_async (fire-and-forget)
}

impl StoreTagService {
    pub(crate) fn new(store, gateway, audit) -> Self { StoreTagService { store, gateway, audit } }
}
```

Wire into `ServiceLayer` (`services/mod.rs:241` struct + `:517`-style construction in
`ServiceLayer::new`): add field `store_tag: StoreTagService`, construct with
`StoreTagService::new(Arc::clone(&store), Arc::clone(&gateway), Arc::clone(&audit))`, add to the
returned struct literal (`:559`). Reuses the same `Arc`s already built for `store_ops`.

## Result type

```
pub(crate) struct TagResult {
    pub action:      TagAction,        // Add | Remove | Replace (defined in OVERVIEW.md)
    pub tag:         String,
    pub namespace:   Option<String>,
    pub prior_value: Option<String>,
}
```

`TagAction` is shared between handler and service — define it in this module (or a small shared
module) and `pub(crate)` it so the handler can construct it from the parsed action string.

## Main function

```
pub(crate) async fn tag(
    &self,
    id:        u64,
    action:    TagAction,
    tag:       String,
    namespace: Option<String>,   // derived + LIKE-validated by the handler; None = colon-less
    audit_ctx: &AuditContext,
    caller_id: &CallerId,
) -> Result<TagResult, ServiceError>:

    // Step 0: live throttle — BEFORE any work (mirror store_correct.rs:29). UdsSession exempt
    //         is handled inside check_write_rate. On exceed → ServiceError::RateLimited (no write).
    self.gateway.check_write_rate(caller_id)?

    // NOTE: NO value-hygiene here. No allow-list, no evaluate(tag). Value-opacity (SD-8, R-04).
    //       Lifecycle guard already applied by the handler (it holds the loaded EntryRecord).

    // Step 1: dispatch to the store primitive → prior_value.
    let (prior_value, new_value) = match action:
        Add:
            self.store.add_tag(id, &tag).await.map_err(map_store_err)?
            (None, Some(tag.clone()))
        Remove:
            let removed = self.store.remove_tag(id, &tag).await.map_err(map_store_err)?
            // ADR-009: prior_value MANDATORY, non-null on remove — the client named the exact tag.
            // Use the client's tag as prior_value regardless of whether a row existed (intent-of-record).
            (Some(tag.clone()), None)
        Replace:
            match namespace:
                Some(ns):   // ns is non-empty + LIKE-safe (handler-guaranteed)
                    let prior = self.store.replace_tag(id, &ns, &tag).await.map_err(map_store_err)?
                    (prior, Some(tag.clone()))        // prior non-null iff a prior existed
                None:       // colon-less / null-namespace → DEGRADE TO ADD (ADR-004 edge case)
                    self.store.add_tag(id, &tag).await.map_err(map_store_err)?
                    (None, Some(tag.clone()))         // pure insert; prior_value:null; NEVER hard-errors

    // Step 2: build the audit event — session_id captured HERE, before spawn (#4388/#4389).
    let session_id = audit_ctx.session_id.clone().unwrap_or_default()
    let metadata_str = match build_tag_metadata(action.as_str(), &namespace, &tag, &prior_value, &new_value):
        Ok(s)  => s
        Err(e) => { tracing::warn!(?e, "context_tag audit metadata serialize failed; SKIPPING event");
                    // R-03 / #5468: do NOT emit "{}" — the mutation already succeeded; accept the
                    // rare audit gap over a corrupt record. Return success without spawning audit.
                    return Ok(TagResult { action, tag, namespace, prior_value }) }

    let event = AuditEvent {
        event_id: 0, timestamp: 0,                    // assigned by the sink
        session_id,
        agent_id: audit_ctx.caller_id.clone(),
        operation: "context_tag".to_string(),
        target_ids: vec![id],
        outcome: Outcome::Success,
        detail: format!("context_tag {} on #{id}", action.as_str()),
        capability_used: "write".to_string(),          // == Capability::Write.as_audit_str()
        metadata: metadata_str,
        ..AuditEvent::default()                         // credential_type "none", agent_attribution ""
    }

    // Step 3: fire-and-forget after commit (mirror store_correct.rs:98-102). ONE event per mutation.
    {
        let audit = Arc::clone(&self.audit)
        tokio::spawn(async move { let _ = audit.log_event_async(event).await; })
    }

    Ok(TagResult { action, tag, namespace, prior_value })
```

### `build_tag_metadata` (serialize the TagAuditMetadata → String)

```
fn build_tag_metadata(action: &str, namespace: &Option<String>, tag: &str,
                      prior_value: &Option<String>, new_value: &Option<String>)
                      -> Result<String, serde_json::Error>:
    // A serde struct or serde_json::json!({...}) — either is fine. action is a STRING (not int, #4366).
    // null vs absent: emit explicit JSON null for None (forensic clarity), not key omission.
    serde_json::to_string(&json!({
        "action":      action,
        "namespace":   namespace,      // Option<String> → string or null
        "tag":         tag,
        "prior_value": prior_value,    // Option<String> → string or null
        "new_value":   new_value,      // Option<String> → string or null
    }))
```

### `map_store_err` (StoreError → ServiceError)

```
fn map_store_err(e: StoreError) -> ServiceError:
    match e:
        StoreError::EntryNotFound(id) => ServiceError::Core(CoreError::Store(StoreError::EntryNotFound(id)))
        other                         => ServiceError::Core(CoreError::Store(other))
```

## Data flow

- **In:** `id`, `action` (verb), `tag` (opaque), `namespace` (derived/None), `&audit_ctx`, `&caller_id`.
- **Out:** `TagResult` for handler formatting; one `AuditEvent` spawned to the sink.
- **Transformations:** action + store return → `(prior_value, new_value)` → metadata JSON → AuditEvent.

## Error handling & ordering (binding)

| Origin | Error | Note |
|--------|-------|------|
| Rate exceeded (step 0) | `ServiceError::RateLimited` | BEFORE write — no tag row, no audit (RISK Failure Modes) |
| Store write failure | `ServiceError::Core(CoreError::Store(_))` | replace rolls back atomically (R-02) |
| Metadata serialize error | none returned | WARN + SKIP audit; mutation still succeeded (never "{}") |
| Audit spawn drop | none returned | best-effort async; matches `store_correct` posture (Residual #2) |

Ordering is load-bearing: **throttle → store write → (after commit) audit**. Reordering (audit before
commit, throttle after write) is a seam defect (RISK Integration Risks).

## Key test scenarios (hints — service seam over `make_server()`, + audit read-back)

1. **Value-opacity (R-04/AC-05):** table — `delivery:proven`, `delivery:anythingelse`, free-form `foo`
   all return `Ok` under the same `Write` caller; no rejection path exists; no `TrustLevel` consulted.
2. **Replace routing (R-02):** namespaced replace evicts prior and returns it in `TagResult.prior_value`;
   colon-less replace (`namespace=None`) degrades to add, `prior_value=None`, removes nothing.
3. **Audit completeness (R-03/AC-04):** after add/remove/replace/colon-less-replace, read back the
   `audit_log` (with a ~50ms settle, #4377); assert `operation="context_tag"`, `target_ids=[id]`,
   `capability_used="write"`, and `metadata` = full `{action,namespace,tag,prior_value,new_value}`;
   `prior_value` non-null on remove and namespaced-replace-with-prior; **exactly one** event per
   mutation (one for a replace); metadata is a real object, never `"{}"`.
4. **session_id present (R-03/#4389):** audit event carries the `audit_ctx.session_id`, not a default.
5. **Throttle (R-07/AC-06a):** exceed the per-caller `check_write_rate` limit → `RateLimited`, no write,
   no audit; `UdsSession` caller exempt.
6. **op-list count (R-07/AC-06b):** after N mutations, `audit_write_count_since(agent, 0)` reflects them.
7. **Serialize-error skip (R-03):** with a forced metadata serialize failure, assert the mutation
   still succeeded and NO `"{}"` audit row was written.
