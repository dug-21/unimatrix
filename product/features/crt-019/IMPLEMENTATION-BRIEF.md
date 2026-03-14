# crt-019: Confidence Signal Activation — Implementation Brief

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-019/SCOPE.md |
| Scope Risk Assessment | product/features/crt-019/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/crt-019/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-019/specification/SPECIFICATION.md |
| Risk Strategy | product/features/crt-019/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-019/ALIGNMENT-REPORT.md |

---

## Goal

Fix the structural dead-weight floor in the Unimatrix confidence formula that compresses 88% of
active entries into a 0.1471-wide band, rendering confidence meaningless as a search tiebreaker.
Seven coordinated changes replace fixed-constant components with signal-bearing alternatives,
wire the dormant helpful-vote path through query skills and formula, and make the search blend
weight self-adjusting so that formula improvements automatically translate into stronger result
differentiation without a manual gate. Downstream features crt-018b and crt-020 depend on this
feature establishing a confidence spread of >= 0.20.

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| confidence-formula-engine | pseudocode/confidence-formula-engine.md | test-plan/confidence-formula-engine.md |
| confidence-state | pseudocode/confidence-state.md | test-plan/confidence-state.md |
| empirical-prior-computation | pseudocode/empirical-prior-computation.md | test-plan/empirical-prior-computation.md |
| confidence-refresh-batch | pseudocode/confidence-refresh-batch.md | test-plan/confidence-refresh-batch.md |
| deliberate-retrieval-signal | pseudocode/deliberate-retrieval-signal.md | test-plan/deliberate-retrieval-signal.md |
| query-skills | pseudocode/query-skills.md | test-plan/query-skills.md |
| test-infrastructure | pseudocode/test-infrastructure.md | test-plan/test-infrastructure.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Adaptive blend state management | Parameter-passing with server-side `Arc<RwLock<ConfidenceState>>`; engine stays stateless; `rerank_score` gains `confidence_weight: f64` parameter; `SEARCH_SIMILARITY_WEIGHT` constant removed | ARCHITECTURE.md Component 2 | architecture/ADR-001-adaptive-blend-state-management.md |
| Bayesian prior state and cold-start threshold | `ConfidenceState` holds `{ alpha0, beta0, observed_spread, confidence_weight }`; threshold for empirical estimation is **>= 10** voted entries (not 5 — see Alignment Note); cold-start defaults `alpha0=3.0, beta0=3.0`; single `RwLock` write covers all four values atomically per tick | ARCHITECTURE.md ADR-002 | architecture/ADR-002-bayesian-prior-state-cold-start.md |
| base_score differentiation scope | Clean two-parameter signature `base_score(status, trust_source)`; differentiation applies to `Status::Active` **only** (`"auto"` -> 0.35, others -> 0.5); `Status::Proposed` with `trust_source="auto"` retains 0.5 to preserve T-REG-01 ordering | ARCHITECTURE.md ADR-003 | architecture/ADR-003-base-score-trust-source-scope.md |
| context_lookup doubled access count | `access_weight: u32` field added to `UsageContext` (default 1, lookup sets 2); dedup fires before multiplier; store ID dedup behavior must be verified before `flat_map` repeat approach is committed (R-11 blocking prerequisite) | ARCHITECTURE.md ADR-004 | architecture/ADR-004-context-lookup-doubled-access.md |

### Alignment Note — Threshold Contradiction Resolved

SPEC FR-09 / C-08 originally stated `>= 5` voted entries; ARCHITECTURE ADR-002 and
RISK-TEST-STRATEGY designate `>= 10` as authoritative (population stability rationale). The
spawn prompt confirms R-05 is resolved. The implementation uses `>= 10`. SPEC text is
superseded by ADR-002 on this point.

---

## Critical Implementation Notes

These items are flagged P1/P2 and must be verified before merge.

