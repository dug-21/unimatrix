# Unimatrix Eval Report

Generated: 1775086628 (unix epoch) | Scenarios: 1443

## 1. Summary

| Profile | Scenarios | P@K | MRR | CC@k | ICD (max=ln(n)) | Avg Latency (ms) | ΔP@K | ΔMRR | ΔCC@k | ΔICD | ΔLatency (ms) |
|---------|-----------|-----|-----|------|----------------|-----------------|------|------|--------|------|---------------|
| ablation-conf-zero | 1443 | 0.1117 | 0.2671 | 0.4524 | 0.6637 | 6.7 | — | — | — | — | — |
| ablation-cosine-only | 1443 | 0.1117 | 0.2671 | 0.4524 | 0.6637 | 7.1 | — | -0.0116 | — | — | +0.9 |
| ablation-phase-zero | 1443 | 0.1117 | 0.2884 | 0.4524 | 0.6637 | 7.0 | — | -0.0116 | — | — | +0.9 |
| ablation-util-prov-zero | 1443 | 0.1117 | 0.2914 | 0.4524 | 0.6637 | 7.0 | — | -0.0116 | — | — | +0.9 |
| baseline-nli | 1443 | 0.1117 | 0.2884 | 0.4524 | 0.6637 | 6.9 | — | -0.0116 | — | — | +0.9 |
| conf-boost-c | 1443 | 0.1117 | 0.2913 | 0.4524 | 0.6637 | 5.9 | — | -0.0116 | — | — | +0.9 |

## 2. Notable Ranking Changes

### obs-f094870a-1775006708000

**Query**: multi-type background tick independent budget per edge type fan-out  
**Kendall τ**: -1.0000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 3976: Multi-type background tick: gi | 3976: Multi-type background tick: gi |
| 2 | 3970: Shared tick cap starves low-pr | 3970: Shared tick cap starves low-pr |

### obs-88cecd44-1773700604000

**Query**: write queue analytics background tick single writer SQLite contention  
**Kendall τ**: -0.8000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 2150: SqlxStore dual-pool architectu | 2150: SqlxStore dual-pool architectu |
| 2 | 2147: SqlxStore dual-pool architectu | 2147: SqlxStore dual-pool architectu |
| 3 | 2153: SqlxStore dual-pool architectu | 2153: SqlxStore dual-pool architectu |
| 4 | 2152: SqlxStore dual-pool architectu | 2152: SqlxStore dual-pool architectu |
| 5 | 2130: sqlx write_pool: set max_conne | 2130: sqlx write_pool: set max_conne |

### obs-c6f8fddb-1774478268000

**Query**: long running agent integration test listener complexity lifespan  
**Kendall τ**: -0.6000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 2675: Protocol and Agent Definition  | 2675: Protocol and Agent Definition  |
| 2 | 3952: Long-wait parallel agent resum | 3952: Long-wait parallel agent resum |
| 3 | 3328: Service Replacement in unimatr | 3328: Service Replacement in unimatr |
| 4 | 3561: Avoid sleep polling in tester  | 3561: Avoid sleep polling in tester  |
| 5 | 487: How to run workspace tests wit | 487: How to run workspace tests wit |

### obs-5f33a80a-1774486402000

**Query**: add field to SessionState confirmed selections tracking  
**Kendall τ**: -0.4000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 3922: SessionState in-memory counter | 3922: SessionState in-memory counter |
| 2 | 3517: ADR-005 col-028: confirmed_ent | 3517: ADR-005 col-028: confirmed_ent |
| 3 | 3180: SessionState field additions r | 3180: SessionState field additions r |
| 4 | 759: ADR-017-002: HashMap Accumulat | 759: ADR-017-002: HashMap Accumulat |
| 5 | 3210: SessionRegistry access in Sear | 3210: SessionRegistry access in Sear |

### obs-e275cc04-1774459252000

**Query**: col-027 architectural decisions  
**Kendall τ**: -0.4000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 3620: Architecture document governs  | 3620: Architecture document governs  |
| 2 | 200: ASS-014 Architecture Synthesis | 200: ASS-014 Architecture Synthesis |
| 3 | 723: Architecture and Specification | 723: Architecture and Specification |
| 4 | 3623: ADR correction cascade: changi | 3623: ADR correction cascade: changi |
| 5 | 5: Roadmap milestones 5-6: Orches | 5: Roadmap milestones 5-6: Orches |

### obs-edf4842f-1774829259000

**Query**: sqlx store module patterns read_pool write_pool  
**Kendall τ**: -0.4000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 2153: SqlxStore dual-pool architectu | 2153: SqlxStore dual-pool architectu |
| 2 | 2151: SqlxStore dual-pool architectu | 2151: SqlxStore dual-pool architectu |
| 3 | 2152: SqlxStore dual-pool architectu | 2152: SqlxStore dual-pool architectu |
| 4 | 2147: SqlxStore dual-pool architectu | 2147: SqlxStore dual-pool architectu |
| 5 | 3799: Acquire write connection befor | 3799: Acquire write connection befor |

### obs-50273e07-1774566840000

