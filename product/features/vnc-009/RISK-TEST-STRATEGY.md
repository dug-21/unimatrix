# Risk-Based Test Strategy: vnc-009

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | UsageService `AccessSource::McpTool` routing diverges from original `record_usage_for_entries` vote semantics (NewVote/CorrectedVote/NoOp) | High | Med | High |
| R-02 | Rate limiter Mutex contention blocks MCP search hot path under concurrent load | Med | Low | Low |
| R-03 | StatusReportJson intermediate struct produces JSON field names/nesting that differs from existing manual json! output | High | Med | High |
| R-04 | Session ID prefix not stripped before storage writes, corrupting injection log / co-access pair data | High | Med | High |
| R-05 | `session_id` parameter on MCP tools breaks existing callers that do not send the field | Med | Low | Low |
| R-06 | RateLimiter sliding window does not evict expired entries, causing false rate limit rejections | High | Low | Med |
| R-07 | BriefingService rate limiting (check_search_rate when include_semantic=true) unexpectedly blocks briefing assembly for active agents | Med | Med | Med |
| R-08 | UDS auth failure audit write blocks connection cleanup path | Med | Low | Low |
| R-09 | CallerId::UdsSession exemption logic fails, causing UDS hook operations to be rate-limited | High | Low | Med |
| R-10 | UsageService fire-and-forget tasks capture references to UsageService struct across spawn boundary, causing lifetime errors or panics | Med | Med | Med |
| R-11 | ServiceLayer constructor change breaks all existing test setups that create ServiceLayer | Med | High | High |
| R-12 | `#[derive(Serialize)]` on ContradictionPair/EmbeddingInconsistency in infra/ introduces serde dependency propagation issues | Low | Med | Low |

## Risk-to-Scenario Mapping

### R-01: Vote Semantics Preservation in UsageService

**Severity**: High
**Likelihood**: Medium
**Impact**: Incorrect vote counts corrupt confidence scores. Agents see wrong confidence values, affecting search re-ranking.

**Test Scenarios**:
1. First helpful vote on entry via McpTool: `helpful_count` increments by 1
2. Same agent changes vote from helpful to unhelpful: `helpful_count` decrements, `unhelpful_count` increments
3. Same agent sends duplicate helpful vote: no-op (counts unchanged)
4. Multiple agents vote on same entry: each agent's vote tracked independently
5. McpTool vote triggers confidence recomputation via spawn_blocking
6. HookInjection path does NOT trigger vote processing (no helpful param)

**Coverage Requirement**: Unit tests for each VoteAction variant. Integration test comparing store state before/after UsageService call vs before/after old record_usage_for_entries call with identical inputs.

### R-03: StatusReport JSON Backward Compatibility

**Severity**: High
**Likelihood**: Medium
**Impact**: Agents parsing JSON status output break if field names or nesting change.

**Test Scenarios**:
1. Full StatusReport with all fields populated -> JSON output has identical keys and nesting to manual json! version
2. StatusReport with contradictions (contradiction_scan_performed=true) -> `contradictions` array present with correct object shape
3. StatusReport without contradictions -> `contradictions` key absent
4. StatusReport with embedding inconsistencies -> `embedding_inconsistencies` array with `self_match_similarity` field (not `expected_similarity`)
5. StatusReport without embedding check -> `embedding_inconsistencies` key absent
6. `category_distribution` serializes as `{"decision": 5, "pattern": 3}` object, NOT as `[["decision", 5], ["pattern", 3]]` array
7. `co_access` section nesting: `{ total_pairs, active_pairs, stale_pairs_cleaned, top_clusters: [...] }`
8. `outcomes` section nesting: `{ total, by_type, by_result, top_feature_cycles }`
9. `observation` section nesting: `{ file_count, total_size_bytes, oldest_file_days, ... }`
10. `correction_chains` section nesting: `{ entries_with_supersedes, entries_with_superseded_by, total_correction_count }`

**Coverage Requirement**: Snapshot test: build StatusReport with known data, serialize via StatusReportJson, compare against golden JSON file. Must cover all conditional sections.

### R-04: Session ID Prefix Stripping Before Storage

**Severity**: High
**Likelihood**: Medium
**Impact**: Injection logs written with `mcp::sess-123` or `uds::sess-456` instead of raw session IDs. Breaks session lookups, co-access pair queries, and any code that expects UUID-format session IDs.

**Test Scenarios**:
1. `strip_session_prefix("mcp::abc")` returns `"abc"`
2. `strip_session_prefix("uds::sess-123")` returns `"sess-123"`
3. `strip_session_prefix("raw-id")` returns `"raw-id"` (no prefix)
4. `strip_session_prefix("mcp::")` returns `""` (empty after prefix)
5. `strip_session_prefix("")` returns `""` (empty input)
6. HookInjection via UsageService: injection log entry has raw session_id, not prefixed
7. Co-access pair recording uses raw session_id

**Coverage Requirement**: Unit tests for strip function. Integration test verifying storage writes use unprefixed session IDs.

### R-05: MCP Backward Compatibility with session_id

**Severity**: Medium
**Likelihood**: Low
**Impact**: Existing agent code that sends SearchParams without session_id field fails deserialization.

