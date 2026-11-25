# C4b Capability Ceiling — `mcp/cap_ceiling.rs` (new) + mechanical touches

ADR-003 (#5819), ADR-004 (#5826), HD 5. Depends on C4 (W0), C2 (W0). Target < 200 lines incl. unit tests; HTTP tests in `mcp/cap_ceiling_http_tests.rs`.

## Step 0 — FIRST implementation step (gates everything else in C4b)

Goal: prove over real HTTP, in a stateful rmcp session, that the `BearerPrincipal` inserted by the auth layer is visible in the tool's `RequestContext` via `Parts.extensions` for POST `initialize` then POST `tools/call`.

Prerequisites (W0): C4 types; C2 `TokenTableValidator` + `validate_principal` + extension insert; `cap_ceiling::ceiling_from` (below) with test probe.

Test probe (compiled only under `cfg(test)`; zero production surface):
```
#[cfg(test)]
pub(crate) static PROBE: Mutex<Vec<ProbeRecord>> = Mutex::new(Vec::new());
#[cfg(test)]
#[derive(Clone, Debug)] pub(crate) enum ProbeRecord { NoParts, PartsNoPrincipal, Principal(BearerPrincipal) }
// ceiling_from pushes one ProbeRecord per call under cfg(test)
```
Test `step0_bearer_principal_reaches_tool_context` (file `mcp/cap_ceiling_http_tests.rs`, a `#[cfg(test)]` module so it sees PROBE):
```
spin up real listener: start_http_listener with TLS acceptor (existing listener/tests.rs fixture),
    StaticTokenAuthLayer::with_validator(TokenTableValidator(legacy, table with one read token for slug A)),
    PathRouter over a real resolver for slug A
client (hyper/reqwest-equivalent already used in listener tests, accepting the test cert):
  POST /v1/A/mcp  initialize           (Bearer scoped)  -> capture Mcp-Session-Id
  POST /v1/A/mcp  notifications/initialized
  POST /v1/A/mcp  tools/call context_search {query:"x"}  (same session)
assert PROBE contains Principal(Scoped{label:"ci-read", mode:Read, ..}) for the tools/call
repeat with legacy token -> Principal(Legacy)
```
Decision:
- PASS → proceed to W1 using `ceiling_from` as written.
- FAIL (NoParts or PartsNoPrincipal at the tool) → apply ADR-003 fallback in `McpAdapter::handle` (`http/router.rs`, Step 3):
  ```
  let (parts, body) = request.into_parts();      // existing
  // fallback: parts.extensions already carries BearerPrincipal from the auth layer;
  // re-insert explicitly so rmcp's Parts injection carries it
  if let Some(p) = parts.extensions.get::<BearerPrincipal>().cloned() { full_request.extensions_mut().insert(p) }
  ```
  If still failing, escalate (rmcp path rebuilds extensions; architecture decision needed). Re-run step 0.
The test stays permanently (rmcp upgrade guard).

## Types / Functions (`mcp/cap_ceiling.rs`)

```
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CapCeiling(Vec<Capability>);

pub(crate) fn ceiling_from(rc: &RequestContext<RoleServer>) -> Option<CapCeiling> {
    match rc.extensions.get::<http::request::Parts>() {
        None => { probe(NoParts); None }                               // stdio: no ceiling
        Some(parts) => match parts.extensions.get::<BearerPrincipal>() {
            None => { probe(PartsNoPrincipal); Some(CapCeiling(vec![])) }   // HTTP w/o principal: deny all (never None)
            Some(BearerPrincipal::Legacy) => { probe(..); None }            // legacy: today's semantics
            Some(BearerPrincipal::Scoped{mode, ..}) => { probe(..); Some(CapCeiling(mode.capabilities())) }  // C4 table
        }
    }
}

pub(crate) fn permits(c: Option<&CapCeiling>, cap: Capability) -> bool {
    match c { None => true, Some(CapCeiling(v)) => v.contains(&cap) }
}

pub(crate) fn records_learning(c: Option<&CapCeiling>) -> bool {
    permits(c, Capability::Write)       // None -> true; ceiling -> contains Write
}
```

## Mechanical Touches

`mcp/context.rs`: add field
```
/// Token-mode capability ceiling (vnc-050 ADR-003). None = no ceiling (stdio, UDS, legacy token).
pub cap_ceiling: Option<CapCeiling>,
```
Also update `test_support.rs:408` literal: `cap_ceiling: None`.

`server.rs` `build_context_with_external_identity` (+2 lines total incl. C4 guard):
```
crate::http::principal::reject_reserved_agent_id(params_agent_id).map_err(rmcp::ErrorData::from)?;   // C4, first line
...
Ok(ToolContext { ..existing.., cap_ceiling: crate::mcp::cap_ceiling::ceiling_from(request_context) })
```
`external_identity` branch untouched (dormant).

`server.rs` `require_cap` (server.rs:631):
```
pub(crate) async fn require_cap(&self, ctx: &ToolContext, cap: Capability) -> Result<(), rmcp::ErrorData> {
    if !cap_ceiling::permits(ctx.cap_ceiling.as_ref(), cap) {
        return Err(rmcp::ErrorData::from(ServerError::CapabilityDenied { agent_id: ctx.agent_id.clone(), capability: cap }));
    }   // same error type as registry deny; message does not reveal which set denied
    let registry = Arc::clone(&self.registry);
    let agent_id = ctx.agent_id.clone();
    ... unchanged spawn_blocking registry.require_capability(&agent_id, cap)
}
```

`mcp/tools.rs` (15 sites): `self.require_cap(&ctx.agent_id, X)` → `self.require_cap(&ctx, X)`. No other change at these sites. Static check: no `registry.require_capability` / `has_capability` call in `tools.rs` outside `require_cap` (R-02.5).

`services/usage.rs`:
```
pub(crate) struct UsageContext { ..existing.., 
    /// vnc-050 ADR-004: false for read-token calls; record_access returns early.
    pub(crate) learning_allowed: bool }

pub(crate) fn record_access(&self, entry_ids, source, ctx) {
    if !ctx.learning_allowed { return; }        // before ANY enqueue/spawn (RTS integration risk)
    if entry_ids.is_empty() { return; }
    ... unchanged
}
```
Every `UsageContext {` literal gets `learning_allowed`: the 5 MCP sites in `tools.rs` (lines ~821, 964, 1182, 1328, 2209) set `learning_allowed: cap_ceiling::records_learning(ctx.cap_ceiling.as_ref())`; test literals in `usage.rs` set `true`. No `Default` impl is added (a default would silently yield true on a new MCP site, R-04.6).

`tools.rs` query_log insert (search path ~line 846): wrap the existing `QueryLogRecord::new … spawn_blocking(insert_query_log)` block in
```
if cap_ceiling::records_learning(ctx.cap_ceiling.as_ref()) { ...existing block... }
```

Sink inventory obligation (R-04.6, FR-15): implementer greps `tools.rs` for other learning sinks reachable from read tools (`helpful`, implicit signals, `feature_entries`, briefing log, `co_access`, session `signal_queue`, `topic_signals`). Each sink not routed through `record_access` or the query_log block gets the same `records_learning(ctx.cap_ceiling.as_ref())` guard (one line) and its own test. Report the inventory in the agent report.

## Error Handling
Ceiling deny → `CapabilityDenied` (existing MCP error mapping). Audit rows (tool audit, `http_access`) still written.

## Key Test Scenarios (all over real HTTP, never calling `require_cap` directly — R-02)
- Step 0 (above), permanent.
- Read token × {store, correct, deprecate, quarantine, edge, tag, enroll, cycle writes} × agent_id {omitted, `human`, registry Write-holder, `token:x`→InvalidInput} → denied; DB unchanged (AC-08, AC-25).
- Read-write companion: same writes succeed; rw + `human` on Admin tool → denied.
  Admin-gated tools (`require_cap(.., Capability::Admin)` in `mcp/tools.rs`): `context_quarantine` (line ~2247) and `context_enroll` (~2371). These are the only two; `rw_token_human_admin_tool_denied` runs both. Neither is in `TokenMode::ReadWrite.capabilities()` (C4), so the ceiling denies them regardless of registry Admin. Implementer re-greps `Capability::Admin` at delivery; any new site is added to this list and the test.
- Narrow-only: registry agent lacking Write under rw token → denied.
- HTTP request reaching tool with Parts but no principal (`StripPrincipalLayer`, OVERVIEW §Test-Only Seams) → denied Read and Write; stdio unchanged.
- Read token search/lookup/get/briefing with `helpful:true`, `feature`, `agent_id:"human"` → served; usage, co_access, query_log, feature_entries unchanged after `UsageService::flush` (OVERVIEW §Test-Only Seams; `enqueued()==0` for read token) (AC-15, AC-26); rw companion changes them; legacy + stdio still record.
- R-20 pin: rw + `human` non-Admin write succeeds, `http_access` carries real label. R-22 pin: rw + omitted agent_id → `anonymous`, no `token:` in agents table (AC-27).
- Unit: `permits`/`records_learning` truth table for None, empty, read, rw.