**Query**: risk pattern SQL JOIN entries status filter  
**Kendall τ**: -0.3333

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 3981: Co-access promotion tick: quar | 3981: Co-access promotion tick: quar |
| 2 | 3980: Promotion tick SELECT must JOI | 3980: Promotion tick SELECT must JOI |
| 3 | 3594: ADR-004 col-029: Cross-Categor | 3594: ADR-004 col-029: Cross-Categor |

### obs-d44a584e-1774229472000

**Query**: bugfix-311 311 confidence params serving path  
**Kendall τ**: -0.3333

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 2398: API Extension Gap: New Struct  | 2398: API Extension Gap: New Struct  |
| 2 | 2324: ConfidenceParams migration: re | 2324: ConfidenceParams migration: re |
| 3 | 1372: Bugfix spawn prompts should in | 1372: Bugfix spawn prompts should in |
| 4 | 3974: Security reviewer must verify  | 3974: Security reviewer must verify  |

### obs-29d12dbd-1774901990000

**Query**: knowledge graph clustering community detection partitioning  
**Kendall τ**: -0.2000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 3913: ADR-001 bugfix-458: GRAPH_EDGE | 3913: ADR-001 bugfix-458: GRAPH_EDGE |
| 2 | 3592: ADR-002 col-029: Two SQL Queri | 3592: ADR-002 col-029: Two SQL Queri |
| 3 | 3731: ADR-001 crt-030: graph_ppr.rs  | 3731: ADR-001 crt-030: graph_ppr.rs  |
| 4 | 3374: ADR-004 col-024: Shared enrich | 3374: ADR-004 col-024: Shared enrich |
| 5 | 3659: ADR-004 crt-029: Separate quer | 3659: ADR-004 crt-029: Separate quer |

### obs-5becb974-1774967683000

**Query**: SQLite relation_type CHECK constraint DDL schema column free-text  
**Kendall τ**: -0.2000

| Rank | Baseline Entry | Candidate Entry |
|------|---------------|-----------------|
| 1 | 2745: SQLite NOT NULL DEFAULT '' got | 2745: SQLite NOT NULL DEFAULT '' got |
| 2 | 3962: EntryRecord.feature_cycle is S | 3962: EntryRecord.feature_cycle is S |
| 3 | 3929: observation_phase_metrics requ | 3929: observation_phase_metrics requ |
| 4 | 3543: Nullable column addition in te | 3543: Nullable column addition in te |
| 5 | 373: Junction Table with FK CASCADE | 373: Junction Table with FK CASCADE |

## 3. Latency Distribution

| ≤ ms | Count |
|------|-------|
| 50 | 8655 |
| 100 | 3 |
| 200 | 0 |
| 500 | 0 |
| 1000 | 0 |
| 2000 | 0 |
| > 2000 | 0 |

## 4. Entry-Level Analysis

**Most Promoted Entries** (avg rank gain):

| Entry ID | Title | Avg Rank Gain |
|----------|-------|---------------|
| 200 | ASS-014 Architecture Synthesis: Cortical | +4 |
| 2150 | SqlxStore dual-pool architecture: read_p | +4 |
| 195 | ASS-014: Transport Trait Design — Sync I | +3 |
| 2133 | Compile-time bool to runtime env var: co | +3 |
| 2161 | ADR-001 alc-003 test | +3 |
| 2398 | API Extension Gap: New Struct Fields Not | +3 |
| 3663 | VectorIndex is synchronous — rayon closu | +3 |
| 3884 | unimatrix-server graph edges: pattern | +3 |
| 3956 | ADR-003 crt-037: Directional Dedup for q | +3 |
| 3813 | Custom serde deserializer + schemars sch | +3 |

**Most Demoted Entries** (avg rank loss):

| Entry ID | Title | Avg Rank Loss |
|----------|-------|---------------|
| 2130 | sqlx write_pool: set max_connections=1 t | -4 |
| 76 | ADR-006: Object-Safe Send+Sync Traits | -3 |
| 373 | Junction Table with FK CASCADE for Tag-L | -3 |
| 2673 | EvalServiceLayer snapshot must load Vect | -3 |
| 1544 | ADR-002 crt-018b: Hold (Not Increment) c | -3 |
| 2417 | ADR-001 (crt-021): Typed Edge Weight Mod | -3 |
| 487 | How to run workspace tests without hangi | -2 |
| 2999 | ADR-002 crt-025: seq Is Advisory; Timest | -2 |
| 362 | ADR-008: Wave Ordering and Cross-Crate S | -2 |
| 3397 | ADR-002 col-025: synthesize_from_session | -2 |

## 5. Distribution Gate / Zero-Regression Check

### 5.1 Zero-Regression Check — ablation-cosine-only

**No regressions detected.** All candidate profiles maintain or improve MRR and P@K across all scenarios.

### 5.2 Zero-Regression Check — ablation-phase-zero

**38 regression(s) detected:**

