# ASS-035: Entry Pair Selection for NLI Extraction Strategy Evaluation

**Date**: 2026-04-01
**Researcher**: Claude (claude-sonnet-4-6)
**Source database**: `~/.unimatrix/0d62f3bf1bf46a0a/unimatrix.db`

Pairs are drawn from the live knowledge base to test whether input extraction strategy (A/B/C)
affects NLI entailment scores. The labeling question is:

> "Does knowing entry A make entry B more trustworthy, better understood, or less surprising?"
> `should_support: true` if yes, `false` if clearly unrelated, `borderline` if related but
> not logically entailing.

Category pair legend (from config `informs_category_pairs`):
- `lesson-learned → decision` ✓
- `lesson-learned → convention` ✓
- `pattern → decision` ✓
- `pattern → convention` ✓

---

## Group A: Same-Feature Pairs (8 pairs)

Positive controls — the NLI model SHOULD score these high if any extraction strategy works.
Both entries are from the same feature cycle and directly address the same principle.

---

### P01
- **A**: #376 · lesson-learned · nxs-008 · `database-init`
  - _DDL-before-migration ordering causes post-merge production failures_
- **B**: #375 · procedure · nxs-008 · `database-init`
  - _Database init ordering: migration before DDL in Store::open()_
- **should_support**: `true`
- **rationale**: The procedure IS the takeaway from the lesson. The lesson describes the failure
  caused by wrong ordering; the procedure defines the correct ordering rule. If you know A, B
  is not only unsurprising — it is the direct prescription A calls for.
- **category pair**: lesson-learned → procedure

---

### P02
- **A**: #2798 · lesson-learned · crt-023 · `unimatrix-embed`
  - _ONNX cross-encoder token_type_ids bug: NLI models often omit it from exported inputs_
- **B**: #2809 · pattern · crt-023 · `unimatrix-embed`
  - _NliProvider gotchas: softmax double-exp, pub(crate) visibility, label order, token_type_ids ONNX input_
- **should_support**: `true`
- **rationale**: The lesson is literally item 4 in the pattern's gotcha list. Entry A is a
  standalone deep-dive on the token_type_ids failure; entry B aggregates it with three other
  gotchas. A logically entails B's fourth item, and B's first three items are unrelated to A.
  Net: strong partial entailment.
- **category pair**: lesson-learned → pattern

---

### P03
- **A**: #665 · lesson-learned · vnc-004 · `unimatrix-server`
  - _File::create truncates before flock — TOCTOU in lock-file guards_
- **B**: #667 · pattern · vnc-004 · `unimatrix-server`
  - _Lock-then-mutate pattern for flock-based PID file guards_
- **should_support**: `true`
- **rationale**: The pattern is the direct fix from the lesson. The lesson says "don't truncate
  before lock"; the pattern says "open without truncate, acquire lock, THEN truncate and write."
  One entails the other completely.
- **category pair**: lesson-learned → pattern

---

### P04
- **A**: #3353 · lesson-learned · bugfix-358 · `contradiction`
  - _Rayon worker threads have no Tokio runtime — never call Handle::current() inside a rayon closure_
- **B**: #3354 · pattern · bugfix-358 · `contradiction`
  - _Pre-fetch Vec<EntryRecord> in Tokio context before RayonPool::spawn — quality gate pattern_
- **should_support**: `true`
- **rationale**: The pattern defines the solution to the panic described in the lesson.
  The lesson says Handle::current() panics in rayon; the pattern says "pre-fetch before
  spawn." Direct prescription-from-failure relationship.
- **category pair**: lesson-learned → pattern

---

### P05
- **A**: #1688 · lesson-learned · bugfix-277 · `unimatrix-server`
  - _spawn_blocking_with_timeout must be applied to ALL hot-path handlers at introduction time_
- **B**: #1369 · convention · vnc-008 · `rust-dev`
  - _MCP Tool 6-Step Handler Pipeline_
- **should_support**: `true`
- **rationale**: The convention's Step 4 explicitly mandates `spawn_blocking_with_timeout` for
  every MCP handler and references bugfix-236 — the same fix lineage as the lesson. The lesson's
  rule ("apply to ALL handlers") is encoded as a convention requirement. A entails B's Step 4.
