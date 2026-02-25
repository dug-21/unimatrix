# Acceptance Map: crt-003

## AC-to-Component Tracing

| AC | Description | Component | Spec Ref | Risk Ref | Test Type |
|----|-------------|-----------|----------|----------|-----------|
| AC-01 | Quarantined variant with repr(u8)=3, TryFrom, Display, counter key | C1 | FR-01a-d | R-01 | Unit |
| AC-02 | context_quarantine params: id, reason, action, agent_id, format; Admin capability | C3 | FR-02a-c | -- | Integration |
| AC-03 | Quarantine: atomic status transition + audit | C3 | FR-02d,f | R-10 | Integration |
| AC-04 | Restore: atomic status transition + audit | C3 | FR-02e,f | R-10 | Integration |
| AC-05 | Quarantine already-quarantined is idempotent | C3 | FR-02d | R-09 | Integration |
| AC-06 | Restore non-quarantined entry returns error | C3 | FR-02e | -- | Integration |
| AC-07 | context_search excludes Quarantined | C2 | FR-03a | R-02 | Integration |
| AC-08 | context_lookup excludes Quarantined (default) | C2 | FR-03b | R-02 | Integration |
| AC-09 | context_briefing excludes Quarantined | C2 | FR-03c | R-02 | Integration |
| AC-10 | context_get returns Quarantined entries | C2 | FR-03d | R-02 | Integration |
| AC-11 | Contradiction scan finds high-similarity conflicting pairs | C4 | FR-04a,b | R-05 | Integration |
| AC-12 | Contradiction pairs deduplicated | C4 | FR-04c | R-11 | Unit |
| AC-13 | Conflict heuristic: negation + directives, tunable sensitivity | C4 | FR-04d,e | R-04,R-05 | Unit |
| AC-14 | context_status includes total_quarantined | C5 | FR-06c | R-03 | Integration |
| AC-15 | context_status includes contradictions (default ON) | C5 | FR-06d | -- | Integration |
| AC-16 | Embedding consistency check: re-embed + compare | C4 | FR-05a-c | R-07 | Integration |
| AC-17 | Embedding consistency in context_status (check_embeddings=true) | C5 | FR-06e | -- | Integration |
| AC-18 | Scan uses HNSW search (not brute-force) | C4 | FR-04b | R-06 | Unit |
| AC-19 | StatusReport extended with new fields | C5 | FR-06a | -- | Unit |
| AC-20 | Unit + integration tests; CI passes | All | -- | -- | All |
| AC-21 | Existing tests pass | All | NFR-02 | R-01 | Regression |
| AC-22 | context_quarantine: non-existent ID returns error | C3 | FR-02d,e | -- | Integration |
| AC-23 | Confidence recomputed after quarantine/restore | C3 | FR-02d,e | R-08 | Integration |
| AC-24 | All tools support format param (summary/markdown/json) | C3,C5 | FR-07 | -- | Integration |

## Component Coverage Summary

| Component | AC Count | Unit Tests | Integration Tests |
|-----------|----------|------------|------------------|
| C1: status-extension | 1 (AC-01) | 7 | 0 |
| C2: retrieval-filtering | 4 (AC-07..AC-10) | 0 | 6 |
| C3: quarantine-tool | 6 (AC-02..AC-06, AC-22, AC-23) | 0 | 10 |
| C4: contradiction-detection | 5 (AC-11..AC-13, AC-16, AC-18) | 6 | 3 |
| C5: status-report-extension | 4 (AC-14,AC-15,AC-17,AC-19,AC-24) | 2 | 4 |
| Cross-cutting | 2 (AC-20,AC-21) | -- | -- |

## Risk Coverage

| Risk | Covering ACs | Test Count |
|------|-------------|------------|
| R-01 (match regression) | AC-01, AC-21 | 7+ |
| R-02 (quarantine leak) | AC-07, AC-08, AC-09, AC-10 | 6 |
| R-03 (counter desync) | AC-14, AC-03, AC-04 | 5 |
| R-04 (false positives) | AC-13 | 5 |
| R-05 (false negatives) | AC-11, AC-13 | 4 |
| R-06 (scan performance) | AC-18 | 3 |
| R-07 (embed consistency FP) | AC-16 | 2 |
| R-08 (confidence drift) | AC-23 | 3 |
| R-09 (idempotency) | AC-05 | 2 |
| R-10 (STATUS_INDEX orphans) | AC-03, AC-04 | 4 |
| R-11 (dedup failure) | AC-12 | 2 |
| R-12 (correct quarantined) | AC-07 (+ additional test) | 1 |
