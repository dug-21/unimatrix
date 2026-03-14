# crt-019 Pseudocode Overview

## Problem Being Solved

The confidence formula has 46.7% constant dead weight (W_BASE * 0.5 + W_HELP * 0.5
collapsed to neutral for unvoted Active entries). This compresses 88% of active
entries into a 0.1471-wide band. The feature replaces three constant-returning
sub-components with signal-bearing alternatives, wires the dormant helpful-vote
path through query tools, and makes the search blend weight self-adjusting.

## Components Involved

| Component | File(s) | Role |
|-----------|---------|------|
| confidence-formula-engine | `crates/unimatrix-engine/src/confidence.rs` | Pure formula: rewrite helpfulness, extend base_score and rerank_score signatures, update weights, add adaptive_confidence_weight |
| confidence-state | `crates/unimatrix-server/src/services/confidence.rs` | New `ConfidenceState` struct + `ConfidenceStateHandle`; wired through `ServiceLayer` |
| empirical-prior-computation | `crates/unimatrix-server/src/services/status.rs` | New Step 2b in `run_maintenance`: computes alpha0/beta0 from voted population, updates `ConfidenceState` |
| confidence-refresh-batch | `crates/unimatrix-server/src/services/status.rs` + `infra/coherence.rs` | Batch size 100->500, duration guard, pass alpha0/beta0 snapshot to compute_confidence |
| deliberate-retrieval-signal | `crates/unimatrix-server/src/mcp/tools.rs` + `services/usage.rs` | context_get implicit helpful vote; context_lookup doubled access_weight |
| query-skills | `.claude/skills/uni-knowledge-search/SKILL.md` + `uni-knowledge-lookup/SKILL.md` | Documentation update: add `helpful: true` to examples |
| test-infrastructure | `crates/unimatrix-engine/tests/pipeline_regression.rs`, `pipeline_calibration.rs`, `pipeline_retrieval.rs`, inline unit tests | T-REG-02 update first; new Bayesian and scenario tests |

## Data Flow Between Components

```
SERVER START
  ConfidenceState::new()  ->  { alpha0=3.0, beta0=3.0,
                                observed_spread=0.1471,
                                confidence_weight=0.184 }

MAINTENANCE TICK (context_status maintain=true)
  StatusService::run_maintenance()
    Step 2: confidence refresh loop
      snapshot = ConfidenceState.read()         <- read lock, clone alpha0/beta0
      for each stale entry (up to 500, 200ms wall):
        compute_confidence(entry, now, snapshot.alpha0, snapshot.beta0) -> f64
        store.update_confidence(id, f64)
    Step 2b: empirical prior + spread
      voted_entries = load active entries with helpful+unhelpful >= 1
      (alpha0, beta0) = compute_empirical_prior(&voted_entries)
      all_confs = load all active entry confidence values
      observed_spread = percentile_95(all_confs) - percentile_5(all_confs)
      confidence_weight = adaptive_confidence_weight(observed_spread)
      ConfidenceState.write() <- atomic: {alpha0, beta0, observed_spread, confidence_weight}

MCP context_search
  SearchService::search()
    Step 7: re-rank
      cw = ConfidenceState.read().confidence_weight  <- read lock, clone f64
      rerank_score(sim, conf, cw)

MCP context_get handler
  helpful = params.helpful.or(Some(true))           <- C-04: fold before spawn
  UsageContext { helpful, access_weight: 1, ... }
  UsageService::record_mcp_usage() -> spawn_blocking

MCP context_lookup handler
  UsageContext { helpful: params.helpful, access_weight: 2, ... }
  UsageService::record_mcp_usage()
    access_ids = dedup.filter_access(entry_ids)     <- C-05: dedup first
    access_ids_doubled = flat_map(|id| [id, id]) or explicit (id, 2) pairs
    -> spawn_blocking -> store.record_usage_with_confidence(
         closure capturing alpha0, beta0)           <- R-01: capturing closure

RECORD USAGE (all MCP tools)
  spawn_blocking:
    let (alpha0, beta0) = ConfidenceStateHandle.read().{alpha0, beta0}
    store.record_usage_with_confidence(
      &all_ids, &access_ids_for_increment,
      &helpful_ids, &unhelpful_ids, ...,
      Some(Box::new(move |entry, now| {
          compute_confidence(entry, now, alpha0, beta0)
      }))
    )
```

