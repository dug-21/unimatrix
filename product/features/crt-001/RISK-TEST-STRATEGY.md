# Risk-Based Test Strategy: crt-001

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Schema migration v1->v2 corrupts or loses entries during scan-and-rewrite | High | Medium | Critical |
| R-02 | access_count/helpful_count update race: concurrent write transaction fails, leaving partial counter state | High | Medium | Critical |
| R-03 | Dedup bypass: UsageDedup filter_access/filter_votes returns incorrect results, allowing count inflation | High | Medium | Critical |
| R-04 | FEATURE_ENTRIES orphan writes: feature entries written for entries that failed retrieval or don't exist | Medium | Low | Medium |
| R-05 | EntryStore trait object safety broken by record_access signature | High | Low | High |
| R-06 | bincode positional encoding mismatch: helpful_count/unhelpful_count appended in wrong order or position | High | Low | High |
| R-07 | last_accessed_at stale: dedup incorrectly blocks timestamp update, or timestamp not set for some code paths | Medium | Medium | High |
| R-08 | AuditLog::write_count_since scan correctness: reverse iteration misses events or counts non-write events | Medium | Medium | High |
| R-09 | Fire-and-forget masking: usage recording silently fails for all requests, counters permanently stuck at 0 | Medium | Medium | High |
| R-10 | context_briefing double-counting: entries appearing in both lookup and search get usage recorded twice | Medium | Medium | High |
| R-11 | Tool parameter backward compatibility: existing calls without feature/helpful parameters break | Medium | Low | Medium |
| R-12 | Migration idempotency: v1->v2 migration re-runs on already-migrated database, corrupting data | Medium | Low | Medium |
| R-13 | Dedup memory growth: long-running session with many unique (agent, entry) pairs exhausts memory | Low | Low | Low |
| R-14 | record_usage partial batch: some entries in the batch fail to update while others succeed | Medium | Low | Medium |
| R-15 | deserialize_audit_event visibility: promoting from cfg(test) to pub(crate) breaks encapsulation or existing tests | Low | Low | Low |
| R-16 | Vote correction atomicity: decrement of old counter and increment of new counter happen non-atomically, leaving inconsistent state | Medium | Medium | High |
| R-17 | FEATURE_ENTRIES trust-level bypass: Restricted agent's feature param is not correctly filtered, allowing analytics pollution | Medium | Low | Medium |

## Risk-to-Scenario Mapping

### R-01: Schema Migration Corruption (Critical)

**Severity**: High
**Likelihood**: Medium
**Impact**: If the v1->v2 migration fails mid-scan or writes incorrect data, entries become unreadable or have wrong field values. This is the highest-severity risk because it affects all existing data.

**Test Scenarios**:
1. Create a v1 database with 10 entries (various statuses), open with crt-001 code, verify all 10 entries are readable with original field values preserved AND new fields (helpful_count=0, unhelpful_count=0) present
2. Create a v1 database with entries containing non-zero access_count, supersedes, correction_count, security fields -- verify all are preserved through migration
3. Create a v1 database with Unicode content -- verify content and content_hash survive migration
4. Create a v1 database with empty strings in all string fields -- verify migration handles edge cases
5. Create a v0 database (pre-nxs-004) -- verify both migrations run sequentially (v0->v1->v2) and result is correct
6. Open an already-migrated v2 database -- verify migration is a no-op (schema_version already 2)
7. Verify schema_version counter is set to 2 after migration
8. Verify counters (total_active, total_deprecated, total_proposed, next_entry_id) are preserved

**Coverage Requirement**: All 8 scenarios. Scenario 5 validates the migration chain. Scenario 6 validates idempotency (R-12).

### R-02: Counter Update Atomicity (Critical)

**Severity**: High
**Likelihood**: Medium
**Impact**: If `record_usage` fails mid-batch, some entries might have updated access_count while others don't. Worse, an entry's access_count could be incremented without last_accessed_at being set, or helpful_count incremented without the corresponding ENTRIES write completing.