| Scenario | Query | Profile | Reason | Baseline MRR | Candidate MRR | Baseline P@K | Candidate P@K |
|----------|-------|---------|--------|-------------|--------------|-------------|---------------|
| obs-bd98afa6-1773804590000 | SQLite connection pool async storage pattern unimatrix-store | ablation-phase-zero | MRR dropped | 1.0000 | 0.2500 | 0.2000 | 0.2000 |
| obs-ea862451-1774815867000 | scope expansion mid-session design rework vision alignment | ablation-phase-zero | MRR dropped | 1.0000 | 0.2500 | 0.2000 | 0.2000 |
| obs-3d9b719e-1774217292000 | crt-026 WA-2 session context enrichment patterns procedures | ablation-phase-zero | MRR dropped | 1.0000 | 0.5000 | 0.2000 | 0.2000 |
| obs-4a5ba677-1774402758000 | FeatureKnowledgeReuse cross_feature_reuse compute_knowledge_reuse_for_sessions | ablation-phase-zero | MRR dropped | 1.0000 | 0.5000 | 0.2000 | 0.2000 |
| obs-50273e07-1774566839000 | lesson-learned failures gate rejection SQL aggregate | ablation-phase-zero | MRR dropped | 1.0000 | 0.5000 | 0.4000 | 0.4000 |
| obs-50273e07-1774566845000 | connected_entry_count COUNT DISTINCT double-count UNION subquery graph edges | ablation-phase-zero | MRR dropped | 1.0000 | 0.5000 | 0.2500 | 0.2500 |
| obs-531f376f-1773534194000 | crt-018b effectiveness-driven retrieval | ablation-phase-zero | MRR dropped | 1.0000 | 0.5000 | 0.2000 | 0.2000 |
| obs-a1aaf24a-1774371798000 | error handling graceful degradation None fallback DB read session registration | ablation-phase-zero | MRR dropped | 1.0000 | 0.5000 | 0.2000 | 0.2000 |
| obs-af1f3351-1774269500000 | mcp response module unconditional compile IndexBriefingService graceful degradation empty fallback | ablation-phase-zero | MRR dropped | 1.0000 | 0.5000 | 0.2000 | 0.2000 |
| obs-b0be618b-1774523351000 | eval harness pattern procedure lesson Shannon entropy dual copy | ablation-phase-zero | MRR dropped | 1.0000 | 0.5000 | 0.2000 | 0.2000 |
| obs-d4042945-1774306402000 | validate_briefing_params MCP tool parameter validation | ablation-phase-zero | MRR dropped | 1.0000 | 0.5000 | 0.2000 | 0.2000 |
| obs-d4042945-1774306403000 | validate_briefing_params MCP tool parameter validation | ablation-phase-zero | MRR dropped | 1.0000 | 0.5000 | 0.2000 | 0.2000 |
| obs-ea4a711a-1774638811000 | bugfix-421 graph inference tick shuffle embedded_ids | ablation-phase-zero | MRR dropped | 1.0000 | 0.5000 | 0.2000 | 0.2000 |
| obs-5f33a80a-1774486351000 | context_cycle phase capture Thompson sampling relevance learning | ablation-phase-zero | MRR dropped | 0.5000 | 0.2500 | 0.2000 | 0.2000 |
| obs-18f5d7e9-1774716252000 | check inputs still in scope before proposing call site function design review | ablation-phase-zero | MRR dropped | 0.5000 | 0.3333 | 0.2000 | 0.2000 |
| obs-1ebc220a-1774994875000 | NLI detection guard predicate nli_detection_tick inference | ablation-phase-zero | MRR dropped | 0.5000 | 0.3333 | 0.2500 | 0.2500 |
| obs-3cd7df2a-1774750121000 | graph traversal unit testing patterns | ablation-phase-zero | MRR dropped | 0.5000 | 0.3333 | 0.2000 | 0.2000 |
| obs-5becb974-1774967689000 | PPR direction semantics reverse walk outgoing edges traversal | ablation-phase-zero | MRR dropped | 0.5000 | 0.3333 | 0.2000 | 0.2000 |
| obs-7c4e3cc4-1773804579000 | PERMISSIVE_AUTO_ENROLL registry enrollment trust level | ablation-phase-zero | MRR dropped | 0.5000 | 0.3333 | 0.2000 | 0.2000 |
| obs-a02f1841-1774916916000 | saturating arithmetic integer overflow u32 counter | ablation-phase-zero | MRR dropped | 0.5000 | 0.3333 | 0.2000 | 0.2000 |
| obs-abcc3cc8-1773157713000 | idempotent counter update topic deliveries | ablation-phase-zero | MRR dropped | 0.5000 | 0.3333 | 0.2000 | 0.2000 |
| obs-fb91e6f4-1773933243000 | working tree uncommitted changes gate validation SM commit before gate | ablation-phase-zero | MRR dropped | 0.5000 | 0.3333 | 0.2000 | 0.2000 |
| obs-fb91e6f4-1773933244000 | working tree uncommitted changes gate validation SM commit before gate | ablation-phase-zero | MRR dropped | 0.5000 | 0.3333 | 0.2000 | 0.2000 |
| obs-a02f1841-1774957353000 | markdown escaping MCP response formatter table cell | ablation-phase-zero | MRR dropped | 0.3333 | 0.2000 | 0.2000 | 0.2000 |
| obs-2eb0aff5-1773310460000 | nan-002 knowledge import | ablation-phase-zero | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-30b7b9c9-1774906266000 | risk pattern | ablation-phase-zero | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-4268d867-1774745473000 | NLI graph inference source candidate selection phase data influence | ablation-phase-zero | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-50273e07-1774566402000 | cache handle pattern Arc RwLock background tick read compute_report | ablation-phase-zero | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-9bb67ef4-1774002107000 | subprocess test coverage missing offline acceptance criteria gate failure | ablation-phase-zero | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-a92b01c4-1774484832000 | infra-001 tester background tasks integration test execution | ablation-phase-zero | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-a92b01c4-1774484836000 | infra-001 tester background tasks integration test execution | ablation-phase-zero | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-a92b01c4-1774484839000 | infra-001 tester background tasks integration test execution | ablation-phase-zero | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-a92b01c4-1774484841000 | infra-001 tester background tasks integration test execution | ablation-phase-zero | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-fb91e6f4-1773882525000 | supersession graph penalty | ablation-phase-zero | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-50273e07-1774566492000 | ContradictionScanCacheHandle Arc RwLock background tick compute_report | ablation-phase-zero | MRR dropped | 0.2500 | 0.2000 | 0.2000 | 0.2000 |
| obs-5becb974-1774975696000 | NLI inference graph edge testing patterns PPR discriminator routing | ablation-phase-zero | MRR dropped | 0.2500 | 0.2000 | 0.2000 | 0.2000 |
| obs-5f33a80a-1774523338000 | positional column index SQLite analytics INSERT SELECT atomic change unit | ablation-phase-zero | MRR dropped | 0.2500 | 0.2000 | 0.2000 | 0.2000 |
| obs-cdbdd4fc-1774263383000 | topic scan full table query caching store reads extraction pipeline | ablation-phase-zero | MRR dropped | 0.2500 | 0.2000 | 0.2000 | 0.2000 |

