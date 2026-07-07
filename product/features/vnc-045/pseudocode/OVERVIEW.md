# vnc-045 Pseudocode — OVERVIEW: `context_tag` (mechanism only)

> Source of truth: ARCHITECTURE.md (§1 lifecycle, §2 components, §4 integration surface),
> SPECIFICATION.md (FR-01..11), IMPLEMENTATION-BRIEF.md (Function Signatures, Delivery-Critical Items),
> active ADR-001/002/004/008/009. Every interface named here is traced to HEAD; none is invented.
> `protected_tags` (validator, config, min_trust_level, cadence guard) is DEFERRED — NOT designed here.

## Components (build order)

The dependency order is **store → service → handler**, with the audit op-list change independent.

| # | Component | File (HEAD) | Pseudocode | Depends on |
|---|-----------|-------------|-----------|-----------|
| 1 | Direct tag-write primitives | `unimatrix-store/src/write.rs` (new fns beside `:161`) | store-tag-primitive.md | — |
| 2 | Audit op-list update | `unimatrix-store/src/audit.rs:84` | audit-op-list.md | — |
| 3 | `StoreTagService` | `unimatrix-server/src/services/store_tag.rs` (new) | store-tag-service.md | 1, 2 |
| 4 | `context_tag` handler | `unimatrix-server/src/mcp/tools.rs` (router `:611`) | context-tag-handler.md | 3 |

`context_correct`, `update()`, the hash chain, embedding, and edges are untouched (SD-10, SD-3).

## Data flow (one request)

```
MCP client  ──context_tag(id, action, tag, agent_id?, format?)──▶
[HANDLER tools.rs]
  build_context_with_external_identity → ctx.{agent_id, audit_ctx, caller_id}
  require_cap(Capability::Write)                     ── RETROFIT SEAM #2 (gate LOCATION; comment only)
  parse action → TagAction; validate tag non-empty
  derive namespace = prefix-before-first-':' (else None)
  entry_store.get(id) → EntryRecord (status)
  lifecycle guard: Quarantined→refuse; Deprecated→ALLOW
  ── RETROFIT SEAM #1: value-opacity pre-write point (comment only; NO evaluate(tag)) ──
        │  action, tag, namespace, &ctx.audit_ctx, &ctx.caller_id
        ▼
[SERVICE store_tag.rs]  StoreTagService::tag(...)
  gateway.check_write_rate(caller_id)                ── the ONE live throttle (SD-11)
  match action → store.{add_tag | remove_tag | replace_tag}   → prior_value
  build TagAuditMetadata → serialize → AuditEvent    (session_id captured BEFORE spawn)
  tokio::spawn( audit.log_event_async(event) )        ── fire-and-forget, after commit
  return TagResult
        │  TagResult
        ▼
[HANDLER]  format success → CallToolResult
```

Rejections **before** the store write (authz, malformed tag, lifecycle, throttle) leave NO trace —
no tag row, no audit event. The audit event is emitted **only after** the store txn commits.

## Shared types (defined once here; components reference these exact shapes)

### `TagParams` (handler input struct — `mcp/tools.rs`, in the `#[tool_router]` block)

```
#[derive(Debug, Deserialize, JsonSchema)]
struct TagParams {
    // integer entry id; accept int-or-string per the CorrectParams convention
    // (#[serde(deserialize_with = deserialize_i64_or_string)], #[schemars(with="i64")])
    id:       i64,
    action:   String,          // "add" | "remove" | "replace" — first-class client value (ADR-004)
    tag:      String,          // opaque tag text; engine never interprets it (SD-8)
    agent_id: Option<String>,  // self-declared; AUDIT-ONLY, never authz (SD-9, convention #1301)
    format:   Option<String>,  // response format
}
```

### `TagAction` (parsed verb — handler-local enum)

```
enum TagAction { Add, Remove, Replace }
// parse from params.action; anything else → rmcp::ErrorData::invalid_params (no silent default).
// as_str(): Add→"add", Remove→"remove", Replace→"replace" (used verbatim in audit metadata).
```

### `TagResult` (service → handler; audit already emitted inside the service)

```
struct TagResult {
    action:      TagAction,
    tag:         String,
    namespace:   Option<String>,   // derived by handler, passed through
    prior_value: Option<String>,   // evicted/removed value; drives the success message
}
```

### Handler seam functions (unit-callable, NO `RequestContext` — pattern #5389 / #5468)

The `#[tool] context_tag` handler is not unit-constructible, so its two pure decisions are
**extracted as `pub(crate)` fns** in `mcp/tools.rs` (module scope, beside the router block). The
handler body CALLS them; tests exercise them directly. These are the seams the test plan binds
(R-06 namespace boundary table, R-05 lifecycle guard).

```
// Namespace derivation — substring before the FIRST ':' , else None. Positional, value-opaque.
// "delivery:proven"→Some("delivery"); "x-delivery:p"→Some("x-delivery"); "delivery:"→Some("delivery");
// "delivery:a:b"→Some("delivery"); "reviewed"→None.
pub(crate) fn derive_namespace(tag: &str) -> Option<String>

// Lifecycle-guard DECISION — quarantined→refuse (any action); deprecated/active→allow.
// Pure: takes the loaded entry's Status, returns the allow/refuse verdict. The handler maps
// Err → rmcp::ErrorData::invalid_params. Action is NOT consulted (quarantine refuses uniformly).
pub(crate) enum LifecycleRejection { Quarantined }   // carries the refusal reason for the handler's error text
pub(crate) fn check_tag_lifecycle(status: &Status) -> Result<(), LifecycleRejection>
```