**R-01 (Critical/High) — compute_confidence must become a capturing closure.**
The store's `record_usage_with_confidence` currently takes `Option<&dyn Fn(&EntryRecord, u64) -> f64>`.
With the new `compute_confidence(entry, now, alpha0, beta0)` signature, this must become
`Box<dyn Fn(&EntryRecord, u64) -> f64 + Send>` (or equivalent) capturing the `alpha0`/`beta0`
snapshot. A bare function pointer cannot capture the prior values; silently ignoring this change
means the Bayesian prior is never used — the feature's primary goal is undermined with no
compile error. An integration test (not unit test alone) is required to prove the empirical prior
flows through the closure to stored confidence.

**R-04 (High/High) — T-REG-02 must be updated FIRST.**
Constraint C-02 mandates updating the T-REG-02 weight assertions to new values before changing
the weight constants in `confidence.rs`. Changing constants first creates a window where tests
fail in non-obvious ways. The first commit / first hunk of the implementation diff must be the
T-REG-02 update.

**R-11 (High/High) — store-layer ID dedup is an unverified blocking prerequisite.**
ADR-004 flags the `flat_map` repeat approach as contingent on the store not deduplicating IDs
in `record_usage_with_confidence`. If the store deduplicates, passing ID X twice produces
access_count += 1, not += 2, and the doubled-access signal silently vanishes. A store-layer
unit test calling `record_usage_with_confidence` with `entry_ids = [42, 42]` and asserting
access_count += 2 must pass before the multiplier strategy is committed. If it fails, the
fallback is explicit `(id, increment)` pairs or `update_access_count(id, 2)`.

**R-06 (High/Med) — ConfidenceState initial value.**
`ConfidenceState` must initialize with `observed_spread = 0.1471` (pre-crt-019 measured value),
giving `confidence_weight = 0.184` on server start before the first tick. Initializing with
`observed_spread = 0.0` silently regresses to the floor (0.15) until maintenance runs.

**FM-03 — RwLock poison recovery.**
All `ConfidenceState` lock acquisitions must use `unwrap_or_else(|e| e.into_inner())`,
consistent with the `CategoryAllowlist` poison recovery pattern in the codebase.

---

## Implementation Ordering

1. Update T-REG-02 weight assertions to new values (C-02 mandatory first step)
2. Verify `record_usage_with_confidence` store behavior with duplicate IDs (R-11 gate)
3. Implement `ConfidenceState` struct and `ConfidenceStateHandle` in `services/confidence.rs`
4. Wire `ConfidenceStateHandle` through `ServiceLayer::new` to `StatusService` and `SearchService`
5. Rewrite `helpfulness_score` -> Bayesian Beta-Binomial; remove `wilson_lower_bound`, `MINIMUM_SAMPLE_SIZE`, `WILSON_Z`
6. Update weight constants (`W_BASE`, `W_USAGE`, `W_HELP`, `W_TRUST`); confirm sum == 0.92_f64 exactly
7. Update `base_score` signature (add `trust_source: &str`); update all 5+ call sites
8. Update `compute_confidence` signature (add `alpha0: f64, beta0: f64`)
9. Update `rerank_score` signature (add `confidence_weight: f64`); update all 4 call sites in search.rs
10. Change `record_usage_with_confidence` closure signature in store/usage; construct capturing closure in `UsageService`
11. Add empirical prior computation sub-step to `run_maintenance` (Step 2b); add duration guard to refresh loop (Step 2)
12. Increase `MAX_CONFIDENCE_REFRESH_BATCH` to 500 in `infra/coherence.rs`
13. Inject `helpful: params.helpful.or(Some(true))` in `context_get` handler; set `access_weight: 2` in `context_lookup` handler
14. Add `access_weight: u32` field to `UsageContext`; update all construction sites with `access_weight: 1`
15. Add new calibration scenarios and Bayesian unit tests; update existing Wilson tests
16. Update skill files (FR-08)
17. Run full test suite; verify T-REG-01, T-REG-02, T-ABL-01..06, T-CAL-04, weight_sum_invariant_f64

---

## Files to Create / Modify

### New Files

| File | Summary |
|------|---------|
| `crates/unimatrix-server/src/services/confidence.rs` | New: `ConfidenceState` struct + `ConfidenceStateHandle = Arc<RwLock<ConfidenceState>>` + `ConfidenceService` update |

