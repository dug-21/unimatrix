# Risk-Based Test Strategy: col-008

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | SessionRegistry lock contention corrupts or loses injection history | High | Low | Medium |
| R-02 | CompactPayload returns stale entries (deleted/quarantined since injection) | Medium | Medium | Medium |
| R-03 | Token budget overflow produces invalid UTF-8 or exceeds MAX_COMPACTION_BYTES | High | Medium | High |
| R-04 | Fallback path returns empty payload when knowledge base has relevant entries | Medium | Medium | Medium |
| R-05 | ContextSearch injection tracking fails silently, leaving injection history empty | High | Medium | High |
| R-06 | Session_id mismatch between UserPromptSubmit and PreCompact events | High | Low | Medium |
| R-07 | CoAccessDedup behavior changes when absorbed into SessionRegistry | High | Low | Medium |
| R-08 | CompactPayload latency exceeds 50ms with large injection histories | Medium | Low | Low |
| R-09 | Wire protocol backward incompatibility from session_id addition to ContextSearch | Medium | Low | Low |
| R-10 | PreCompact hook classified as fire-and-forget, missing response | High | Low | Medium |
| R-11 | Entry fetch failures during compaction payload construction cause partial/empty payload | Medium | Medium | Medium |
| R-12 | SessionRegister not called before first ContextSearch, injection tracking silently skipped | Medium | High | High |

## Risk-to-Scenario Mapping

### R-01: SessionRegistry Lock Contention

**Severity**: High
**Likelihood**: Low
**Impact**: Injection history data loss or corruption. CompactPayload returns incomplete or wrong entries.

**Test Scenarios**:
1. Concurrent record_injection and get_state calls for the same session — verify injection_history is consistent
2. Concurrent register_session and clear_session — verify no panic or deadlock
3. Rapid sequential record_injection calls (simulating burst of prompts) — verify all entries recorded

**Coverage Requirement**: Unit tests with sequential access patterns. Concurrency testing is low-priority given hook event serialization by Claude Code.

### R-02: CompactPayload Returns Stale Entries

**Severity**: Medium
**Likelihood**: Medium
**Impact**: Agent receives outdated or quarantined knowledge after compaction, potentially acting on superseded decisions.

**Test Scenarios**:
1. Inject entry, quarantine it via MCP tool, then trigger CompactPayload — verify quarantined entry is excluded
2. Inject entry, deprecate it, then trigger CompactPayload — verify deprecated entry is included with indicator
3. Inject entry, delete/remove it (entry_store.get returns error), then trigger CompactPayload — verify entry is skipped gracefully

**Coverage Requirement**: Integration test with actual entry state changes between injection and compaction.

### R-03: Token Budget Overflow or Invalid UTF-8

**Severity**: High
**Likelihood**: Medium
**Impact**: Claude Code receives corrupted content or excessively long payload that wastes context window.

**Test Scenarios**:
1. Entries totaling > 8000 bytes — verify output.len() <= MAX_COMPACTION_BYTES
2. Multi-byte UTF-8 content (CJK, emoji) at truncation boundary — verify valid UTF-8 and no mid-character split
3. Single entry exceeding its category budget — verify truncation at char boundary
4. All categories at capacity — verify total does not exceed MAX_COMPACTION_BYTES
5. Empty injection history with fallback entries exceeding budget — verify budget enforced on fallback path
6. Session context section consumes more than CONTEXT_BUDGET_BYTES — verify it is truncated

**Coverage Requirement**: Unit tests with controlled entry payloads. Must test both ASCII and multi-byte content.

### R-04: Fallback Path Returns Empty Payload

**Severity**: Medium
**Likelihood**: Medium
**Impact**: Agent loses all context after compaction when server has relevant knowledge but fallback query misses it.

**Test Scenarios**:
1. No injection history, knowledge base has active decisions — verify fallback returns decisions
2. No injection history, knowledge base has active conventions but no decisions — verify fallback returns conventions
3. No injection history, knowledge base is empty — verify empty BriefingContent returned (not error)
4. Feature tag available in SessionState — verify fallback filters decisions by feature tag
5. Feature tag not available — verify fallback returns all active decisions (unfiltered)

**Coverage Requirement**: Integration test with populated knowledge base, session without injection history.

### R-05: ContextSearch Injection Tracking Fails Silently

**Severity**: High
**Likelihood**: Medium
**Impact**: Injection history stays empty despite active injections. CompactPayload falls back to generic query every time, defeating the session-aware compaction defense.

**Test Scenarios**:
1. ContextSearch with valid session_id — verify injection_history populated after response
2. ContextSearch with session_id but SessionRegistry has no session registered — verify silent skip (no error, response still sent)
3. ContextSearch without session_id (None) — verify no injection tracking attempted, response unaffected
4. Multiple ContextSearch calls for same session — verify injection_history accumulates
5. ContextSearch returns empty results — verify no injection recorded (nothing to track)

**Coverage Requirement**: Integration test: ContextSearch -> verify SessionState -> CompactPayload returns tracked entries.

### R-06: Session ID Mismatch