> `Status` is `unimatrix_store::Status` (`Active | Deprecated | Quarantined`). Both fns live in
> `mcp/tools.rs` at module scope (directly callable), NOT inside the `#[tool]` fn body.

### Store primitive signatures (unimatrix-store, beside `write.rs:161`)

```
async fn add_tag(&self, entry_id: u64, tag: &str) -> Result<()>
async fn remove_tag(&self, entry_id: u64, tag: &str) -> Result<Option<String>> // Some(tag) if a row was deleted, else None
async fn replace_tag(&self, entry_id: u64, namespace: &str, new_tag: &str) -> Result<Option<String>> // evicted prior, else None
```

> Note vs BRIEF: `remove_tag` returns `Option<String>` (not `()`), so the service can honor
> ADR-009 "`prior_value` mandatory and non-null on every `remove`" from the primitive's own report
> rather than re-deriving it. `prior_value` = the tag the client named (always non-null on a real
> removal). This is an additive refinement of the BRIEF signature, not an invented interface —
> flagged in Open Questions.

### Audit metadata shape (JSON, lives in existing `AuditEvent.metadata: String` — no schema change)

```
// serialize this to the metadata String; on serialize error, WARN + SKIP the event (never "{}").
struct TagAuditMetadata {
    action:      &str,           // "add" | "remove" | "replace" — string form, NOT an integer (#4366)
    namespace:   Option<String>, // prefix before first ':', else null — DERIVED, NEVER validated
    tag:         String,         // full tag written, verbatim
    prior_value: Option<String>, // non-null on remove + replace-with-prior; null on add / no-prior replace
    new_value:   Option<String>, // written value; null on remove
}
```

Per-action metadata (ADR-009 / ARCHITECTURE §4.3):

| action | namespace | prior_value | new_value |
|--------|-----------|-------------|-----------|
| add | derived or null | **null** | tag |
| remove | derived or null | **the removed tag (non-null)** | null |
| replace (namespaced, prior existed) | derived | **evicted prior (non-null)** | tag |
| replace (colon-less OR no prior) → degrades to add | null / derived | null | tag |

Exactly **one** AuditEvent per mutation (a replace is one event carrying prior + new together).

### Enclosing `AuditEvent` (reused verbatim — `schema.rs:360`, no column change)

```
AuditEvent {
    operation:       "context_tag",
    target_ids:      vec![id],
    agent_id:        audit_ctx.caller_id,                       // self-declared
    session_id:      audit_ctx.session_id.clone().unwrap_or_default(),  // CAPTURE BEFORE spawn (#4388/#4389)
    capability_used: Capability::Write.as_audit_str() → "write",
    outcome:         Outcome::Success,                           // stored as u8; read-back reconstructs the variant
    credential_type: "none",                                     // AuditEvent::default() sentinel
    detail:          e.g. "context_tag {action} on #{id}",
    metadata:        <serialized TagAuditMetadata>,
    ..AuditEvent::default()                                      // event_id/timestamp assigned by the sink
}
```

## Sequencing constraints (binding)

- **Store primitives (1) before service (3).** The service calls them by name.
- **replace = ONE transaction** (ADR-004): namespace-scoped `DELETE ... LIKE 'namespace:%'` + `INSERT`
  share one `txn` handle, commit once; a forced INSERT failure rolls the DELETE back (R-02).
- **Namespace derived, never validated** (value-opacity). The handler CALLS the extracted
  `pub(crate) fn derive_namespace(tag)` helper (`prefix before first ':'`) — the logic is NOT inline
  in the `#[tool]` fn — and passes the result to the service; the service passes it to the store's
  `replace_tag` and to audit. R-08: the derived prefix must be LIKE-escaped or rejected if it
  contains `%`/`_` before it reaches the `LIKE 'namespace:%'` DELETE.
- **Lifecycle decision is an extracted seam** — the handler CALLS `pub(crate) fn check_tag_lifecycle(status)`
  (quarantined→refuse, deprecated/active→allow) rather than inlining the `if status == Quarantined`
  check, so the R-05 guard tests run without a `RequestContext`.
- **Throttle in the service, not the handler** — `check_write_rate` is step 0 of `StoreTagService::tag`,
  mirroring `store_correct.rs:29` (a reorder is a seam defect, per RISK Integration Risks).
- **Audit after commit, fire-and-forget** — mirror `store_correct.rs:98-102`; capture `session_id`
  before `tokio::spawn`; read-back tests need a settle delay (~50ms, #4377).

## What is explicitly NOT in these files

No `ProtectedTagsConfig`, no `evaluate(tag)` validator, no `min_trust_level`, no cadence guard,
no config threading, no `Capability::Tag`, no `update()` reuse, no `content_hash`/`previous_hash`
touch, no schema/migration, no learning-column write. The two retrofit seams ship as **comments only**.
