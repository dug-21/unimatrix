# Risk-Based Test Strategy: crt-003

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Exhaustive match regression: adding Quarantined variant breaks unhandled match arms at compile time, but runtime default arms in tests/helpers may silently produce wrong behavior | High | Med | High |
| R-02 | Quarantine status leak in retrieval: search, lookup, or briefing returns quarantined entries to agents | High | Med | High |
| R-03 | Counter desync on quarantine/restore: COUNTERS table values drift from actual entry counts after repeated quarantine/restore cycles | Med | Med | Med |
| R-04 | Conflict heuristic false positives: complementary entries flagged as contradictions, causing operator alert fatigue | Med | High | Med |
| R-05 | Conflict heuristic false negatives: real contradictions with subtle language patterns missed by rule-based detection | Med | High | Med |
| R-06 | Contradiction scan performance: scanning 2000+ entries exceeds 30-second budget due to embedding + HNSW overhead | Med | Med | Med |
| R-07 | Embedding consistency false positive: deterministic model produces slightly different embeddings across runs due to floating-point non-determinism | Med | Low | Low |
| R-08 | Quarantine/restore confidence drift: repeated quarantine/restore cycles cause confidence to settle at unexpected values | Low | Med | Low |
| R-09 | Idempotency violation: quarantining already-quarantined entry modifies counters or audit log | Med | Low | Low |
| R-10 | STATUS_INDEX orphan entries: quarantine/restore fails mid-transaction, leaving STATUS_INDEX inconsistent with ENTRIES | High | Low | Med |
| R-11 | Contradiction dedup failure: same pair reported as both (A,B) and (B,A) | Low | Med | Low |
| R-12 | context_correct on quarantined entry: correction attempt on quarantined entry succeeds, creating inconsistent correction chain | Med | Med | Med |

## Risk-to-Scenario Mapping

### R-01: Exhaustive Match Regression
**Severity**: High
**Likelihood**: Med
**Impact**: Compiler catches most issues, but runtime code with `_ =>` default arms may silently produce wrong behavior for Quarantined entries.

**Test Scenarios**:
1. Compile-time verification: build succeeds with all match arms covered
2. `status_counter_key(Quarantined)` returns `"total_quarantined"`
3. `TryFrom::<u8>::try_from(3)` returns `Ok(Quarantined)`
4. `Display` for Quarantined returns `"Quarantined"`
5. `parse_status("quarantined")` returns `Ok(Quarantined)`
6. `status_to_str(Quarantined)` returns `"quarantined"`
7. `base_score(Quarantined)` returns `0.1`

**Coverage Requirement**: Every production match/if-let on Status must handle Quarantined correctly.

### R-02: Quarantine Status Leak in Retrieval
**Severity**: High
**Likelihood**: Med
**Impact**: Quarantined (potentially poisoned) entries returned to agents, defeating the purpose of quarantine.

**Test Scenarios**:
1. Insert entry, quarantine it, run context_search with a query that would match -- verify excluded
2. Insert entry, quarantine it, run context_lookup with default status -- verify excluded
3. Insert entry, quarantine it, run context_lookup with status="quarantined" -- verify included
4. Insert entry, quarantine it, run context_briefing -- verify excluded from all sections
5. Insert entry, quarantine it, run context_get by ID -- verify returned (forensic access)
6. Insert 10 entries, quarantine 3, run context_search -- verify only 7 in results

**Coverage Requirement**: All four retrieval tools tested with quarantined entries present.

### R-03: Counter Desync on Quarantine/Restore
**Severity**: Med
**Likelihood**: Med
**Impact**: `context_status` reports incorrect entry counts, misleading operators.

**Test Scenarios**:
1. Insert entry (Active), verify total_active=1, total_quarantined=0
2. Quarantine it, verify total_active=0, total_quarantined=1
3. Restore it, verify total_active=1, total_quarantined=0
4. Quarantine and restore 10 times, verify counters are correct after each cycle
5. Insert 5 entries, quarantine 2, deprecate 1, verify all counters consistent

**Coverage Requirement**: Counter arithmetic verified after every status transition combination.

### R-04: Conflict Heuristic False Positives
**Severity**: Med
**Likelihood**: High
**Impact**: Operators waste time reviewing non-contradictions.