### Modified Files

| File | Change |
|------|--------|
| `crates/unimatrix-engine/src/confidence.rs` | Remove `MINIMUM_SAMPLE_SIZE`, `WILSON_Z`, `SEARCH_SIMILARITY_WEIGHT`; add `COLD_START_ALPHA`, `COLD_START_BETA`; update weight constants; rewrite `helpfulness_score`; update `base_score`, `compute_confidence`, `rerank_score` signatures; update all unit tests |
| `crates/unimatrix-engine/tests/pipeline_regression.rs` | T-REG-02: update weight constant assertions (must happen first per C-02); verify T-REG-01 with new formula |
| `crates/unimatrix-engine/tests/pipeline_calibration.rs` | Update `confidence_with_adjusted_weight` helper (base_score call at line 94); add `auto_vs_agent_spread` scenario (AC-12); update ablation_pair helpfulness case to pass alpha0/beta0 |
| `crates/unimatrix-engine/tests/pipeline_retrieval.rs` | Update any `rerank_score` calls to pass `confidence_weight` |
| `crates/unimatrix-engine/src/test_scenarios.rs` | Update `base_score` call site if present |
| `crates/unimatrix-server/src/infra/coherence.rs` | `MAX_CONFIDENCE_REFRESH_BATCH: usize = 500` (was 100) |
| `crates/unimatrix-server/src/services/status.rs` | Add duration guard (Instant) to refresh loop; add Step 2b: prior + spread computation; update `compute_confidence` calls to pass alpha0/beta0 snapshot |
| `crates/unimatrix-server/src/services/search.rs` | All 4 `rerank_score` call sites pass `confidence_weight` read from `ConfidenceState`; `SearchService` gains `ConfidenceStateHandle` |
| `crates/unimatrix-server/src/services/usage.rs` | Add `access_weight: u32` to `UsageContext`; `record_mcp_usage` multiplies access increment by `access_weight`; update `compute_confidence` closure to capture alpha0/beta0; update test construction sites |
| `crates/unimatrix-server/src/services/mod.rs` | Re-export `ConfidenceState`, `ConfidenceStateHandle`; update `ServiceLayer::new` to wire handle to both services |
| `crates/unimatrix-server/src/mcp/tools.rs` | `context_get`: inject `helpful: params.helpful.or(Some(true))`; `context_lookup`: set `access_weight: 2`; all `UsageContext` constructions: add `access_weight: 1` |
| `.claude/skills/uni-knowledge-search/SKILL.md` | Add `helpful: true` to primary example; add guidance on `helpful: false` |
| `.claude/skills/uni-knowledge-lookup/SKILL.md` | Add `helpful: true` to primary example; add guidance on `helpful: false` |

---

## Data Structures

### ConfidenceState (new — `services/confidence.rs`)

```rust
pub(crate) struct ConfidenceState {
    pub alpha0: f64,            // Bayesian prior — positive pseudo-votes
    pub beta0: f64,             // Bayesian prior — negative pseudo-votes
    pub observed_spread: f64,   // p95 - p5 confidence spread of active population
    pub confidence_weight: f64, // clamp(observed_spread * 1.25, 0.15, 0.25)
}

pub(crate) type ConfidenceStateHandle = Arc<RwLock<ConfidenceState>>;
```

Default (initial server-start values):
- `alpha0 = 3.0`, `beta0 = 3.0` (cold-start)
- `observed_spread = 0.1471` (pre-crt-019 measured value — not 0.0)
- `confidence_weight = 0.184` (clamp(0.1471 * 1.25, 0.15, 0.25))

### UsageContext (modified — `services/usage.rs`)

```rust
pub(crate) struct UsageContext {
    pub session_id: Option<String>,
    pub agent_id: Option<String>,
    pub helpful: Option<bool>,
    pub feature_cycle: Option<String>,
    pub trust_level: Option<TrustLevel>,
    pub access_weight: u32,  // NEW: 1 = normal, 2 = deliberate retrieval (context_lookup)
}
```

`Default` for `access_weight` must be 1 (not 0); EC-04 risk if 0.

