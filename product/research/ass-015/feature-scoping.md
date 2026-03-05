# ASS-015: Passive Knowledge Acquisition — Feature Scoping

## Overview

Five features transform Unimatrix from requiring explicit `context_store` calls to passively extracting knowledge from agent behavioral signals. The system that delivers and evolves knowledge now also *creates* it.

Research basis: 8 documents in `product/research/ass-015/` covering existing signals, novel approaches, signal taxonomy, architecture patterns, competitive landscape, decision analysis, self-learning neural design, and data unification analysis.

---

## Feature 1: Data Path Unification — `col-012`

**Goal:** Eliminate the dual data path (JSONL files + UDS/SQLite tables) by persisting all hook events in SQLite. Provides indexed, JOINable observation data for both retrospective analysis and knowledge extraction.

### What Gets Built
- `observations` table in SQLite (schema migration):
  ```sql
  CREATE TABLE observations (
      session_hash INTEGER NOT NULL,
      ts_millis    INTEGER NOT NULL,
      hook         TEXT NOT NULL,
      session_id   TEXT NOT NULL,
      tool         TEXT,
      input        TEXT,
      response_size INTEGER,
      response_snippet TEXT,
      PRIMARY KEY (session_hash, ts_millis)
  );
  CREATE INDEX idx_observations_session ON observations(session_id);
  CREATE INDEX idx_observations_ts ON observations(ts_millis);
  ```
- `RecordEvent` handler in `uds_listener.rs` persists ALL events (not just rework candidates) via `spawn_blocking`
- Retrospective pipeline (`unimatrix-observe`) reads from `observations` table instead of JSONL files
- Session discovery via `SESSIONS` table instead of directory scanning
- Feature attribution via `SESSIONS.feature_cycle` instead of content scanning
- JSONL write path removed from hooks
- JSONL discovery/parsing code removed from `unimatrix-observe`

### What Doesn't Change
- All 21 detection rules — same input type (`Vec<ObservationRecord>`), different data source
- `MetricVector` computation — unchanged
- Baseline comparison — unchanged
- `context_retrospective` MCP tool — unchanged interface

### Why First
Every subsequent feature depends on queryable, indexed observation data. JSONL is fragile (no indexing, no JOINs, manual rotation). The hook stream already carries all the data — RecordEvent just discards it. This also resolves the "Retrospective Pipeline v2" note in PRODUCT-VISION.md.

### Scope
~200 lines changed, net code reduction. Comparable to a small feature cycle.

### Dependencies
- nxs-008 ✅ (Schema Normalization — SQLite is the backend)
- col-010 ✅ (SESSIONS table exists for session discovery)

### Research Reference
`product/research/ass-015/data-unification-analysis.md`

---

## Feature 2: Extraction Rule Engine — `col-013`

**Goal:** Rule-based knowledge extraction from observation data. Deterministic, testable, zero external dependencies. Covers the highest-confidence extraction patterns.

### What Gets Built
- `ExtractionRule` trait (mirrors `DetectionRule` from col-002):
  ```rust
  pub trait ExtractionRule {
      fn name(&self) -> &str;
      fn evaluate(&self, observations: &[ObservationRow], store: &Store) -> Vec<ProposedEntry>;
  }
  ```
- 5 initial extraction rules:
  1. **Knowledge Gap** — `context_search` with zero results, same query across 2+ features → gap entry (`category: "gap"`)
  2. **Implicit Convention** — Same file/path pattern in 100% of features → convention entry
  3. **Dead Knowledge** — Entry with access cliff (accessed in features 1-N, not accessed in N+1 through N+5) → deprecation signal
  4. **Recurring Friction** — Same hotspot (from col-002 detection rules) in 3+ features → lesson-learned entry
  5. **File Dependency** — Consistent read-before-edit chains across 3+ features → dependency entry