**Test Scenarios**:
1. SearchParams JSON without session_id field deserializes successfully (session_id = None)
2. SearchParams JSON with session_id: null deserializes successfully (session_id = None)
3. SearchParams JSON with session_id: "abc" deserializes successfully (session_id = Some("abc"))
4. Same tests for LookupParams, GetParams, BriefingParams

**Coverage Requirement**: Deserialization unit tests for each param struct with and without session_id.

### R-06: Rate Limiter Eviction Correctness

**Severity**: High
**Likelihood**: Low
**Impact**: Stale timestamps not evicted -> caller appears to have used all their budget -> false RateLimited errors -> agents cannot search or write.

**Test Scenarios**:
1. 300 requests, wait > 1 hour (mock time), 301st request succeeds (window expired)
2. 300 requests over 59 minutes, 301st at 59:30 fails (window not expired)
3. 150 requests at T=0, 150 at T=30min, request at T=61min: only the T=30min requests remain (partial eviction)
4. Empty window for new caller: first request always succeeds

**Coverage Requirement**: Unit tests with controlled Instant values (inject time source or use short windows for testing). Must test partial eviction.

### R-07: Briefing Rate Limiting Interaction

**Severity**: Medium
**Likelihood**: Medium
**Impact**: Agent doing search + briefing burns through search rate budget faster than expected. 300 searches/hr becomes effectively ~150 if every search is paired with a briefing.

**Test Scenarios**:
1. Agent does 200 searches + 100 semantic briefings = 300 total check_search_rate calls -> OK
2. Agent does 200 searches + 101 semantic briefings = 301st check_search_rate call -> RateLimited
3. Agent does 300 searches + non-semantic briefing (include_semantic=false) -> briefing does NOT call check_search_rate -> OK
4. UDS briefing with include_semantic=true: UdsSession exempt from rate limiting

**Coverage Requirement**: Integration test verifying briefing with semantic search counts against search rate bucket.

### R-09: UDS Exemption Correctness

**Severity**: High
**Likelihood**: Low
**Impact**: UDS hook operations blocked by rate limiting -> context injection fails -> agents lose ambient knowledge.

**Test Scenarios**:
1. CallerId::UdsSession("any") -> check_search_rate returns Ok always (even after 1000 calls)
2. CallerId::UdsSession("any") -> check_write_rate returns Ok always
3. CallerId::Agent("bot") -> check_search_rate enforces limits normally
4. Verify exemption is structural (match arm), not conditional (flag)

**Coverage Requirement**: Unit test: UDS caller makes unlimited calls without rejection.

### R-10: UsageService Spawn Safety

**Severity**: Medium
**Likelihood**: Medium
**Impact**: Lifetime errors prevent compilation (caught at build time). If somehow bypassed, runtime panic in spawned task.

**Test Scenarios**:
1. UsageService::record_access spawns task that completes without panic
2. UsageService dropped while spawned task is running: task completes safely (all data owned, no references to self)
3. Multiple concurrent record_access calls: no data races (Arcs + Mutex protect shared state)

**Coverage Requirement**: Unit test: call record_access, drop UsageService, verify no panic. Concurrent test: spawn 10 record_access calls simultaneously.

### R-11: ServiceLayer Constructor Test Breakage

**Severity**: Medium
**Likelihood**: High
**Impact**: All tests that construct ServiceLayer fail to compile until updated with UsageService parameter.

**Test Scenarios**:
1. ServiceLayer::new() accepts UsageService as parameter or constructs it internally
2. Existing test helpers updated to provide UsageService

**Coverage Requirement**: Compilation. All existing tests pass after ServiceLayer modification.

## Integration Risks

| Risk | Components | Scenario |
|------|-----------|----------|
| SearchService + SecurityGateway rate limit call ordering | SearchService, SecurityGateway | Rate limit checked BEFORE search starts (no wasted embedding computation on rate-limited calls) |
| StoreService + SecurityGateway rate limit vs content scan ordering | StoreService, SecurityGateway | Rate limit checked BEFORE content scan (fast rejection path) |
| UsageService + ConfidenceService coupling | UsageService, ConfidenceService | McpTool access triggers confidence recompute via spawn_blocking — same code path as old record_usage_for_entries |
| ToolContext + AuditContext session_id threading | ToolContext, AuditContext, ServiceLayer | session_id flows from MCP params -> ToolContext -> AuditContext -> service methods -> audit events |
| StatusReportJson + StatusService | StatusReportJson, StatusService | StatusService builds StatusReport, response formatter maps to StatusReportJson — no StatusService changes needed |

## Edge Cases

