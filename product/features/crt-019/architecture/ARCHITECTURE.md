# crt-019: Confidence Signal Activation — Architecture

## System Overview

crt-019 fixes a structural problem in Unimatrix's confidence formula: 46.7% of the active-entry
score is constant dead weight, leaving confidence unable to differentiate knowledge quality. The
result is that confidence contributes at most 0.022 to search re-ranking scores — weaker than the
co-access boost alone.

The feature applies seven coordinated changes across the confidence pipeline, from the formula
constants in `unimatrix-engine` through the refresh batch in `unimatrix-server`, to skill prose
in `.claude/skills/`. No schema changes are required. The changes are designed to be implemented
as a single atomic feature cycle that passes all existing calibration and regression tests (with
deliberate T-REG-02 update) plus new tests for the additions.

This feature establishes the confidence spread baseline that crt-018b (effectiveness-driven
retrieval) and crt-020 (implicit helpfulness) depend on.

## Component Breakdown

### Component 1: Confidence Formula Engine (`crates/unimatrix-engine/src/confidence.rs`)

The pure formula module. All component functions are stateless — given the same inputs they return
the same output. This module is the only source of truth for confidence computation.

**Changes in scope:**
- Replace `wilson_lower_bound` / `MINIMUM_SAMPLE_SIZE` with Bayesian Beta-Binomial posterior.
  New function: `helpfulness_score(helpful, unhelpful, alpha0, beta0) -> f64`.
  The `alpha0`/`beta0` parameters are passed in from the server layer; the engine is unaware of
  how they are stored.
- Weight constants: `W_BASE 0.18→0.16`, `W_HELP 0.14→0.12`, `W_USAGE 0.14→0.16`,
  `W_TRUST 0.14→0.16`. Sum remains exactly 0.92.
- `base_score(status)` gains a second parameter `trust_source: &str`. Active entries with
  `trust_source == "auto"` return 0.35; all other Active entries return 0.5. Proposed status
  returns 0.5 regardless of trust_source (preserves T-REG-01 ordering — see ADR-003).
- `compute_confidence` gains two additional parameters: `alpha0: f64, beta0: f64` — passed
  through to `helpfulness_score`. Signature becomes:
  `compute_confidence(entry: &EntryRecord, now: u64, alpha0: f64, beta0: f64) -> f64`.
- `rerank_score(similarity, confidence, confidence_weight: f64) -> f64` — `confidence_weight`
  replaces the compiled `SEARCH_SIMILARITY_WEIGHT` constant. The constant is removed.
  Callers in `search.rs` pass the runtime-computed weight. Engine remains stateless.
- Remove: `MINIMUM_SAMPLE_SIZE`, `WILSON_Z`, `SEARCH_SIMILARITY_WEIGHT` constants.
  Add: `COLD_START_ALPHA: f64 = 3.0`, `COLD_START_BETA: f64 = 3.0` (kept as documentation
  constants, not used in computation path — the values are passed as arguments).

### Component 2: Adaptive Blend State (`crates/unimatrix-server/src/services/confidence.rs`)

A new struct `ConfidenceState` holds the runtime-variable quad
`{ alpha0, beta0, observed_spread, confidence_weight }` computed during each maintenance tick.

```
pub(crate) struct ConfidenceState {
    pub alpha0: f64,           // Bayesian prior — positive pseudo-votes
    pub beta0: f64,            // Bayesian prior — negative pseudo-votes
    pub observed_spread: f64,  // p95 - p5 confidence spread of active population
    pub confidence_weight: f64 // clamp(observed_spread * 1.25, 0.15, 0.25)
}
```

`ConfidenceState` is wrapped in `Arc<RwLock<ConfidenceState>>` and held by `UnimatrixServer`
(on `ConfidenceService` or a new `ConfidenceStateHandle`). The background tick (writer) holds
the write lock for a short critical section at the end of each maintenance tick. Query paths
(search.rs) hold the read lock only long enough to clone the `confidence_weight` f64.

The decision to use `RwLock` rather than `AtomicU64` is documented in ADR-001. The decision to
pass confidence_weight as a parameter rather than embedding shared state in the engine is
documented in ADR-001.

### Component 3: Empirical Prior Computation (`crates/unimatrix-server/src/services/status.rs`)

The maintenance tick's `run_maintenance` function gains a new sub-step: prior computation.

After the confidence refresh loop (Step 2), and before returning, the tick:
1. Loads the confidence values of all active entries that have ≥1 vote.
2. If the count is ≥10 (minimum threshold for stable estimation — see ADR-002), computes
   method-of-moments estimates of `alpha0` and `beta0` from the population's vote ratios.
3. Always computes `observed_spread = p95 - p5` of the active confidence population
   (all active entries, not just voted ones).