**Severity**: High
**Likelihood**: Low
**Impact**: Injection history recorded under one session_id but CompactPayload queries with a different one. Compaction defense always falls back.

**Test Scenarios**:
1. SessionRegister with session_id "A", ContextSearch with session_id "A", CompactPayload with session_id "A" — verify end-to-end
2. CompactPayload with session_id "B" (no matching session) — verify fallback path used
3. ContextSearch with session_id "A" but no prior SessionRegister for "A" — verify injection silently skipped

**Coverage Requirement**: End-to-end integration test covering the full lifecycle: register -> inject -> compact.

### R-07: CoAccessDedup Behavior Changes

**Severity**: High
**Likelihood**: Low
**Impact**: Co-access pair recording regresses — either duplicate pairs flood the database or valid pairs are lost.

**Test Scenarios**:
1. Two ContextSearch calls with same entry set, same session — verify co-access recorded only once (same as col-007 CoAccessDedup behavior)
2. Two ContextSearch calls with different entry sets, same session — verify both sets recorded
3. SessionClose clears dedup state — verify subsequent session with same ID starts fresh
4. check_and_insert_coaccess with no registered session — verify returns false, no panic

**Coverage Requirement**: Unit tests that replicate col-007's CoAccessDedup test cases exactly, running against SessionRegistry's check_and_insert_coaccess method.

### R-08: CompactPayload Latency Exceeds Budget

**Severity**: Medium
**Likelihood**: Low
**Impact**: PreCompact hook times out, agent receives no compaction defense content.

**Test Scenarios**:
1. CompactPayload with 50 entries in injection history — verify server-side processing < 15ms
2. CompactPayload fallback path with 100 active entries in knowledge base — verify processing < 15ms
3. CompactPayload with 5 entries (minimal case) — verify processing < 5ms

**Coverage Requirement**: Benchmark test: repeated CompactPayload dispatch, measure p95 latency.

### R-09: Wire Protocol Backward Incompatibility

**Severity**: Medium
**Likelihood**: Low
**Impact**: Existing col-007 hook processes fail to deserialize ContextSearch responses or send malformed requests.

**Test Scenarios**:
1. ContextSearch without session_id field — verify deserialization succeeds, session_id is None
2. ContextSearch with session_id field — verify deserialization includes session_id
3. CompactPayload round-trip serialization/deserialization — verify all fields preserved
4. BriefingContent round-trip — verify content and token_count preserved

**Coverage Requirement**: Unit tests for wire protocol round-trips. Must test with and without optional fields.

### R-10: PreCompact Classified as Fire-and-Forget

**Severity**: High
**Likelihood**: Low
**Impact**: Hook process exits before receiving CompactPayload response. No content printed to stdout. Compaction defense completely ineffective.

**Test Scenarios**:
1. Verify `is_fire_and_forget` check excludes HookRequest::CompactPayload
2. build_request("PreCompact", ...) returns CompactPayload, not RecordEvent
3. End-to-end: hook process sends CompactPayload, receives BriefingContent, prints to stdout

**Coverage Requirement**: Unit test for is_fire_and_forget classification. Integration test for full hook flow.

### R-11: Entry Fetch Failures During Payload Construction

**Severity**: Medium
**Likelihood**: Medium
**Impact**: CompactPayload silently drops entries from the payload, returning fewer entries than expected.

**Test Scenarios**:
1. Injection history contains entry ID that no longer exists in store — verify entry is skipped, other entries still included
2. All entry fetches fail — verify empty BriefingContent returned (not error)
3. One entry fetch fails in a batch of 10 — verify 9 entries included in payload

**Coverage Requirement**: Unit test with mock entry_store that returns errors for specific IDs.

### R-12: SessionRegister Not Called Before ContextSearch

**Severity**: Medium
**Likelihood**: High
**Impact**: Session state not initialized. Injection tracking silently skipped. CompactPayload always falls back.

**Test Scenarios**:
1. ContextSearch with session_id but no prior SessionRegister — verify response is normal, injection tracking is silently skipped
2. SessionRegister then ContextSearch — verify injection tracking works
3. CompactPayload for session that was never registered — verify fallback path used

**Coverage Requirement**: Integration test covering the "no SessionRegister" edge case.

## Integration Risks

### IR-01: SessionRegistry Threading with UDS Listener

The SessionRegistry is shared across all UDS connection handlers via Arc. Each connection handler may read or write session state. The Mutex ensures correctness but must be held for the minimum duration (microseconds per operation).

**Test**: Verify no deadlock when CompactPayload handler holds the lock while accessing entry_store (which does NOT hold the lock — entry_store operations happen after releasing the Mutex).

### IR-02: ContextSearch Handler Modification

col-008 adds injection tracking to col-007's ContextSearch handler. The tracking call happens after the response is constructed but the response has not yet been sent. If tracking panics or blocks, the response could be delayed.

**Test**: Verify ContextSearch response latency is not significantly affected by injection tracking (< 1ms additional).

### IR-03: SessionRegister/SessionClose Lifecycle

