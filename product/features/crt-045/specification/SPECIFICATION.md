# SPECIFICATION: crt-045 — Eval Harness: Wire TypedGraphState Rebuild into EvalServiceLayer

## Objective

`EvalServiceLayer::from_profile()` constructs a `ServiceLayer` that starts with `TypedGraphState`
in cold-start mode (`use_fallback = true`, empty `TypedRelationGraph`). The `if !use_fallback`
guard in `search.rs` Step 6d prevents graph_expand (Phase 0), PPR (Phase 1), and graph-penalty
traversal from executing during every eval run regardless of `ppr_expander_enabled` value. This
specification defines the requirements to fix the eval path by calling
`TypedGraphState::rebuild(&store)` during `EvalServiceLayer::from_profile()`, writing the rebuilt
state into the existing handle, fixing the malformed `ppr-expander-enabled.toml`, and adding a
verification integration test.

---

## Functional Requirements

**FR-01** `EvalServiceLayer::from_profile()` MUST call `TypedGraphState::rebuild(&store_arc).await`
after the `SqlxStore` is opened (after step 5 in layer.rs) and before `ServiceLayer::with_rate_config()` is called.

**FR-02** After `ServiceLayer::with_rate_config()` completes construction, the rebuilt
`TypedGraphState` value MUST be written into the handle returned by
`layer.inner.typed_graph_handle()` via a write-lock swap. The post-construction write propagates
to `SearchService` because the handle is `Arc<RwLock<TypedGraphState>>` and `SearchService` holds
an `Arc::clone()` of the same backing allocation, not a value copy.

**FR-03** On `StoreError::InvalidInput` (cycle detected during rebuild) OR any store I/O error
during rebuild, `from_profile()` MUST log `tracing::warn!` and leave `use_fallback = true`,
returning `Ok(layer)`. Rebuild failure MUST NOT abort `from_profile()` with `Err(...)`.

**FR-04** On a successful rebuild, `from_profile()` MUST log the completed rebuild at
`tracing::info!` level, recording at minimum that the typed graph was rebuilt. The rebuild is a
significant operation that makes the entire profile meaningful — it must be visible without debug
mode.

**FR-05** `EvalServiceLayer` MUST expose a `typed_graph_handle()` accessor that returns the
`TypedGraphStateHandle` (`Arc<RwLock<TypedGraphState>>`). The accessor MUST be `pub(crate)`.
This mirrors the existing `embed_handle()` and `nli_handle()` accessors and enables test inspection.

**FR-06** `product/research/ass-037/harness/profiles/ppr-expander-enabled.toml` MUST be fixed
such that `eval run` does not fail at profile parse time. The required TOML changes are defined
in the TOML Schema section below.

**FR-07** One new integration test MUST be added to
`crates/unimatrix-server/src/eval/profile/layer_tests.rs`. The test MUST:
- Seed a `SqlxStore` with at least two Active (non-Quarantined, non-Deprecated) entries and at
  least one S1, S2, or S8 graph edge between those two entries.
- Call `EvalServiceLayer::from_profile()` against the seeded snapshot.
- Assert `use_fallback == false` on the resolved `TypedGraphState`.
- Assert `typed_graph` is non-empty (at least one node and one edge present).
- Invoke at least one live search operation against the constructed `EvalServiceLayer` to confirm
  that `SearchService` observes the rebuilt graph at query time — not merely that the handle field
  holds the correct value (per SR-05: wired-but-unused anti-pattern).

**FR-08** All existing integration tests in `layer_tests.rs` and `eval/profile/tests.rs` MUST
continue to pass after this change. Tests that construct `EvalServiceLayer` without graph edges
continue to exercise the degraded `use_fallback = true` path — that path remains valid and must
not regress.

---

## Non-Functional Requirements

**NFR-01 Performance** — `TypedGraphState::rebuild()` consists entirely of DB reads against a
static snapshot. For evaluation-sized snapshots (up to approximately 10,000 entries and 50,000
edges) the rebuild MUST complete within 5 seconds. No background tick or retry loop is introduced.

**NFR-02 Memory** — The rebuild allocates one `TypedRelationGraph` in memory and immediately swaps
it into the existing handle. The prior cold-start empty state is dropped. Peak memory overhead is
one graph allocation plus the prior value until the write-lock is released.