## Shared Types Introduced or Modified

### ConfidenceState (NEW — `services/confidence.rs`)

```
struct ConfidenceState {
    alpha0: f64,            // Bayesian prior positive pseudo-votes; cold-start 3.0
    beta0: f64,             // Bayesian prior negative pseudo-votes; cold-start 3.0
    observed_spread: f64,   // p95-p5 of active confidence population; initial 0.1471
    confidence_weight: f64, // clamp(observed_spread * 1.25, 0.15, 0.25); initial 0.184
}
type ConfidenceStateHandle = Arc<RwLock<ConfidenceState>>
```

Initial values (R-06): observed_spread = 0.1471, confidence_weight = 0.184.
Do NOT initialize with 0.0 — that silently regresses to the floor before first tick.

### UsageContext (MODIFIED — `services/usage.rs`)

```
struct UsageContext {
    session_id: Option<String>,
    agent_id: Option<String>,
    helpful: Option<bool>,
    feature_cycle: Option<String>,
    trust_level: Option<TrustLevel>,
    access_weight: u32,   // NEW: default 1; context_lookup sets 2
}
```

`access_weight` default MUST be 1, not 0. `access_weight: 0` silently drops the
access increment entirely (EC-04).

### Updated Function Signatures (`unimatrix-engine/src/confidence.rs`)

```
pub fn base_score(status: Status, trust_source: &str) -> f64
pub fn helpfulness_score(helpful: u32, unhelpful: u32, alpha0: f64, beta0: f64) -> f64
pub fn compute_confidence(entry: &EntryRecord, now: u64, alpha0: f64, beta0: f64) -> f64
pub fn rerank_score(similarity: f64, confidence: f64, confidence_weight: f64) -> f64
pub fn adaptive_confidence_weight(observed_spread: f64) -> f64
```

### Updated Weight Constants (`unimatrix-engine/src/confidence.rs`)

```
W_BASE  = 0.16  (was 0.18)
W_USAGE = 0.16  (was 0.14)
W_FRESH = 0.18  (unchanged)
W_HELP  = 0.12  (was 0.14)
W_CORR  = 0.14  (unchanged)
W_TRUST = 0.16  (was 0.14)
Sum     = 0.92  (invariant, exact f64)
```

New documentation constants (not used in code paths, document cold-start defaults):
```
COLD_START_ALPHA: f64 = 3.0
COLD_START_BETA: f64  = 3.0
MINIMUM_VOTED_POPULATION: usize = 10
```

Removed constants: `MINIMUM_SAMPLE_SIZE`, `WILSON_Z`, `SEARCH_SIMILARITY_WEIGHT`.

## Sequencing Constraints

1. **T-REG-02 update MUST be the first code change** (C-02, R-04). Update
   `pipeline_regression.rs` weight assertions to new values before touching
   `confidence.rs` constants. This is the critical ordering constraint.

2. **R-11 store-layer dedup verification** is a blocking prerequisite before
   committing the `flat_map` repeat approach for doubled access. Run the
   store-layer unit test `[42, 42] -> access_count += 2` first. If it fails,
   switch to explicit `(id, increment)` pairs.

3. `ConfidenceState` struct must be created (step 3) before wiring it through
   `ServiceLayer` (step 4) and before updating `compute_confidence` call sites.

4. `record_usage_with_confidence` store signature change (R-01: function pointer
   -> capturing closure) must be verified before `UsageService` is updated to
   pass a capturing closure.

5. The empirical prior computation (Step 2b in run_maintenance) reads from
   `ConfidenceState` (read) and writes to it (write) — `ConfidenceState` must
   exist and be wired before implementing Step 2b.

## RwLock Poison Recovery Convention

All `ConfidenceState` lock acquisitions use the existing codebase pattern:
```
state_handle.read().unwrap_or_else(|e| e.into_inner())
state_handle.write().unwrap_or_else(|e| e.into_inner())
```
This matches `CategoryAllowlist` poison recovery (FM-03).
