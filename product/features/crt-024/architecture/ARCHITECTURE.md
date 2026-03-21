# crt-024: Ranking Signal Fusion (WA-0) — Architecture

## System Overview

crt-024 replaces the two-pass sequential ranking pipeline in `SearchService` with a single fused
scoring pass. The structural defect being fixed: `apply_nli_sort` (Step 7) ranks candidates by
entailment, then Step 8 re-sorts by a formula that omits NLI, allowing high co-access entries to
overtake high-entailment entries. The fix is a single linear combination where every ranking
signal is a weighted term, all normalized to [0, 1], computed in one pass.

This feature is exclusively a scoring formula change inside `unimatrix-server`. No engine crates,
no store schema, no MCP response shape, no evaluation harness changes. The fused formula's six
config-driven weights are the feature vector interface for W3-1 (GNN training), making this a
prerequisite for the entire Wave 1A roadmap.

### Position in the System

```
context_search call
  → SearchService.search()
    → [Steps 0-6b: unchanged — rate check, embed, HNSW, filter, penalty map, inject]
    → [Step 7: NEW — NLI scoring yields NliScores per candidate, no sort]
    → [Step 7b: NEW — fused score computation, single sort]
    → [Steps 9-12: unchanged — truncate, floors, build ScoredEntry, audit]
```

The `BriefingService` is explicitly out of scope and retains its own pipeline
(`MAX_BRIEFING_CO_ACCESS_BOOST = 0.01`, separate from `MAX_CO_ACCESS_BOOST = 0.03`).

---

## Component Breakdown

### 1. `SearchService` (`unimatrix-server/src/services/search.rs`)

The only component that changes. Responsibilities after crt-024:

- Owns the fused scoring formula as a pure function `fused_score(inputs) -> f64`
- Prefetches the co-access `boost_map` via `spawn_blocking_with_timeout` before the scoring pass
- Calls NLI scoring (when enabled) to collect `Vec<NliScores>` indexed parallel to candidates
- Runs a single iteration over candidates to compute `fused_score * status_penalty` for each
- Sorts by fused score descending, then truncates to k
- Updates `ScoredEntry.final_score` to reflect the fused formula

`apply_nli_sort` is removed (see ADR-002). Its test coverage migrates to the single-pass
scoring tests.

### 2. `InferenceConfig` (`unimatrix-server/src/infra/config.rs`)

Gains six new `f64` fields for fusion weights: `w_sim`, `w_nli`, `w_conf`, `w_coac`, `w_util`,
`w_prov`. Validation in `InferenceConfig::validate()` adds: per-field range `[0.0, 1.0]` and
sum-of-six ≤ 1.0 check. All fields carry `#[serde(default)]` so existing configs load without
errors.

### 3. `unimatrix-engine` crates (unchanged)

- `unimatrix_engine::coaccess::compute_search_boost` — unchanged; only the normalization step
  in `SearchService` is new
- `unimatrix_engine::confidence::rerank_score` — unchanged; retained for the NLI-absent fallback
  path and existing tests
- `unimatrix_engine::effectiveness::{UTILITY_BOOST, UTILITY_PENALTY, SETTLED_BOOST}` — unchanged;
  read by `SearchService` for `util_norm` computation
- `unimatrix_engine::confidence::PROVENANCE_BOOST` — unchanged; read for `prov_norm` computation

---

## Component Interactions

```
SearchService.search()
    |
    |-- [pre-pass] spawn_blocking: compute_search_boost(anchor_ids, result_ids, ...) -> boost_map
    |
    |-- [optional] rayon_pool.spawn_with_timeout: nli.score_batch(pairs) -> Vec<NliScores>
    |
    |-- [single pass] for each (entry, sim) in candidates:
    |       coac_norm = boost_map.get(id).unwrap_or(0.0) / MAX_CO_ACCESS_BOOST
    |       nli_score = nli_scores[i].entailment (or 0.0 if NLI absent)
    |       util_norm = (utility_delta(category) + UTILITY_PENALTY) / (UTILITY_BOOST + UTILITY_PENALTY)
    |       prov_norm = if boosted_category { 1.0 } else { 0.0 }
    |       fused = w_sim*sim + w_nli*nli + w_conf*conf + w_coac*coac_norm
    |                + w_util*util_norm + w_prov*prov_norm
    |       final = fused * status_penalty
    |
    |-- sort by final_score DESC, truncate to k
    |-- build ScoredEntry with final_score = fused * penalty
```