4. Computes `confidence_weight = clamp(observed_spread * 1.25, 0.15, 0.25)`.
5. Atomically updates `ConfidenceState` via the write lock.

If the voted-entry count is below 10, cold-start defaults `alpha0=3.0, beta0=3.0` are used.
The `observed_spread` and `confidence_weight` are always updated from real data.

### Component 4: Confidence Refresh Batch (`crates/unimatrix-server/src/services/status.rs`)

The refresh loop (Step 2 of `run_maintenance`) gains two changes:
- `MAX_CONFIDENCE_REFRESH_BATCH: usize` increases from 100 to 500 (in `infra/coherence.rs`).
- A wall-clock duration guard using `std::time::Instant::now()` checked at each loop iteration:
  if `start.elapsed() > Duration::from_millis(200)`, break early and record the partial count.
- `compute_confidence` calls in this loop must pass the current `alpha0`/`beta0` from
  `ConfidenceState` (read lock, snapshot before loop).

### Component 5: Deliberate Retrieval Signal (`crates/unimatrix-server/src/mcp/tools.rs`)

Two injection points in the MCP tool handlers:

**context_get** (line ~601): When `params.helpful.is_none()`, override to
`helpful: Some(true)` in the `UsageContext` passed to `record_access`. This folds the
implicit helpful signal into the existing single `spawn_blocking` task. No second task is
spawned. UsageDedup's one-vote-per-agent protection remains in effect.

**context_lookup** (line ~457): When the lookup returns results, pass an additional
`access_weight: u32 = 2` via `UsageContext` (or a new enum variant, see ADR-004). The
`UsageService::record_mcp_usage` function multiplies the access count increment by this weight
before passing to `store.record_usage_with_confidence`. No change to helpful vote path.

Both changes require no schema migration and no new spawn_blocking tasks.

### Component 6: Query Skills (`crates/.claude/skills/`)

Two skill files receive documentation updates only — no code changes:
- `.claude/skills/uni-knowledge-search/SKILL.md`
- `.claude/skills/uni-knowledge-lookup/SKILL.md`

Updated to include `helpful: true` as standard practice in example invocations, and guidance on
passing `helpful: false` when entries were retrieved but not applicable.

### Component 7: Test Infrastructure

**`crates/unimatrix-engine/tests/pipeline_regression.rs`** (T-REG-02):
Update weight constant assertions from old values to new values. This test is designed to fail
on weight change — update it first before any weight change, as per SR-06 recommendation.

**`crates/unimatrix-engine/tests/pipeline_calibration.rs`**:
- `confidence_with_adjusted_weight` helper uses `base_score(entry.status)` (line 94) — must
  update to `base_score(entry.status, &entry.trust_source)`.
- Add new scenario: `auto_vs_agent_spread` — confirms auto-sourced active entries score below
  identically-signaled agent entries (AC-12).
- The ablation pair for "helpfulness" in `ablation_pair` uses `helpful_count=100/0` with 5+
  votes, so Bayesian scoring already produces the correct direction. Verify with new signature.

**`crates/unimatrix-engine/src/confidence.rs` (unit tests)**:
- Remove Wilson score tests that rely on `MINIMUM_SAMPLE_SIZE` and `WILSON_Z`.
- Add Bayesian helpfulness tests: cold-start, two unhelpful votes, balanced votes (AC-02).
- Update `base_score_active` test to account for two-parameter signature.
- Update `rerank_score` tests to pass `confidence_weight` parameter.

**`crates/unimatrix-engine/tests/pipeline_retrieval.rs`**:
- Any `rerank_score` call must pass `confidence_weight`.

**`crates/unimatrix-server/src/services/usage.rs`** (existing tests):
- `test_record_access_mcp_helpful_vote` remains valid — no change to vote infrastructure.
- Add new test `test_context_get_implicit_helpful_vote` — verify `helpful_count` increments
  when `params.helpful.is_none()`.
- Add new test `test_context_lookup_doubled_access` — verify `access_count += 2` per lookup.

## Component Interactions

```
Background Tick (every 15 min)
  └── status_svc.run_maintenance(active_entries)
        ├── Step 2: Confidence refresh loop (batch 500, 200ms guard)
        │     └── compute_confidence(entry, now, α₀, β₀)   ← snapshot from ConfidenceState
        └── Step 2b: Prior + spread computation (new)
              ├── method_of_moments(voted_entries) → α₀, β₀
              ├── percentile_spread(active_confs)   → observed_spread
              ├── confidence_weight = clamp(spread * 1.25, 0.15, 0.25)
              └── ConfidenceState.write() ← atomic update of all four values

MCP context_search
  └── SearchService::search()
        └── Step 7: re-rank with rerank_score(sim, conf, confidence_weight)
              └── confidence_weight ← ConfidenceState.read().confidence_weight

MCP context_get
  └── tools.rs context_get handler
        └── UsageContext { helpful: params.helpful.or(Some(true)), ... }
              └── UsageService::record_mcp_usage (existing spawn_blocking)

MCP context_lookup
  └── tools.rs context_lookup handler
        └── UsageContext { access_weight: 2, helpful: params.helpful, ... }
              └── UsageService::record_mcp_usage (access_count * 2 for lookup)
```

