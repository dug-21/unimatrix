# Pseudocode Overview: vnc-007 Briefing Unification

## Component Interaction

```
MCP Transport (tools.rs)                UDS Transport (uds_listener.rs)
  context_briefing [feature-gated]        CompactPayload      HookRequest::Briefing
  |                                       |                    |
  | BriefingParams{                       | BriefingParams{    | BriefingParams{
  |   include_semantic=true,              |   include_semantic  |   include_semantic=true,
  |   include_conventions=true,           |   =false,           |   include_conventions=true,
  |   injection_history=None,             |   injection_history |   injection_history=None,
  |   max_tokens=validated}               |   =Some(..)}        |   max_tokens=..}
  v                                       v                    v
  +---------------------------------------+--------------------+
  |          services/briefing.rs                               |
  |   BriefingService::assemble(params, audit_ctx)              |
  |     1. S3 validate inputs                                   |
  |     2. If injection_history: fetch + partition + budget      |
  |     3. If include_conventions: query conventions + budget    |
  |     4. If include_semantic: SearchService::search + budget   |
  |     5. Collect entry_ids, S5 audit                          |
  |     -> BriefingResult                                       |
  +-------------------------------------------------------------+
       |               |                    |
       v               v                    v
  AsyncEntryStore  SearchService     SecurityGateway
  (convention       (semantic          (S3/S4/S5)
   + ID queries)     search)
```

## Data Flow

1. Transport receives request, resolves identity/capability, constructs BriefingParams
2. BriefingService::assemble validates inputs (S3) then executes up to 3 independent fetch paths:
   - **Injection history path**: fetch by ID, exclude quarantined, deduplicate, partition, sort by confidence
   - **Convention path**: query by role/topic, sort feature-tagged first
   - **Semantic path**: delegate to SearchService with k=3, feature boost, co-access boost
3. Token budget applied to each path's results (proportional for injection, linear for others)
4. BriefingResult returned to transport for formatting

## Shared Types (services/briefing.rs)

```rust
pub(crate) struct BriefingService { entry_store, search, gateway }
pub(crate) struct BriefingParams { role, task, feature, max_tokens, include_conventions, include_semantic, injection_history }
pub(crate) struct BriefingResult { conventions, relevant_context, injection_sections, entry_ids, search_available }
pub(crate) struct InjectionSections { decisions, injections, conventions }
pub(crate) struct InjectionEntry { entry_id: u64, confidence: f64 }
```

## Component List

| Component | File | Lines (est.) | Dependencies |
|-----------|------|-------------|-------------|
| BriefingService | services/briefing.rs | ~280 new | AsyncEntryStore, SearchService, SecurityGateway |
| MCP Rewiring | tools.rs | -200 (net) | BriefingService via ServiceLayer |
| UDS Rewiring | uds_listener.rs | -250 (net) | BriefingService via ServiceLayer, SessionRegistry |
| Duties Removal | response.rs | -30 (net) | Briefing struct, format_briefing |
| Feature Flag | Cargo.toml | +4 | rmcp, mcp-briefing feature |

## Patterns Used

- **ServiceLayer injection**: BriefingService added to ServiceLayer following SearchService/StoreService/ConfidenceService pattern (vnc-006)
- **SecurityGateway S3/S4/S5**: Same gateway pattern as SearchService (validate, quarantine-check, audit-emit)
- **Fire-and-forget audit**: Same pattern as SearchService (gateway.emit_audit)
- **AuditContext threading**: Same pattern as SearchService/StoreService
- **Token budget estimation**: Reuses `(title.len() + content.len() + 50) / 4` pattern from tools.rs
- **Graceful embed degradation**: Same EmbedNotReady handling as tools.rs context_briefing

## Integration Harness

BriefingService delegates to SearchService. The following existing test infrastructure applies:
- `make_store()`, `make_embed_service()`, `make_dispatch_deps()` helpers in uds_listener.rs tests
- `SecurityGateway::new_permissive()` in gateway.rs tests
- `make_entry()` helper in response.rs tests

New integration tests needed:
- BriefingService unit tests (mock-free, using real store + permissive gateway)
- MCP context_briefing delegation test (verify same output minus duties)
- UDS CompactPayload delegation test (verify behavioral equivalence)
- UDS HookRequest::Briefing handler test (verify BriefingContent response)
- Feature flag compilation tests (both configurations)
