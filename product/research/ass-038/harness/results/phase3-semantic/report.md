# Unimatrix Eval Report

Generated: 1775080982 (unix epoch) | Scenarios: 2356

## 1. Summary

| Profile | Scenarios | P@K | MRR | CC@k | ICD (max=ln(n)) | Avg Latency (ms) | ΔP@K | ΔMRR | ΔCC@k | ΔICD | ΔLatency (ms) |
|---------|-----------|-----|-----|------|----------------|-----------------|------|------|--------|------|---------------|
| combined-ppr-disabled | 2356 | 0.1530 | 0.3420 | 0.4390 | 0.6173 | 13.9 | — | — | — | — | — |
| combined-ppr-enabled | 2356 | 0.1530 | 0.3420 | 0.4390 | 0.6173 | 14.3 | — | — | — | — | +0.4 |

## 2. Notable Ranking Changes

### qlog-10

**Query**: it is now time to begin design of col-020 from @product/PRODUCT-VISION.md and @product/research/ass-018/MILESTONE-PROPOSAL.md .  Lets talk about meaningful metrics this adds that we don't already have available  
**Kendall τ**: 1.0000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 5: Roadmap milestones 5-6: Orches | 5: Roadmap milestones 5-6: Orches |
| 2 | 2969: ADR-001 crt-024: Six-Term Form | 2969: ADR-001 crt-024: Six-Term Form |
| 3 | 3208: Validate new scoring weight de | 3208: Validate new scoring weight de |
| 4 | 2844: UniversalMetrics Database Cons | 2844: UniversalMetrics Database Cons |
| 5 | 194: ASS-014 RQ-3: Existing Feature | 194: ASS-014 RQ-3: Existing Feature |

### qlog-1000

**Query**: let me give you my take... There are 2 dimensions worth considering.  Source & Use that need to drive decisions.  The concept of having Sessions and 'feature_entries' (topics), exist because of a need to tie hook generated data, to a knowledge context (topic).  I'm wondering, if these move to knowledge, how will context_retrospective be able to parse activity data (in analytics) and derive meaningful information?  If there is a way that I don't see, I'll accept it.  
**Kendall τ**: 1.0000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 3382: topic_signal write-time enrich | 3382: topic_signal write-time enrich |
| 2 | 759: ADR-017-002: HashMap Accumulat | 759: ADR-017-002: HashMap Accumulat |
| 3 | 3396: ADR-001 col-025: Goal Stored o | 3396: ADR-001 col-025: Goal Stored o |
| 4 | 864: ADR-001 col-020: Knowledge Reu | 864: ADR-001 col-020: Knowledge Reu |
| 5 | 3374: ADR-004 col-024: Shared enrich | 3374: ADR-004 col-024: Shared enrich |

### qlog-1001

**Query**: We have a set of tables that we consider to be premium 'knowledge'.  Retrieval and injection at the right time is our goal.. no doubt.  We have on our roadmap leaning harder into graph edges/relationships (potentially sourced out of activity??)  I think we need to think about this strucutre deeply to ensure we are making this database split decision for the right reason, and accruately describe potential downsides of taking this road.  We are partially doing this for scaling reasons.  It is a concern.  But from a pure operational side... I do beleive there will be limitations we're creating by doing this we've not fully accounted for.  I just named 1 example that we previously missed.  This is a 'no going back' type decision.. thats why I want to look at this decision from every angle before taking it.  
**Kendall τ**: 1.0000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 3659: ADR-004 crt-029: Separate quer | 3659: ADR-004 crt-029: Separate quer |
| 2 | 864: ADR-001 col-020: Knowledge Reu | 864: ADR-001 col-020: Knowledge Reu |
| 3 | 884: Server-Side Cross-Table Comput | 884: Server-Side Cross-Table Comput |
| 4 | 60: ADR-003: Manual Secondary Inde | 60: ADR-003: Manual Secondary Inde |
| 5 | 3423: ADR-003 col-026: Batch IN-Clau | 3423: ADR-003 col-026: Batch IN-Clau |

### qlog-1002

