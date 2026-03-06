# Gate 3c Report: Risk-Based Validation

**Feature:** crt-010 (Status-Aware Retrieval)
**Result:** PASS
**Date:** 2026-03-06

## Validation Summary

### 1. Test Results Prove Risk Mitigation — PASS

All 12 risks from RISK-TEST-STRATEGY.md have test evidence:

| Risk | Severity | Evidence | Mitigated? |
|------|----------|----------|------------|
| R-01 (get_embedding API) | Critical | VectorIndex::get_embedding implemented via HNSW layer 0 iteration; 104 vector tests pass; 8 cosine_similarity unit tests | YES |
| R-02 (Penalty ranking) | High | Named constants (0.7, 0.5) with 4 dedicated tests; ranking invariant AC-02/AC-03 verified | YES |
| R-03 (Strict empty results) | High | Strict mode retains only Active non-superseded; empty vec returned safely | YES |
| R-04 (Latency) | High | Stored embedding cosine avoids ONNX inference; batch fetch; AC-16 deferred to manual benchmark | PARTIAL (by design) |
| R-05 (Dangling supersession) | Med | entry_store.get error -> continue (line 243); no panic path | YES |
| R-06 (Co-access signature) | High | deprecated_ids HashSet parameter on all 3 functions; all callers updated; 774 server tests pass | YES |
| R-07 (Explicit status + injection) | Med | explicit_status_filter disables both penalties (line 200) and injection (line 217) | YES |
| R-08 (Post-compaction) | Resolved | col-013 existing behavior; verification only | YES |
| R-09 (Race condition) | Med | get_embedding returns None gracefully; injection skipped | YES |
| R-10 (Briefing over-filtering) | Med | Deprecated entries excluded from briefing injection history; test updated | YES |
| R-11 (Denormalized vectors) | Med | cosine_similarity clamps to [0.0, 1.0]; zero vector returns 0.0; mismatched dims returns 0.0 | YES |
| R-12 (Default Flexible change) | Med | RetrievalMode default Flexible; UDS explicitly Strict; MCP explicitly Flexible | YES |

### 2. Test Coverage Matches Risk Strategy — PASS

- 12 new unit tests cover penalty constants + cosine similarity edge cases
- 1 existing test updated for AC-11 (briefing deprecated exclusion)
- All 17 ACs have verification evidence (16 code/test, 1 manual/AC-16)
- All Critical and High risks have dedicated test scenarios

### 3. Risks Without Coverage — AC-16 Only

AC-16 (latency p95 < 15%) is deferred to manual benchmark per RISK-TEST-STRATEGY.md R-04.
This is acceptable because:
- Cosine similarity from stored embedding is O(d) where d=384 (sub-microsecond)
- No ONNX inference on hot path
- Batch fetch limits store reads
- Architecture explicitly budgeted 15% p95 headroom

### 4. Code Matches Specification — PASS

| Spec Requirement | Implementation |
|-----------------|----------------|
| FR-1.1 (RetrievalMode enum) | search.rs lines 31-38 |
| FR-1.2 (Default Flexible) | `#[default] Flexible` |
| FR-1.3 (UDS → Strict) | listener.rs:770 |
| FR-1.4 (MCP → Flexible) | tools.rs:298 |
| FR-1.5 (Strict drops non-Active) | search.rs lines 194-197 |
| FR-2.1 (Supersession injection) | search.rs lines 219-262 |
| FR-2.2 (Batch fetch) | search.rs lines 240-260 |
| FR-2.3 (Active + no superseded_by) | search.rs lines 247-251 |
| FR-2.7 (Dangling → skip) | search.rs line 243 |
| FR-3.1 (DEPRECATED_PENALTY=0.7) | confidence.rs constant |
| FR-3.2 (SUPERSEDED_PENALTY=0.5) | confidence.rs constant |
| FR-4.1 (Co-access deprecated exclusion) | coaccess.rs deprecated_ids param |
| FR-5.1 (Briefing deprecated exclusion) | briefing.rs `if entry.status == Status::Deprecated { continue }` |
| FR-6.2 (Explicit status bypass) | search.rs lines 183-188, 200, 217 |
| NFR-3.1 (No schema changes) | No new tables/fields; RetrievalMode is in-memory |
| NFR-4.1 (No new MCP params) | git diff confirms no new tool parameters |

### 5. Integration Smoke Tests — PASS (with pre-existing exception)

- 18/19 smoke tests passed
- 1 failure: `test_store_1000_entries` — rate limiter blocks bulk store (GH #111)
- Pre-existing: confirmed same failure on `main` branch
- No xfail marker added (already tracked in #111, not a test this feature should modify)
- No integration tests deleted or commented out

### 6. RISK-COVERAGE-REPORT.md — PASS

Report exists at `product/features/crt-010/testing/RISK-COVERAGE-REPORT.md`.
Includes:
- Unit test counts per crate (1369 total)
- Integration test results (18 passed, 1 pre-existing failure)
- 12 new tests listed with AC mapping
- Full risk coverage matrix (12/12 risks)
- Full AC verification table (17/17 ACs)

## Issues Found

None.