- Quality gate pipeline:
  - Near-duplicate check (cosine >= 0.92, reuses existing dedup infrastructure)
  - **Point-of-insertion contradiction check** — refactored from crt-003 (see CRT Integration below)
  - Content validation (min length, category allowlist)
  - Rate limit (max 10 auto-extractions per hour)
  - Cross-feature validation (no entry from single observation; minimum 2-5 features depending on type)
  - Confidence floor (< 0.2 → discard)
- Auto-extracted entries stored with:
  - `trust_source: "auto"` (new value — see CRT Integration below)
  - `status: Active` for rules with >= 0.6 extraction confidence
  - `status: Proposed` for rules with 0.4-0.6 extraction confidence
  - Provenance metadata linking to source observations
- **Background maintenance tick** — automatic periodic maintenance replacing the `maintain=true` manual trigger:
  - Single `tokio::spawn` with `tokio::time::interval` (~1 hour)
  - `maintenance_tick()` checks what's needed and runs only what's necessary:
    - Confidence refresh (batch 100 stale entries) — was crt-005 maintain path
    - Co-access cleanup (>30 day pairs) — was crt-004 maintain path
    - HNSW graph compaction (if stale ratio > 10%) — was crt-005 maintain path
    - Session GC (timed-out cleanup) — was col-010 maintain path
    - Observation file cleanup (60-day retention) — was col-002 maintain path
  - Same `spawn_blocking` pattern as existing operations
  - The extraction pipeline itself also needs periodic triggers (run rules on accumulated observations) — maintenance piggybacks on the same timer infrastructure
  - ~100 lines
- **`context_status` becomes read-only** — the `maintain` parameter is deprecated (or retained as "run NOW" emergency override). `context_status` reports maintenance status (last run, next scheduled) but does not perform writes. This resolves the security concern of a "read" function performing writes.

### CRT Integration: Refactors Required

**crt-002 (Confidence Evolution):**
- Add `"auto"` to trust_source scoring: `"auto" → 0.35` (between "agent" at 0.5 and "other" at 0.3)
- Later: add `"neural"` → 0.4 when crt-007 ships
- ~5 lines in confidence computation

**crt-003 (Contradiction Detection):**
- Extract `check_entry_contradiction(proposed: &str, store: &Store, vector: &VectorIndex, embed: &EmbedAdapter) -> Option<ContradictionPair>` from existing `scan_contradictions()` batch function
- The batch scan loops over all entries calling the core logic; factor out the core logic as a reusable function
- Extraction pipeline calls the single-entry function before storing
- Full `scan_contradictions()` remains unchanged for `context_status`
- ~30 lines refactored (extract inner function, no logic change)

**crt-005 (Coherence Gate):**
- Add per-trust_source breakdown to lambda diagnostics
- StatusReport gains: `coherence_by_source: HashMap<String, f64>` — segments lambda dimensions by trust_source
- Enables monitoring: "Lambda for auto-extracted entries: 0.65 vs agent-stored: 0.89"
- ~40 lines in status computation

### What This Supersedes
- **col-005 (Auto-Knowledge Extraction)** — col-013 is the expanded, research-informed version of col-005. col-005's three tiers (structural conventions, procedural knowledge, dependency graphs) map directly to rules 2, 4, and 5 above. col-005 is absorbed into col-013.

### Scope
~600 lines new code + ~75 lines CRT refactors. Extends unimatrix-observe patterns.

### Dependencies
- col-012 ✅ (observations table for queryable data)
- crt-002, crt-003, crt-005 (minor refactors, not blocking — can be done in same feature cycle)
- crt-005 `maintain=true` path (code moves from status handler to background tick — not deleted, relocated)

### Research References
- `product/research/ass-015/signal-taxonomy.md` (extraction patterns KE-01 through KE-10)
- `product/research/ass-015/architecture-patterns.md` (hybrid architecture)
- `product/research/ass-013/auto-knowledge.md` (original col-005 scoping)

---

## Feature 3: Neural Extraction Pipeline — `crt-007`

**Goal:** Integrate ruv-fann neural models for signal classification and convention scoring. Shadow mode validation before activation. Factor training infrastructure out of unimatrix-adapt for shared use.

### What Gets Built