**Query**: product/research/ass-022 is where most of this research for this roadmap occurred... I think this was related to existing limitations (the move to a daemon solves the near term issues we've had, I believe), and the addition of rayon layer for high CPU functions is a second item).  My current thoughts... the SDLC use case deployed in a local project does not require db split, if we carefully address scalability.  A centralized model with higher scalability needs, may imply a different data backend technology choice.  Review the research, and provide comment  
**Kendall τ**: 1.0000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 884: Server-Side Cross-Table Comput | 884: Server-Side Cross-Table Comput |
| 2 | 2579: How to deliver cross-cutting i | 2579: How to deliver cross-cutting i |
| 3 | 1162: Two-Phase Import: DB Transacti | 1162: Two-Phase Import: DB Transacti |
| 4 | 2535: Shared rayon pool monopolisati | 2535: Shared rayon pool monopolisati |
| 5 | 1875: ADR: SQLite (rusqlite bundled) | 1875: ADR: SQLite (rusqlite bundled) |

### qlog-1003

**Query**: so.. one more strategic question... can I package postgres the same way we package sqllite, within the confines of the rust deployable?  Is that a strategic shift now that makes a conversion to truly scalable platform later simply swapping the backend infrastructure?  
**Kendall τ**: 1.0000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 1103: Explicit SQL-to-JSONL Row Seri | 1103: Explicit SQL-to-JSONL Row Seri |
| 2 | 2088: rusqlite + sqlx cannot coexist | 2088: rusqlite + sqlx cannot coexist |
| 3 | 336: ADR-004: Import Uses Store::op | 336: ADR-004: Import Uses Store::op |
| 4 | 1098: ADR-002: Explicit Column-to-JS | 1098: ADR-002: Explicit Column-to-JS |
| 5 | 1875: ADR: SQLite (rusqlite bundled) | 1875: ADR: SQLite (rusqlite bundled) |

### qlog-1004

**Query**: with that framing... I think we need to address the scalability issues and drive flexibility now into our future choices (doing it later is always more expensive than taking the medicine at the onset).  We will NOT split the database, but we will unlock our application to more efficiently use the database across current and planned features.  I'm inclined to adopt sqlx to resolve this.. I'm assuming rayon would be layered in as we originally planned later.  
**Kendall τ**: 1.0000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 1162: Two-Phase Import: DB Transacti | 1162: Two-Phase Import: DB Transacti |
| 2 | 2535: Shared rayon pool monopolisati | 2535: Shared rayon pool monopolisati |
| 3 | 1875: ADR: SQLite (rusqlite bundled) | 1875: ADR: SQLite (rusqlite bundled) |
| 4 | 2574: ADR-003: Contradiction Scan as | 2574: ADR-003: Contradiction Scan as |

### qlog-1005

**Query**: yes... I think I read you're recommending the separate read/write pools... I agree, make the updates, if so.. if not, lets talk.  
**Kendall τ**: 1.0000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 2060: ADR-003 nxs-011: Migration Con | 2060: ADR-003 nxs-011: Migration Con |
| 2 | 2152: SqlxStore dual-pool architectu | 2152: SqlxStore dual-pool architectu |
| 3 | 2147: SqlxStore dual-pool architectu | 2147: SqlxStore dual-pool architectu |
| 4 | 2153: SqlxStore dual-pool architectu | 2153: SqlxStore dual-pool architectu |
| 5 | 2145: ADR-001 nxs-011: Pool Acquire  | 2145: ADR-001 nxs-011: Pool Acquire  |

### qlog-1006

**Query**: I'm assuming our @product/test/infra-001/USAGE-PROTOCOL.md environment is the backstop... this should not be changed, and everything should still work...  
**Kendall τ**: 1.0000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 919: ADR-002 col-020b: Rust-Only Te | 919: ADR-002 col-020b: Rust-Only Te |
| 2 | 3611: Interface signature correction | 3611: Interface signature correction |
| 3 | 3789: ADR-003: Mandatory Integration | 3789: ADR-003: Mandatory Integration |
| 4 | 3714: col018_topic_signal_null_for_g | 3714: col018_topic_signal_null_for_g |
| 5 | 3814: MCP tool param deserialization | 3814: MCP tool param deserialization |

### qlog-1007

**Query**: OK.. its now time to start nxs-011 design from @product/PRODUCT-VISION.md  We recently updated product-vision for this feature, so include the commit for product-vision in the feature commits.  Begin the design protocol pls  
**Kendall τ**: 1.0000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 684: Always launch feature sessions | 684: Always launch feature sessions |
| 2 | 239: Feature Naming and Directory C | 239: Feature Naming and Directory C |
| 3 | 3: Roadmap milestones 1-2: Founda | 3: Roadmap milestones 1-2: Founda |
| 4 | 765: Design agents do not read sibl | 765: Design agents do not read sibl |
| 5 | 1563: Design agents following an est | 1563: Design agents following an est |

### qlog-1010

**Query**: Q4 - Updates come later in the roadmap for cpu bound workload.. out of scope for this feature, Q3-Have architect decide, Need more information on Q1 and Q2  
**Kendall τ**: 1.0000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 1271: Context load and cold restart  | 1271: Context load and cold restart  |
| 2 | 1628: Per-query full-store reads ins | 1628: Per-query full-store reads ins |
| 3 | 1511: Parallel worktrees harm featur | 1511: Parallel worktrees harm featur |
| 4 | 192: ASS-014 RQ-1: Unified Data Mod | 192: ASS-014 RQ-1: Unified Data Mod |
| 5 | 362: ADR-008: Wave Ordering and Cro | 362: ADR-008: Wave Ordering and Cro |

## 3. Latency Distribution

| ≤ ms | Count |
|------|-------|
| 50 | 4711 |
| 100 | 1 |
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
| combined-ppr-disabled | 2356 | 0.2000 | 1.0000 | 0.4390 |
| combined-ppr-enabled | 2356 | 0.2000 | 1.0000 | 0.4390 |

### ICD Range by Profile (max=ln(n))

| Profile | Scenarios | Min | Max | Mean |
|---------|-----------|-----|-----|------|
| combined-ppr-disabled | 2356 | 0.0000 | 1.6094 | 0.6173 |
| combined-ppr-enabled | 2356 | 0.0000 | 1.6094 | 0.6173 |
