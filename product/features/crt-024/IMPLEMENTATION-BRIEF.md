# crt-024: Ranking Signal Fusion (WA-0) — Implementation Brief

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-024/SCOPE.md |
| Architecture | product/features/crt-024/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-024/specification/SPECIFICATION.md |
| Risk Strategy | product/features/crt-024/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-024/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| `SearchService` (search pipeline) | pseudocode/search-service.md | test-plan/search-service.md |
| `InferenceConfig` (weight fields + validation) | pseudocode/inference-config.md | test-plan/inference-config.md |
| `compute_fused_score` (pure function) | pseudocode/compute-fused-score.md | test-plan/compute-fused-score.md |
| `ScoreWeights` / `FusedScoreInputs` (structs) | pseudocode/score-structs.md | test-plan/score-structs.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Goal

Replace the sequential two-pass ranking pipeline in `SearchService` (NLI sort in Step 7, co-access re-sort in Step 8) with a single linear combination where all six ranking signals are normalized to [0, 1] and weighted proportionally via config-driven `InferenceConfig` fields. The resulting fused formula is the canonical feature vector interface for W3-1 (GNN training), ensuring every signal that influences ranking is a named, weighted, tunable dimension — and that no later additive step can override an earlier semantic judgment.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|------------|--------|----------|
| Six-term vs. four-term formula | Six-term is the implementation target; vision's four-term formula was illustrative, not exhaustive | SCOPE.md AC-10, ADR-001 | architecture/ADR-001-six-term-formula-canonicalization.md |
| `apply_nli_sort` fate | Removed; `try_nli_rerank` returns `Option<Vec<NliScores>>` (raw scores, no sort); NLI entailment consumed inline in the scoring loop; all crt-023 unit tests for `apply_nli_sort` migrated to fused scorer tests | ADR-002 | architecture/ADR-002-apply-nli-sort-removal.md |
| Default fusion weights | `w_nli=0.35, w_sim=0.25, w_conf=0.15, w_coac=0.10, w_util=0.05, w_prov=0.05` (sum=0.95, 0.05 headroom for WA-2); derived from signal-role reasoning and numerically verified for AC-11, Constraint 9, Constraint 10 | ADR-003 | architecture/ADR-003-default-fusion-weights.md |
| Formula implementation shape | Extracted as `pub(crate) fn compute_fused_score(inputs: &FusedScoreInputs, weights: &FusionWeights) -> f64`; status_penalty applied at call site, not inside the function; NLI re-normalization applied at call site before passing `FusionWeights` | ADR-004 | architecture/ADR-004-formula-as-extractable-pure-function.md |
| `utility_delta` normalization | Shift-and-scale: `(utility_delta + UTILITY_PENALTY) / (UTILITY_BOOST + UTILITY_PENALTY)` maps [-0.05, +0.05] to [0, 1]; resolves WARN-2 (R-01 divergence) — FR-05 canonically uses this formula | SPECIFICATION.md FR-05, ALIGNMENT-REPORT.md WARN-2 | architecture/ADR-001-six-term-formula-canonicalization.md |
| boost_map prefetch placement | Moved to Step 6c — fully awaited before Step 7 NLI scoring begins; resolves SR-07 | ARCHITECTURE.md §Data Flow | architecture/ARCHITECTURE.md |
| WA-2 extension contract | Add `phase_boost_norm: f64` to `FusedScoreInputs`, `w_phase: f64` to `FusionWeights`, one term to `compute_fused_score`, and `w_phase` to the sum validation; `w_phase` defaults to 0.0 | ARCHITECTURE.md §WA-2 Extension Point | architecture/ADR-004-formula-as-extractable-pure-function.md |

---

## Files to Create / Modify