**Shared training infrastructure (extracted from unimatrix-adapt):**
- `unimatrix-learn` module or crate (name TBD) containing:
  - `TrainingReservoir<T>` — generic reservoir sampling with configurable capacity (currently hardcoded in unimatrix-adapt)
  - `EwcState` — Elastic Weight Consolidation++ state management (currently coupled to MicroLoRA in unimatrix-adapt)
  - `ModelRegistry` — production/shadow/previous model versioning with promotion criteria
  - `TrainingTrigger` — threshold-based fire-and-forget training dispatch
- unimatrix-adapt refactored to consume these shared primitives instead of its own copies
- Both crt-006 (embedding adaptation) and crt-007 (extraction models) use the same infrastructure

**Neural models (ruv-fann):**
- Signal Classifier MLP (~5MB): `input(N) → hidden(64, sigmoid-symmetric) → hidden(32) → output(5, softmax)`
  - Classifies signal digests → `convention | pattern | gap | dead | noise`
  - Input: structured features from extraction rule outputs (search miss count, co-access density, consistency score, feature count, etc.)
  - Retraining trigger: every 20 classified signals with feedback
  - CPU time: <5 seconds
- Convention Scorer MLP (~2MB): `input(M) → hidden(32) → output(1, sigmoid)`
  - Scores whether a cross-feature pattern is a genuine convention (0.0-1.0)
  - Input: consistency_ratio, feature_count, deviation_count, category_distribution, age_days
  - Retraining trigger: every 5 convention evaluations with outcome
  - CPU time: <2 seconds

**Shadow mode infrastructure:**
- Models run alongside extraction rules, results logged but not stored
- Comparison metrics: precision, recall vs rule-only extraction
- Promotion criteria: shadow accuracy >= production (rule-only) accuracy, minimum 20 evaluations, no regression per category
- Auto-rollback: accuracy drop > 5% after promotion

**Cold start:**
- Models start with hand-tuned baseline weights (bias toward conservative: classifier biases toward `noise`, convention scorer requires strong evidence)
- Features 1-5: observation-only (rules extract, models observe)
- Features 6+: shadow mode (models predict, results compared but not stored)
- After shadow validation: promote to production

### CRT Integration: Training Infrastructure Refactor