**Test Scenarios**:
1. Two entries about the same topic but complementary advice -- should NOT be flagged at default sensitivity
2. "Use X for case A" and "Use Y for case B" (different contexts) -- should NOT be flagged
3. "Use X" and "X is a good choice" (agreement) -- should NOT be flagged
4. Sensitivity=0.9 (very sensitive) -- more pairs flagged than at 0.5
5. Sensitivity=0.1 (very specific) -- fewer pairs flagged than at 0.5

**Coverage Requirement**: At least 3 false-positive scenarios verified as not-flagged at default sensitivity.

### R-05: Conflict Heuristic False Negatives
**Severity**: Med
**Likelihood**: High
**Impact**: Real contradictions remain undetected in the knowledge base.

**Test Scenarios**:
1. "Use serde for config" vs "Avoid serde for config" -- must be flagged (negation opposition)
2. "Always enable X" vs "Never enable X" -- must be flagged
3. "Use library A for HTTP" vs "Use library B for HTTP" -- must be flagged (incompatible directives)
4. "X is recommended" vs "X is an anti-pattern" -- should be flagged (opposing sentiment)

**Coverage Requirement**: All three conflict signal types verified with at least one true-positive case each.

### R-06: Contradiction Scan Performance
**Severity**: Med
**Likelihood**: Med
**Impact**: `context_status` times out or blocks the server for too long.

**Test Scenarios**:
1. Scan with 0 active entries -- returns immediately
2. Scan with 1 active entry -- returns empty (no pairs possible)
3. Scan with 100 entries -- completes within budget
4. (Benchmark, not gated) Scan with 2000 entries -- measure wall time

**Coverage Requirement**: Basic performance sanity checks; explicit budget test at 100 entries.

### R-07: Embedding Consistency False Positive
**Severity**: Med
**Likelihood**: Low
**Impact**: Valid entries flagged as inconsistent due to floating-point drift.

**Test Scenarios**:
1. Insert entry, immediately run consistency check -- should NOT be flagged (same model, same input)
2. Verify the consistency threshold (0.99) accommodates typical f32 precision variance

**Coverage Requirement**: One round-trip consistency test with known-good entry.

### R-08: Quarantine/Restore Confidence Drift
**Severity**: Low
**Likelihood**: Med
**Impact**: Confidence not fully recovering after restore.

**Test Scenarios**:
1. Record initial confidence of Active entry
2. Quarantine -- verify confidence drops (base_score 0.1)
3. Restore -- verify confidence returns to approximately the original value

**Coverage Requirement**: Confidence recomputation verified after quarantine and restore.

### R-09: Idempotency Violation
**Severity**: Med
**Likelihood**: Low
**Impact**: Double-quarantine inflates audit log or skews counters.

**Test Scenarios**:
1. Quarantine entry, quarantine again -- counters unchanged, single audit event for second call (or none)
2. Verify total_quarantined=1 after two quarantine calls on same entry

**Coverage Requirement**: Counter and audit verification after duplicate operations.

### R-10: STATUS_INDEX Orphan Entries
**Severity**: High
**Likelihood**: Low
**Impact**: Index inconsistency causes retrieval errors or missed entries.

**Test Scenarios**:
1. After quarantine: verify old STATUS_INDEX entry removed, new one added
2. After restore: verify quarantine STATUS_INDEX entry removed, active one added
3. Read-back verification after each transition

**Coverage Requirement**: STATUS_INDEX state verified after every status transition.

### R-11: Contradiction Dedup Failure
**Severity**: Low
**Likelihood**: Med
**Impact**: Same contradiction reported twice, cluttering the report.

**Test Scenarios**:
1. Two contradictory entries A and B -- verify exactly one pair in results
2. Verify pair ordering: entry_id_a < entry_id_b (canonical order)

**Coverage Requirement**: Dedup tested with known contradictory pair.

### R-12: context_correct on Quarantined Entry
**Severity**: Med
**Likelihood**: Med
**Impact**: Correction chain includes a quarantined entry, potentially propagating suspicious content.

**Test Scenarios**:
1. Insert entry, quarantine it, attempt context_correct -- should return error
2. Restore entry, then context_correct -- should succeed
3. Verify error message is clear about quarantine state

**Coverage Requirement**: Correction rejection verified for quarantined entries.

## Integration Risks

1. **crt-002 confidence integration**: `base_score()` exhaustive match must handle Quarantined. `update_confidence()` must be called after quarantine/restore transitions. Confidence formula weights are unchanged.

2. **QueryFilter default status**: The default `Some(Status::Active)` in `query.rs` already excludes non-Active entries. Quarantined entries are naturally excluded. But explicit status filtering in `parse_status` must accept "quarantined" as a valid value.