| File | Change | Summary |
|------|--------|---------|
| `crates/unimatrix-server/src/infra/config.rs` | Modify | Add six `f64` weight fields (`w_sim`, `w_nli`, `w_conf`, `w_coac`, `w_util`, `w_prov`) with `#[serde(default)]` to `InferenceConfig`; extend `validate()` with per-field range checks and six-term sum ≤ 1.0 check |
| `crates/unimatrix-server/src/services/search.rs` | Modify | Remove `apply_nli_sort`; change `try_nli_rerank` return type to `Option<Vec<NliScores>>`; add Step 6c boost_map prefetch; implement single fused scoring pass (Step 7); update `ScoredEntry.final_score` to fused formula; update all score-value test assertions |
| `crates/unimatrix-server/src/services/search.rs` or adjacent module | Modify/Create | Define `FusedScoreInputs` and `FusionWeights` structs; implement `pub(crate) fn compute_fused_score`; implement `FusionWeights::effective(nli_available: bool)` for re-normalization |

No engine crate files are modified. No schema migration. No new external dependencies.

---

## Data Structures

### `FusedScoreInputs`

All fields `f64` in [0.0, 1.0].

```
FusedScoreInputs {
    similarity:     f64,  // HNSW cosine (bi-encoder recall)
    nli_entailment: f64,  // cross-encoder entailment (0.0 when NLI absent)
    confidence:     f64,  // Wilson score composite (EntryRecord.confidence)
    coac_norm:      f64,  // raw_boost / MAX_CO_ACCESS_BOOST
    util_norm:      f64,  // (utility_delta + UTILITY_PENALTY) / (UTILITY_BOOST + UTILITY_PENALTY)
    prov_norm:      f64,  // prov_boost / PROVENANCE_BOOST; 0.0 when PROVENANCE_BOOST == 0
}
```

WA-2 extension: add `phase_boost_norm: f64` here.

### `FusionWeights`

All fields `f64` in [0.0, 1.0], sum ≤ 1.0.

```
FusionWeights {
    w_sim:  f64,   // default 0.25
    w_nli:  f64,   // default 0.35
    w_conf: f64,   // default 0.15
    w_coac: f64,   // default 0.10
    w_util: f64,   // default 0.05
    w_prov: f64,   // default 0.05
}
```

`FusionWeights::effective(nli_available: bool) -> FusionWeights` — returns a derived weight set. When `!nli_available`: sets `w_nli = 0.0`, re-normalizes remaining five by dividing each by `(w_sim + w_conf + w_coac + w_util + w_prov)`; guards zero-denominator by returning all-zeros without panic.

WA-2 extension: add `w_phase: f64` here and in the sum validation.

### `InferenceConfig` additions (in `infra/config.rs`)

Six new `f64` fields under `[inference]` TOML section:

```
w_sim:  f64   #[serde(default = "default_w_sim")]   // 0.25
w_nli:  f64   #[serde(default = "default_w_nli")]   // 0.35
w_conf: f64   #[serde(default = "default_w_conf")]  // 0.15
w_coac: f64   #[serde(default = "default_w_coac")]  // 0.10
w_util: f64   #[serde(default = "default_w_util")]  // 0.05
w_prov: f64   #[serde(default = "default_w_prov")]  // 0.05
```

`validate()` additions: per-field `[0.0, 1.0]` check (structured `NliFieldOutOfRange`-style error); six-term sum > 1.0 check (new `FusionWeightSumExceeded` variant naming all six values and computed sum).

### Canonical Fused Scoring Formula

```
fused_score =
    w_sim  * similarity_score
  + w_nli  * nli_entailment_score
  + w_conf * confidence_score
  + w_coac * (raw_co_access_boost / MAX_CO_ACCESS_BOOST)
  + w_util * ((utility_delta + UTILITY_PENALTY) / (UTILITY_BOOST + UTILITY_PENALTY))
  + w_prov * (prov_boost / PROVENANCE_BOOST)   // 0.0 if PROVENANCE_BOOST == 0

final_score = fused_score * status_penalty
```

`fused_score ∈ [0.0, 1.0]` by construction when weights sum ≤ 1.0 and all inputs are in [0, 1].

---

## Function Signatures

