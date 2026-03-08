# crt-011: Risk-Test Strategy — Confidence Signal Integrity

## Risk Register

### R-01: Three-Pass Race in run_confidence_consumer (SR-02)

**Severity:** MEDIUM
**Likelihood:** LOW (single-threaded consumer calls)
**Impact:** Double-counted success_session_count for entries added between passes

**Description:** The three-pass structure (lock → unlock for fetch → lock) in `run_confidence_consumer` means `PendingEntriesAnalysis` can be modified between Pass 1 and Pass 3. If another consumer or handler adds an entry for the same `(session_id, entry_id)` between passes, the third pass could increment again.

**Mitigation:** The dedup HashSet persists across all three passes. Pass 1 inserts all `(session_id, entry_id)` pairs it processes. Pass 3 checks the same HashSet before incrementing. Even if the entry was added between passes, the HashSet prevents double-counting.

**Test:** T-CON-01 (same session, overlapping entries), T-CON-02 (different sessions).

### R-02: Integration Test Insufficiency (SR-03)

**Severity:** MEDIUM
**Likelihood:** LOW
**Impact:** Handler wiring bugs undetected

**Description:** Tests at `UsageService` and `UnimatrixServer` level do not exercise MCP transport-layer dispatch. A bug in tool parameter routing or JSON-RPC deserialization would not be caught.

**Mitigation:** Accepted risk. Existing param deserialization tests cover JSON → struct mapping. The service layer is where business logic lives. MCP transport tests are disproportionately expensive for the risk level.

**Test:** T-INT-01 through T-INT-04.

### R-03: Semantic Confusion Between rework Counters (SR-04)

**Severity:** LOW
**Likelihood:** MEDIUM (future contributors)
**Impact:** Future code changes might incorrectly dedup rework_flag_count or fail to dedup rework_session_count

**Description:** Both counters are incremented in the same loop with different dedup behavior. Without clear documentation, a future contributor might "fix" rework_flag_count by adding dedup, or remove dedup from rework_session_count.

**Mitigation:** ADR-002 documents the decision. Code comments at the increment site explain the distinction. Test T-CON-04 explicitly verifies that rework_flag_count is NOT deduped.

**Test:** T-CON-04.

### R-04: Signal Queue Backlog Amplifies Bug Impact

**Severity:** LOW
**Likelihood:** LOW (requires server downtime + busy sessions)
**Impact:** Larger over-counting if many signals accumulate before drain

**Description:** If the signal queue accumulates many records (up to the 10,000 cap) before a drain cycle runs, the over-counting bug would be amplified. After the fix, this scenario is handled correctly, but it's worth testing.

**Test:** T-CON-02 (tests multiple sessions with overlapping entries).

## Scope Risk Traceability

| Scope Risk | Architecture Response | Test Coverage |
|-----------|----------------------|---------------|
| SR-01 (String cloning) | Accepted — bounded by 10K cap | No dedicated test needed |
| SR-02 (Three-pass race) | HashSet persists across passes (ADR-001) | T-CON-01, T-CON-02 |
| SR-03 (Test setup complexity) | Use existing make_server() + UsageService (ADR-003) | T-INT-01..04 |
| SR-04 (Semantic ambiguity) | ADR-002 + code comments | T-CON-04 |

## Test Strategy Summary

### Unit Tests (Consumer Dedup)

| ID | Target | Scenario | Assertion |
|----|--------|----------|-----------|
| T-CON-01 | run_confidence_consumer | 2 signals, same session_id, overlapping entry_ids | success_session_count = 1 per entry |
| T-CON-02 | run_confidence_consumer | 2 signals, different session_ids, overlapping entry_ids | success_session_count = 2 per entry |
| T-CON-03 | run_retrospective_consumer | 2 signals, same session_id, overlapping entry_ids | rework_session_count = 1 per entry |
| T-CON-04 | run_retrospective_consumer | 2 signals, same session_id, same entry_id | rework_flag_count = 2 (no dedup) |

### Integration Tests (Handler-Service-Store)

| ID | Target | Scenario | Assertion |
|----|--------|----------|-----------|
| T-INT-01 | UsageService::record_usage_for_entries_mcp | Insert entry, record usage | confidence > 0, access_count = 1 |
| T-INT-02 | UsageService::record_usage_for_entries_mcp | Same agent+entry twice | access_count = 1 (dedup) |
| T-INT-03 | UnimatrixServer::record_usage_for_entries | Insert entry, record usage | access_count + confidence updated |
| T-INT-04 | UnimatrixServer::record_usage_for_entries | Two calls same agent+entry | access_count stays 1 |

### Regression

All existing tests in unimatrix-server, unimatrix-store, unimatrix-engine, unimatrix-observe must pass unchanged.

## Risk Summary

| ID | Risk | Severity | Covered By |
|----|------|----------|------------|
| R-01 | Three-pass race | MEDIUM | T-CON-01, T-CON-02, ADR-001 |
| R-02 | Integration test gap | MEDIUM | T-INT-01..04, ADR-003 |
| R-03 | Semantic confusion | LOW | T-CON-04, ADR-002, code comments |
| R-04 | Queue backlog | LOW | T-CON-02 |

**Top 3 by severity:**
1. R-01: Three-pass race (MEDIUM) — mitigated by HashSet lifecycle
2. R-02: Integration test gap (MEDIUM) — accepted, covered at service level
3. R-03: Semantic confusion (LOW) — mitigated by ADR + tests