**unimatrix-adapt refactor:**
- Extract `TrainingReservoir` (currently `Reservoir` in adapt/src/reservoir.rs) into shared module
- Extract `EwcState` (currently `EwcPlusPlus` in adapt/src/ewc.rs`) into shared module
- Extract model persistence helpers (atomic write, generation tracking) into shared module
- unimatrix-adapt becomes a consumer of the shared module + MicroLoRA-specific code
- ~200 lines moved (not rewritten), ~50 lines of new generic interfaces

### What This Does NOT Include
- Duplicate Detector, Pattern Merger, Entry Writer Scorer — deferred to crt-009
- LLM API integration — deferred to crt-009
- Lesson extraction from failures — permanently agent-driven

### Scope
~800 lines (shared infrastructure extraction: ~250, neural models: ~350, shadow mode: ~200).

### Dependencies
- col-013 ✅ (extraction rules producing signal digests for classification)
- crt-006 ✅ (unimatrix-adapt exists to be refactored)

### Research References
- `product/research/ass-015/self-learning-neural-design.md` (model architectures, training signals)
- `product/research/ass-015/self-retraining.md` (retraining patterns, Rust ML frameworks)

### Open Risk
ruv-fann is v0.2.0 (~4K downloads). If RPROP implementation proves insufficient, fall back to ndarray + hand-rolled training following unimatrix-adapt's proven approach. The shared training infrastructure is ML-framework-agnostic.

---

## Feature 4: Continuous Self-Retraining — `crt-008`

**Goal:** Close the self-learning loop. Utilization signals become training labels. Models retrain incrementally via fire-and-forget background tasks. The system gets better with every feature delivered.

### What Gets Built
- Feedback-to-label pipeline:
  | Utilization Event | Training Label | For Which Model(s) |
  |-------------------|---------------|---------------------|
  | Entry gets helpful vote | Positive for classification + writer quality | Classifier |
  | Entry gets unhelpful vote | Negative for classification | Classifier |
  | Entry never accessed (10+ features) | Weak negative | Classifier, Convention Scorer |
  | Entry corrected (category changed) | Ground truth re-labeling | Classifier |
  | Entry deprecated | Negative for whatever produced it | Classifier, Convention Scorer |
  | Convention followed by all agents | Positive convention label | Convention Scorer |
  | Convention deviated from successfully | Negative convention label | Convention Scorer |
  | Feature outcome: success | Positive for entries injected | Classifier |
  | Feature outcome: rework | Weak negative for entries injected | Classifier |
- EWC++ regularization per extraction model (reusing shared infrastructure from crt-007)
- Threshold-triggered retraining:
  - Classifier: every 20 classified signals with feedback, batch size 16
  - Convention Scorer: every 5 convention evaluations, batch size 4
- Model promotion criteria (from crt-007 shadow mode):
  - Shadow accuracy >= production accuracy on held-out validation
  - No regression on any category
  - Minimum 20 shadow evaluations
- Auto-rollback:
  - Accuracy drops > 5% after promotion → revert to previous
  - Any category accuracy drops > 10% → revert
  - NaN/Inf in weights → revert

### What Gets Built (Signal Capture Points)
- Hook into `UsageService::record_mcp_usage()` — when helpful/unhelpful recorded for auto-extracted entries, generate training label
- Hook into `context_correct` handler — when auto-extracted entry corrected, generate ground truth label
- Hook into `context_deprecate` handler — when auto-extracted entry deprecated, generate negative label
- Hook into session close — when feature outcome known, generate weak labels for all injected auto-entries
- All labels flow to appropriate `TrainingReservoir` via fire-and-forget

### Timeline to Value
- After ~5 features with feedback: first meaningful retraining step
- After ~20 features: models well-calibrated for project domain
- After ~50 features: deeply domain-adapted, high precision

### Scope
~600 lines. Heavily reuses shared infrastructure from crt-007.

### Dependencies
- crt-007 ✅ (neural models + shared training infrastructure exist)
- crt-001 ✅ (usage tracking provides helpful/unhelpful signals)
- col-001 ✅ (outcome tracking provides feature success/rework signals)

### Research References
- `product/research/ass-015/self-learning-neural-design.md` (training label generation, retraining schedule)
- `product/research/ass-015/self-retraining.md` (online learning patterns, EWC++)

---

## Feature 5: Advanced Models + Optional LLM — `crt-009`

**Goal:** Add the remaining 3 neural models for dedup, merging, and text quality. Optionally add LLM API tier for lesson extraction from failures (the one thing neural models can't do).

### What Gets Built

**Neural models (ruv-fann):**
- Duplicate Detector (~10MB): Siamese MLP on 384-dim embeddings
  - Input: concat of two adapted embeddings (768-dim)
  - Output: duplicate | extend | distinct
  - Training signal: correction chains (duplicate), high co-access (extend), different topics (distinct)
  - Retraining: every 10 correction events
- Pattern Merger (~50MB): Encoder + merger MLP
  - Decides whether N similar signal observations should merge into one entry
  - Training signal: manual merges via `context_correct`, separate entries created instead of merge
  - Retraining: every 10 merge decisions
- Entry Writer Scorer (~20MB): MLP quality scorer for template-generated entries
  - Template fills structured fields; scorer evaluates coherence, completeness, specificity
  - Training signal: helpful/unhelpful votes on generated entries, access counts
  - Retraining: every 10 generated entries with feedback

**Optional LLM tier:**
- `LlmClient` trait + Claude Haiku implementation
- Prompt builder with signal batch formatting
- Response parser with structured JSON extraction
- Batch scheduler (session-count or time-based triggers)
- Graceful degradation: LLM tier disabled when no API key configured
- Cost: ~$0.01-$2.00/day with batch processing

**New MCP tool:**
- `context_review` — human review of Proposed entries with accept/reject/modify actions
- Feeds rejection signals back to training pipeline

### What This Enables
- Neural models handle dedup and merge decisions that rules can't
- Entry text quality improves over time (writer scorer learns from votes)
- LLM tier handles lesson extraction from failure traces (optional)
- Human review provides highest-quality training labels

### Scope
~1,000 lines (3 models: ~500, LLM client: ~300, review tool: ~200).

### Dependencies
- crt-008 ✅ (retraining pipeline exists)
- col-003 (Process Proposals) — `context_review` is related to proposal review workflow; coordinate scope

### Research References
- `product/research/ass-015/self-learning-neural-design.md` (model architectures)
- `product/research/ass-015/decision-analysis.md` (three-tier architecture)

---

## Summary: Dependency Chain

```
col-012: Data Path Unification
    │   (~200 lines, net reduction)
    │   Eliminates JSONL, observations table in SQLite
    │
    └─► col-013: Extraction Rule Engine
         │   (~675 lines, absorbs col-005)
         │   5 extraction rules + quality gates
         │   Background maintenance tick (automatic, replaces maintain=true)
         │   context_status becomes read-only
         │   CRT refactors: trust_source, contradiction check, lambda segmentation
         │
         └─► crt-007: Neural Extraction Pipeline
              │   (~800 lines)
              │   Shared training infra (extracted from unimatrix-adapt)
              │   Signal Classifier + Convention Scorer
              │   Shadow mode validation
              │
              └─► crt-008: Continuous Self-Retraining
                   │   (~600 lines)
                   │   Feedback-to-label pipeline
                   │   EWC++ per model, fire-and-forget training
                   │
                   └─► crt-009: Advanced Models + Optional LLM
                        (~1,000 lines)
                        3 more neural models + LLM tier + review tool
```

**Total: ~3,275 lines across 5 features.**

### Minimum Viable Passive Acquisition
col-012 + col-013 (~875 lines) delivers rule-based extraction with quality gates and automatic maintenance. The system detects knowledge gaps, discovers conventions, flags dead knowledge, and extracts file dependencies — all without neural models or LLM. Maintenance runs automatically without agent or human intervention.

### Full Self-Learning Pipeline
All 5 features (~3,275 lines) delivers the complete self-learning vision: rules + neural models + continuous retraining + optional LLM. The system gets better with every feature delivered.

---

## CRT Integration Summary

| Existing Feature | Refactor | Lines | In Which Feature |
|-----------------|----------|-------|-----------------|
| crt-002 (Confidence) | Add `"auto"` and `"neural"` trust_source values | ~5 | col-013 |
| crt-003 (Contradiction) | Extract single-entry check from batch scan | ~30 | col-013 |
| crt-005 (Coherence) | Per-trust_source lambda breakdown | ~40 | col-013 |
| crt-005 (Maintenance) | Relocate `maintain=true` operations to background tick, `context_status` read-only | ~100 | col-013 |
| crt-006 / unimatrix-adapt | Extract TrainingReservoir, EWC++, persistence into shared module | ~250 (moved) | crt-007 |

---

## Architectural Boundary: Per-Repo Scope

All passive acquisition features (col-012 through crt-009) are scoped to a **single repository**. Each project's data directory (`~/.unimatrix/{project_hash}/`) contains its own observations, extraction models, training state, and knowledge entries. Intelligence does not bleed across repositories.

A persistent daemon serving multiple repositories simultaneously is a **dsn-phase concern** (Milestone 8: Multi-Project). The current session-based server model (spawned per Claude Code session, exits on session close) is sufficient for the entire ASS-015 pipeline. Background tasks (maintenance tick, extraction triggers, neural retraining) run during the session lifetime via `tokio::spawn` — no daemon mode required.

When multi-repo support arrives (dsn-001/002), the daemon model becomes relevant: a single long-running process partitioning data streams, training state, and extraction pipelines per project. The shared training infrastructure (extracted in crt-007) and per-project data isolation (already enforced by `{project_hash}` partitioning) provide the foundation for that evolution — but it is explicitly out of scope for ASS-015.