### Confidence Formula (updated weights)

```
W_BASE  = 0.16   (was 0.18)
W_USAGE = 0.16   (was 0.14)
W_FRESH = 0.18   (unchanged)
W_HELP  = 0.12   (was 0.14)
W_CORR  = 0.14   (unchanged)
W_TRUST = 0.16   (was 0.14)
Sum     = 0.92   (invariant — exact f64)
```

---

## Function Signatures

### Updated signatures in `unimatrix-engine/src/confidence.rs`

```rust
pub fn base_score(status: Status, trust_source: &str) -> f64

pub fn helpfulness_score(
    helpful: u32,
    unhelpful: u32,
    alpha0: f64,
    beta0: f64,
) -> f64

pub fn compute_confidence(
    entry: &EntryRecord,
    now: u64,
    alpha0: f64,
    beta0: f64,
) -> f64

pub fn rerank_score(
    similarity: f64,
    confidence: f64,
    confidence_weight: f64,
) -> f64

pub fn adaptive_confidence_weight(observed_spread: f64) -> f64
// clamp(observed_spread * 1.25, 0.15, 0.25)
```

### Updated signature in `unimatrix-vector` store layer

```rust
// record_usage_with_confidence must accept Box<dyn Fn(&EntryRecord, u64) -> f64 + Send>
// (or FnMut equivalent) — not a bare function pointer — to allow alpha0/beta0 capture
fn record_usage_with_confidence(
    entry_ids: &[u64],
    now: u64,
    confidence_fn: Option<Box<dyn Fn(&EntryRecord, u64) -> f64 + Send>>,
) -> Result<()>
```

Exact signature determined during R-11 store-verification step; must accommodate the
`flat_map` repeat approach or explicit `(id, increment)` pairs depending on store behavior.

### Empirical prior computation (new function in `services/status.rs` or `services/confidence.rs`)

```rust
fn compute_empirical_prior(voted_entries: &[(u32, u32)]) -> (f64, f64)
// Input: (helpful_count, unhelpful_count) pairs for entries with >= 1 vote
// Returns: (alpha0, beta0) clamped to [0.5, 50.0]
// Falls back to (3.0, 3.0) cold-start when voted_entries.len() < MINIMUM_VOTED_POPULATION
// Handles zero-variance degeneracy (all entries same rate) via clamp

pub const MINIMUM_VOTED_POPULATION: usize = 10;
```

---

## Constraints

| Constraint | Detail |
|-----------|--------|
| C-01 Sum-to-0.92 | `W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST == 0.92_f64` exact; `assert_eq!` not tolerance; new vector `{0.16, 0.16, 0.18, 0.12, 0.14, 0.16}` satisfies IEEE 754 binary64 exactly |
| C-02 T-REG-02 order | Update T-REG-02 weight assertions before changing constants; first diff hunk must be the test update |
| C-03 Active status only | `base_score` differentiation applies to `Status::Active` only; `Proposed` with `"auto"` trust_source returns 0.5 (preserves T-REG-01) |
| C-04 No second spawn | Implicit helpful vote for `context_get` folded into existing `UsageContext.helpful` before spawn; zero new `spawn_blocking` or `tokio::spawn` calls in the handler |
| C-05 Dedup before multiply | UsageDedup `filter_access` fires before `access_weight` multiplier; repeated lookup by same agent produces 0 increments, not 2 |
| C-06 No schema change | `helpful_count`, `unhelpful_count` are existing columns; schema version stays at 12 |
| C-07 State in server layer | `ConfidenceState` lives in `unimatrix-server`; engine stays stateless (all functions pure) |
| C-08 Voted threshold | `MINIMUM_VOTED_POPULATION = 10`; below threshold cold-start `alpha0=3.0, beta0=3.0`; empirical estimation only at >= 10 voted entries |
| C-09 T-REG-01 ordering | Verify `expert > good > auto > stale > quarantined` passes with `auto_extracted_new()` profile after all changes; `auto` uses `Status::Proposed` so base_score = 0.5 unchanged |
| RwLock poison | All `ConfidenceState` lock acquisitions use `unwrap_or_else(|e| e.into_inner())` |
| NFR-06 No blocking regression | Zero direct `lock_conn()` calls in async context; all paths mediated by `spawn_blocking` (post-vnc-010 invariant) |

