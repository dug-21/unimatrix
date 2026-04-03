# crt-045 Implementation Brief
# Eval Harness — Wire TypedGraphState Rebuild into EvalServiceLayer

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-045/SCOPE.md |
| Architecture | product/features/crt-045/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-045/specification/SPECIFICATION.md |
| Risk Strategy | product/features/crt-045/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-045/ALIGNMENT-REPORT.md |

---

## Goal

`EvalServiceLayer::from_profile()` constructs a `ServiceLayer` whose `TypedGraphState` always
starts cold (`use_fallback = true`, empty graph), silently disabling graph_expand, PPR, and
graph-penalty phases for every eval profile. This makes `baseline.toml` and
`ppr-expander-enabled.toml` produce bit-identical results, blocking the W1-3 eval gate (AC-02).
crt-045 adds a single `TypedGraphState::rebuild()` call in `from_profile()`, writes the result
into the existing shared handle via post-construction write-back, fixes the malformed
`ppr-expander-enabled.toml` that fails at parse time, and adds an integration test with a
three-layer assertion to confirm the graph is observed at live search time.

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|------------|-----------|
| `EvalServiceLayer` (layer.rs) | pseudocode/EvalServiceLayer.md | test-plan/EvalServiceLayer.md |
| `ppr-expander-enabled.toml` | pseudocode/ppr-expander-enabled-toml.md | test-plan/ppr-expander-enabled-toml.md |
| `layer_tests.rs` (integration test) | pseudocode/layer_tests.md | test-plan/layer_tests.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Post-construction write-back vs. pre-populated handle parameter | Use Option B (write-back after `with_rate_config()`). SR-01 resolved: `services/mod.rs:419` confirms `Arc::clone()` — write propagates. No `with_rate_config()` signature change required. | SCOPE.md Proposed Approach (b), ARCHITECTURE.md | architecture/ADR-001-post-construction-write-vs-parameter.md |
| Rebuild error handling — abort vs. degrade | On `StoreError::InvalidInput` (cycle) or store I/O error, log `tracing::warn!`, leave `use_fallback = true`, return `Ok(layer)`. Eval run proceeds in degraded mode. | SCOPE.md AC-05, SPECIFICATION.md FR-03 | architecture/ADR-002-rebuild-error-handling.md |
| Integration test must invoke live search, not only inspect handle state | Three-layer assertion required: (1) handle state `use_fallback == false`, (2) graph connectivity via `find_terminal_active`, (3) live `search()` call returns `Ok(_)`. Guards against wired-but-unused anti-pattern (entry #1495). | SCOPE-RISK-ASSESSMENT.md SR-05, RISK-TEST-STRATEGY.md R-02 | architecture/ADR-003-test-live-search-not-just-handle-state.md |
| `typed_graph_handle()` accessor visibility on `EvalServiceLayer` | `pub(crate)` — no `#[cfg(test)]` guard. Mirrors `embed_handle()` and `nli_handle()`. Also available to `runner.rs` for pre-replay diagnostics. | SCOPE.md C-04, SPECIFICATION.md FR-05, C-04, C-10 | architecture/ADR-004-typed-graph-handle-accessor-visibility.md |
| `ppr-expander-enabled.toml` fix: `distribution_change` value | Set `distribution_change = false`. Gate on `mrr_floor = 0.2651` and `p_at_5_min = 0.1083`. CC@k and ICD deferred until first-run baselines are measured. Add TOML comment explaining intentional `false`. | SCOPE.md OQ-01, SPECIFICATION.md C-06 | architecture/ADR-005-toml-distribution-change-false.md |

---

## Files to Create / Modify

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-server/src/eval/profile/layer.rs` | Modify | Add `TypedGraphState::rebuild()` call (Step 5b), add post-construction write-back (Step 13b), add `pub(crate) typed_graph_handle()` accessor |
| `product/research/ass-037/harness/profiles/ppr-expander-enabled.toml` | Modify | Set `distribution_change = false`, add `mrr_floor = 0.2651` and `p_at_5_min = 0.1083` gates, add explanatory comment |
| `crates/unimatrix-server/src/eval/profile/layer_tests.rs` | Modify | Add integration test `test_from_profile_typed_graph_rebuilt_after_construction` with three-layer assertion (AC-06, SR-05, SR-06) |

**No other files change.** The following are read-only reference points:

| File | Role |
|------|------|
| `crates/unimatrix-server/src/services/mod.rs` | Confirms Arc clone chain at line 419; `typed_graph_handle()` already `pub` here |
| `crates/unimatrix-server/src/services/typed_graph.rs` | `TypedGraphState::rebuild()` at line 91; `TypedGraphStateHandle` type alias at line 161 |
| `crates/unimatrix-server/src/services/search.rs` | `if !use_fallback` guard at Step 6d — target of the fix |

---

## Data Structures

### TypedGraphState (existing — `services/typed_graph.rs:~44`)

```rust
pub struct TypedGraphState {
    pub use_fallback: bool,
    pub typed_graph: TypedRelationGraph,   // petgraph StableGraph<u64, RelationEdge>
    pub all_entries: Vec<EntryRecord>,
}
```

Cold-start initial value: `use_fallback: true`, empty graph, empty `all_entries`.
After `rebuild()`: `use_fallback: false`, populated graph (Quarantined entries excluded), full entry set.

### TypedGraphStateHandle (existing — `services/typed_graph.rs:161`)

```rust
pub type TypedGraphStateHandle = Arc<RwLock<TypedGraphState>>;
```

Three clones share the same backing allocation: `ServiceLayer.typed_graph_state`,
`SearchService.typed_graph_state`, and the clone returned by `typed_graph_handle()`.
Post-construction write through any clone is immediately visible to all holders.

### EvalServiceLayer (existing — `eval/profile/layer.rs`)

```rust
pub struct EvalServiceLayer {
    pub(crate) inner: ServiceLayer,
    // ... other fields (db_path, profile_name, analytics_mode, ...)
}
```

`inner.typed_graph_state` is the `TypedGraphStateHandle` shared with `inner.search_service`.

---

## Function Signatures

### Existing — called but not changed

```rust
// services/typed_graph.rs:91
pub async fn rebuild(store: &Store) -> Result<TypedGraphState, StoreError>

// services/mod.rs:297
pub fn typed_graph_handle(&self) -> TypedGraphStateHandle

// services/mod.rs (ServiceLayer constructor)
pub(crate) async fn with_rate_config(
    store: Arc<Store>,
    inference_config: Arc<InferenceConfig>,
    // ... other params
) -> Result<ServiceLayer, EvalError>  // signature MUST NOT change
```

### New — added in crt-045

```rust
// eval/profile/layer.rs — new accessor on EvalServiceLayer
pub(crate) fn typed_graph_handle(&self) -> TypedGraphStateHandle {
    self.inner.typed_graph_handle()
}
```

### Write-back idiom (Step 13b inside `from_profile()`)

```rust
// After with_rate_config() returns, if rebuild succeeded:
if let Some(rebuilt_state) = rebuilt_state {
    let handle = layer.inner.typed_graph_handle();
    let mut guard = handle.write().unwrap_or_else(|e| e.into_inner());
    *guard = rebuilt_state;
    // info! already logged at rebuild time (ADR-002)
}
```

---

## Constraints

| ID | Constraint |
|----|-----------|
| C-01 | `TypedGraphState::rebuild()` is `async fn` — MUST be called with `.await` directly from `from_profile()`. No `spawn_blocking`. |
| C-02 | Rebuild errors MUST NOT abort `from_profile()`. On error: `tracing::warn!`, set `rebuilt_state = None`, return `Ok(layer)`. |
| C-03 | `ServiceLayer::with_rate_config()` signature MUST NOT change. |
| C-04 | `EvalServiceLayer::typed_graph_handle()` MUST be `pub(crate)`. |
| C-05 | Snapshot database is read-only — `rebuild()` only reads. No WAL or locking concerns. |
| C-06 | TOML gate values: `mrr_floor = 0.2651`, `p_at_5_min = 0.1083`, `distribution_change = false`. Human-approved (OQ-01). Delivery agent MUST use these exact values. |
| C-07 | `ScenarioResult`, `ProfileResult`, and all runner/report types MUST NOT change. |
| C-08 | `typed_graph_handle()` on `EvalServiceLayer` delegates to `self.inner.typed_graph_handle()`. No new state needed on `EvalServiceLayer`. |
| C-09 | Integration test snapshot MUST contain at least two Active (not Quarantined, not Deprecated) entries and at least one S1, S2, or S8 graph edge between them. Quarantined-only snapshots produce vacuously empty graphs (ADR-004 addendum, entry #3768). |
| C-10 | No `#[cfg(test)]` guard on `typed_graph_handle()` — also used by `runner.rs` for pre-replay diagnostics (ADR-004). |

---

## Dependencies

### Crates (existing — no new dependencies)

| Crate | Component | Role in crt-045 |
|-------|-----------|----------------|
| `unimatrix-server` | `eval/profile/layer.rs` | Primary change site |
| `unimatrix-server` | `eval/profile/layer_tests.rs` | New integration test |
| `unimatrix-server` | `services/mod.rs` | Arc clone chain confirmed — read only |
| `unimatrix-server` | `services/typed_graph.rs` | `rebuild()`, `TypedGraphStateHandle` |
| `unimatrix-server` | `services/search.rs` | `if !use_fallback` guard — behavioral target |
| `unimatrix-store` | `SqlxStore` | Snapshot database opened in `from_profile()` |

### External (existing — no new crate additions)

| Dependency | Purpose |
|-----------|---------|
| `tokio` | `async/.await` for `rebuild()` call and `from_profile()` body |
| `tokio::sync::RwLock` | Write-lock acquisition for post-construction state swap |
| `tracing` | `info!` on success; `warn!` on cycle or I/O error |
| `sqlx` | Raw SQL in test fixture: `INSERT INTO graph_edges` for edge seeding |

---

## NOT in Scope

- Changing `TypedGraphState::rebuild()` implementation
- Changing `ServiceLayer::with_rate_config()` signature
- Changing `SearchService` fields or the `graph_expand` BFS algorithm
- Adding a periodic graph-rebuild loop inside `EvalServiceLayer`
- Adding `tokio::time::timeout` wrapper around rebuild (R-07 deferred; sqlx query timeout is implicit guard)
- Adding new `InferenceConfig` fields beyond what crt-042 shipped
- Enabling `ppr_expander_enabled = true` as default
- Fixing NLI wiring gaps (unrelated to graph population)
- Changes to `eval scenarios`, `eval report`, `run_eval.py`, or the Python harness
- Changes to `ScenarioResult`, `ProfileResult`, or runner/report types
- Changes to the background tick mechanism (`spawn_background_tick`)

---

## Alignment Status

**Overall: PASS — 0 variances requiring approval.**

| Check | Status |
|-------|--------|
| Vision Alignment | PASS — Fix enables the W1-3 eval gate condition; no vision drift |
| Milestone Fit | PASS — Wave 1 infrastructure correction only; no future-milestone capabilities |
| Scope Gaps | PASS — All SCOPE.md goals and AC addressed in all three design documents |
| Architecture Consistency | PASS — Architecture, specification, and risk documents are internally consistent |
| Risk Completeness | PASS — All six SR-01 through SR-06 risks resolved with dedicated test scenarios |
| Scope Additions | WARN (scope-conservative, no approval needed) — SPECIFICATION.md C-10 introduces an option to guard `typed_graph_handle()` with `#[cfg(test)]`. ADR-004 resolves this: no `#[cfg(test)]` guard applied; accessor is also useful to `runner.rs`. |

The single WARN is informational only and has been resolved by ADR-004. No scope variance flag needed.

---

## Pre-Implementation Checklist (Delivery Agent)

Before writing code, verify:
- [ ] Confirm `services/mod.rs:419` uses `Arc::clone(&typed_graph_state)` — not a value copy (SR-01; already documented as resolved in ADR-001 but must be confirmed at implementation time against the actual file)
- [ ] Confirm `find_terminal_active` is `pub(crate)` or accessible in `typed_graph.rs` (IR-04; if not, use direct graph node count assertion instead)
- [ ] Verify current baseline MRR against snapshot to confirm `mrr_floor = 0.2651` has not drifted since crt-042 (R-09; manual step before merge)
- [ ] Confirm snapshot used for manual harness run is post-crt-021 (contains `GRAPH_EDGES` table; pre-crt-021 snapshots will trigger degraded mode, not the fix)