**NFR-03 Concurrency** — The `from_profile()` function is not called concurrently for the same
`EvalServiceLayer` instance. The write-lock acquisition for the post-construction swap must not
hold the lock for longer than the duration of the swap operation itself (the rebuild completes
before the lock is acquired).

**NFR-04 Observability** — Rebuild success logs at `info!`; rebuild failure (cycle or I/O error)
logs at `warn!`. No `error!` or `debug!`-only events for these two outcomes.

**NFR-05 API Stability** — `ServiceLayer::with_rate_config()` signature MUST NOT change.
`SearchService` fields MUST NOT change. `ScenarioResult`, `ProfileResult`, and all runner/report
types MUST NOT change.

**NFR-06 Test Suite** — `cargo test --workspace` MUST pass with zero new failures after the
change.

---

## Acceptance Criteria

| AC-ID | Criterion | Verification Method |
|-------|-----------|---------------------|
| AC-01 | After `EvalServiceLayer::from_profile()` on a snapshot containing graph edges, `TypedGraphState.use_fallback` is `false` and `typed_graph` is non-empty. | Integration test in `layer_tests.rs` — direct handle inspection via `typed_graph_handle()` accessor. |
| AC-02 | Running `unimatrix eval run` with `ppr-expander-enabled.toml` and a populated snapshot produces measurably different MRR/P@5 results from `baseline.toml`. The PPR+graph_expand path executes, not the fallback path. | Manual harness run comparing `ppr-expander-enabled.toml` vs `baseline.toml` metric output. Results must not be bit-identical. |
| AC-03 | Running `unimatrix eval run` with `ppr-expander-enabled.toml` does not fail at profile parse time. | Manual `unimatrix eval run --profile ppr-expander-enabled.toml` exits without `EvalError::ConfigInvariant`. |
| AC-04 | Running `unimatrix eval run` with `baseline.toml` (empty inference section) produces the same results as before this fix. No regression in the baseline path. | Automated regression: existing baseline eval output is stable. Manual: MRR/P@5 for `baseline.toml` must not shift. |
| AC-05 | `EvalServiceLayer::from_profile()` does not abort on rebuild failure (cycle detected or store I/O error). It logs a warning, leaves `use_fallback = true`, and returns `Ok(layer)`. | Integration test: seed store with a cycle-producing edge set, call `from_profile()`, assert result is `Ok` and `use_fallback == true`. |
| AC-06 | Integration test in `layer_tests.rs` asserts `use_fallback = false` and non-empty `typed_graph` after `from_profile()` with a graph-seeded snapshot. The test also invokes a search operation to confirm `SearchService` observes the rebuilt graph at query time (SR-05: no wired-but-unused). | Automated: test must compile and pass under `cargo test`. |
| AC-07 | `cargo test --workspace` passes with no new failures. | CI gate — full workspace test run. |
| AC-08 | All existing eval profile integration tests (`layer_tests.rs`, `eval/profile/tests.rs`) continue to pass — no behavioral regression in non-graph paths. | Automated: existing tests pass unchanged. |

---

## Domain Model

### Entities and Relationships

```
EvalServiceLayer
  ├── inner: ServiceLayer (pub(crate))
  │     ├── typed_graph_state: TypedGraphStateHandle   ← Arc<RwLock<TypedGraphState>>
  │     ├── search_service: SearchService
  │     │     └── typed_graph_state: TypedGraphStateHandle  ← Arc::clone() of same allocation
  │     ├── embed_handle: EmbedServiceHandle
  │     └── nli_handle: Option<NliServiceHandle>
  └── accessors (pub(crate)):
        ├── embed_handle()      ← existing
        ├── nli_handle()        ← existing
        └── typed_graph_handle() ← NEW (FR-05)
```

### Key Terms (Ubiquitous Language)

**TypedGraphState** — Runtime container for the typed relation graph. Holds three fields:
`use_fallback: bool`, `typed_graph: TypedRelationGraph`, and `all_entries: Vec<EntryRecord>`.
Starts cold (`use_fallback = true`, empty graph) and is populated by `rebuild(&store)`.