- **category pair**: lesson-learned → convention ✓

---

### P06
- **A**: #3744 · pattern · crt-030 · `unimatrix-engine`
  - _PPR power iteration uses Direction::Outgoing (reverse walk) despite ADR-003 saying Incoming_
- **B**: #3750 · decision · crt-030 · `crt-030`
  - _ADR-003 crt-030: Edge Direction Semantics for PPR Traversal — Outgoing for Reverse/Transpose PPR_
- **should_support**: `true`
- **rationale**: The pattern describes the implementation reality (Outgoing direction); the ADR
  documents the decision and its rationale. The pattern supersedes an earlier pseudocode error;
  the ADR canonizes that correction. A and B are two forms of the same truth.
- **category pair**: pattern → decision ✓

---

### P07
- **A**: #374 · procedure · nxs-008 · `schema-migration`
  - _How to perform an in-place SQLite schema migration (column decomposition)_
- **B**: #375 · procedure · nxs-008 · `database-init`
  - _Database init ordering: migration before DDL in Store::open()_
- **should_support**: `true`
- **rationale**: Both procedures describe the same database migration sequencing requirement from
  different angles (A = full migration how-to, B = Store::open() init sequence). They address the
  same architectural invariant: migration before DDL.
- **category pair**: procedure → procedure (same-feature, same invariant)

---

### P08
- **A**: #2571 · pattern · crt-022 · `ml-inference-concurrency`
  - _Rayon-Tokio bridge pattern for CPU-bound ML inference_
- **B**: #2728 · pattern · crt-023 · `testing`
  - _Rayon W1-2 Compliance Test Pattern for NLI Inference Components_
- **should_support**: `true`
- **rationale**: The test pattern directly validates the bridge pattern's W1-2 compliance
  requirement. Entry B exists specifically to verify that code using pattern A runs on a rayon
  thread (not a tokio thread). A describes the implementation; B describes how to test it.
- **category pair**: pattern → pattern (W1-2 bridge → W1-2 test)

---

## Group B: Cross-Feature Pairs (7 pairs)

Real test of the extraction strategy — semantically related entries from different feature cycles.
These span the `informs_category_pairs` taxonomy from config.

---

### P09
- **A**: #376 · lesson-learned · nxs-008 · `database-init`
  - _DDL-before-migration ordering causes post-merge production failures_
- **B**: #2060 · decision · nxs-011 · `nxs-011`
  - _ADR-003 nxs-011: Migration Connection Sequencing — Dedicated non-pooled connection before pool construction_
- **should_support**: `borderline`
- **rationale**: The canonical cross-feature bridge from ASS-034 (confirmed missed by the
  knowledge base). Both enforce "migration first" ordering but at different abstraction layers
  (A: DDL after migration; B: pool construction after migration). A informs B — knowing A makes
  B less surprising — but A doesn't strictly entail B (B addresses connection sequencing, a
  distinct sub-problem). This is the hard test case.
- **category pair**: lesson-learned → decision ✓ | cross-feature (nxs-008 → nxs-011)

---

### P10
- **A**: #735 · lesson-learned · vnc-010 · `server-resilience`
  - _spawn_blocking Pool Saturation from Unbatched Fire-and-Forget DB Writes_
- **B**: #1369 · convention · vnc-008 · `rust-dev`
  - _MCP Tool 6-Step Handler Pipeline_
- **should_support**: `borderline`
- **rationale**: The lesson describes pool saturation from unbatched writes; the convention's
  Step 4 mandates client-facing timeout protection. Related diagnosis (spawn_blocking contention)
  but different fixes: A prescribes batching, B prescribes timeout. Informs but doesn't entail.
- **category pair**: lesson-learned → convention ✓ | cross-feature (vnc-010 → vnc-008)

---

### P11
- **A**: #3353 · lesson-learned · bugfix-358 · `contradiction`
  - _Rayon worker threads have no Tokio runtime — never call Handle::current() inside a rayon closure_
