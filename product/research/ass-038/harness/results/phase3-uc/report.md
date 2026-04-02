# Unimatrix Eval Report

Generated: 1775080982 (unix epoch) | Scenarios: 20

## 1. Summary

| Profile | Scenarios | P@K | MRR | CC@k | ICD (max=ln(n)) | Avg Latency (ms) | ΔP@K | ΔMRR | ΔCC@k | ΔICD | ΔLatency (ms) |
|---------|-----------|-----|-----|------|----------------|-----------------|------|------|--------|------|---------------|
| combined-ppr-disabled | 20 | 0.1167 | 0.4600 | 0.4900 | 0.7428 | 8.5 | — | — | — | — | — |
| combined-ppr-enabled | 20 | 0.1167 | 0.4600 | 0.4900 | 0.7428 | 7.9 | — | — | — | — | -0.6 |

## 2. Notable Ranking Changes

### uc1-01

**Query**: What characters are allowed in Unimatrix feature_cycle identifiers, and what validation approach does the engine use?  
**Kendall τ**: 1.0000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 604: Safety-Guard Validation: Permi | 604: Safety-Guard Validation: Permi |
| 2 | 606: ADR-001 col-014: Permissive Fe | 606: ADR-001 col-014: Permissive Fe |
| 3 | 3103: str::len() Returns Bytes, Not  | 3103: str::len() Returns Bytes, Not  |
| 4 | 3105: Use chars().count() not len()  | 3105: Use chars().count() not len()  |
| 5 | 427: Fixed-Width Feature Vector wit | 427: Fixed-Width Feature Vector wit |

### uc1-02