3. **HNSW index integrity**: Quarantined entries remain in the HNSW index. Search returns them, then the server filters them out. This means the effective top-k may be reduced (e.g., top-5 search with 2 quarantined results yields 3 visible results). The search should over-fetch to compensate, or this is accepted as a known trade-off.

4. **Audit log continuity**: Quarantine/restore audit events must use the same monotonic ID sequence as all other events. The existing `audit.log_event_in_txn()` pattern handles this.

## Edge Cases

1. **Zero active entries**: Contradiction scan returns empty results immediately
2. **Single active entry**: No pairs possible, scan returns empty
3. **All entries quarantined**: All retrieval tools return empty results; status report shows counts
4. **Entry quarantined between search and response**: Race condition in async context; tolerable because the status filter happens after the write transaction commits
5. **Embed service not ready**: Both scans silently skipped; report has counts only
6. **Entry with empty content**: Embedding is generated from title only; heuristic may produce unexpected results on empty content
7. **Self-match in HNSW**: When searching with an entry's own embedding, the entry itself should be the top-1 result. Filter self-matches from contradiction pairs.
8. **Very high similarity (>0.99) but no conflict**: Duplicate entries, not contradictions. The conflict heuristic should score low for duplicates (same content = no negation patterns).

## Security Risks

1. **Quarantine tool as denial-of-service**: An Admin-level agent could quarantine all entries, effectively disabling the knowledge base. Mitigated by: Admin-only capability (highest trust level), audit logging of every quarantine action, easy restore mechanism.

2. **Contradiction scan as information disclosure**: The scan reveals which entries are semantically similar, which could expose relationships not visible through normal retrieval. Mitigated by: Admin-only `context_status` capability.

3. **Conflict heuristic evasion**: An attacker could craft semantically poisoned entries that avoid directive language, bypassing the heuristic. Mitigated by: embedding consistency checks (detect relevance hijacking), and the heuristic is a defense layer not a sole defense.

4. **Quarantine/restore cycling for audit flooding**: An attacker could rapidly quarantine/restore to flood the audit log. Mitigated by: Admin-only access, and audit log is append-only with minimal per-event cost.

## Failure Modes

1. **Embed service not ready during context_status**: Graceful degradation. Status report includes counts and index distributions but no contradiction or embedding sections. `contradiction_scan_performed = false`.

2. **HNSW search error during scan**: Individual entry scan failures are logged and skipped. The scan continues with remaining entries. Partial results are returned.

3. **Quarantine write transaction failure**: Entry status unchanged. Error returned to caller. No partial state change (atomic transaction).

4. **Re-embedding failure during scan**: Individual entry failures are logged and skipped. The scan continues. Entries that failed re-embedding are not included in results (neither contradiction nor consistency).

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (exhaustive match breakage) | R-01 | Exhaustive match sites cataloged in ARCHITECTURE.md; compile-time enforcement + runtime tests for each site |
| SR-02 (heuristic false positive rate) | R-04, R-05 | Tunable sensitivity threshold (ADR-003); scored output with configurable cutoff |
| SR-03 (HNSW stored embedding retrieval) | -- | Resolved by ADR-002: re-embed from text instead of retrieving stored embeddings |
| SR-04 (scope creep into automated quarantine) | -- | SCOPE.md non-goal enforced; quarantine is manual/Admin only |
| SR-05 (context_status latency) | R-06 | Graceful degradation when embed service unavailable; embedding consistency check is opt-in |
| SR-06 (quarantine vs deprecated confusion) | -- | Distinct status values, distinct tool (context_quarantine vs context_deprecate), clear error messages |
| SR-07 (crt-002 confidence formula) | R-01, R-08 | ADR-001: Quarantined base_score = 0.1; confidence recomputed after transitions |
| SR-08 (QueryFilter backward compatibility) | R-02 | Default behavior unchanged (Active only); "quarantined" added as valid parse_status value |
| SR-09 (quarantined entries in HNSW) | R-02, integration risk #3 | Accepted: post-search filter. Quarantined entries remain in HNSW; filtering is cheap. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| High | 3 (R-01, R-02, R-10) | 16 scenarios |
| Medium | 5 (R-03, R-04, R-05, R-06, R-12) | 18 scenarios |
| Low | 4 (R-07, R-08, R-09, R-11) | 6 scenarios |
| **Total** | **12** | **40 scenarios** |