## Technology Decisions

See individual ADRs for rationale.

| Decision | Choice | ADR |
|----------|--------|-----|
| Adaptive blend state management | RwLock parameter-passing hybrid | ADR-001 |
| Bayesian prior state and cold-start | Shared ConfidenceState, cold-start at <10 voted entries | ADR-002 |
| base_score signature change | Clean two-parameter function, Active status only | ADR-003 |
| context_lookup doubled access | UsageContext access_weight field | ADR-004 |

## Integration Points

### Dependencies on this feature

| Downstream | Dependency |
|------------|------------|
| crt-018b (Effectiveness-Driven Retrieval) | Requires spread ≥ 0.20 established by crt-019 |
| crt-020 (Implicit Helpfulness) | Requires formula calibrated to use votes (crt-019 removes 5-vote wall) |

### External surfaces affected

No public API changes. MCP tool parameter schemas are unchanged (`helpful: Option<bool>` already
exists on all query tools). No schema migration required.

## Integration Surface

All call sites that must be updated when signatures change:

| Integration Point | New Signature | Files to Update |
|-------------------|--------------|-----------------|
| `helpfulness_score` | `fn helpfulness_score(helpful: u32, unhelpful: u32, alpha0: f64, beta0: f64) -> f64` | `confidence.rs` (definition), `confidence.rs` unit tests |
| `base_score` | `fn base_score(status: Status, trust_source: &str) -> f64` | `confidence.rs` (definition + 4 unit tests), `test_scenarios.rs` line 68 (if present), `pipeline_calibration.rs` line 94 |
| `compute_confidence` | `fn compute_confidence(entry: &EntryRecord, now: u64, alpha0: f64, beta0: f64) -> f64` | `confidence.rs`, `services/usage.rs` line 158 (`compute_confidence` closure), `services/status.rs` line 670 (refresh loop), any other direct callers |
| `rerank_score` | `fn rerank_score(similarity: f64, confidence: f64, confidence_weight: f64) -> f64` | `confidence.rs` (definition + unit tests), `services/search.rs` lines 275, 327, 370 (3 call sites) |
| `ConfidenceState` | New struct in `services/confidence.rs` | `services/confidence.rs` (new), `services/mod.rs` (re-export), `services/status.rs` (writer), `services/search.rs` (reader), `server.rs` (holder via ServiceLayer) |
| `UsageContext` | Add `access_weight: u32` field | `services/usage.rs` (struct + `record_mcp_usage`), `mcp/tools.rs` (all 3 UsageContext construction sites) |
| `MAX_CONFIDENCE_REFRESH_BATCH` | `500` | `infra/coherence.rs` |

### Confirmed call-site inventory for `rerank_score`

From reading `services/search.rs`:
- Line 275: `rerank_score(*sim_a, entry_a.confidence)` — Step 7 initial sort
- Line 327: `rerank_score(*sim_a, entry_a.confidence)` — Step 8 co-access re-sort (base_a)
- Line 328: `rerank_score(*sim_b, entry_b.confidence)` — Step 8 co-access re-sort (base_b)
- Line 370: `rerank_score(*sim, entry.confidence)` — Step 11 ScoredEntry construction

Total: 4 call sites in search.rs (not 6 as estimated — confirmed from source).

Additionally, `pipeline_calibration.rs` imports `rerank_score` but does not call it in the
ablation path; `pipeline_retrieval.rs` may call it — verify during implementation.

### Confirmed call-site inventory for `compute_confidence`

- `crates/unimatrix-server/src/services/usage.rs` line 158: passed as closure `&crate::confidence::compute_confidence`
- `crates/unimatrix-server/src/services/status.rs` line 670: `crate::confidence::compute_confidence(e, now_ts)`
- `crates/unimatrix-engine/src/confidence.rs` line 204: definition

The closure at usage.rs:158 is `Some(&crate::confidence::compute_confidence)` — this requires
special attention. The Store's `record_usage_with_confidence` takes a confidence function
pointer. With the new signature requiring `alpha0`/`beta0`, a closure capturing the current
state values must be constructed at the call site rather than using a bare function pointer.

## Files to be Modified