_This list is a human-reviewed artifact. No automated gate logic is applied._

### 5.3 Zero-Regression Check — ablation-util-prov-zero

**39 regression(s) detected:**

| Scenario | Query | Profile | Reason | Baseline MRR | Candidate MRR | Baseline P@K | Candidate P@K |
|----------|-------|---------|--------|-------------|--------------|-------------|---------------|
| obs-bd98afa6-1773804590000 | SQLite connection pool async storage pattern unimatrix-store | ablation-util-prov-zero | MRR dropped | 1.0000 | 0.2500 | 0.2000 | 0.2000 |
| obs-ea862451-1774815867000 | scope expansion mid-session design rework vision alignment | ablation-util-prov-zero | MRR dropped | 1.0000 | 0.2500 | 0.2000 | 0.2000 |
| obs-50273e07-1774566845000 | connected_entry_count COUNT DISTINCT double-count UNION subquery graph edges | ablation-util-prov-zero | MRR dropped | 1.0000 | 0.3333 | 0.2500 | 0.2500 |
| obs-531f376f-1773534194000 | crt-018b effectiveness-driven retrieval | ablation-util-prov-zero | MRR dropped | 1.0000 | 0.3333 | 0.2000 | 0.2000 |
| obs-af1f3351-1774269500000 | mcp response module unconditional compile IndexBriefingService graceful degradation empty fallback | ablation-util-prov-zero | MRR dropped | 1.0000 | 0.3333 | 0.2000 | 0.2000 |
| obs-3d9b719e-1774217292000 | crt-026 WA-2 session context enrichment patterns procedures | ablation-util-prov-zero | MRR dropped | 1.0000 | 0.5000 | 0.2000 | 0.2000 |
| obs-4a5ba677-1774402758000 | FeatureKnowledgeReuse cross_feature_reuse compute_knowledge_reuse_for_sessions | ablation-util-prov-zero | MRR dropped | 1.0000 | 0.5000 | 0.2000 | 0.2000 |
| obs-50273e07-1774566839000 | lesson-learned failures gate rejection SQL aggregate | ablation-util-prov-zero | MRR dropped | 1.0000 | 0.5000 | 0.4000 | 0.4000 |
| obs-a1aaf24a-1774371798000 | error handling graceful degradation None fallback DB read session registration | ablation-util-prov-zero | MRR dropped | 1.0000 | 0.5000 | 0.2000 | 0.2000 |
| obs-b0be618b-1774523351000 | eval harness pattern procedure lesson Shannon entropy dual copy | ablation-util-prov-zero | MRR dropped | 1.0000 | 0.5000 | 0.2000 | 0.2000 |
| obs-d4042945-1774306402000 | validate_briefing_params MCP tool parameter validation | ablation-util-prov-zero | MRR dropped | 1.0000 | 0.5000 | 0.2000 | 0.2000 |
| obs-d4042945-1774306403000 | validate_briefing_params MCP tool parameter validation | ablation-util-prov-zero | MRR dropped | 1.0000 | 0.5000 | 0.2000 | 0.2000 |
| obs-ea4a711a-1774638811000 | bugfix-421 graph inference tick shuffle embedded_ids | ablation-util-prov-zero | MRR dropped | 1.0000 | 0.5000 | 0.2000 | 0.2000 |
| obs-5f33a80a-1774486351000 | context_cycle phase capture Thompson sampling relevance learning | ablation-util-prov-zero | MRR dropped | 0.5000 | 0.2500 | 0.2000 | 0.2000 |
| obs-18f5d7e9-1774716252000 | check inputs still in scope before proposing call site function design review | ablation-util-prov-zero | MRR dropped | 0.5000 | 0.3333 | 0.2000 | 0.2000 |
| obs-1ebc220a-1774994875000 | NLI detection guard predicate nli_detection_tick inference | ablation-util-prov-zero | MRR dropped | 0.5000 | 0.3333 | 0.2500 | 0.2500 |
| obs-3cd7df2a-1774750121000 | graph traversal unit testing patterns | ablation-util-prov-zero | MRR dropped | 0.5000 | 0.3333 | 0.2000 | 0.2000 |
| obs-5becb974-1774967689000 | PPR direction semantics reverse walk outgoing edges traversal | ablation-util-prov-zero | MRR dropped | 0.5000 | 0.3333 | 0.2000 | 0.2000 |
| obs-7c4e3cc4-1773804579000 | PERMISSIVE_AUTO_ENROLL registry enrollment trust level | ablation-util-prov-zero | MRR dropped | 0.5000 | 0.3333 | 0.2000 | 0.2000 |
| obs-a02f1841-1774916916000 | saturating arithmetic integer overflow u32 counter | ablation-util-prov-zero | MRR dropped | 0.5000 | 0.3333 | 0.2000 | 0.2000 |
| obs-abcc3cc8-1773157713000 | idempotent counter update topic deliveries | ablation-util-prov-zero | MRR dropped | 0.5000 | 0.3333 | 0.2000 | 0.2000 |
| obs-fb91e6f4-1773933243000 | working tree uncommitted changes gate validation SM commit before gate | ablation-util-prov-zero | MRR dropped | 0.5000 | 0.3333 | 0.2000 | 0.2000 |
| obs-fb91e6f4-1773933244000 | working tree uncommitted changes gate validation SM commit before gate | ablation-util-prov-zero | MRR dropped | 0.5000 | 0.3333 | 0.2000 | 0.2000 |
| obs-a02f1841-1774957353000 | markdown escaping MCP response formatter table cell | ablation-util-prov-zero | MRR dropped | 0.3333 | 0.2000 | 0.2000 | 0.2000 |
| obs-2eb0aff5-1773310460000 | nan-002 knowledge import | ablation-util-prov-zero | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-30b7b9c9-1774906266000 | risk pattern | ablation-util-prov-zero | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-4268d867-1774745473000 | NLI graph inference source candidate selection phase data influence | ablation-util-prov-zero | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-50273e07-1774566402000 | cache handle pattern Arc RwLock background tick read compute_report | ablation-util-prov-zero | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-50273e07-1774575754000 | worktree isolation design agents file leak main repo path discipline | ablation-util-prov-zero | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-9bb67ef4-1774002107000 | subprocess test coverage missing offline acceptance criteria gate failure | ablation-util-prov-zero | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-a92b01c4-1774484832000 | infra-001 tester background tasks integration test execution | ablation-util-prov-zero | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-a92b01c4-1774484836000 | infra-001 tester background tasks integration test execution | ablation-util-prov-zero | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-a92b01c4-1774484839000 | infra-001 tester background tasks integration test execution | ablation-util-prov-zero | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-a92b01c4-1774484841000 | infra-001 tester background tasks integration test execution | ablation-util-prov-zero | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-fb91e6f4-1773882525000 | supersession graph penalty | ablation-util-prov-zero | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-50273e07-1774566492000 | ContradictionScanCacheHandle Arc RwLock background tick compute_report | ablation-util-prov-zero | MRR dropped | 0.2500 | 0.2000 | 0.2000 | 0.2000 |
| obs-5becb974-1774975696000 | NLI inference graph edge testing patterns PPR discriminator routing | ablation-util-prov-zero | MRR dropped | 0.2500 | 0.2000 | 0.2000 | 0.2000 |
| obs-5f33a80a-1774523338000 | positional column index SQLite analytics INSERT SELECT atomic change unit | ablation-util-prov-zero | MRR dropped | 0.2500 | 0.2000 | 0.2000 | 0.2000 |
| obs-cdbdd4fc-1774263383000 | topic scan full table query caching store reads extraction pipeline | ablation-util-prov-zero | MRR dropped | 0.2500 | 0.2000 | 0.2000 | 0.2000 |