---

## Dependencies

### Crates Modified

| Crate | Role |
|-------|------|
| `unimatrix-engine` | Confidence formula, weight constants, scoring functions |
| `unimatrix-server` | ConfidenceState, refresh batch, tool handlers, UsageContext |

### External Crates — No New Dependencies

All implementation uses `std::time::Instant`, `std::time::Duration`, `std::sync::{Arc, RwLock}` from the Rust standard library. No new crate entries in `Cargo.toml`.

### Existing Infrastructure Relied Upon

| Infrastructure | Used By |
|---------------|---------|
| `UsageDedup` (in-process, per-agent-per-entry) | FR-06 implicit vote dedup; FR-07 access dedup before multiply |
| `UsageContext.helpful: Option<bool>` | FR-06 implicit helpful injection for `context_get` |
| `record_usage_with_confidence` | Extended to accept capturing closure (R-01) |
| `spawn_blocking` single-task pattern (vnc-010 fix) | C-04: no second spawn in `context_get` |
| `unwrap_or_else(|e| e.into_inner())` poison pattern | FM-03: all `ConfidenceState` lock acquisitions |
| `CategoryAllowlist` convention | Precedent for RwLock poison recovery |

---

## NOT in Scope

1. crt-018b — Effectiveness-driven retrieval (depends on crt-019 spread first)
2. crt-020 — Implicit helpfulness from session outcomes (follow-on feature)
3. crt-014 — Topology-aware supersession via petgraph (Track B, independent)
4. Contradiction detection changes — belongs to crt-020
5. base_score changes for non-auto trust sources in Active status
6. base_score changes for Proposed, Deprecated, or Quarantined status regardless of trust_source
7. MCP parameter schema changes — `helpful: Option<bool>` already exists on all query tools
8. Persistent storage of alpha0/beta0 or observed_spread — in-memory only, no new DB columns
9. Scheduled refresh trigger changes — increased batch applies to existing `maintain=true` path
10. UDS injection path changes — deliberate retrieval signals apply to MCP tool handlers only

---

## Alignment Status

Overall: PASS with one resolved variance.

**VARIANCE 1 (RESOLVED) — Bayesian Prior Threshold Contradiction:**
SPEC FR-09 / C-08 stated `>= 5` voted entries. ARCHITECTURE ADR-002 stated `>= 10`. The
vision guardian identified this in R-05 and designated ADR-002 (`>= 10`) as authoritative.
The spawn prompt confirms this is resolved. Implementation uses `MINIMUM_VOTED_POPULATION = 10`.

**Vision alignment:** PASS — directly advances "confidence evolution from real usage signals"
(Cortical phase goal). The adaptive blend and deliberate-retrieval signals advance the
"invisible delivery" pillar.

**Milestone fit:** PASS — correctly targets "Search Quality Enhancements — NEXT", Track A, P1.
No out-of-scope capabilities introduced. crt-018b and crt-020 downstream dependencies respected.

**Architecture consistency:** PASS — all SR-* scope risks resolved with explicit ADRs.
`ConfidenceState` design with `Arc<RwLock<>>`, parameter-passing for `rerank_score`, and
`UsageContext.access_weight` are coherent and non-regressive.

**Remaining open implementation question (not a design variance):** The Store's
`record_usage_with_confidence` signature change (bare function pointer -> capturing closure,
R-01) is confirmed as necessary but the exact new type is determined during implementation
step 2 (store-layer ID dedup verification). This is a P1 risk with mandatory integration test
coverage before merge.

**Cosmetic inconsistencies (no implementation impact):**
- ARCHITECTURE Component 6 references `crates/.claude/skills/`; correct path is `.claude/skills/`
- SPEC Workflow 4 prose misequences the prior computation step; FR-09 and ARCHITECTURE Component 3 are authoritative

---

*Produced by crt-019-synthesizer on 2026-03-14*
