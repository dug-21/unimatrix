# crt-018b: Effectiveness-Driven Retrieval — Implementation Brief

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-018b/SCOPE.md |
| Scope Risk Assessment | product/features/crt-018b/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/crt-018b/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-018b/specification/SPECIFICATION.md |
| Risk/Test Strategy | product/features/crt-018b/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-018b/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|------------|-----------|
| EffectivenessState cache | pseudocode/effectiveness-state.md | test-plan/effectiveness-state.md |
| Background tick writer | pseudocode/background-tick-writer.md | test-plan/background-tick-writer.md |
| Search utility delta | pseudocode/search-utility-delta.md | test-plan/search-utility-delta.md |
| Briefing effectiveness tiebreaker | pseudocode/briefing-tiebreaker.md | test-plan/briefing-tiebreaker.md |
| Auto-quarantine guard | pseudocode/auto-quarantine-guard.md | test-plan/auto-quarantine-guard.md |
| Auto-quarantine audit event | pseudocode/auto-quarantine-audit.md | test-plan/auto-quarantine-audit.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Goal

Activate the effectiveness classifications produced by crt-018 as live retrieval signals in the search re-ranking and briefing assembly pipelines, so that Effective entries are boosted and Ineffective/Noisy entries are penalized at query time. Introduce a background-tick-driven auto-quarantine mechanism that removes persistently Ineffective or Noisy entries after N consecutive maintenance cycles without manual intervention, with full operator audit visibility and a configurable (or disabled) threshold.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|------------|--------|----------|
| HashMap clone avoidance on hot search path | Generation counter `u64` in `EffectivenessState` + `Arc<Mutex<EffectivenessSnapshot>>` cached per service; readers skip clone when generation unchanged | ARCHITECTURE Component 1, Component 3 | architecture/ADR-001-generation-counter-for-snapshot-cache.md |
| `consecutive_bad_cycles` behavior on tick error | Hold counters at current value (no increment, no reset); emit `tick_skipped` audit event; do not modify `EffectivenessState` | ARCHITECTURE Component 2 | architecture/ADR-002-tick-error-semantics-consecutive-bad-cycles.md |
| Utility delta placement relative to `status_penalty` | Utility delta placed inside the penalty multiplication, alongside provenance and co-access boosts; a Deprecated Effective entry receives `(base + delta) * 0.7`, not `base * 0.7 + delta` | ARCHITECTURE Component 3 | architecture/ADR-003-utility-delta-inside-penalty-multiplication.md |
| `EffectivenessStateHandle` on `BriefingService` | Required (non-optional) constructor parameter; missing wiring is a compile error, not silent degradation | ARCHITECTURE Component 4 | architecture/ADR-004-effectiveness-handle-required-briefing-constructor.md |
| Briefing `effectiveness_priority` numeric scale | Use ARCHITECTURE Component 4 canonical scale: Effective=2, Settled=1, None/Unmatched=0, Ineffective=-1, Noisy=-2. Supersedes the 3-2-1-0 scale in SPECIFICATION FR-07 (semantically equivalent, but ARCHITECTURE scale must be used consistently in code) | ALIGNMENT-REPORT §Specification Review | architecture/ARCHITECTURE.md |
| Utility delta magnitude | Symmetric: `UTILITY_BOOST = 0.05`, `UTILITY_PENALTY = 0.05`, `SETTLED_BOOST = 0.01`; 0.05 is meaningful at both crt-019 spread extremes without overwhelming similarity signal | SPECIFICATION FR-04, FR-05; ADR-003 range analysis | — |
| Auto-quarantine default threshold | `AUTO_QUARANTINE_CYCLES = 3` (env: `UNIMATRIX_AUTO_QUARANTINE_CYCLES`); value 0 disables entirely; minimum 45 minutes wall time before first auto-quarantine | SPECIFICATION FR-12 | — |

> **RESOLVED — Option A selected.** Generation counter (ADR-001) is in scope. SPECIFICATION §NOT in Scope item 7 removed. R-06 test scenarios stand.

---