**TypedGraphStateHandle** — Type alias for `Arc<RwLock<TypedGraphState>>`. The live server
background tick and the eval path both write through this handle; `SearchService` reads from it
under a short read lock at query time. The Arc ensures all holders reference the same backing
allocation — writing to the handle via write-lock is observable to all clone holders.

**TypedRelationGraph** — The petgraph `StableGraph<u64, RelationEdge>` containing all active and
deprecated entries as nodes, with typed edges (S1, S2, S8, Supersedes, Contradicts). Quarantined
entries are excluded (ADR-004 addendum, entry #3768).

**cold-start mode** — The initial state of `TypedGraphState` before `rebuild()` is called:
`use_fallback = true`, `typed_graph = TypedRelationGraph::empty()`, `all_entries = Vec::new()`.
During eval, cold-start means `graph_expand`, PPR, and graph-penalty traversal are all bypassed.

**EvalServiceLayer** — The eval-path analogue of `ServiceLayer`. Constructed from a profile TOML
via `from_profile()`. Holds an inner `ServiceLayer` configured with the profile's
`InferenceConfig` overrides. The eval path does not run a background tick; it must rebuild the
graph once at construction time.

**from_profile()** — The async constructor for `EvalServiceLayer`. Reads the profile TOML, opens
the snapshot `SqlxStore`, constructs the `ServiceLayer` via `with_rate_config()`, and (after this
fix) calls `TypedGraphState::rebuild()` and writes the result into the handle.

**snapshot database** — The read-only SQLite file used by `eval run`. Contains the graph edges
and entry records that `TypedGraphState::rebuild()` reads. No writes occur during an eval run.

**use_fallback** — The boolean sentinel inside `TypedGraphState`. When `true`, the
`if !use_fallback` guard in `search.rs` Step 6d prevents all graph-dependent search phases.
When `false`, `ppr_expander_enabled`, PPR, and graph-penalty traversal can execute.

**ppr_expander_enabled** — `InferenceConfig` field. When `true` and `use_fallback` is `false`,
Phase 0 (`graph_expand`) and Phase 1 (PPR) execute. Config wiring from TOML through
`UnimatrixConfig` → `InferenceConfig` → `SearchService` is correct and unchanged.

---

## User Workflows

### Workflow 1: Developer A/B Testing a Graph-Dependent Feature via Eval

1. Developer writes or edits a profile TOML under `product/research/ass-037/harness/profiles/`
   with an `[inference]` section enabling `ppr_expander_enabled = true`.
2. Developer runs `unimatrix eval run --profile ppr-expander-enabled.toml`.
3. `EvalServiceLayer::from_profile()` opens the snapshot store, calls
   `TypedGraphState::rebuild()`, logs the rebuild at `info!`, and writes the result into the
   handle.
4. Scenario replay executes with `use_fallback = false`; `graph_expand` and PPR paths activate.
5. `ProfileResult` contains MRR/P@5 measurably different from `baseline.toml`.
6. Developer compares metric outputs and gates against `mrr_floor = 0.2651` and
   `p_at_5_min = 0.1083`.

### Workflow 2: Developer Running Baseline (No Graph Profile)

1. Developer runs `unimatrix eval run --profile baseline.toml`.
2. `baseline.toml` has no `[inference]` section; `ppr_expander_enabled` defaults to `false`.
3. `TypedGraphState::rebuild()` is still called (it always runs after this fix), but
   `ppr_expander_enabled = false` means Phase 0 and Phase 1 are not entered.
4. Results are identical to pre-fix baseline — no behavioral regression.

### Workflow 3: Rebuild Failure During Eval Construction

1. `from_profile()` calls `TypedGraphState::rebuild()` against a snapshot with a corrupt or
   cycle-producing `GRAPH_EDGES` table.
2. Rebuild returns `StoreError::InvalidInput` (cycle) or an I/O error.
3. `from_profile()` logs `tracing::warn!` and leaves `use_fallback = true`.
4. `from_profile()` returns `Ok(layer)` — eval run proceeds in degraded mode.
5. Graph-dependent phases do not execute; the profile behaves as if no graph data is available.

---

## ppr-expander-enabled.toml Schema

The fixed `product/research/ass-037/harness/profiles/ppr-expander-enabled.toml` MUST conform to
the following schema. Fields marked REQUIRED must be present; fields marked FIXED are
human-approved values (OQ-01 resolved in SCOPE.md).

### `[profile]` section

| Field | Value | Rationale |
|-------|-------|-----------|
| `distribution_change` | `false` (FIXED) | `distribution_change = true` requires all three `[profile.distribution_targets]` sub-fields. CC@k and ICD have never been measured against this profile; inventing floors would gate-fail a correct implementation. Set to `false` until first-run baselines are established. |

Note: When `distribution_change = false`, the `[profile.distribution_targets]` sub-table and its
fields (`cc_at_k_min`, `icd_min`) are structurally optional and MUST NOT be present unless the
gate is intentionally activated. A TOML comment MUST explain why `distribution_change = false`
is intentional to prevent a future editor from setting it to `true` without the required targets.

### `[profile.gates]` or equivalent gate fields

| Field | Value | Rationale |
|-------|-------|-----------|
| `mrr_floor` | `0.2651` (FIXED) | No regression from current baseline MRR (OQ-01). |
| `p_at_5_min` | `0.1083` (FIXED) | Improvement gate — first run where P@5 should respond to cross-category entries introduced by graph_expand (OQ-01). |

### `[inference]` section

| Field | Value | Notes |
|-------|-------|-------|
| `ppr_expander_enabled` | `true` | Activates Phase 0 (graph_expand) and Phase 1 (PPR) in `search.rs`. |
| `expansion_depth` | (existing crt-042 default or explicit value) | Must be present if required by `InferenceConfig` deserialization. |
| `max_expansion_candidates` | (existing crt-042 default or explicit value) | Must be present if required by `InferenceConfig` deserialization. |

---

## Constraints

| ID | Constraint |
|----|-----------|
| C-01 | `TypedGraphState::rebuild()` is `async fn` — MUST be called from within the `async` `from_profile()` body via `.await`. No `spawn_blocking` or thread-pool dispatch needed (rebuild is pure DB reads). |
| C-02 | Rebuild errors (cycle: `StoreError::InvalidInput`, or store I/O) MUST NOT abort `from_profile()`. On error: log `tracing::warn!`, leave `use_fallback = true`, return `Ok(layer)`. |
| C-03 | `ServiceLayer::with_rate_config()` signature MUST NOT change. The post-construction write-after-construction approach (option b from SCOPE.md) is used: rebuild before `with_rate_config()`, write result into handle after `ServiceLayer` is built. |
| C-04 | `EvalServiceLayer` MUST expose `typed_graph_handle()` as `pub(crate)`. Visibility MUST NOT be `pub` — this accessor widens the API surface and may conflict with future `ServiceLayer` encapsulation (SR-03). |
| C-05 | The snapshot database is read-only — `TypedGraphState::rebuild()` only reads from it. No WAL or write-locking concerns apply. |
| C-06 | `ppr-expander-enabled.toml` gate values: `mrr_floor = 0.2651`, `p_at_5_min = 0.1083`. `distribution_change = false`. These are human-approved (OQ-01, SCOPE.md). The delivery agent MUST use these exact values. |
| C-07 | `ScenarioResult`, `ProfileResult`, and all runner/report types MUST NOT change. The dual-type JSON schema boundary is unchanged (entry #3526). |
| C-08 | `typed_graph_handle()` on `EvalServiceLayer` MUST delegate to `self.inner.typed_graph_handle()`. The accessor already exists as `pub` on `ServiceLayer`; no new accessor is needed there. |
| C-09 | The integration test (FR-07 / AC-06) snapshot MUST contain at least two Active (not Quarantined, not Deprecated) entries and at least one S1, S2, or S8 graph edge between them. An empty-graph snapshot will produce a vacuous pass where `use_fallback == false` but `typed_graph` is empty (SR-06). |
| C-10 | The `typed_graph_handle()` accessor body MAY use `#[cfg(test)]` if it is only invoked from test code. If the accessor is used in production code paths (e.g., future background tick integration), the guard must be removed. |

---

## Dependencies

### Crates (existing, no new dependencies)

| Crate | Component | Role |
|-------|-----------|------|
| `unimatrix-server` | `eval/profile/layer.rs` | Primary change site — `EvalServiceLayer::from_profile()` |
| `unimatrix-server` | `eval/profile/layer_tests.rs` | New integration test |
| `unimatrix-server` | `services/mod.rs` | `ServiceLayer::with_rate_config()` — read-only reference, no changes |
| `unimatrix-server` | `search.rs` | `if !use_fallback` guard — read-only reference, confirms the fix works end-to-end |
| `unimatrix-engine` | `typed_graph.rs` | `TypedGraphState::rebuild()`, `TypedGraphStateHandle`, `TypedRelationGraph` |
| `unimatrix-store` | `SqlxStore` | Snapshot database opened in `from_profile()` |

### External Infrastructure

| Dependency | Purpose |
|-----------|---------|
| `tokio` | `async/.await` for `from_profile()` and `rebuild()` |
| `tokio::sync::RwLock` | Write-lock acquisition for post-construction handle swap |
| `tracing` | `info!` and `warn!` logging for rebuild outcome |

### Existing Patterns Applied

- **NLI handle wiring (crt-023):** `from_profile()` already conditionally creates and starts an
  `NliServiceHandle` for NLI-enabled profiles (Step 6b). The graph rebuild follows the same
  conditional-init pattern in the same function.
- **Background tick rebuild:** The live server background tick calls `TypedGraphState::rebuild()`
  and writes the result through the same `Arc<RwLock<TypedGraphState>>` handle. The eval path
  mirrors this write pattern exactly.
- **ADR-004 addendum (entry #3768):** `TypedGraphState::rebuild()` already excludes Quarantined
  entries. The test fixture must use Active entries only — Quarantined entries in the seed would
  cause the graph to appear empty even after a successful rebuild.

---

## NOT in Scope

- Changing `TypedGraphState::rebuild()` implementation.
- Changing `ServiceLayer::with_rate_config()` signature.
- Changing `SearchService` fields or `graph_expand` BFS algorithm.
- Adding a periodic graph-rebuild loop inside `EvalServiceLayer`.
- Adding any new `InferenceConfig` fields beyond what crt-042 shipped.
- Enabling `ppr_expander_enabled = true` as a default.
- Fixing NLI wiring gaps (NLI model loading is unrelated to graph population).
- Changes to `eval scenarios`, `eval report`, `run_eval.py`, or the Python harness.
- Changes to `ScenarioResult`, `ProfileResult`, or runner/report types.
- Changes to the background tick mechanism (`spawn_background_tick`).
- Adding a `tokio::time::timeout` wrapper around rebuild (SR-02: sqlx query timeout is sufficient;
  if not, this is a follow-up issue, not in scope for crt-045).

---

## Open Questions

None — all OQs from SCOPE.md are resolved:

- **OQ-01 (RESOLVED):** `distribution_change = false`; `mrr_floor = 0.2651`; `p_at_5_min = 0.1083`. CC@k and ICD gates deferred until first-run baselines are measured.
- **OQ-02 (RESOLVED):** Rebuild logs at `info!` level.
- **OQ-03 / OQ-04 (RESOLVED, non-blocking):** Post-construction `Arc<RwLock<_>>` write propagates to `SearchService` because the handle is shared by `Arc::clone()`. Verified via entry #4096. Results confirmed by running the harness post-delivery.

The architect must verify SR-01 (that `SearchService` holds `Arc::clone()` of the handle, not a
value copy) before committing to the write-after-construction approach. This is a pre-implementation
read of `services/mod.rs` lines ~432–434 and the `SearchService` constructor. If `SearchService`
holds a value copy, the implementation must use the pre-populated handle constructor approach
(option a), which requires a `with_rate_config()` signature change — that change would elevate
to a constraint revision requiring a scope variance flag.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned entry #4096 (EvalServiceLayer cold-start
  pattern, directly applicable), entry #3768 (ADR-004 addendum: Quarantined entry filter, applied
  in C-09 and FR-07), entry #3526 (eval dual-type JSON boundary, applied in C-07 and NFR-05).
  All three entries were directly incorporated into this specification.