**Test Scenarios**:
1. Call record_usage with 5 entry IDs as both all_ids and access_ids. Verify all 5 entries have access_count=1 and last_accessed_at > 0
2. Call record_usage with overlapping sets: all_ids=[1,2,3], access_ids=[1,2], helpful_ids=[2,3], unhelpful_ids=[]. Verify correct per-entry field values
3. Call record_usage with a non-existent entry_id in the batch. Verify the method handles gracefully (skip missing entries, don't fail the whole batch)
4. Call record_usage with empty all_ids. Verify no transaction is opened (optimization)
5. Call record_usage multiple times for the same entries. Verify access_count increments cumulatively (store has no dedup -- that's the server's job)
6. After record_usage, verify the entry's other fields (title, content, topic, tags, content_hash, version) are unchanged

**Coverage Requirement**: All 6 scenarios. Scenario 3 is critical for graceful degradation when entries are deleted between retrieval and usage recording.

### R-03: Dedup Bypass (Critical)

**Severity**: High
**Likelihood**: Medium
**Impact**: If dedup doesn't work correctly, access_count and helpful_count are trivially gameable by repeated retrieval. This feeds directly into crt-002's confidence formula, allowing an attacker to boost any entry.

**Test Scenarios**:
1. filter_access returns entry_ids on first call; returns empty on second call with same (agent_id, entry_id)
2. filter_access for different agent_ids on the same entry_id returns the entry_id each time (dedup is per-agent)
3. filter_votes returns entry_ids on first call; returns empty on second call
4. filter_votes for different agent_ids on the same entry_id returns the entry_id each time
5. filter_access and filter_votes are independent: counting access does not block voting, and vice versa
6. filter_access with a mix of new and already-counted entries returns only the new ones
7. Large batch (100 entries): verify all are correctly deduped on second call
8. Verify dedup state is not persisted (check redb tables after dedup operations -- no dedup-related tables exist)

**Coverage Requirement**: All 8 scenarios. Scenarios 1-4 are the core correctness checks. Scenario 8 validates the "not persisted" contract.

### R-04: FEATURE_ENTRIES Orphan Writes (Medium)

**Severity**: Medium
**Likelihood**: Low
**Impact**: If FEATURE_ENTRIES is written for entry IDs that don't exist in ENTRIES (e.g., entry deleted between retrieval and feature recording), the multimap contains dangling references. Not immediately harmful but could confuse downstream features (col-004) that join FEATURE_ENTRIES with ENTRIES.

**Test Scenarios**:
1. Call record_feature_entries with valid feature and entry IDs. Verify entries exist in multimap.
2. Call record_feature_entries with duplicate (feature, entry_id) pairs. Verify no duplicates in multimap (idempotency).
3. Call record_feature_entries with an entry_id that doesn't exist in ENTRIES. Verify it's still inserted (the multimap doesn't validate entry existence -- it's a lightweight link).
4. Call record_feature_entries with empty feature string. Verify it works (empty string is a valid redb key).
5. Query FEATURE_ENTRIES for a feature and verify all linked entry IDs are returned.

**Coverage Requirement**: All 5 scenarios.

### R-05: EntryStore Trait Object Safety (High)

**Severity**: High
**Likelihood**: Low
**Impact**: If record_access breaks object safety, `Arc<dyn EntryStore>` stops compiling, breaking the entire async wrapper chain. This is a compile-time error but would block delivery.

**Test Scenarios**:
1. Compile check: `fn _check(_: &dyn EntryStore) {}` -- must compile
2. Compile check: `fn _check(_: Arc<dyn EntryStore>) {}` -- must compile
3. Call record_access through the async wrapper: `async_entry_store.record_access(&[1, 2, 3]).await`

**Coverage Requirement**: All 3 scenarios. Scenarios 1-2 are compile-time; scenario 3 is runtime.

### R-06: bincode Positional Encoding Mismatch (High)

**Severity**: High
**Likelihood**: Low
**Impact**: If helpful_count or unhelpful_count are appended at the wrong position (e.g., before trust_source instead of after), all existing entries become unreadable after migration, and new entries serialize incorrectly.

**Test Scenarios**:
1. Serialize an EntryRecord with helpful_count=42 and unhelpful_count=7. Deserialize and verify values.
2. Serialize with all default fields. Verify helpful_count=0 and unhelpful_count=0 after deserialize.
3. Serialize a record with ALL fields populated (no defaults). Verify full roundtrip.
4. Verify that a V1-serialized record (24 fields) cannot be deserialized as v2 EntryRecord (26 fields) -- must use V1EntryRecord.

**Coverage Requirement**: All 4 scenarios.

### R-07: last_accessed_at Staleness (High)

**Severity**: Medium
**Likelihood**: Medium
**Impact**: If last_accessed_at is not updated on some code paths (e.g., dedup incorrectly blocks it), the recency signal for crt-002 is stale. The SCOPE explicitly states last_accessed_at is ALWAYS updated, with no dedup.

**Test Scenarios**:
1. Retrieve entry, verify last_accessed_at > 0
2. Retrieve same entry again (deduped for access_count), verify last_accessed_at is updated to a new (or same-second) timestamp
3. Retrieve entry via context_search, context_lookup, context_get, context_briefing -- all four code paths update last_accessed_at
4. Verify last_accessed_at is set even when helpful is None (no vote)

**Coverage Requirement**: All 4 scenarios. Scenario 2 is the critical dedup-exception check.

### R-08: AuditLog write_count_since Correctness (High)

**Severity**: Medium
**Likelihood**: Medium
**Impact**: If the reverse scan counts wrong operations or misses events near the boundary, rate limiting infrastructure is unreliable.

**Test Scenarios**:
1. Log 5 write events and 5 read events for the same agent. write_count_since returns 5.
2. Log events for 3 different agents. write_count_since for agent A returns only agent A's writes.
3. Log events at known timestamps. Query with since=T returns only events after T.
4. Log 0 events. write_count_since returns 0.
5. Mix of context_store and context_correct -- both count as writes.
6. context_search, context_lookup, context_get, context_briefing, context_deprecate, context_status -- none count as writes.

**Coverage Requirement**: All 6 scenarios.

### R-09: Fire-and-Forget Masking (High)

**Severity**: Medium
**Likelihood**: Medium
**Impact**: If usage recording silently fails on every request (e.g., bug in record_usage that always errors), all counters stay at 0 and no one notices because errors are logged but not propagated. crt-002 gets useless data.

**Test Scenarios**:
1. After a successful retrieval, verify access_count > 0 on the returned entries (end-to-end integration test)
2. Simulate a Store::record_usage failure (e.g., closed database). Verify the tool still returns valid search results AND a warning is logged.
3. Verify that record_usage is actually called (not accidentally skipped) for each of the 4 retrieval tools.

**Coverage Requirement**: All 3 scenarios. Scenario 1 is the critical end-to-end check that usage recording actually works.

### R-10: context_briefing Double-Counting (High)

**Severity**: Medium
**Likelihood**: Medium
**Impact**: An entry that appears in both the lookup and search phases of context_briefing would have access_count incremented twice, violating the one-access-per-retrieval contract.

**Test Scenarios**:
1. Create an entry that matches both the role lookup and the task search. Call context_briefing. Verify access_count=1 (not 2).
2. Create entries that only match lookup and entries that only match search. Verify each has access_count=1.
3. Call context_briefing with helpful=true. Verify helpful_count=1 for the overlapping entry (not 2).

**Coverage Requirement**: All 3 scenarios.

### R-11: Tool Parameter Backward Compatibility (Medium)

**Severity**: Medium
**Likelihood**: Low
**Impact**: If adding feature/helpful parameters changes existing tool behavior, all current Unimatrix users' workflows break.

**Test Scenarios**:
1. Call context_search without feature or helpful params. Verify same results as before crt-001.
2. Call context_lookup without feature or helpful params. Verify same results.
3. Call context_get without feature or helpful params. Verify same results.
4. Call context_briefing without helpful param (feature already existed). Verify same results.
5. Verify JSON schema generated by schemars includes feature and helpful as optional.

**Coverage Requirement**: All 5 scenarios.

### R-14: record_usage Partial Batch (Medium)

**Severity**: Medium
**Likelihood**: Low
**Impact**: If an entry is deleted between the retrieval read and the usage write, record_usage might fail trying to read/update a non-existent entry.

**Test Scenarios**:
1. Call record_usage with entry IDs [1, 2, 3] where entry 2 has been deleted. Verify entries 1 and 3 are updated and 2 is skipped.
2. Call record_usage with all non-existent entry IDs. Verify no error (graceful no-op).
3. Verify transaction atomicity: if the transaction commits, all non-deleted entries are updated.

**Coverage Requirement**: All 3 scenarios.

### R-16: Vote Correction Atomicity (High)

**Severity**: Medium
**Likelihood**: Medium
**Impact**: When an agent corrects a vote (e.g., from unhelpful to helpful), the system must decrement `unhelpful_count` and increment `helpful_count` in the same write transaction. If these happen non-atomically (e.g., increment succeeds but decrement fails), the entry's vote totals become inconsistent -- total votes (helpful + unhelpful) would exceed total voting events, corrupting the Wilson score in crt-002.

**Test Scenarios**:
1. Agent votes helpful=false on entry 42, then votes helpful=true on the same entry in the same session. Verify helpful_count=1, unhelpful_count=0 (not helpful_count=1, unhelpful_count=1).
2. Agent votes helpful=true, then votes helpful=true again. Verify helpful_count=1 (no change on repeat same-value vote).
3. Agent votes helpful=true, then votes helpful=false. Verify helpful_count=0, unhelpful_count=1.
4. Correction on an entry where the counter being decremented is already 0 (edge case from cross-session state). Verify saturating subtraction (floor at 0), no underflow.
5. Batch correction: 5 entries, agent changes vote on 3 of them. Verify correct per-entry counters.

**Coverage Requirement**: All 5 scenarios. Scenario 1 is the critical correctness check. Scenario 4 validates the saturating subtraction guard.

### R-17: FEATURE_ENTRIES Trust-Level Bypass (Medium)

**Severity**: Medium
**Likelihood**: Low
**Impact**: If the trust-level gate is not correctly applied, a Restricted agent (read-only, auto-enrolled unknown) can write arbitrary feature associations to FEATURE_ENTRIES, polluting analytics data consumed by col-004 and col-002.

**Test Scenarios**:
1. Restricted agent calls context_search with feature="test-feature". Verify FEATURE_ENTRIES has no entries for "test-feature".
2. Internal agent calls context_search with feature="test-feature". Verify FEATURE_ENTRIES has entries for "test-feature".
3. Privileged agent calls context_search with feature="test-feature". Verify FEATURE_ENTRIES has entries.
4. Restricted agent's retrieval results are unchanged (same entries returned) regardless of feature parameter being ignored.

**Coverage Requirement**: All 4 scenarios.

## Integration Risks

### IR-01: Store-to-Server Field Alignment
The server creates EntryRecord objects in `insert_with_audit` and `correct_with_audit` (server.rs). These constructors must include the new `helpful_count` and `unhelpful_count` fields (initialized to 0). If the server doesn't include these fields, the compiler will catch it (struct literal exhaustiveness), but the risk is that a developer comments out a field with a default.

**Mitigation**: The compiler enforces struct literal completeness. No `..Default::default()` patterns are used in server.rs.

### IR-02: EntryStore Trait and StoreAdapter Consistency
The new `record_access` method must be implemented on both `Store` (directly, as part of `record_usage`) and `StoreAdapter` (delegating to `Store`). If the adapter delegates incorrectly, the async path produces wrong results.

**Mitigation**: StoreAdapter integration test that calls record_access through the async wrapper and verifies EntryRecord fields are updated.

### IR-03: UsageDedup Lifetime and Server State
UsageDedup is added to UnimatrixServer, which is `Clone` (required by rmcp). UsageDedup uses `Arc<Mutex<...>>` internally to ensure cloned servers share the same dedup state.

**Mitigation**: Test that cloned servers share dedup state (same Arc).

## Edge Cases

### EC-01: Empty Retrieval Results
A search that returns 0 results should not call record_usage or record_feature_entries. No crash, no empty-batch transactions.

### EC-02: Retrieval of a Single Entry
context_get returns exactly 1 entry. record_usage with a 1-element batch should work correctly.

### EC-03: Large Retrieval Batch
A lookup returning 100+ entries. record_usage must handle the batch in a single transaction without timeout or memory issues.

### EC-04: Empty Feature String
A tool call with `feature=""` should write to FEATURE_ENTRIES with empty string key. This is valid but unusual. Consider whether to skip empty strings (validation decision).

### EC-05: Helpful = false on First Retrieval
Agent's first retrieval includes `helpful=false`. Should increment unhelpful_count=1 and helpful_count remains 0. Access_count=1.

### EC-06: Vote After Access-Only Retrieval
First retrieval has no helpful param (access only). Second retrieval has helpful=true. The vote should register (vote_counted set is separate from access_counted).

### EC-07: Multiple Agents Same Entry
Agent A and Agent B both retrieve entry 42. Both should get access_count incremented (dedup is per-agent, not per-entry).

### EC-08: Vote Correction Across Multiple Retrievals
Agent retrieves entry 42 with helpful=false. Later retrieves entry 42 with helpful=true. The correction should apply: unhelpful_count decremented (saturating at 0), helpful_count incremented. Net result: helpful_count=1, unhelpful_count=0.

### EC-09: Cross-Session Vote (No Correction)
Agent votes helpful=false in session 1. Server restarts. Agent votes helpful=true in session 2. Both votes count independently (no correction across sessions). Result: helpful_count=1, unhelpful_count=1. This is by design -- cross-session votes are independent observations.

### EC-10: Restricted Agent Feature Parameter
Restricted agent calls context_search with feature="crt-001". The feature parameter is silently ignored -- no FEATURE_ENTRIES written. The retrieval results are unchanged.

## Security Risks

### SR-01: Read-Path Side Effects
crt-001 introduces write side effects on read operations. A Restricted agent (Read + Search only) now causes writes (access_count, last_accessed_at, helpful_count) as a side effect of reading. These writes are analytics (deduped counters), not knowledge mutations. FEATURE_ENTRIES writes are gated by trust level (Internal+ only), so Restricted agents cannot create feature associations. The trust model accepts the remaining side effects: deduped counters have bounded impact per session.

**Assessment**: Low severity. Dedup limits the blast radius. A Restricted agent can increment access_count by at most 1 per entry per session. FEATURE_ENTRIES is fully gated.

### SR-02: Helpful Flag Abuse (Boosting)
A malicious agent could set `helpful=true` on every retrieval to boost entries. Session dedup (one vote per agent per entry per session) limits this to +1 per session. Across multiple sessions, this is further mitigated by crt-002's Wilson score (requires both helpful and unhelpful signals for statistical significance).

**Assessment**: Low severity post-dedup. The 3-layer gaming resistance strategy handles this.

### SR-05: Unhelpful Flag Abuse (Active Suppression)
A malicious agent could set `helpful=false` on every retrieval to suppress entries by driving down Wilson scores. Unlike boosting (which targets specific entries), suppression can be sprayed broadly -- a single broad search with `helpful=false` votes unhelpful on all returned entries simultaneously. Session dedup limits this to one vote per entry per session, and vote correction (last-vote-wins) means a legitimate agent can override an early incorrect negative vote.

**Mitigation layers:**
- Layer 1 (crt-001): Session dedup limits to 1 negative vote per entry per session per agent. Vote correction allows recovery.
- Layer 2 (crt-002): Wilson score requires a minimum sample size before the helpfulness factor deviates significantly from neutral (0.5). A few negative votes do not meaningfully move the score. crt-002 should also consider requiring a minimum observation count (e.g., n >= 5) before helpfulness departs from the neutral prior.
- Layer 3 (future): Anomaly detection can flag agents that systematically vote unhelpful on all entries.

**Assessment**: Medium severity. More impactful than boosting because it degrades the entire knowledge base rather than targeting specific entries. Mitigated by Wilson score statistics but crt-002 must implement minimum-sample-size guards. See PRODUCT-VISION.md crt-002 note.

### SR-03: Feature Parameter Injection
The `feature` parameter is an opaque string written to FEATURE_ENTRIES. Only agents with Internal or higher trust level can trigger FEATURE_ENTRIES writes (Restricted agents' feature params are silently ignored). For authorized agents, input validation (max length, no control characters) should be applied.

**Assessment**: Low severity. Trust-level gating prevents unauthorized writes. Standard input validation mitigates injection for authorized agents.

### SR-04: write_count_since Information Disclosure
The `write_count_since` method returns write counts for any agent_id. If exposed to arbitrary callers, it leaks information about other agents' activity. Currently, this method is called server-internally for rate limiting, not exposed as a tool parameter.

**Assessment**: Not applicable for crt-001 (method is internal). If exposed in a future tool, it needs capability checks.

## Failure Modes

### FM-01: Store Write Transaction Failure
If the write transaction for record_usage fails (disk full, database locked), the retrieval result is still returned to the agent (fire-and-forget per ADR-004). Usage counters are not updated for this request. A tracing::warn! is emitted.

### FM-02: Migration Failure on Store::open()
If v1->v2 migration fails mid-scan (e.g., corrupted entry), Store::open() fails and the server does not start. This is the correct behavior -- a corrupted database should not silently continue.

### FM-03: UsageDedup Mutex Poison
If the thread holding the Mutex panics, the Mutex becomes poisoned. Subsequent calls to filter_access/filter_votes will fail. Since this is in the usage recording path (fire-and-forget), the retrieval still succeeds. The dedup set is effectively disabled until server restart.

### FM-04: AuditLog write_count_since with No Events
If the AUDIT_LOG is empty, write_count_since returns 0. This is correct -- no events means no writes.

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 3 (R-01, R-02, R-03) | 22 scenarios |
| High | 6 (R-05, R-06, R-07, R-08, R-09, R-10, R-16) | 27 scenarios |
| Medium | 5 (R-04, R-11, R-12, R-14, R-17) | 17 scenarios |
| Low | 2 (R-13, R-15) | 0 explicit scenarios (monitored) |
| **Total** | **16** | **66 scenarios** |