- **B**: #3660 · pattern · crt-029 · `testing`
  - _Grep Gate as Primary Test Coverage for Compile-Invisible Rayon/Tokio Boundary (R-09 Pattern)_
- **should_support**: `true`
- **rationale**: The grep gate pattern directly prevents the panic class described in the lesson.
  The lesson says Handle::current() panics in rayon and is compile-invisible; the pattern says
  "use grep gates as primary coverage for exactly this compile-invisible risk class." A's
  failure mode is B's target. Strong entailment across features.
- **category pair**: lesson-learned → pattern | cross-feature (bugfix-358 → crt-029)

---

### P12
- **A**: #378 · lesson-learned · nxs-008 · `testing`
  - _Schema migration tests must include old-schema databases, not only fresh ones_
- **B**: #238 · convention · project · `tester`
  - _Testing Infrastructure Convention_
- **should_support**: `borderline`
- **rationale**: The lesson describes a specific testing gap (no old-schema DB in CI);
  the convention describes general test infrastructure rules. Related domain (testing) but
  the convention doesn't reference the specific lesson. A informs B but doesn't entail it.
- **category pair**: lesson-learned → convention ✓ | cross-feature (nxs-008 → project-baseline)

---

### P13
- **A**: #1628 · lesson-learned · bugfix-264 · `unimatrix-server`
  - _Per-query full-store reads inside spawn_blocking cause MCP instability under load_
- **B**: #1367 · pattern · bugfix-236 · `server-resilience`
  - _spawn_blocking_with_timeout for MCP Handler Mutex Acquisition_
- **should_support**: `borderline`
- **rationale**: Both address spawn_blocking danger in MCP handlers: A is about per-query
  store reads causing mutex saturation; B is about adding timeouts to prevent indefinite
  client hangs. Shared diagnosis space (spawn_blocking contention) but different failure
  modes and fixes.
- **category pair**: lesson-learned → pattern | cross-feature (bugfix-264 → bugfix-236)

---

### P14
- **A**: #2571 · pattern · crt-022 · `ml-inference-concurrency`
  - _Rayon-Tokio bridge pattern for CPU-bound ML inference_
- **B**: #3741 · decision · crt-030 · `crt-030`
  - _ADR-008: Step 6d Latency Budget and RayonPool Offload (DEFERRED) — crt-030_