```rust
// Pure fused scorer — no async, no locks, no service state
pub(crate) fn compute_fused_score(
    inputs: &FusedScoreInputs,
    weights: &FusionWeights,
) -> f64

// Weight re-normalization for NLI-absent path
impl FusionWeights {
    pub(crate) fn effective(&self, nli_available: bool) -> FusionWeights
}

// InferenceConfig validation extension
impl InferenceConfig {
    pub fn validate(&self) -> Result<(), ConfigError>
    // (extended; existing signature unchanged)
}

// try_nli_rerank — new return type (was Option<Vec<(EntryRecord, f64)>>)
async fn try_nli_rerank(
    nli: &NliServiceHandle,
    candidates: &[(EntryRecord, f64)],
    query: &str,
) -> Option<Vec<NliScores>>
```

---

## Constraints

1. Implementation surface is `search.rs` and `config.rs` only — no engine crates, no store schema.
2. `MAX_CO_ACCESS_BOOST` must be imported from `unimatrix_engine::coaccess`, not redefined in `search.rs`.
3. `rerank_score` in `unimatrix-engine/src/confidence.rs` must not be removed or modified.
4. `UTILITY_BOOST`, `UTILITY_PENALTY` from `unimatrix_engine::effectiveness`; `PROVENANCE_BOOST` from `unimatrix_engine::confidence` — all read-only imports.
5. `status_penalty` applied as multiplier after `compute_fused_score` returns, never inside it.
6. NLI re-normalization (`FusionWeights::effective`) applied at call site, not inside `compute_fused_score`.
7. boost_map (Step 6c) must be fully awaited before the candidate scoring loop begins — correctness constraint, not optimization.
8. No secondary sort after the fused scoring pass. Pipeline order: HNSW → filters → [Step 6c boost_map prefetch] → [NLI scoring] → fused score + sort → truncate → floors → audit.
9. `BriefingService` and `MAX_BRIEFING_CO_ACCESS_BOOST` must not be touched.
10. All existing tests updated (not deleted) to reflect new formula values; net test count must not decrease.
11. Weight validation uses the same structured error pattern as existing NLI config validation (named fields, `Result`, no panics).
12. `prov_norm` computation must guard `PROVENANCE_BOOST == 0.0` (return 0.0, no division).

---

## Dependencies

### Internal Crates (read-only imports)

| Constant / Function | Crate | Notes |
|--------------------|-------|-------|
| `MAX_CO_ACCESS_BOOST` | `unimatrix_engine::coaccess` | Normalization denominator; must not be duplicated |
| `compute_search_boost` | `unimatrix_engine::coaccess` (via server wrapper) | Produces raw co-access boost values |
| `rerank_score` | `unimatrix_engine::confidence` | Retained; used by fallback path and existing tests |
| `UTILITY_BOOST`, `UTILITY_PENALTY` | `unimatrix_engine::effectiveness` | Utility normalization constants |
| `PROVENANCE_BOOST` | `unimatrix_engine::confidence` | Provenance normalization denominator |
| `NliScores` | `unimatrix_embed` | NLI output struct; `.entailment` field is `f32`, cast to `f64` in scoring |

### External Runtime

| Dependency | Notes |
|------------|-------|
| `tokio::task::spawn_blocking` | Existing pattern; boost_map prefetch position moves earlier in pipeline |
| `rusqlite` (via store) | No changes; scoring is in-memory after Step 6 |

No new crate dependencies are introduced.

---

## NOT in Scope