**Lock ordering** (unchanged from crt-023): confidence_state read → effectiveness_state read →
cached_snapshot mutex → typed_graph read. No new locks introduced.

---

## The Canonical Fused Scoring Formula

This is the authoritative formula definition. It supersedes the four-term illustrative formula in
PRODUCT-VISION.md (see ADR-001) and resolves SR-02 from the risk assessment.

```
fused_score =
    w_sim  * similarity_score                              // [0,1] bi-encoder cosine
  + w_nli  * nli_entailment_score                         // [0,1] cross-encoder precision
  + w_conf * confidence_score                             // [0,1] Wilson score composite
  + w_coac * (raw_co_access_boost / MAX_CO_ACCESS_BOOST)  // [0,1] usage pattern, normalized
  + w_util * ((utility_delta + UTILITY_PENALTY)           // [0,1] effectiveness classification
              / (UTILITY_BOOST + UTILITY_PENALTY))
  + w_prov * (prov_boost / PROVENANCE_BOOST)              // {0.0, 1.0} category provenance

final_score = fused_score * status_penalty                // topology multiplier [0,1]
```

**Range guarantee**: when `w_sim + w_nli + w_conf + w_coac + w_util + w_prov ≤ 1.0` and all
six normalized inputs are in [0, 1], `fused_score ∈ [0.0, 1.0]` by construction.