## Files to Create or Modify

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-server/src/services/effectiveness.rs` | Create | New file: `EffectivenessState`, `EffectivenessStateHandle` type alias, `EffectivenessSnapshot` (ADR-001 cache struct), cold-start empty constructor |
| `crates/unimatrix-server/src/services/mod.rs` | Modify | Add `effectiveness_state: EffectivenessStateHandle` field to `ServiceLayer`; construct handle once and clone into `SearchService`, `BriefingService`, and background tick |
| `crates/unimatrix-server/src/services/search.rs` | Modify | Add `effectiveness_state: EffectivenessStateHandle` and `cached_snapshot: Arc<Mutex<EffectivenessSnapshot>>` fields; snapshot categories at top of `search()`; apply `utility_delta` at all four `rerank_score` call sites inside the `status_penalty` multiplication |
| `crates/unimatrix-server/src/services/briefing.rs` | Modify | Add `effectiveness_state: EffectivenessStateHandle` as required constructor parameter; snapshot categories at top of `assemble()`; apply `effectiveness_priority` as secondary sort key in `process_injection_history` and convention lookup sort |
| `crates/unimatrix-server/src/background.rs` | Modify | After `compute_report()` succeeds in `maintenance_tick()`: acquire write lock, update `categories` and `consecutive_bad_cycles`, increment generation, release write lock; then scan for auto-quarantine threshold and call `store.quarantine_entry()` per entry inside `spawn_blocking`; emit `auto_quarantine` audit events; emit `tick_skipped` audit event on `compute_report()` error |
| `crates/unimatrix-engine/src/effectiveness/mod.rs` | Modify | Add three public constants: `UTILITY_BOOST: f64 = 0.05`, `SETTLED_BOOST: f64 = 0.01`, `UTILITY_PENALTY: f64 = 0.05`; add `auto_quarantined_this_cycle: Vec<u64>` field to `EffectivenessReport` |

---

## Data Structures

### EffectivenessState (new — `services/effectiveness.rs`)

```
EffectivenessState {
    categories: HashMap<u64, EffectivenessCategory>
        // entry_id -> last-known category from background tick
        // absent key: not yet classified, utility_delta = 0.0

    consecutive_bad_cycles: HashMap<u64, u32>
        // entry_id -> consecutive background ticks where entry was Ineffective or Noisy
        // absent key: counter is 0
        // in-memory only; resets on server restart

    generation: u64
        // incremented on every write; readers skip HashMap clone when unchanged
        // incremented on every write; readers skip HashMap clone when unchanged
}

EffectivenessStateHandle = Arc<RwLock<EffectivenessState>>
```

### EffectivenessSnapshot (new — `services/effectiveness.rs` or `services/search.rs`)

```
EffectivenessSnapshot {
    generation: u64,
    categories: HashMap<u64, EffectivenessCategory>,
}
// Held as Arc<Mutex<EffectivenessSnapshot>> in SearchService and BriefingService
// to share the cached copy across rmcp-cloned service instances.
```

### EffectivenessReport additions (existing — `unimatrix-engine/src/effectiveness/mod.rs`)

```
EffectivenessReport {
    // ... existing fields unchanged ...
    auto_quarantined_this_cycle: Vec<u64>
        // entry IDs quarantined in the most recent background tick; surfaced via context_status
}
```

### Utility Delta Mapping

| `EffectivenessCategory` | `utility_delta` | `effectiveness_priority` |
|-------------------------|-----------------|--------------------------|
| `Effective` | +0.05 | 2 |
| `Settled` | +0.01 | 1 |
| `Unmatched` | 0.0 | 0 |
| (absent / None) | 0.0 | 0 |
| `Ineffective` | -0.05 | -1 |
| `Noisy` | -0.05 | -2 |

### Combined Final Score Formula

```
confidence_weight = clamp(spread * 1.25, 0.15, 0.25)   // from crt-019

final_score = (
    (1 - confidence_weight) * similarity
    + confidence_weight * confidence
    + utility_delta           // {-0.05, 0.0, +0.01, +0.05}
    + provenance_boost        // 0.02 for lesson-learned, else 0.0
    + co_access_boost         // [0.0, 0.03]
) * status_penalty            // 0.5 superseded, 0.7 deprecated, 1.0 active
```

---

## Function Signatures

```rust
// unimatrix-engine/src/effectiveness/mod.rs
pub const UTILITY_BOOST: f64 = 0.05;
pub const SETTLED_BOOST: f64 = 0.01;
pub const UTILITY_PENALTY: f64 = 0.05;

// services/effectiveness.rs
pub struct EffectivenessState { ... }
pub type EffectivenessStateHandle = Arc<RwLock<EffectivenessState>>;
impl EffectivenessState {
    pub fn new() -> Self;  // returns empty state for cold-start
}