- **WA-1** — Phase Signal + FEATURE_ENTRIES tagging; `current_phase` in `SessionState`.
- **WA-2** — `w_phase * phase_boost_norm` term; `category_counts` histogram; affinity boost formula. Extension point is documented but not implemented.
- **WA-3** — MissedRetrieval post-store signal collection.
- **WA-4** — `context_briefing` injection pipeline changes.
- **W3-1** — GNN training; WA-0 establishes the initialization weights that W3-1 will learn to replace.
- **GH #329 re-implementation** — crt-024 structurally supersedes any targeted co-access patch.
- **NLI model or NLI post-store detection changes** — crt-023 pipeline unchanged except `apply_nli_sort` removal.
- **GRAPH_EDGES schema or any DB schema change** — no migration, schema version unchanged.
- **MCP response schema changes** — `ScoredEntry` field names and shape are unchanged.
- **Eval harness changes** — no eval gate; formula-deterministic feature.
- **BriefingService** — `MAX_BRIEFING_CO_ACCESS_BOOST = 0.01` pipeline untouched.
- **Config migration tooling** — operators update `config.toml` manually.

---

## Alignment Status

**Overall: PASS with two WARNs (no FAILs). Both WARNs accepted.**

### WARN-1 (accepted): Six-Term Formula Extends Vision's Four-Term Illustrative Formula

The product vision WA-0 section shows a four-term formula (`w_sim`, `w_nli`, `w_conf`, `w_coac`). The implementation targets a six-term formula that additionally includes `w_util * util_norm` and `w_prov * prov_norm`. This variance is intentional and documented.

**Rationale for acceptance**: `utility_delta` and `PROVENANCE_BOOST` already influence ranking in the current pipeline (Steps 7/8). Leaving them outside the fused formula as additive afterthoughts would recreate the structural defect WA-0 is designed to fix — no signal that influences ranking should live outside the learnable formula. ADR-001 canonicalizes the six-term formula and notes the vision formula was "illustrative, not exhaustive." The six-term formula is more faithful to the vision's own stated goal ("not a retrieval engine with additive boosts") than a four-term formula that leaves two additive terms outside.

### WARN-2 (resolved): R-01 Spec/Architecture Divergence on utility_delta Normalization

The risk document (RISK-TEST-STRATEGY.md R-01) described a divergence between FR-05 and the architecture on `utility_delta` normalization. On inspection, both SPECIFICATION.md FR-05 and ARCHITECTURE.md §Signal Normalization Details correctly specify the shift-and-scale formula: `(utility_delta + UTILITY_PENALTY) / (UTILITY_BOOST + UTILITY_PENALTY)`. The divergence R-01 referenced was against an earlier draft; the filed documents are consistent.

**Resolution**: FR-05's shift-and-scale is the canonical authoritative formula. The RISK-TEST-STRATEGY.md notes R-01 as "Divergence resolved" with "Likelihood: Low." No implementation ambiguity remains. R-01's test scenarios are retained as correctness regression guards.

---

## Key Numbers (for implementer reference)

| Constant | Value | Source |
|----------|-------|--------|
| `w_nli` default | 0.35 | ADR-003 |
| `w_sim` default | 0.25 | ADR-003 |
| `w_conf` default | 0.15 | ADR-003 |
| `w_coac` default | 0.10 | ADR-003 |
| `w_util` default | 0.05 | ADR-003 |
| `w_prov` default | 0.05 | ADR-003 |
| Default sum | 0.95 (0.05 WA-2 headroom) | ADR-003 |
| `MAX_CO_ACCESS_BOOST` | 0.03 | `unimatrix_engine::coaccess` |
| `UTILITY_BOOST` | 0.05 | `unimatrix_engine::effectiveness` |
| `UTILITY_PENALTY` | 0.05 | `unimatrix_engine::effectiveness` |
| `PROVENANCE_BOOST` | 0.02 | `unimatrix_engine::confidence` |
| AC-11 check: Entry A (nli=0.9, coac=0.0, sim=0.5, conf=0.5, util neutral) | score = 0.540 | ADR-003 numerical verification |
| AC-11 check: Entry B (nli=0.3, coac=1.0, sim=0.5, conf=0.5, util neutral) | score = 0.430 | ADR-003 numerical verification |
| NLI-disabled re-normalized w_sim | 0.4167 | ADR-003 Constraint 9 verification |
| NLI-disabled re-normalized w_conf | 0.2500 | ADR-003 Constraint 9 verification |