**`status_penalty` as multiplier**: this is a topology modifier (deprecated/superseded entries),
not a relevance signal. It uniformly scales all relevance signals down, consistent with semantics
established in crt-010, crt-013, crt-014, and ADR-003 (#703). It is not a term in the formula
subject to W3-1 training.

### Signal Normalization Details

| Signal | Raw source | Raw range | Normalization | Notes |
|--------|-----------|-----------|---------------|-------|
| `similarity` | HNSW cosine (L2-normalized) | [0, 1] | identity | Already normalized |
| `nli_entailment` | `NliScores.entailment` softmax | [0, 1] | identity | Already normalized |
| `confidence` | `EntryRecord.confidence` | [0, 1] | identity | Wilson score composite |
| `co_access` | `compute_search_boost()` | [0, 0.03] | `÷ MAX_CO_ACCESS_BOOST` | Constant from engine crate |
| `utility` | `utility_delta()` return value | [-0.05, +0.05] | `(val + UTILITY_PENALTY) / (UTILITY_BOOST + UTILITY_PENALTY)` | Maps -0.05→0.0, 0.0→0.5, +0.05→1.0 |
| `provenance` | binary: `PROVENANCE_BOOST` or 0 | {0, 0.02} | `÷ PROVENANCE_BOOST` | Binary [0,1] after division |

**Critical note on `utility` normalization**: The raw `utility_delta` can be negative
(-UTILITY_PENALTY = -0.05) for Ineffective and Noisy entries. Dividing by UTILITY_BOOST alone
would yield a value outside [0,1]. The shift-and-scale formula `(val + UTILITY_PENALTY) /
(UTILITY_BOOST + UTILITY_PENALTY)` is required to map the full signed range to [0,1].
This is the resolution of the SR-31 assumption risk in the scope risk assessment.

---

## Default Weights (Resolution of SR-01)

**See ADR-003 for full derivation.** Summary:

| Weight | Default | Signal role |
|--------|---------|-------------|
| `w_nli` | 0.35 | Cross-encoder precision; semantically richest signal |
| `w_sim` | 0.25 | Bi-encoder recall anchor; topical match |
| `w_conf` | 0.15 | Historical reliability; tiebreaker |
| `w_coac` | 0.10 | Usage pattern; useful lagging signal |
| `w_util` | 0.05 | Effectiveness classification; sparse early on |
| `w_prov` | 0.05 | Category provenance hint; weakest signal |
| **Sum** | **0.95** | **0.05 headroom reserved for WA-2 phase boost (SR-06)** |

**Numerical verification:**

AC-11 regression test assertion (high NLI vs max co-access, equal sim=0.5, conf=0.5, util neutral):
- Entry A (nli=0.9, coac=0.0): 0.35×0.9 + 0.25×0.5 + 0.15×0.5 + 0 + 0.05×0.5 + 0 = 0.540
- Entry B (nli=0.3, coac=1.0): 0.35×0.3 + 0.25×0.5 + 0.15×0.5 + 0.10×1.0 + 0.05×0.5 = 0.430
- Entry A > Entry B. AC-11 holds.

Constraint 10 (sim dominant over conf at defaults, no NLI, no coac):
- Entry A (sim=0.9, conf=0.3): 0.25×0.9 + 0.15×0.3 = 0.225 + 0.045 = 0.270
- Entry B (sim=0.5, conf=0.9): 0.25×0.5 + 0.15×0.9 = 0.125 + 0.135 = 0.260
- Entry A > Entry B. Constraint 10 holds.

Constraint 9 (NLI disabled: sim dominant, conf secondary in re-normalized weights):
- Re-normalization denominator = 0.25 + 0.15 + 0.10 + 0.05 + 0.05 = 0.60
- w_sim' = 0.417, w_conf' = 0.250, w_coac' = 0.167, w_util' = 0.083, w_prov' = 0.083
- Entry A (sim=0.9, conf=0.3): 0.417×0.9 + 0.250×0.3 = 0.375 + 0.075 = 0.450
- Entry B (sim=0.5, conf=0.9): 0.417×0.5 + 0.250×0.9 = 0.209 + 0.225 = 0.434
- Entry A > Entry B. Constraint 9 holds: sim dominant, conf secondary.

---

## NLI Absence Re-normalization

When NLI is absent (`nli_enabled = false` or provider not ready), `w_nli` contributes 0.0.
The remaining five weights are re-normalized by dividing each by their sum:

```
nli_absent_sum = w_sim + w_conf + w_coac + w_util + w_prov
w_sim'  = w_sim  / nli_absent_sum
w_conf' = w_conf / nli_absent_sum
w_coac' = w_coac / nli_absent_sum
w_util' = w_util / nli_absent_sum
w_prov' = w_prov / nli_absent_sum
```

This resolves SR-03: the denominator includes ALL five remaining weights, not a hardcoded
three-weight subset. If any of the five are zero (operator-tuned to 0.0), they do not change
the denominator behavior — their term was already contributing 0.0.

**Guard**: if `nli_absent_sum == 0.0` (all non-NLI weights are zero — pathological config),
re-normalization is skipped and all scores default to 0.0. This cannot be reached with default
weights but must be handled defensively.

---

## WA-2 Extension Point (Resolution of SR-04)

WA-2 adds a session-context phase boost term. The extension contract is:

```
fused_score =
    [current six terms as above]
  + w_phase * phase_boost_norm                            // [0,1] WA-2 session context

// WA-2 adds w_phase to InferenceConfig and raises sum-validation ceiling from 1.0 to 1.0
// (sum constraint unchanged; operators must reduce other weights to accommodate w_phase)
```

WA-2's implementation steps:
1. Add `w_phase: f64` to `InferenceConfig` with `#[serde(default)]` default 0.0.
2. Add `phase_boost_norm: f64` to the scoring input structure (whatever it evolves to).
3. Add one term `w_phase * phase_boost_norm` to the accumulator.
4. Validation: check `w_sim + w_nli + w_conf + w_coac + w_util + w_prov + w_phase <= 1.0`.

The default `w_phase = 0.0` means WA-2 ships as a no-op until operators configure it. The 0.05
headroom in default weights allows `w_phase = 0.05` without operators needing to retune other
weights. This design treats the formula as an open accumulator, not a fixed-arity function,
satisfying SR-04.

---

## Data Flow: Co-Access Boost Map Prefetch

The co-access `boost_map` must be computed before the single scoring pass. In the current
pipeline it is computed in Step 8 (after NLI sorts in Step 7). After crt-024, the sequence is:

```
Step 6b: supersession injection (unchanged)
  ↓
[prefetch] spawn_blocking_with_timeout: compute_search_boost -> boost_map   ← MOVED EARLIER
  ↓
Step 7: NLI scoring (if enabled): rayon_pool.spawn_with_timeout -> Vec<NliScores>
  ↓
Step 7b: single fused score pass (boost_map and NliScores available)
  ↓
Step 7c: sort by fused_score DESC
  ↓
Step 9: truncate to k
```

The co-access prefetch must happen before Step 7 so both NLI scores and boost values are in
memory when the scoring loop runs. The `spawn_blocking_with_timeout` call is preserved unchanged
— only its position in the pipeline moves (from after the sort to before scoring).

This resolves SR-07 as a sequencing constraint.

---

## `InferenceConfig` Changes

Six new fields added to `InferenceConfig` in `infra/config.rs`:

```rust
/// Fusion weight for cosine similarity signal (bi-encoder recall). Default: 0.25.
#[serde(default = "default_w_sim")]
pub w_sim: f64,

/// Fusion weight for NLI entailment signal (cross-encoder precision). Default: 0.35.
/// When NLI is disabled, this term is 0.0 and remaining weights are re-normalized.
#[serde(default = "default_w_nli")]
pub w_nli: f64,

/// Fusion weight for confidence signal (Wilson score composite). Default: 0.15.
#[serde(default = "default_w_conf")]
pub w_conf: f64,

/// Fusion weight for co-access affinity signal (normalized usage pattern). Default: 0.10.
#[serde(default = "default_w_coac")]
pub w_coac: f64,

/// Fusion weight for utility delta signal (effectiveness classification). Default: 0.05.
#[serde(default = "default_w_util")]
pub w_util: f64,

/// Fusion weight for provenance signal (boosted-category hint). Default: 0.05.
/// WA-2 reserves 0.05 headroom (defaults sum to 0.95) for the phase boost term.
#[serde(default = "default_w_prov")]
pub w_prov: f64,
```

**Validation additions to `InferenceConfig::validate()`** (following existing NLI field pattern):
1. Each weight individually in `[0.0, 1.0]` — structured `NliFieldOutOfRange`-style error.
2. Sum of all six `> 1.0` — new structured error variant `FusionWeightSumExceeded` naming the
   sum and all six field values.

The pattern precedent from crt-023 (`NliFieldOutOfRange` with `path`, `field`, `value`, `reason`)
is reused for per-field validation. The sum check produces a distinct error variant because it
is a cross-field invariant.

---

## Integration Surface

| Integration Point | Type / Signature | Source |
|-------------------|-----------------|--------|
| `compute_search_boost` | `fn(&[u64], &[u64], &Store, u64, &HashSet<u64>) -> HashMap<u64, f64>` | `unimatrix-server/src/coaccess.rs` |
| `MAX_CO_ACCESS_BOOST` | `f64 = 0.03` | `unimatrix_engine::coaccess` — NOT duplicated |
| `UTILITY_BOOST` | `f64 = 0.05` | `unimatrix_engine::effectiveness` |
| `UTILITY_PENALTY` | `f64 = 0.05` | `unimatrix_engine::effectiveness` |
| `PROVENANCE_BOOST` | `f64 = 0.02` | `unimatrix_engine::confidence` |
| `rerank_score` | `fn(f64, f64, f64) -> f64` | `unimatrix-server/src/confidence.rs` — retained, not called in NLI-active path |
| `NliScores.entailment` | `f32` | `unimatrix_embed::NliScores` |
| `utility_delta` | `fn(Option<EffectivenessCategory>) -> f64` | local to `search.rs` |
| `InferenceConfig.w_{sim,nli,conf,coac,util,prov}` | `f64` fields | `infra/config.rs` |
| `ScoredEntry.final_score` | `f64` (formula changes, field name unchanged) | `search.rs` |
| `apply_nli_sort` | **REMOVED** (see ADR-002) | was `search.rs` |

---

## Technology Decisions

- **No new dependencies.** All signal inputs are computed from existing data already fetched
  in the pipeline. The fused formula is pure arithmetic over already-held values.
- **`rerank_score` retained.** The NLI-absent fallback path in `SearchService` continues to
  use `rerank_score` from `unimatrix-server/src/confidence.rs` for compatibility with existing
  tests and the fallback sort. The fused formula in the NLI-active path does not call
  `rerank_score` — it computes each signal term directly (SR-09 guidance followed).
- **Formula as closure or standalone function.** The single-pass scorer can be implemented as
  an inline closure inside `SearchService.search()` receiving the collected signal inputs, or
  extracted as a `pub(crate) fn compute_fused_score(inputs: FusedScoreInputs, weights: FusionWeights) -> f64`
  for testability. The standalone function is preferred — it enables pure unit tests without
  constructing a full `SearchService`. See ADR-004.

---

## EvalServiceLayer Integration

`EvalServiceLayer` constructs `SearchService` from a profile config for use in the D1–D4 eval
harness. Before crt-024, the `[inference]` section in profile TOMLs was documented as "accepted
but has no effect" — the eval harness docs carried this statement because no fusion weights existed
to wire. **crt-024 supersedes that statement.** After crt-024, the `[inference]` fusion weights
ARE wired and profile TOMLs can express distinct weight configurations.

### Wiring Requirement

`EvalServiceLayer` must pass the full `InferenceConfig` (including `w_sim`, `w_nli`, `w_conf`,
`w_coac`, `w_util`, `w_prov`) through to `SearchService::new()`. If `EvalServiceLayer` constructs
`SearchService` with a hardcoded or default `InferenceConfig` rather than the one deserialized from
the profile TOML, `[inference]` overrides in profile TOMLs have no effect — the eval harness
compares two runs that are actually identical, making the regression report meaningless.

This is a correctness constraint on `EvalServiceLayer`, not on `SearchService` itself. `SearchService`
is correct by design (it accepts weights via `InferenceConfig`). The risk is that the eval wiring
layer silently ignores the profile's `[inference]` section.

### Profile TOML Examples

Two profiles are required for the crt-024 eval run:

**`old-behavior.toml`** — approximates the pre-crt024 ranking formula. Sets NLI and all new
signals to zero weight so the fused formula degrades to the pre-crt024 two-signal blend:

```toml
[inference]
w_sim  = 0.85
w_nli  = 0.0
w_conf = 0.15
w_coac = 0.0
w_util = 0.0
w_prov = 0.0
```

**`crt024-weights.toml`** — uses the new default weights from ADR-003:

```toml
[inference]
w_nli  = 0.35
w_sim  = 0.25
w_conf = 0.15
w_coac = 0.10
w_util = 0.05
w_prov = 0.05
```

### Eval Run Procedure

1. Post-implementation, with snapshot at `/tmp/eval/pre-crt024-snap.db` and scenarios at
   `/tmp/eval/pre-crt024-scenarios.jsonl`:
   ```
   eval-harness run --db /tmp/eval/pre-crt024-snap.db \
     --scenarios /tmp/eval/pre-crt024-scenarios.jsonl \
     --profiles old-behavior.toml crt024-weights.toml \
     --out /tmp/eval/crt024-report.json
   ```
2. Human reviews the report. Ranking changes caused by NLI-override corrections (entries with
   low entailment that previously floated via co-access) are expected and intentional. True
   regressions — cases where a clearly correct result drops rank without an NLI explanation — must
   be zero.
3. Baseline log updated before PR is marked ready.

### "No Effect" Statement Superseded

The eval harness documentation note that `[inference]` is "accepted but has no effect" was written
before crt-024. After crt-024 ships, that note must be removed or updated to: "`[inference]` fusion
weights are honored by `EvalServiceLayer`." This documentation update is part of crt-024's
acceptance criteria (AC-15, AC-16).

---

## ADR Index

| ADR | Title | Decision |
|-----|-------|----------|
| ADR-001 | Six-Term Formula Canonicalization | Six-term formula is the implementation target; vision's four-term formula was illustrative |
| ADR-002 | `apply_nli_sort` Removal | Remove; NLI scoring returns scores per candidate, no intermediate sort |
| ADR-003 | Default Fusion Weights | `w_nli=0.35, w_sim=0.25, w_conf=0.15, w_coac=0.10, w_util=0.05, w_prov=0.05` |
| ADR-004 | Formula as Extractable Pure Function | Standalone `compute_fused_score` for testability |

---

## Open Questions for Spec Writer

1. **`utility_norm` formula in AC-05**: The current AC-05 text says
   `fused = w_sim*sim + w_nli*nli + w_conf*conf + w_coac*coac_norm + w_util*util_norm + w_prov*prov_norm`
   but does not define how `util_norm` is computed from the signed `utility_delta`. Spec must
   update AC-05 and AC-10 to state the shift-and-scale normalization:
   `util_norm = (utility_delta + UTILITY_PENALTY) / (UTILITY_BOOST + UTILITY_PENALTY)`.

2. **`prov_norm` binary value**: `PROVENANCE_BOOST` is currently 0.02 and normalizing by it
   gives {0.0, 1.0}. If PROVENANCE_BOOST is ever zero (operator sets it to 0.0 via config),
   division by zero must be guarded. Spec should note the implementation must guard this.

3. **AC-06 denominator correction**: The current AC-06 text reads "divide each by
   `(w_sim + w_conf + w_coac + w_util + w_prov)`" — this is correct for six-term formula only
   if all five remaining weights are summed. The spec should explicitly list all five as the
   denominator so the implementer does not use a hardcoded three-weight subset.

4. **Co-access staleness cutoff in the prefetch**: The current Step 8 co-access computation
   uses a `staleness_cutoff` based on `CO_ACCESS_STALENESS_SECONDS`. When the prefetch moves
   earlier in the pipeline, this cutoff computation must move with it. Spec should confirm
   no behavioral change is intended here.

5. **`SearchService::new()` signature change**: Adding fusion weights to `SearchService` requires
   either threading `InferenceConfig` directly into `SearchService::new()` or extracting a
   `FusionWeights` struct. Spec should specify which approach to use and update the constructor
   signature accordingly.
