## ADR-004: Read Tokens Record No Learning — Observe Returns 403; Read-Token Tool Calls Skip Usage/Co-Access/Query-Log Recording; Client Treats 403 as Warn-Once

### Context
OQ-6 / AC-15: read automation must not poison learning (SLN1). Learning writes originate from two HTTP paths: (1) observe → hook dispatcher (extraction tick entries, helpful_count, signal_queue, co_access, injection_log, query_log, cycle_events, cycle_tags, sessions.feature_cycle); (2) MCP `tools/call` → `UsageService::record_access` (usage, co-access) and the `tools.rs` query_log insert. Prior version (#5813) gated only path 1 because ADR-003 then left tool identity unchanged. ADR-003 (revised, capability ceiling) restores the means to gate path 2; HUMAN-DECISIONS Decision 5 requires it.

### Decision
- **Observe (server)**: `route_observe` and every session telemetry route under `/v1/{slug}/` return `403 {"error":"token lacks SessionWrite"}` when the auth-layer `ResolvedIdentity` lacks `SessionWrite`, before any dispatcher call.
- **Tool calls (server)**: learning is recorded only when `cap_ceiling::records_learning(ctx.cap_ceiling.as_ref())` is true (`None` → true; ceiling → contains `Write`). Gate points:
  - `UsageContext` gains `learning_allowed: bool`; `UsageService::record_access` returns early when false (covers usage counts, co-access, helpful). MCP construction sites in `tools.rs` set it from `ctx` (one line each); hook/briefing internal sites set `true`.
  - The `tools.rs` query_log insert (search path) is wrapped in the same predicate.
  - Same predicate, same ceiling as `require_cap` — no second identity or policy source (#4453).
- **Client** (`lib/hook-client`, observe POST): on 403, one stderr warning per process (`unimatrix: observe disabled for this credential (403, read-only token); hook events will not be recorded`), latch off further observe POSTs, hook exit code/stdout unchanged; other failures unchanged.
- Audit rows (`http_access`, tool audit) are not learning and are still written.

### Consequences
- A read token produces zero learning rows on either path; AC-15 holds in full.
- Read-token tool responses are unchanged (reads still served); only side-effect recording is skipped.
- `tools.rs` touch is one field line per MCP `UsageContext` site plus one guard; logic lives in `mcp/cap_ceiling.rs`.