| Edge Case | Expected Behavior |
|-----------|-------------------|
| session_id with `::` in value (e.g., `"mcp::nested::value"`) | `strip_session_prefix` strips only first prefix: returns `"nested::value"` |
| session_id with length > 256 characters | Rejected by S3 validation before prefixing |
| session_id containing control characters | Rejected by S3 validation |
| Rate limit at exactly 300/60 (boundary) | 300th/60th request succeeds, 301st/61st fails |
| Rate limit window boundary (request at exactly T+3600s) | Expired entries evicted, request succeeds |
| Empty entry_ids to UsageService::record_access | Early return, no spawn_blocking (matches existing behavior) |
| UsageDedup with agent_id that contains `::` | Works correctly — UsageDedup keys by (agent_id, entry_id), not by session_id |
| StatusReport with zero entries (empty knowledge base) | JSON output has all required top-level fields with zero values |
| CallerId::Agent("") — empty agent_id | Should not occur (identity resolution rejects empty agent_id). If it does, rate limiter treats as a valid caller with empty key. |
| UDS auth failure with no peer credentials extractable | AuditEvent has agent_id="unknown", detail contains error message |

## Security Risks

### Rate Limiting as Security Gate (F-09)

**Untrusted input**: MCP tool calls from agents (agent_id is self-declared, partially trusted via identity resolution).
**Damage potential**: Without rate limiting, a compromised or malfunctioning agent can flood the knowledge base with writes or exhaust embedding resources with searches.
**Blast radius**: System-wide — affects all agents sharing the server.
**Mitigation**: S2 rate limiting keyed by CallerId. Limits: 300 searches/hr, 60 writes/hr. Agents exceeding limits get `RateLimited` error with retry information.

### Session ID Injection

**Untrusted input**: `session_id` parameter on MCP tools. Though sourced from hooks, the field is a String and could be manipulated.
**Damage potential**: Malicious session_id could inject control characters, exceed length limits, or attempt to impersonate UDS sessions.
**Blast radius**: Limited — session_id is used for audit and future features. Transport prefix prevents impersonation.
**Mitigation**: S3 validation (length max 256, no control chars). Transport prefix (`mcp::` vs `uds::`) prevents cross-transport confusion structurally.

### UDS Auth Failure Audit (F-23)

**Untrusted input**: Unix socket connections from unknown processes.
**Damage potential**: Brute-force auth attempts are invisible without audit logging.
**Blast radius**: Local — UDS is localhost-only.
**Mitigation**: AuditEvent written on every auth failure. Enables monitoring and alerting on repeated failures.

## Failure Modes

| Failure | Expected Behavior |
|---------|-------------------|
| Rate limiter Mutex poisoned | Use `unwrap_or_else(\|e\| e.into_inner())` to recover from poisoned Mutex, matching CategoryAllowlist pattern (vnc-004) |
| UsageService spawn_blocking task panics | Panic is caught by tokio runtime. Error logged. Request processing unaffected (fire-and-forget) |
| AuditLog write fails during UDS auth audit | Error logged via tracing::warn. Connection still closed. Auth failure still prevents access |
| serde_json::to_string_pretty fails on StatusReportJson | `unwrap_or_default()` returns empty string. CallToolResult contains empty content (graceful degradation) |
| Rate limiter HashMap grows unbounded (many unique callers) | Bounded by number of unique agents. In practice: <50 callers. Memory: <50KB. Not a concern at expected scale |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (AccessSource routing complexity) | R-01 | Exhaustive match with per-variant unit tests. Each variant calls distinct internal method. |
| SR-02 (Mutex contention on RateLimiter) | R-02 | Sub-microsecond critical section. Lazy eviction in same lock acquisition. Benchmark target: <10us. |
| SR-03 (Serialize propagation to infra types) | R-12 | ContradictionPair and EmbeddingInconsistency get `#[derive(Serialize)]`. serde already a dependency. |
| SR-04 (JSON key ordering) | R-03 | StatusReportJson intermediate struct controls nesting. Snapshot test verifies. |
| SR-05 (Vote semantics preservation) | R-01 | Direct code move from record_usage_for_entries to UsageService. Integration test with identical inputs. |
| SR-06 (Session ID storage compatibility) | R-04 | ADR-004: prefix at boundary, strip before storage. Unit tests for strip function. |
| SR-07 (Hook dependency for session_id) | R-05 | Graceful degradation: session_id=None produces identical behavior. serde(default) on param struct. |
| SR-08 (Briefing sharing search rate bucket) | R-07 | Documented interaction. BriefingService only calls check_search_rate when include_semantic=true. |
| SR-09 (Unused CallerId::ApiKey) | — | ADR-003: deferred. Only Agent and UdsSession variants. No dead code. |
| SR-10 (ServiceLayer constructor change) | R-11 | Follow existing pattern. UsageService constructed in ServiceLayer::new(). Test helpers updated. |
| SR-11 (AuditLog parameter in UDS handler) | R-08 | Arc<AuditLog> added as parameter. Audit write is fire-and-forget (emit_audit). |
| SR-12 (Lock ordering) | R-02 | RateLimiter has single internal Mutex. No nesting. No async across lock boundary. |
| SR-13 (spawn_blocking ownership) | R-10 | Clone Arcs before spawn. Move owned data into closure. No &self capture. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| High | 4 (R-01, R-03, R-04, R-11) | 25 scenarios |
| Medium | 4 (R-06, R-07, R-09, R-10) | 15 scenarios |
| Low | 4 (R-02, R-05, R-08, R-12) | 10 scenarios |
| **Total** | **12** | **50 scenarios** |
