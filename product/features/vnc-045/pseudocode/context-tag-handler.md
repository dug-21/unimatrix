# Component: `context_tag` handler

**Crate/file:** `unimatrix-server/src/mcp/tools.rs` — new `#[tool(name="context_tag")]` fn +
`TagParams` struct inside the existing `#[rmcp::tool_router]` block (`:611`).
**Template:** `context_correct` handler (`tools.rs:1226-1426`).
**FRs:** FR-01, FR-02, FR-07 (seam), FR-11 (lifecycle). **Risks:** R-05 (lifecycle), R-06 (parse/namespace), R-08 (LIKE-safety).

## Purpose

MCP entry point. Resolves identity, gates on `Capability::Write`, parses `action`, validates `tag`,
derives + LIKE-validates the namespace, applies the lifecycle guard against the loaded entry, marks
the two retrofit seams as **comments only**, then delegates to `StoreTagService::tag`. Contains NO
value interpretation and NO validator. Not unit-constructible (needs a live `RequestContext`, #5468) —
behavior is proven at the service/store/audit seams + Stage-3c route tests.

## `TagParams` struct (in the router block; see OVERVIEW.md for the canonical shape)

```
#[derive(Debug, Deserialize, JsonSchema)]
pub struct TagParams {
    #[serde(deserialize_with = "crate::mcp::serde_util::deserialize_i64_or_string")]
    #[schemars(with = "i64")]
    pub id: i64,                    // integer entry id (int-or-string tolerant, mirrors CorrectParams)
    pub action: String,            // "add" | "remove" | "replace"
    pub tag: String,               // opaque tag text
    pub agent_id: Option<String>,  // self-declared; AUDIT-ONLY (convention #1301 — must exist)
    pub format: Option<String>,    // response format
}
```

## Handler body

```
#[tool(name = "context_tag",
       description = "Add, remove, or replace a single volatile tag on an entry in place. \
                      Preserves the entry's content hash, edges, embedding, and learning vector \
                      (unlike context_correct). Gated on Capability::Write.")]
async fn context_tag(
    &self,
    Parameters(params): Parameters<TagParams>,
    request_context: RequestContext<RoleServer>,
) -> Result<CallToolResult, rmcp::ErrorData>:

    // 1. Identity + format + audit context (reuse verbatim — tools.rs:1236).
    let ctx = self.build_context_with_external_identity(
        &params.agent_id, &params.format, &None, &request_context, None).await?

    // 2. Capability gate (reuse verbatim — tools.rs:1245). This LOCATION is RETROFIT SEAM #2.
    //    ── RETROFIT SEAM #2 (comment only): a future enterprise trust-elevation check
    //       (min_trust_level) attaches HERE, where the principal is already resolved.
    //       vnc-045 wires NOTHING beyond Write. agent_id is audit-only, never authz (SD-9).
    self.require_cap(&ctx.agent_id, Capability::Write).await?

    // 3. Parse action → TagAction (no silent default).
    let action = match params.action.as_str():
        "add"     => TagAction::Add
        "remove"  => TagAction::Remove
        "replace" => TagAction::Replace
        other     => return Err(rmcp::ErrorData::invalid_params(
                         format!("unknown action '{other}'; expected add|remove|replace"), None))

    // 4. Validate tag (malformed → invalid_params BEFORE any write; no audit, no partial state).
    let tag = params.tag
    if tag.trim().is_empty():
        return Err(rmcp::ErrorData::invalid_params("tag must not be empty".into(), None))
    //  (no length/vocabulary/shape check beyond non-empty — value-opacity, SD-8, R-04.)

    // 5. Derive namespace via the EXTRACTED pub(crate) seam fn (NOT inline — pattern #5389/#5468).
    //    derive_namespace is module-scope in tools.rs so the R-06 boundary table tests it directly
    //    without a RequestContext. Semantics (substring before FIRST ':' , else None):
    //    "delivery:proven"→Some("delivery"); "delivery:proven:extra"→Some("delivery"); (first colon)
    //    "x-delivery:proven"→Some("x-delivery"); (positional, NOT "delivery")
    //    "delivery:"→Some("delivery"); (empty value; stored as-is, value-opaque)  "reviewed"→None.
    let namespace: Option<String> = derive_namespace(&tag)

    // 5b. R-08: a derived namespace used in the replace `LIKE 'namespace:%'` DELETE must be LIKE-safe.
    //     The store escapes it (store-tag-primitive.md like_escape + ESCAPE clause). This handler
    //     does NOT need to reject %/_ — but if the chosen strategy is REJECT (not escape), enforce
    //     it here for action==Replace with a Some(namespace) containing '%' or '_':
    //         return invalid_params("namespace contains LIKE metacharacters"). (Pick ONE strategy —
    //         Open Question in OVERVIEW.) Recommended: escape in store, no reject here.

    // 6. Load the entry (lifecycle status + existence). Reuse entry_store.get (read.rs:163).
    let entry = self.entry_store.get(params.id as u64).await
        .map_err(|e| rmcp::ErrorData::from(ServerError::Core(CoreError::Store(e))))?
        // EntryNotFound → mapped error (no write, no audit).

    // 7. Lifecycle guard — RE-IMPLEMENTED in-op (NOT inherited from write_ext.rs:471-482) (FR-11, R-05).
    //    The DECISION is the EXTRACTED pub(crate) seam fn check_tag_lifecycle(status) (module-scope,
    //    unit-callable — R-05 tests run without a RequestContext). Quarantined → refuse ANY action;
    //    Deprecated → ALLOW (free-form; no protected-tag rule ships); Active → allow.
    match check_tag_lifecycle(&entry.status):
        Ok(()) => {}                                    // Deprecated + Active fall through → allowed
        Err(LifecycleRejection::Quarantined) =>
            return Err(rmcp::ErrorData::invalid_params(
                format!("entry {} is quarantined; cannot tag", params.id), None))

    // ── RETROFIT SEAM #1: VALUE-OPACITY PRE-WRITE INTERCEPTION POINT (comment only) ──
    //    A future protected_tags policy drops exactly one call here:  evaluate(tag) -> Allowed|Rejected.
    //    vnc-045 ships NO validator, NO stub, NO config, NO call — a marked seam ONLY. The tag is
    //    written uninterpreted. Do NOT invoke validate_outcome_tags (tools.rs:895-898) — unrelated.

    // 8. Delegate: throttle + store write + fire-and-forget audit all happen in the service.
    let result = self.services.store_tag.tag(
        params.id as u64, action, tag, namespace,
        &ctx.audit_ctx, &ctx.caller_id,
    ).await.map_err(rmcp::ErrorData::from)?     // ServiceError → ErrorData (RateLimited, Core, etc.)

    // 9. Format success response.
    Ok(format_tag_success(&result, ctx.format))
```

## Extracted seam functions (module scope in `mcp/tools.rs`, `pub(crate)`, unit-callable)

Both are pure and take no `RequestContext`, so the R-06 boundary table and R-05 guard tests
exercise them directly (pattern #5389; the #5468 trap is that inline logic in the `#[tool]` fn is
unreachable in unit scope). Declared at module scope beside the router block — NOT inside the
`#[tool]` fn body.

### `derive_namespace(tag) -> Option<String>`

```
pub(crate) fn derive_namespace(tag: &str) -> Option<String>:
    match tag.find(':'):                 // FIRST colon (positional, not vocabulary-aware)
        Some(i) => Some(tag[..i].to_string())
        None    => None
```

### `check_tag_lifecycle(status) -> Result<(), LifecycleRejection>`

```
pub(crate) enum LifecycleRejection { Quarantined }

pub(crate) fn check_tag_lifecycle(status: &Status) -> Result<(), LifecycleRejection>:
    match status:
        Status::Quarantined => Err(LifecycleRejection::Quarantined)   // refuse ANY action (FR-11)
        _                   => Ok(())                                  // Deprecated + Active → allow
```

- Action is intentionally NOT a parameter: quarantine refuses add/remove/replace uniformly, and
  deprecated allows all (no protected-tag rule ships). Keeping the decision status-only makes the
  R-05 table exhaustive over the three `Status` values.
- The handler (Step 7) maps `Err(Quarantined)` → `rmcp::ErrorData::invalid_params`. Keeping the
  rejection type transport-agnostic lets the same seam be reused by any future non-MCP caller.

## `format_tag_success` (new formatter helper — mirror `format_correct_success` shape)

```
fn format_tag_success(result: &TagResult, format: FormatKind) -> CallToolResult:
    // One text line acknowledging the mutation. Keep minimal; no edge/redirect machinery.
    // e.g. add:     "Added tag 'reviewed' to #<id>."
    //      remove:  "Removed tag 'delivery:proven' from #<id>."
    //      replace: "Replaced 'delivery:partial' with 'delivery:proven' on #<id>."  (prior_value in msg)
    //      replace-degrade / no-prior: "Added tag 'delivery:proven' to #<id>."
    // honor `format` (summary/markdown/json) consistent with other tool formatters.
```

## Data flow

- **In:** `TagParams` (over MCP) + `RequestContext`.
- **Derived:** `TagAction`, `namespace: Option<String>`, loaded `EntryRecord`.
- **Out:** delegates to `StoreTagService::tag`; formats `TagResult` → `CallToolResult`.
- Handler performs NO SQL and NO audit directly — those are the service/store's job.

## Error boundaries (all leave NO trace — before the store write)

| Condition | Surfaced as |
|-----------|-------------|
| Missing `Capability::Write` | `require_cap` error (Denied) |
| Unknown `action` | `rmcp::ErrorData::invalid_params` |
| Empty/whitespace `tag` | `rmcp::ErrorData::invalid_params` |
| Entry not found | `ServerError::Core(CoreError::Store(EntryNotFound))` |
| Quarantined entry (any action) | `rmcp::ErrorData::invalid_params` (lifecycle) |
| (from service) rate exceeded | `ServiceError::RateLimited` → `ErrorData` |
| (from service) store failure | `ServiceError::Core(...)` → `ErrorData` |

Tagging a **Deprecated** entry is NOT an error (FR-11, SD-12). There is NO hygiene/allow-list error
path (value-opaque). `namespace` is derived and passed on — never validated for vocabulary.

## Key test scenarios (hints — mostly Stage-3c integration/route seam, #5468)

1. **Registered + gated (AC-01):** tool is registered/callable; an agent lacking `Write` is rejected
   at the route seam (handler not unit-constructible — assert via `make_server()` route).
2. **Action parse (R-06):** `add`/`remove`/`replace` accepted; unknown action → `invalid_params`.
3. **Namespace derivation table (R-06):** call the extracted `derive_namespace(tag)` seam fn directly
   (no `RequestContext`): `delivery:proven`→`Some("delivery")`; `delivery:proven:extra`→`Some("delivery")`;
   `x-delivery:proven`→`Some("x-delivery")`; `delivery:`→`Some("delivery")`; `reviewed`→`None`. The
   empty/whitespace-tag `invalid_params` case is a handler-body check (Step 4), proven at the route seam.
4. **Lifecycle (R-05/AC-07):** call `check_tag_lifecycle(status)` directly over the three `Status`
   values — `Quarantined`→`Err`, `Deprecated`→`Ok`, `Active`→`Ok`; the handler's `Err`→`invalid_params`
   mapping and deprecated-write-through are confirmed at the route/service seam.
5. **Value-opacity end-to-end (R-04/AC-05):** `delivery:anythingelse` and `foo` both succeed on `Write`;
   route emits the same audit shape; `validate_outcome_tags` NOT invoked.
6. **No validator shipped (R-04):** grep/review — no `ProtectedTagsConfig`/allow-list/`evaluate` type;
   the two seams exist as comments only.
7. **Route/format parity (Integration):** end-to-end call produces the same `{action,namespace,tag,`
   `prior_value,new_value}` audit metadata as the service-seam tests.
