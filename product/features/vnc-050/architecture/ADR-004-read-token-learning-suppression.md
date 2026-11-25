## ADR-004: Read Tokens Record No Learning — Observe Returns 403; Read-Token Tool Calls Skip Usage/Co-Access/Query-Log Recording; Client Latches 403 per Credential

### Context
OQ-6 / AC-15: read automation must not poison learning (SLN1). Learning writes originate from two HTTP paths: (1) observe → hook dispatcher (extraction tick entries, helpful_count, signal_queue, co_access, injection_log, query_log, cycle_events, cycle_tags, sessions.feature_cycle); (2) MCP `tools/call` → `UsageService::record_access` (usage, co-access) and the `tools.rs` query_log insert. Prior version (#5813) gated only path 1 because ADR-003 then left tool identity unchanged. ADR-003 (revised, capability ceiling) restores the means to gate path 2; HUMAN-DECISIONS Decision 5 requires it.

### Decision
- **Observe (server)**: `route_observe` and every session telemetry route under `/v1/{slug}/` return `403 {"error":"token lacks SessionWrite"}` when the auth-layer `ResolvedIdentity` lacks `SessionWrite`, before any dispatcher call.
- **Tool calls (server)**: learning is recorded only when `cap_ceiling::records_learning(ctx.cap_ceiling.as_ref())` is true (`None` → true; ceiling → contains `Write`). Gate points:
  - `UsageContext` gains `learning_allowed: bool`; `UsageService::record_access` returns early when false (covers usage counts, co-access, helpful). MCP construction sites in `tools.rs` set it from `ctx` (one line each); hook/briefing internal sites set `true`.
  - The `tools.rs` query_log insert (search path) is wrapped in the same predicate.
  - Same predicate, same ceiling as `require_cap` — no second identity or policy source (#4453).
- **Client** (`lib/hook-client`, observe POST) — persistent per-credential latch (amended by HD 10; the hook runs one process per event, so a per-process latch floods, R-17):
  1. Latch ONLY on HTTP 403 with body `token lacks SessionWrite`. Any other 403, 401, or network failure keeps existing behavior and writes no marker.
  2. On that denial: one stderr warning (`unimatrix: observe disabled for this credential (403, read-only token); hook events will not be recorded`), write marker `observe-denied.json` in the client `stateDir`, mode 0600 (same as the credential file). Marker holds the SHA-256 hex digest of the credential, never the secret. Subsequent processes whose credential digest matches the marker skip observe POSTs silently.
  3. No expiry: token mode is immutable, labels are never reused, a revoked token gets 401 — refusal stays true. A new credential has a different digest and gets a fresh attempt automatically.
  4. Fail open toward retry: missing, corrupt, or unreadable marker = not refused. Marker write failure never changes hook exit code or output.
  5. Hook exit code/stdout unchanged in all cases. Up-front skip by token mode is not possible: ADR-005 keeps the mode server-side, so the client learns only from the 403.
  6. AC-18 tests: N events with read token → exactly one warning and one 403; changing credential resets; corrupt marker → one retry and rewrite; read-write credential never creates a marker.
- Audit rows (`http_access`, tool audit) are not learning and are still written.

### Consequences
- A read token produces zero learning rows on either path; AC-15 holds in full.
- Read-token tool responses are unchanged (reads still served); only side-effect recording is skipped.
- `tools.rs` touch is one field line per MCP `UsageContext` site plus one guard; logic lives in `mcp/cap_ceiling.rs`.