_This list is a human-reviewed artifact. No automated gate logic is applied._

### 5.4 Zero-Regression Check — baseline-nli

**38 regression(s) detected:**

| Scenario | Query | Profile | Reason | Baseline MRR | Candidate MRR | Baseline P@K | Candidate P@K |
|----------|-------|---------|--------|-------------|--------------|-------------|---------------|
| obs-bd98afa6-1773804590000 | SQLite connection pool async storage pattern unimatrix-store | baseline-nli | MRR dropped | 1.0000 | 0.2500 | 0.2000 | 0.2000 |
| obs-ea862451-1774815867000 | scope expansion mid-session design rework vision alignment | baseline-nli | MRR dropped | 1.0000 | 0.2500 | 0.2000 | 0.2000 |
| obs-3d9b719e-1774217292000 | crt-026 WA-2 session context enrichment patterns procedures | baseline-nli | MRR dropped | 1.0000 | 0.5000 | 0.2000 | 0.2000 |
| obs-4a5ba677-1774402758000 | FeatureKnowledgeReuse cross_feature_reuse compute_knowledge_reuse_for_sessions | baseline-nli | MRR dropped | 1.0000 | 0.5000 | 0.2000 | 0.2000 |
| obs-50273e07-1774566839000 | lesson-learned failures gate rejection SQL aggregate | baseline-nli | MRR dropped | 1.0000 | 0.5000 | 0.4000 | 0.4000 |
| obs-50273e07-1774566845000 | connected_entry_count COUNT DISTINCT double-count UNION subquery graph edges | baseline-nli | MRR dropped | 1.0000 | 0.5000 | 0.2500 | 0.2500 |
| obs-531f376f-1773534194000 | crt-018b effectiveness-driven retrieval | baseline-nli | MRR dropped | 1.0000 | 0.5000 | 0.2000 | 0.2000 |
| obs-a1aaf24a-1774371798000 | error handling graceful degradation None fallback DB read session registration | baseline-nli | MRR dropped | 1.0000 | 0.5000 | 0.2000 | 0.2000 |
| obs-af1f3351-1774269500000 | mcp response module unconditional compile IndexBriefingService graceful degradation empty fallback | baseline-nli | MRR dropped | 1.0000 | 0.5000 | 0.2000 | 0.2000 |
| obs-b0be618b-1774523351000 | eval harness pattern procedure lesson Shannon entropy dual copy | baseline-nli | MRR dropped | 1.0000 | 0.5000 | 0.2000 | 0.2000 |
| obs-d4042945-1774306402000 | validate_briefing_params MCP tool parameter validation | baseline-nli | MRR dropped | 1.0000 | 0.5000 | 0.2000 | 0.2000 |
| obs-d4042945-1774306403000 | validate_briefing_params MCP tool parameter validation | baseline-nli | MRR dropped | 1.0000 | 0.5000 | 0.2000 | 0.2000 |
| obs-ea4a711a-1774638811000 | bugfix-421 graph inference tick shuffle embedded_ids | baseline-nli | MRR dropped | 1.0000 | 0.5000 | 0.2000 | 0.2000 |
| obs-5f33a80a-1774486351000 | context_cycle phase capture Thompson sampling relevance learning | baseline-nli | MRR dropped | 0.5000 | 0.2500 | 0.2000 | 0.2000 |
| obs-18f5d7e9-1774716252000 | check inputs still in scope before proposing call site function design review | baseline-nli | MRR dropped | 0.5000 | 0.3333 | 0.2000 | 0.2000 |
| obs-1ebc220a-1774994875000 | NLI detection guard predicate nli_detection_tick inference | baseline-nli | MRR dropped | 0.5000 | 0.3333 | 0.2500 | 0.2500 |
| obs-3cd7df2a-1774750121000 | graph traversal unit testing patterns | baseline-nli | MRR dropped | 0.5000 | 0.3333 | 0.2000 | 0.2000 |
| obs-5becb974-1774967689000 | PPR direction semantics reverse walk outgoing edges traversal | baseline-nli | MRR dropped | 0.5000 | 0.3333 | 0.2000 | 0.2000 |
| obs-7c4e3cc4-1773804579000 | PERMISSIVE_AUTO_ENROLL registry enrollment trust level | baseline-nli | MRR dropped | 0.5000 | 0.3333 | 0.2000 | 0.2000 |
| obs-a02f1841-1774916916000 | saturating arithmetic integer overflow u32 counter | baseline-nli | MRR dropped | 0.5000 | 0.3333 | 0.2000 | 0.2000 |
| obs-abcc3cc8-1773157713000 | idempotent counter update topic deliveries | baseline-nli | MRR dropped | 0.5000 | 0.3333 | 0.2000 | 0.2000 |
| obs-fb91e6f4-1773933243000 | working tree uncommitted changes gate validation SM commit before gate | baseline-nli | MRR dropped | 0.5000 | 0.3333 | 0.2000 | 0.2000 |
| obs-fb91e6f4-1773933244000 | working tree uncommitted changes gate validation SM commit before gate | baseline-nli | MRR dropped | 0.5000 | 0.3333 | 0.2000 | 0.2000 |
| obs-a02f1841-1774957353000 | markdown escaping MCP response formatter table cell | baseline-nli | MRR dropped | 0.3333 | 0.2000 | 0.2000 | 0.2000 |
| obs-2eb0aff5-1773310460000 | nan-002 knowledge import | baseline-nli | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-30b7b9c9-1774906266000 | risk pattern | baseline-nli | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-4268d867-1774745473000 | NLI graph inference source candidate selection phase data influence | baseline-nli | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-50273e07-1774566402000 | cache handle pattern Arc RwLock background tick read compute_report | baseline-nli | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-9bb67ef4-1774002107000 | subprocess test coverage missing offline acceptance criteria gate failure | baseline-nli | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-a92b01c4-1774484832000 | infra-001 tester background tasks integration test execution | baseline-nli | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-a92b01c4-1774484836000 | infra-001 tester background tasks integration test execution | baseline-nli | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-a92b01c4-1774484839000 | infra-001 tester background tasks integration test execution | baseline-nli | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-a92b01c4-1774484841000 | infra-001 tester background tasks integration test execution | baseline-nli | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-fb91e6f4-1773882525000 | supersession graph penalty | baseline-nli | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-50273e07-1774566492000 | ContradictionScanCacheHandle Arc RwLock background tick compute_report | baseline-nli | MRR dropped | 0.2500 | 0.2000 | 0.2000 | 0.2000 |
| obs-5becb974-1774975696000 | NLI inference graph edge testing patterns PPR discriminator routing | baseline-nli | MRR dropped | 0.2500 | 0.2000 | 0.2000 | 0.2000 |
| obs-5f33a80a-1774523338000 | positional column index SQLite analytics INSERT SELECT atomic change unit | baseline-nli | MRR dropped | 0.2500 | 0.2000 | 0.2000 | 0.2000 |
| obs-cdbdd4fc-1774263383000 | topic scan full table query caching store reads extraction pipeline | baseline-nli | MRR dropped | 0.2500 | 0.2000 | 0.2000 | 0.2000 |