- **should_support**: `borderline`
- **rationale**: The deferral decision explicitly references the rayon-tokio bridge as "the
  established pattern" that would be used if/when PPR offload is implemented. A is the
  infrastructure the decision defers to. Strong contextual relationship; not strict entailment
  (the decision doesn't logically follow from the pattern — it just references it).
- **category pair**: pattern → decision ✓ | cross-feature (crt-022 → crt-030)

---

### P15
- **A**: #667 · pattern · vnc-004 · `unimatrix-server`
  - _Lock-then-mutate pattern for flock-based PID file guards_
- **B**: #245 · decision · col-006 · `col-006`
  - _ADR-004: Socket Lifecycle Uses Unconditional Unlink After PidGuard_
- **should_support**: `borderline`
- **rationale**: The socket lifecycle decision builds on top of PidGuard (which implements
  the flock pattern). B says "startup order: PidGuard::acquire → unlink socket → bind socket."
  The PID file pattern is the infrastructure B's ordering depends on, but B addresses socket
  lifecycle, a distinct concern. Informs, does not entail.
- **category pair**: pattern → decision ✓ | cross-feature (vnc-004 → col-006)

---

## Group C: Negative Controls (5 pairs)

These pairs are from completely different topic domains. Should score low (< 0.35) under
all extraction strategies. If any strategy produces high scores here, it indicates false
positives, not genuine entailment.

---

### P16
- **A**: #376 · lesson-learned · nxs-008 · `database-init`
  - _DDL-before-migration ordering causes post-merge production failures_
- **B**: #2701 · decision · crt-023 · `crt-023`
  - _ADR-002 crt-023: NLI Entailment Score Replaces rerank_score for Search Re-ranking Sort_
- **should_support**: `false`
- **rationale**: Database initialization ordering vs search re-ranking algorithm design.
  No semantic overlap beyond both being Unimatrix architectural decisions.

---

### P17
- **A**: #64 · decision · nxs-002 · `nxs-002`
  - _ADR-002: DistDot Distance Metric for Text Embeddings_
- **B**: #735 · lesson-learned · vnc-010 · `server-resilience`
  - _spawn_blocking Pool Saturation from Unbatched Fire-and-Forget DB Writes_
- **should_support**: `false`
- **rationale**: Vector distance metric selection vs server concurrency failure mode.
  Completely different subsystems (embedding index vs async runtime management).

---

### P18
- **A**: #63 · decision · nxs-002 · `nxs-002`
  - _ADR-001: hnsw_rs as Vector Index Library_
- **B**: #1688 · lesson-learned · bugfix-277 · `unimatrix-server`
  - _spawn_blocking_with_timeout must be applied to ALL hot-path handlers_
- **should_support**: `false`
- **rationale**: Library selection for ANN index vs MCP handler timeout discipline.
  No conceptual overlap.

---

### P19
- **A**: #239 · convention · project · `project`
  - _Feature Naming and Directory Convention_
- **B**: #3732 · decision · crt-030 · `crt-030`
  - _ADR-002 crt-030: personalized_pagerank() Function Signature and Algorithm Contract_
- **should_support**: `false`
- **rationale**: Feature naming conventions vs graph algorithm API design. Completely
  different domains (project management vs algorithm correctness).

---

### P20
- **A**: #2393 · decision · dsn-001 · `dsn-001`
  - _ADR-005 dsn-001: Preset Enum Design and Weight Table_
- **B**: #65 · decision · nxs-002 · `nxs-002`
  - _ADR-003: RwLock Concurrency Model for VectorIndex_
- **should_support**: `false`
- **rationale**: Confidence weight preset configuration vs vector index concurrency strategy.
  No semantic relationship.

---

## Group D: Cross-Feature Compatible-Category Negative Controls (5 pairs)

**Extension added 2026-04-01.** Purpose: test whether cosine similarity alone (without
`same_feature_cycle` filter) produces false positives on pairs that ARE in compatible
category pairs but have NO semantic relationship. All existing Group C negatives are in
incompatible categories — this group fills the gap.

If all Group D pairs score < 0.65, the `same_feature_cycle` filter is not strictly necessary.
If any pair scores ≥ 0.65, the filter is load-bearing.

---

### P21
- **A**: #665 · lesson-learned · vnc-004 · `unimatrix-server`
  - _File::create truncates before flock — TOCTOU in lock-file guards_
- **B**: #2701 · decision · crt-023 · `crt-023`
  - _ADR-002 crt-023: NLI Entailment Score Replaces rerank_score for Search Re-ranking Sort_
- **should_support**: `false`
- **rationale**: File locking failure mode vs NLI score sort design. Completely different subsystems
  (server process guards vs search re-ranking algorithm). Compatible category pair (lesson→decision)
  but zero semantic overlap.
- **category pair**: lesson-learned → decision ✓ | cross-feature (vnc-004 → crt-023)

---

### P22
- **A**: #1628 · lesson-learned · bugfix-264 · `unimatrix-server`
  - _Per-query full-store reads inside spawn_blocking cause MCP instability under load_
- **B**: #64 · decision · nxs-002 · `nxs-002`
  - _ADR-002: DistDot Distance Metric for Text Embeddings_
- **should_support**: `false`
- **rationale**: Spawn_blocking mutex contention vs embedding distance metric selection.
  One is about server concurrency failure; the other is a vector math design choice made
  during foundation setup. No semantic relationship.
- **category pair**: lesson-learned → decision ✓ | cross-feature (bugfix-264 → nxs-002)

---

### P23
- **A**: #3353 · lesson-learned · bugfix-358 · `contradiction`
  - _Rayon worker threads have no Tokio runtime — never call Handle::current() inside a rayon closure_
- **B**: #245 · decision · col-006 · `col-006`
  - _ADR-004: Socket Lifecycle Uses Unconditional Unlink After PidGuard_
- **should_support**: `false`
- **rationale**: Rayon/Tokio threading boundary panic vs Unix socket startup ordering.
  Different subsystems, different problem domains. Knowing the rayon panic lesson tells
  you nothing about socket unlink ordering.
- **category pair**: lesson-learned → decision ✓ | cross-feature (bugfix-358 → col-006)

---

### P24
- **A**: #667 · pattern · vnc-004 · `unimatrix-server`
  - _Lock-then-mutate pattern for flock-based PID file guards_
- **B**: #2701 · decision · crt-023 · `crt-023`
  - _ADR-002 crt-023: NLI Entailment Score Replaces rerank_score for Search Re-ranking Sort_
- **should_support**: `false`
- **rationale**: File locking implementation pattern vs search re-ranking algorithm design.
  Completely different subsystems. Compatible category pair (pattern→decision) but zero
  semantic overlap.
- **category pair**: pattern → decision ✓ | cross-feature (vnc-004 → crt-023)

---

### P25
- **A**: #2571 · pattern · crt-022 · `ml-inference-concurrency`
  - _Rayon-Tokio bridge pattern for CPU-bound ML inference_
- **B**: #238 · convention · project · `tester`
  - _Testing Infrastructure Convention_
- **should_support**: `false`
- **rationale**: ML inference concurrency implementation pattern vs general test infrastructure
  conventions. One is about how to offload ONNX inference to a rayon pool; the other defines
  how test fixtures and helpers are structured. No semantic relationship.
- **category pair**: pattern → convention ✓ | cross-feature (crt-022 → project)

---

## Summary Table

| Pair | A ID | B ID | A Category | B Category | Feature Relation | should_support |
|------|------|------|-----------|-----------|-----------------|---------------|
| P01  | 376  | 375  | lesson-learned | procedure | same (nxs-008) | true |
| P02  | 2798 | 2809 | lesson-learned | pattern | same (crt-023) | true |
| P03  | 665  | 667  | lesson-learned | pattern | same (vnc-004) | true |
| P04  | 3353 | 3354 | lesson-learned | pattern | same (bugfix-358) | true |
| P05  | 1688 | 1369 | lesson-learned | convention | cross | true |
| P06  | 3744 | 3750 | pattern | decision | same (crt-030) | true |
| P07  | 374  | 375  | procedure | procedure | same (nxs-008) | true |
| P08  | 2571 | 2728 | pattern | pattern | same (crt-022/023) | true |
| P09  | 376  | 2060 | lesson-learned | decision | cross | borderline |
| P10  | 735  | 1369 | lesson-learned | convention | cross | borderline |
| P11  | 3353 | 3660 | lesson-learned | pattern | cross | true |
| P12  | 378  | 238  | lesson-learned | convention | cross | borderline |
| P13  | 1628 | 1367 | lesson-learned | pattern | cross | borderline |
| P14  | 2571 | 3741 | pattern | decision | cross | borderline |
| P15  | 667  | 245  | pattern | decision | cross | borderline |
| P16  | 376  | 2701 | lesson-learned | decision | cross | false |
| P17  | 64   | 735  | decision | lesson-learned | cross | false |
| P18  | 63   | 1688 | decision | lesson-learned | cross | false |
| P19  | 239  | 3732 | convention | decision | cross | false |
| P20  | 2393 | 65   | decision | decision | cross | false |
| P21  | 665  | 2701 | lesson-learned | decision | cross | false |
| P22  | 1628 | 64   | lesson-learned | decision | cross | false |
| P23  | 3353 | 245  | lesson-learned | decision | cross | false |
| P24  | 667  | 2701 | pattern | decision | cross | false |
| P25  | 2571 | 238  | pattern | convention | cross | false |

**Counts**: 8 `true` · 7 `borderline` · 10 `false` (5 incompatible-category + 5 compatible-category)
**Group A** (same-feature, positive controls): P01–P08
**Group B** (cross-feature, semantic): P09–P15
**Group C** (incompatible-category negatives): P16–P20
**Group D** (compatible-category cross-feature negatives): P21–P25

**Config-defined `informs_category_pairs` coverage**:
- lesson-learned → decision: P09, P16, P21, P22, P23
- lesson-learned → convention: P05, P10, P12
- pattern → decision: P06, P14, P15, P24
- pattern → convention: P25