// services/search.rs (new helper, may be free fn or method)
fn utility_delta(category: Option<EffectivenessCategory>) -> f64;

// services/briefing.rs (new helper)
fn effectiveness_priority(category: Option<EffectivenessCategory>) -> i32;

// services/briefing.rs (modified constructor)
pub(crate) fn new(
    entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
    search: SearchService,
    gateway: Arc<SecurityGateway>,
    semantic_k: usize,
    effectiveness_state: EffectivenessStateHandle,  // new required parameter
) -> Self;

// background.rs (modified signature)
pub fn spawn_background_tick(
    // ... existing parameters ...
    effectiveness_state: EffectivenessStateHandle,  // new required parameter
) -> JoinHandle<()>;

// Audit event constants (background.rs)
// operation = "auto_quarantine"  (agent_id = "system")
// operation = "tick_skipped"     (agent_id = "system")
```

### Lock Ordering Invariant (Critical — R-01, R-13)

Two lock ordering rules must be enforced in code:

1. When both `effectiveness_state` and `cached_snapshot` locks are needed, acquire `effectiveness_state.read()` first, read the generation field, **drop the read guard**, then acquire `cached_snapshot.lock()`. Never hold both guards simultaneously.
2. The write guard on `EffectivenessState` must be dropped (out of scope or explicitly via `drop()`) before any call to `store.quarantine_entry()`. The in-memory scan (find entries at threshold) may happen under the write lock; the SQL write must not.

---

## Constraints

1. `W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST = 0.92` invariant is unchanged. Utility delta is query-time only and does not modify stored confidence.
2. No new database tables or columns. `EffectivenessState` is in-memory. `consecutive_bad_cycles` resets on server restart (intentional).
3. No new MCP tools. Only `context_search`, `context_briefing`, and the background maintenance tick are modified surfaces.
4. `classify_entry()` and the five `EffectivenessCategory` variants are unchanged.
5. `SETTLED_BOOST (0.01) < co-access boost maximum (0.03)` — settled boost must not overwhelm co-access as the dominant query-time differentiator.
6. `EffectivenessStateHandle` is non-optional on `BriefingService::new()`. Incomplete wiring is a compile error.
7. Write lock on `EffectivenessState` is held only for in-memory updates. It must be released before any SQL write (auto-quarantine). Read lock in `search()` must be released before SQL or embedding computation.
8. `compute_report()` failure does not increment `consecutive_bad_cycles`. Old state is retained. `tick_skipped` audit event is emitted.
9. All auto-quarantine SQLite writes are synchronous and called from within `spawn_blocking`.
10. Test infrastructure is cumulative: extend existing `TestDb`, `tests_classify.rs`, `read.rs`, and search pipeline tests. Do not create isolated scaffolding.
11. All `RwLock` and `Mutex` acquisitions on `EffectivenessStateHandle` and `EffectivenessSnapshot` must use `.unwrap_or_else(|e| e.into_inner())` poison recovery — never `.unwrap()` or `.expect()`.
12. The Strict retrieval mode (UDS path) is unmodified; utility delta applies only to Flexible (MCP) mode.
13. Cold-start is safe: empty `EffectivenessState` produces 0.0 utility deltas; behavior is identical to pre-crt-018b. No fallback or guard logic required.
14. `UNIMATRIX_AUTO_QUARANTINE_CYCLES` must be validated at startup: non-negative integer; implausibly large values (> 1000) should produce a startup error, not silent acceptance (security: DoS via env-var).

---

## Dependencies

### Internal Crates

| Crate | Component | Usage |
|-------|-----------|-------|
| `unimatrix-engine` | `effectiveness::{EffectivenessCategory, EffectivenessReport}` | Classification types; new constants added here |
| `unimatrix-engine` | `classify_entry()`, `utility_score()` | Unchanged; called inside `compute_report()` |
| `unimatrix-store` | `Store::compute_effectiveness_aggregates()` | Called inside `compute_report()`, unchanged |
| `unimatrix-store` | `Store::load_entry_classification_meta()` | Called inside `compute_report()`, unchanged |
| `unimatrix-store` | `quarantine_entry()` | Called by auto-quarantine path in `maintenance_tick()` |
| `unimatrix-server` | `services/confidence.rs` `ConfidenceState` | Structural pattern to mirror |
| `unimatrix-server` | `services/search.rs` `SearchService` | Modified to receive handle and apply utility delta |
| `unimatrix-server` | `services/briefing.rs` `BriefingService` | Modified constructor; effectiveness tiebreaker in sort |
| `unimatrix-server` | `services/status.rs` `StatusService` | `compute_report()` already returns `StatusReport.effectiveness` — unchanged |
| `unimatrix-server` | `background.rs` `maintenance_tick()` | New write path for `EffectivenessState` |
| `unimatrix-server` | `server.rs` `UnimatrixServer` | Holds `EffectivenessStateHandle`; wires into constructors |

### External / Prior Features

| Feature | Component | Dependency |
|---------|-----------|------------|
| crt-018 | `EffectivenessCategory`, `EffectivenessReport`, store queries | Must be merged; provides all classification types |
| crt-019 | Adaptive `confidence_weight = clamp(spread * 1.25, 0.15, 0.25)` | Must be merged; integration test fixture must confirm non-zero spread |
| crt-004 | Co-access boost pattern | Structural reference for additive query-time signal pattern |

### Environment Variables

| Variable | Default | Behavior |
|----------|---------|----------|
| `UNIMATRIX_AUTO_QUARANTINE_CYCLES` | `3` | Consecutive bad ticks before auto-quarantine; `0` disables; validated at startup |

---

## NOT in Scope

1. New MCP tools — no tools added; only existing search, briefing, and maintenance tick are modified.
2. Schema migration — no new tables, no new columns, no schema version bump.
3. Classification logic changes — `classify_entry()` and all five `EffectivenessCategory` variants are read-only from this feature's perspective.
4. Embedding/ML training — using effectiveness labels as ML signal (issue #206 item 5) is a separate research track.
5. Retrospective "knowledge-that-helped" surfacing — per-entry contribution in retrospective output (issue #206 item 4) is a separate feature.
6. Persistent `consecutive_bad_cycles` storage — in-memory only; durability across restarts is not in scope.
7. Auto-quarantine undo tool — restore uses the existing `context_quarantine` restore operation; no new undo primitive.
8. UDS (Strict) path re-ranking — Strict mode hard-filters to Active-only; utility delta applies only to Flexible (MCP) path.
9. Retroactive quarantine of existing Ineffective/Noisy entries — entries must accumulate N consecutive bad ticks post-deployment.

---

## Alignment Status

**Overall**: PASS with one implementation team warning. All variances resolved.

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly advances auditable knowledge lifecycle and retrieval quality goals; all four SCOPE goals covered |
| Milestone Fit | PASS | Correct dependency position (crt-018 + crt-019 both merged before implementation) |
| Scope Gaps | PASS | All four SCOPE goals addressed in all three source documents |
| Architecture Consistency | PASS | Generation counter variance resolved (Option A): ADR-001 in scope, SPECIFICATION §NOT in Scope item 7 removed |
| Risk Completeness | PASS | RISK-TEST-STRATEGY covers all 8 SCOPE-RISK-ASSESSMENT items and adds 6 additional risks (4 Critical) |

### Implementation Team Warning: Write Lock Before SQL (NFR-02)

SPECIFICATION NFR-02 requires the `EffectivenessState` write lock to be released before any SQL write (auto-quarantine). ARCHITECTURE Component 2 step 3 describes the auto-quarantine threshold scan as occurring "while holding the write lock." The data-flow diagram is ambiguous on whether the write guard is dropped before or after the `quarantine_entry()` SQL call.

Implementation team must explicitly drop the write guard before calling `store.quarantine_entry()`. RISK-TEST-STRATEGY R-13 (Critical) covers this with a concurrency test scenario.

### Minor Scale Discrepancy: `effectiveness_priority` Numeric Values

SPECIFICATION FR-07 uses scale 3-2-1-0: `Effective(3) > Settled(2) > Unmatched/nil(1) > Noisy/Ineffective(0)`.
ARCHITECTURE Component 4 uses scale 2-1-0-(-1)-(-2): `Effective(2), Settled(1), None/Unmatched(0), Ineffective(-1), Noisy(-2)`.

Semantics are equivalent (Effective highest, Noisy/Ineffective lowest). Implementation must pick one canonical set and use it consistently. ARCHITECTURE scale is recommended (enables future distinct treatment of Noisy vs. Ineffective in briefing if needed).