_This list is a human-reviewed artifact. No automated gate logic is applied._

### 5.5 Zero-Regression Check — conf-boost-c

**35 regression(s) detected:**

| Scenario | Query | Profile | Reason | Baseline MRR | Candidate MRR | Baseline P@K | Candidate P@K |
|----------|-------|---------|--------|-------------|--------------|-------------|---------------|
| obs-bd98afa6-1773804590000 | SQLite connection pool async storage pattern unimatrix-store | conf-boost-c | MRR dropped | 1.0000 | 0.2000 | 0.2000 | 0.2000 |
| obs-ea862451-1774815867000 | scope expansion mid-session design rework vision alignment | conf-boost-c | MRR dropped | 1.0000 | 0.2500 | 0.2000 | 0.2000 |
| obs-50273e07-1774566845000 | connected_entry_count COUNT DISTINCT double-count UNION subquery graph edges | conf-boost-c | MRR dropped | 1.0000 | 0.3333 | 0.2500 | 0.2500 |
| obs-531f376f-1773534194000 | crt-018b effectiveness-driven retrieval | conf-boost-c | MRR dropped | 1.0000 | 0.3333 | 0.2000 | 0.2000 |
| obs-af1f3351-1774269500000 | mcp response module unconditional compile IndexBriefingService graceful degradation empty fallback | conf-boost-c | MRR dropped | 1.0000 | 0.3333 | 0.2000 | 0.2000 |
| obs-3d9b719e-1774217292000 | crt-026 WA-2 session context enrichment patterns procedures | conf-boost-c | MRR dropped | 1.0000 | 0.5000 | 0.2000 | 0.2000 |
| obs-4a5ba677-1774402758000 | FeatureKnowledgeReuse cross_feature_reuse compute_knowledge_reuse_for_sessions | conf-boost-c | MRR dropped | 1.0000 | 0.5000 | 0.2000 | 0.2000 |
| obs-50273e07-1774566839000 | lesson-learned failures gate rejection SQL aggregate | conf-boost-c | MRR dropped | 1.0000 | 0.5000 | 0.4000 | 0.4000 |
| obs-a1aaf24a-1774371798000 | error handling graceful degradation None fallback DB read session registration | conf-boost-c | MRR dropped | 1.0000 | 0.5000 | 0.2000 | 0.2000 |
| obs-b0be618b-1774523351000 | eval harness pattern procedure lesson Shannon entropy dual copy | conf-boost-c | MRR dropped | 1.0000 | 0.5000 | 0.2000 | 0.2000 |
| obs-d4042945-1774306402000 | validate_briefing_params MCP tool parameter validation | conf-boost-c | MRR dropped | 1.0000 | 0.5000 | 0.2000 | 0.2000 |
| obs-d4042945-1774306403000 | validate_briefing_params MCP tool parameter validation | conf-boost-c | MRR dropped | 1.0000 | 0.5000 | 0.2000 | 0.2000 |
| obs-ea4a711a-1774638811000 | bugfix-421 graph inference tick shuffle embedded_ids | conf-boost-c | MRR dropped | 1.0000 | 0.5000 | 0.2000 | 0.2000 |
| obs-5f33a80a-1774486351000 | context_cycle phase capture Thompson sampling relevance learning | conf-boost-c | MRR dropped | 0.5000 | 0.2500 | 0.2000 | 0.2000 |
| obs-18f5d7e9-1774716252000 | check inputs still in scope before proposing call site function design review | conf-boost-c | MRR dropped | 0.5000 | 0.3333 | 0.2000 | 0.2000 |
| obs-1ebc220a-1774994875000 | NLI detection guard predicate nli_detection_tick inference | conf-boost-c | MRR dropped | 0.5000 | 0.3333 | 0.2500 | 0.2500 |
| obs-3cd7df2a-1774750121000 | graph traversal unit testing patterns | conf-boost-c | MRR dropped | 0.5000 | 0.3333 | 0.2000 | 0.2000 |
| obs-5becb974-1774967689000 | PPR direction semantics reverse walk outgoing edges traversal | conf-boost-c | MRR dropped | 0.5000 | 0.3333 | 0.2000 | 0.2000 |
| obs-7c4e3cc4-1773804579000 | PERMISSIVE_AUTO_ENROLL registry enrollment trust level | conf-boost-c | MRR dropped | 0.5000 | 0.3333 | 0.2000 | 0.2000 |
| obs-a02f1841-1774916916000 | saturating arithmetic integer overflow u32 counter | conf-boost-c | MRR dropped | 0.5000 | 0.3333 | 0.2000 | 0.2000 |
| obs-abcc3cc8-1773157713000 | idempotent counter update topic deliveries | conf-boost-c | MRR dropped | 0.5000 | 0.3333 | 0.2000 | 0.2000 |
| obs-fb91e6f4-1773933243000 | working tree uncommitted changes gate validation SM commit before gate | conf-boost-c | MRR dropped | 0.5000 | 0.3333 | 0.2000 | 0.2000 |
| obs-fb91e6f4-1773933244000 | working tree uncommitted changes gate validation SM commit before gate | conf-boost-c | MRR dropped | 0.5000 | 0.3333 | 0.2000 | 0.2000 |
| obs-a02f1841-1774957353000 | markdown escaping MCP response formatter table cell | conf-boost-c | MRR dropped | 0.3333 | 0.2000 | 0.2000 | 0.2000 |
| obs-2eb0aff5-1773310460000 | nan-002 knowledge import | conf-boost-c | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-30b7b9c9-1774906266000 | risk pattern | conf-boost-c | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-4268d867-1774745473000 | NLI graph inference source candidate selection phase data influence | conf-boost-c | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-50273e07-1774566402000 | cache handle pattern Arc RwLock background tick read compute_report | conf-boost-c | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-50273e07-1774575754000 | worktree isolation design agents file leak main repo path discipline | conf-boost-c | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-9bb67ef4-1774002107000 | subprocess test coverage missing offline acceptance criteria gate failure | conf-boost-c | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-fb91e6f4-1773882525000 | supersession graph penalty | conf-boost-c | MRR dropped | 0.3333 | 0.2500 | 0.2000 | 0.2000 |
| obs-50273e07-1774566492000 | ContradictionScanCacheHandle Arc RwLock background tick compute_report | conf-boost-c | MRR dropped | 0.2500 | 0.2000 | 0.2000 | 0.2000 |
| obs-5becb974-1774975696000 | NLI inference graph edge testing patterns PPR discriminator routing | conf-boost-c | MRR dropped | 0.2500 | 0.2000 | 0.2000 | 0.2000 |
| obs-5f33a80a-1774523338000 | positional column index SQLite analytics INSERT SELECT atomic change unit | conf-boost-c | MRR dropped | 0.2500 | 0.2000 | 0.2000 | 0.2000 |
| obs-cdbdd4fc-1774263383000 | topic scan full table query caching store reads extraction pipeline | conf-boost-c | MRR dropped | 0.2500 | 0.2000 | 0.2000 | 0.2000 |