**Query**: How should background fire-and-forget writes to the database be structured to avoid saturating the tokio blocking pool?  
**Kendall τ**: 1.0000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 735: spawn_blocking Pool Saturation | 735: spawn_blocking Pool Saturation |
| 2 | 731: Batched Fire-and-Forget DB Wri | 731: Batched Fire-and-Forget DB Wri |
| 3 | 2266: write_pool max_connections=1 + | 2266: write_pool max_connections=1 + |
| 4 | 771: Blocking store.lock_conn() on  | 771: Blocking store.lock_conn() on  |
| 5 | 2126: Use block_in_place (not Handle | 2126: Use block_in_place (not Handle |

### uc1-03

**Query**: What is the correct SqlxStore dual-pool write architecture to prevent SQLITE_BUSY_SNAPSHOT under concurrent reads?  
**Kendall τ**: 1.0000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 2152: SqlxStore dual-pool architectu | 2152: SqlxStore dual-pool architectu |
| 2 | 2153: SqlxStore dual-pool architectu | 2153: SqlxStore dual-pool architectu |
| 3 | 2270: SqlxStore dual-pool WAL archit | 2270: SqlxStore dual-pool WAL archit |
| 4 | 2147: SqlxStore dual-pool architectu | 2147: SqlxStore dual-pool architectu |
| 5 | 2151: SqlxStore dual-pool architectu | 2151: SqlxStore dual-pool architectu |

### uc1-04

**Query**: What is the complete checklist for bumping CURRENT_SCHEMA_VERSION, including test and migration requirements?  
**Kendall τ**: 1.0000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 3894: Schema Version Cascade: Comple | 3894: Schema Version Cascade: Comple |
| 2 | 760: ADR-017-003: Independent Migra | 760: ADR-017-003: Independent Migra |
| 3 | 836: How to add a new table to the  | 836: How to add a new table to the  |
| 4 | 681: Create-New-Then-Swap Schema Mi | 681: Create-New-Then-Swap Schema Mi |
| 5 | 378: Schema migration tests must in | 378: Schema migration tests must in |

### uc1-05

**Query**: How should signal fusion weights be ordered in the Unimatrix ranking formula, and what are the default values?  
**Kendall τ**: 1.0000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 2964: Signal fusion pattern: sequent | 2964: Signal fusion pattern: sequent |
| 2 | 3207: compute_fused_score extension  | 3207: compute_fused_score extension  |
| 3 | 3677: PhaseFreqTable cold-start neut | 3677: PhaseFreqTable cold-start neut |
| 4 | 3685: ADR-001 col-031: Rank-Based No | 3685: ADR-001 col-031: Rank-Based No |
| 5 | 2610: HashMap profile iteration orde | 2610: HashMap profile iteration orde |

### uc1-06

**Query**: How should the co_access promotion SELECT be written to correctly exclude quarantined entries from being promoted?  
**Kendall τ**: 1.0000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 3978: co_access promotion SELECT mis | 3978: co_access promotion SELECT mis |
| 2 | 3980: Promotion tick SELECT must JOI | 3980: Promotion tick SELECT must JOI |
| 3 | 3981: Co-access promotion tick: quar | 3981: Co-access promotion tick: quar |

### uc2-01

**Query**: What data type should be used for scoring weights and confidence values throughout the Unimatrix scoring pipeline?  
**Kendall τ**: 1.0000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 2284: ADR-001 dsn-001: ConfidencePar | 2284: ADR-001 dsn-001: ConfidencePar |
| 2 | 750: Adding Pipeline Validation Tes | 750: Adding Pipeline Validation Tes |
| 3 | 2393: ADR-005 dsn-001: Preset Enum D | 2393: ADR-005 dsn-001: Preset Enum D |
| 4 | 452: Weighted Loss for Weak Trainin | 452: Weighted Loss for Weak Trainin |
| 5 | 1042: Pure Computation Engine Module | 1042: Pure Computation Engine Module |

### uc2-02

**Query**: How should BriefingService handle semantic search — implement its own HNSW search or delegate to another service?  
**Kendall τ**: 1.0000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 270: ADR-002: BriefingService Deleg | 270: ADR-002: BriefingService Deleg |
| 2 | 284: ADR-002: BriefingService deleg | 284: ADR-002: BriefingService deleg |
| 3 | 1546: ADR-004 crt-018b: Effectivenes | 1546: ADR-004 crt-018b: Effectivenes |
| 4 | 95: ADR-005: Briefing Graceful Deg | 95: ADR-005: Briefing Graceful Deg |
| 5 | 3210: SessionRegistry access in Sear | 3210: SessionRegistry access in Sear |

### uc2-03

**Query**: What serialization format should be used when migrating knowledge entries between storage backends?  
**Kendall τ**: 1.0000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 343: JSON-Lines Intermediate Format | 343: JSON-Lines Intermediate Format |
| 2 | 333: ADR-001: JSON-Lines Intermedia | 333: ADR-001: JSON-Lines Intermedia |
| 3 | 59: ADR-002: bincode v2 Serializat | 59: ADR-002: bincode v2 Serializat |
| 4 | 371: Migration Compatibility Module | 371: Migration Compatibility Module |
| 5 | 1143: ADR-001: Shared Format Types B | 1143: ADR-001: Shared Format Types B |

### uc2-04

**Query**: How is the graph compaction pass structured and when is it triggered during maintenance operations?  
**Kendall τ**: 1.0000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 3762: run_maintenance() tick order:  | 3762: run_maintenance() tick order:  |
| 2 | 3906: GRAPH_EDGES compaction must ex | 3906: GRAPH_EDGES compaction must ex |
| 3 | 3883: GRAPH_EDGES tick writes: use w | 3883: GRAPH_EDGES tick writes: use w |
| 4 | 3913: ADR-001 bugfix-458: GRAPH_EDGE | 3913: ADR-001 bugfix-458: GRAPH_EDGE |
| 5 | 3911: How to add a new maintenance t | 3911: How to add a new maintenance t |

## 3. Latency Distribution

| ≤ ms | Count |
|------|-------|
| 50 | 40 |
| 100 | 0 |
| 200 | 0 |
| 500 | 0 |
| 1000 | 0 |
| 2000 | 0 |
| > 2000 | 0 |

## 4. Entry-Level Analysis

_No entry rank changes recorded._

## 5. Zero-Regression Check

**No regressions detected.** All candidate profiles maintain or improve MRR and P@K across all scenarios.

## 7. Distribution Analysis

_ICD is raw Shannon entropy (natural log). Maximum value is ln(n_categories).
Values are comparable across profiles run with the same configured categories._

### CC@k Range by Profile

| Profile | Scenarios | Min | Max | Mean |
|---------|-----------|-----|-----|------|
| combined-ppr-disabled | 20 | 0.2000 | 0.8000 | 0.4900 |
| combined-ppr-enabled | 20 | 0.2000 | 0.8000 | 0.4900 |

### ICD Range by Profile (max=ln(n))

| Profile | Scenarios | Min | Max | Mean |
|---------|-----------|-----|-----|------|
| combined-ppr-disabled | 20 | 0.0000 | 1.3322 | 0.7428 |
| combined-ppr-enabled | 20 | 0.0000 | 1.3322 | 0.7428 |