SessionRegister creates state, SessionClose destroys it. If events arrive out of order (Close before Register, or Register after Close for same ID), the registry must handle gracefully.

**Test**: SessionClose for non-existent session is a no-op. SessionRegister after SessionClose starts fresh state.

## Edge Cases

1. **Empty session_id in ContextSearch** — treat as None (no injection tracking)
2. **Very long injection history** (1000+ entries) — get_state() clones the entire history. Bounded by session lifetime; 1000 entries x 24 bytes = ~24KB, acceptable.
3. **CompactPayload for session with only 1 injection** — valid case, returns that single entry if it meets category criteria
4. **CompactPayload with token_limit override** — if the request specifies token_limit, use it instead of MAX_COMPACTION_BYTES. Convert: token_limit * 4 = byte budget.
5. **Entry with empty content** — include in payload with title/category/confidence but no content section. Counts only the header bytes toward budget.
6. **Duplicate entry_ids in injection history** — deduplicate at CompactPayload time, keeping highest confidence value. Do not deduplicate at record_injection time (preserves chronological history).
7. **CompactPayload during active ContextSearch** — SessionRegistry Mutex serializes access. No data corruption risk.
8. **Server shutdown while session active** — session state lost. Next PreCompact (if server restarts) uses fallback path.

## Security Risks

### Untrusted Input: CompactPayload Request

The CompactPayload request arrives from a hook process via authenticated UDS (UID verification). The `session_id` field is a string from Claude Code's stdin JSON. Risk: a crafted session_id could attempt to access another session's injection history.

**Mitigation**: Session state is keyed by exact session_id match. There is no wildcard or prefix matching. An attacker would need to guess an exact session_id AND have UID-level access to the UDS socket (Layer 1 + Layer 2 auth from col-006). Risk is negligible.

### Untrusted Input: injected_entry_ids

The `injected_entry_ids` field on CompactPayload allows the hook process to suggest entry IDs. If the server falls back to these IDs, a malicious hook could request arbitrary entries.

**Mitigation**: The entry_store.get() call returns full EntryRecord which includes status — quarantined entries are still filtered. The hook process runs as the same user (UID verification). The injected_entry_ids are only used when the server has no tracked history (fallback). Risk is low.

### Blast Radius

If the CompactPayload handler is compromised, the worst case is injecting misleading content into the compacted window. This could cause an agent to make incorrect decisions after compaction. The blast radius is limited to the current session and the current compaction event — subsequent injections from UserPromptSubmit (col-007) will deliver fresh, correct knowledge.

## Failure Modes

| Failure | Expected Behavior |
|---------|-------------------|
| Server unavailable at PreCompact time | Hook exits 0 with no stdout. Agent continues without compaction defense. |
| Session state lost (server restart) | Fallback to category-based query. Agent gets generic context. |
| Entry fetch fails for some IDs | Skip failed entries, include remaining. Partial payload is better than none. |
| All entry fetches fail | Return empty BriefingContent. Agent continues without compaction defense. |
| SessionRegistry Mutex poisoned | Use `.unwrap_or_else(\|e\| e.into_inner())` pattern (consistent with CategoryAllowlist in vnc-004). |
| Token budget math error | MAX_COMPACTION_BYTES is a hard ceiling. Even if category allocation is wrong, total output never exceeds the max. |
| CompactPayload for unknown session | Fallback path used. Logged at debug level. |
| ContextSearch without session_id | No injection tracking. No error. Response unaffected. |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (In-memory state loss on restart) | R-04 (fallback returns empty) | Fallback path queries knowledge base directly; designed and tested independently. |
| SR-02 (col-007 still in implementation) | R-05 (injection tracking fails), R-07 (CoAccessDedup behavior) | SessionRegistry wraps CoAccessDedup additively; injection tracking is an extension, not modification. |
| SR-03 (Token budget theoretical) | R-03 (budget overflow/invalid UTF-8) | Named constants, priority allocation with hard ceiling. Tunable post-delivery. |
| SR-04 (ContextSearch session_id change) | R-09 (wire backward incompatibility) | `#[serde(default)]` ensures backward compatibility. Existing hooks without session_id continue working. |
| SR-05 (SessionRegistry replaces CoAccessDedup) | R-07 (CoAccessDedup behavior changes) | Replicate col-007's CoAccessDedup tests exactly against SessionRegistry methods. |
| SR-06 (No disk compaction cache) | — | Accepted. Server unavailability is rare during active sessions. Fallback covers restart case. |
| SR-07 (UDS listener parameter growth) | — | Mechanical — one additional Arc<SessionRegistry> parameter. Consistent with col-007 pattern. |
| SR-08 (Entry status change between injection and compaction) | R-02 (stale entries) | Status checked at CompactPayload time via entry_store.get(). Quarantined excluded, deprecated included. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| High | 4 | 17 scenarios (R-03, R-05, R-10, R-12) |
| Medium | 6 | 22 scenarios (R-01, R-02, R-04, R-06, R-07, R-11) |
| Low | 2 | 8 scenarios (R-08, R-09) |
| **Total** | **12** | **47 scenarios** |