### `crates/unimatrix-engine/src/confidence.rs`
- Remove `MINIMUM_SAMPLE_SIZE`, `WILSON_Z`, `SEARCH_SIMILARITY_WEIGHT`
- Add `COLD_START_ALPHA = 3.0`, `COLD_START_BETA = 3.0` (documentation constants)
- Update weight constants: `W_BASE`, `W_USAGE`, `W_HELP`, `W_TRUST`
- Rewrite `helpfulness_score` (Bayesian, two additional params)
- Remove `wilson_lower_bound` (private, safe to delete)
- Update `base_score` signature (add `trust_source: &str`)
- Update `compute_confidence` signature (add `alpha0, beta0`)
- Update `rerank_score` signature (add `confidence_weight`)
- Update all unit tests

### `crates/unimatrix-engine/tests/pipeline_regression.rs`
- T-REG-02: Update weight constant assertions (must happen first — see SR-06)
- T-REG-01: Verify `auto_extracted_new()` still satisfies `good > auto` with new `base_score`

### `crates/unimatrix-engine/tests/pipeline_calibration.rs`
- Update `confidence_with_adjusted_weight` helper — `base_score` call at line 94
- Add `auto_vs_agent_spread` scenario (AC-12)
- Update `ablation_pair` "helpfulness" case to pass `alpha0`/`beta0`

### `crates/unimatrix-engine/tests/pipeline_retrieval.rs`
- Update any `rerank_score` calls to pass `confidence_weight`

### `crates/unimatrix-engine/src/test_scenarios.rs`
- If `base_score` is called directly, update the call

### `crates/unimatrix-server/src/infra/coherence.rs`
- `MAX_CONFIDENCE_REFRESH_BATCH: usize = 500` (was 100)

### `crates/unimatrix-server/src/services/confidence.rs`
- Add `ConfidenceState` struct
- Add `ConfidenceStateHandle = Arc<RwLock<ConfidenceState>>`
- Update `ConfidenceService` to hold a `ConfidenceStateHandle`
- Expose `confidence_state()` accessor on `ConfidenceService`

### `crates/unimatrix-server/src/services/status.rs`
- Step 2: Add duration guard (Instant) to confidence refresh loop
- Step 2b (new): After refresh loop, compute α₀/β₀ from voted entries, compute spread, update `ConfidenceState`
- Update `compute_confidence` calls to pass current α₀/β₀

### `crates/unimatrix-server/src/services/search.rs`
- All 4 `rerank_score` call sites: pass `confidence_weight` read from `ConfidenceState`
- `SearchService` gains access to `ConfidenceStateHandle` (via constructor or `Arc<ConfidenceService>`)

### `crates/unimatrix-server/src/services/usage.rs`
- `UsageContext`: add `access_weight: u32` field (default 1 for all existing callers)
- `record_mcp_usage`: multiply access increment by `access_weight`
- Update `compute_confidence` closure to capture `alpha0`/`beta0` from `ConfidenceState`
- Update all existing `UsageContext` construction in tests (add `access_weight: 1`)

### `crates/unimatrix-server/src/services/mod.rs`
- Re-export `ConfidenceState`, `ConfidenceStateHandle` if needed
- Update `ServiceLayer::new` / `with_rate_config` to wire `ConfidenceStateHandle` through to both `SearchService` and `StatusService`

### `crates/unimatrix-server/src/mcp/tools.rs`
- `context_get` handler: inject `helpful: params.helpful.or(Some(true))` in `UsageContext`
- `context_lookup` handler: set `access_weight: 2` in `UsageContext`
- All existing `UsageContext` construction: add `access_weight: 1`

### `.claude/skills/uni-knowledge-search/SKILL.md`
- Add `helpful: true` to primary example invocations

### `.claude/skills/uni-knowledge-lookup/SKILL.md`
- Add `helpful: true` to primary example invocations

## Open Questions

None. All design decisions are resolved per SCOPE.md §Open Questions and confirmed by the
risk assessment findings. Specific resolutions:

1. SR-03 (blend state): Parameter-passing with `RwLock<ConfidenceState>` on server side — see ADR-001.
2. SR-01 (prior cold-start): Minimum 10 voted entries threshold, cold-start defaults otherwise — see ADR-002.
3. SR-02 (atomicity): Single `RwLock` write covers all four state values — see ADR-002.
4. SR-04 (base_score Proposed): Differentiation applies to `Active` only — see ADR-003.
5. SR-05 (lookup dedup): `access_weight` multiplied inside `record_mcp_usage` after dedup filter — deduped entries get 0 increments, passing entries get ×2 — see ADR-004.
6. SR-07 (implicit vote): Folded into existing `UsageContext.helpful` field, not a new spawn — see Component 5.
7. SR-08 (lock contention): `record_usage_with_confidence` is already `spawn_blocking`; confirmed no direct `lock_conn()` in async context in the usage path.
8. `compute_confidence` closure in usage.rs: Must be a capturing closure, not a bare function pointer, to pass `alpha0`/`beta0`. The store's `record_usage_with_confidence` signature must accept `FnMut` or the closure approach documented in ADR-002.