_This list is a human-reviewed artifact. No automated gate logic is applied._

## 7. Distribution Analysis

_ICD is raw Shannon entropy (natural log). Maximum value is ln(n_categories).
Values are comparable across profiles run with the same configured categories._

### CC@k Range by Profile

| Profile | Scenarios | Min | Max | Mean |
|---------|-----------|-----|-----|------|
| ablation-conf-zero | 1443 | 0.2000 | 1.0000 | 0.4524 |
| ablation-cosine-only | 1443 | 0.2000 | 1.0000 | 0.4524 |
| ablation-phase-zero | 1443 | 0.2000 | 1.0000 | 0.4524 |
| ablation-util-prov-zero | 1443 | 0.2000 | 1.0000 | 0.4524 |
| baseline-nli | 1443 | 0.2000 | 1.0000 | 0.4524 |
| conf-boost-c | 1443 | 0.2000 | 1.0000 | 0.4524 |

### ICD Range by Profile (max=ln(n))

| Profile | Scenarios | Min | Max | Mean |
|---------|-----------|-----|-----|------|
| ablation-conf-zero | 1443 | 0.0000 | 1.6094 | 0.6637 |
| ablation-cosine-only | 1443 | 0.0000 | 1.6094 | 0.6637 |
| ablation-phase-zero | 1443 | 0.0000 | 1.6094 | 0.6637 |
| ablation-util-prov-zero | 1443 | 0.0000 | 1.6094 | 0.6637 |
| baseline-nli | 1443 | 0.0000 | 1.6094 | 0.6637 |
| conf-boost-c | 1443 | 0.0000 | 1.6094 | 0.6637 |
