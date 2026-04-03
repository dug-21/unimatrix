# crt-045: Eval Harness — Wire `[inference]` Profile Overrides into EvalServiceLayer

## Problem Statement

`unimatrix eval run` accepts `[inference]` sections in profile TOMLs but the TypedRelationGraph
is never populated during eval runs. The `EvalServiceLayer::from_profile()` construction starts
`TypedGraphState` in cold-start mode (`use_fallback = true`) and never calls
`TypedGraphState::rebuild()` against the snapshot database. As a result, the `if !use_fallback`
guard in `search.rs` (Step 6d) prevents Phase 0 (`graph_expand`) from executing regardless of the
`ppr_expander_enabled` flag value — both `baseline.toml` and `ppr-expander-enabled.toml` run
identical search paths, producing bit-identical MRR/P@5 results.

The crt-042 eval gate (SR-03) cannot be validated through the offline harness because the graph
is always empty during eval. The `ppr_expander_enabled` config field flows correctly through TOML
parsing → `UnimatrixConfig` → `InferenceConfig` → `ServiceLayer::with_rate_config()` →
`SearchService` — the config wiring is correct. The missing piece is a single call to
`TypedGraphState::rebuild(&store)` inside `EvalServiceLayer::from_profile()`.

A secondary bug exists in `ppr-expander-enabled.toml`: it declares `distribution_change = true`
but omits the required `[profile.distribution_targets]` sub-table, which causes `eval run` to fail
at profile parse time before any graph issue can be observed.

Affected parties: any developer who relies on `eval run` to A/B test features involving the
`TypedRelationGraph` (graph_expand, graph_ppr, graph_suppression, Supersedes chain traversal).

## Goals

1. Call `TypedGraphState::rebuild(&store)` inside `EvalServiceLayer::from_profile()` so the
   pre-built typed relation graph is populated from the snapshot database before scenario replay.
2. Wire the rebuilt `TypedGraphState` into the `SearchService` via the existing
   `TypedGraphStateHandle` mechanism (write the result into the handle; `SearchService` reads it
   under a short read lock — the same pattern used by the live background tick).
3. Fix `ppr-expander-enabled.toml` to include a valid `[profile.distribution_targets]` sub-table
   so that `eval run` does not fail at profile parse time.
4. Add a test that verifies the typed graph is non-empty (and `use_fallback = false`) after
   `EvalServiceLayer::from_profile()` against a snapshot database containing graph edges.

## Non-Goals

- Changing the background tick mechanism (`spawn_background_tick`) — the fix is eval-path-only.
- Adding a periodic graph-rebuild loop inside `EvalServiceLayer` — a single rebuild at
  construction time is sufficient because the eval snapshot database is static (no writes).
- Changing the `TypedGraphState::rebuild()` implementation itself.
- Changing `SearchService` fields, `ServiceLayer::with_rate_config()` signature, or the
  `graph_expand` BFS algorithm — those are correct.
- Enabling `ppr_expander_enabled = true` as the default (post-eval decision, not in scope).
- Adding new InferenceConfig fields beyond what crt-042 already implemented.
- Fixing any NLI wiring gaps (NLI model loading is unrelated to graph population).
- Changes to the `eval scenarios`, `eval report`, or `run_eval.py` Python harness.

## Background Research

### Codebase Trace: The Configuration Flow

The `[inference]` section of a profile TOML IS deserialized and wired correctly:

1. `parse_profile_toml()` (validation.rs) strips `[profile]` and deserializes the remainder
   as `UnimatrixConfig`. The `[inference]` section populates `config_overrides.inference`.
2. `EvalServiceLayer::from_profile()` (layer.rs, Step 13) passes
   `Arc::new(profile.config_overrides.inference.clone())` to `ServiceLayer::with_rate_config()`.
3. `ServiceLayer::with_rate_config()` (services/mod.rs, lines 432–434) reads
   `inference_config.ppr_expander_enabled`, `inference_config.expansion_depth`, and
   `inference_config.max_expansion_candidates` and passes them directly to `SearchService::new()`.
4. `SearchService` stores these as fields and uses them in the `if self.ppr_expander_enabled`
   block in Phase 0 of `search()`.

**The config wiring is complete and correct. The flag reaches `SearchService`.**

### Root Cause: TypedGraphState Never Rebuilt During Eval

`TypedGraphState` starts in cold-start mode:
```
use_fallback: true
typed_graph: TypedRelationGraph::empty()
all_entries: Vec::new()
```

In `search.rs` Step 6d the guard is:
```rust
let (typed_graph, all_entries, use_fallback) = { ... };
...
if !use_fallback {       // <- always false during eval
    if self.ppr_expander_enabled {  // <- never reached
        // Phase 0 graph_expand
    }
    // Phase 1 (PPR) also never reached
}
```

`TypedGraphState::rebuild(&store)` is an `async fn` that queries `GRAPH_EDGES` and all entries
from the store, then calls `build_typed_relation_graph()`. In the live server path, the background
tick calls this and writes the result back through the `Arc<RwLock<TypedGraphState>>` handle. In
the eval path, no equivalent call is made. The handle starts cold and stays cold.

The fix is a single call to `TypedGraphState::rebuild(&store_arc).await` inside
`EvalServiceLayer::from_profile()`, followed by a write-lock swap into `typed_graph_state`. This
mirrors what the background tick does on every tick interval.

### Secondary Bug: ppr-expander-enabled.toml Missing distribution_targets

`product/research/ass-037/harness/profiles/ppr-expander-enabled.toml` declares
`distribution_change = true` but provides no `[profile.distribution_targets]` section.
`parse_profile_toml()` requires all three target fields (`cc_at_k_min`, `icd_min`, `mrr_floor`)
when `distribution_change = true`, returning `EvalError::ConfigInvariant` if any are absent.
This means `unimatrix eval run` fails immediately at profile parse time, before any graph issue
is observable. This TOML must be fixed to provide concrete threshold values.

### Existing Patterns Applicable Here

- **NLI handle wiring (crt-023):** `EvalServiceLayer::from_profile()` already conditionally
  creates and starts an `NliServiceHandle` for NLI-enabled profiles (Step 6b in layer.rs). The
  typed graph rebuild follows the same conditional-init pattern: create the handle, rebuild, write
  result in.
- **TypedGraphState::rebuild() error handling:** On `StoreError::InvalidInput` (cycle detected),
  the live tick sets `use_fallback = true` and continues — eval should do the same. On store I/O
  errors, log a warning and leave `use_fallback = true` (safe degraded mode, not a fatal error).
- **`eval/profile/layer_tests.rs`:** Integration tests already exercise `from_profile()` against a
  real `SqlxStore`. The new test adds entries + graph edges, calls `from_profile()`, and then
  directly inspects the `TypedGraphState` via `EvalServiceLayer.inner.typed_graph_handle()`.

### Unimatrix Knowledge Retrieved

- Entry #3582 (eval-harness sidecar pattern): confirms the side-car metadata isolation pattern
  from nan-010 — no changes to `ScenarioResult` needed for this fix.
- Entry #3610 (7-component eval harness extension pattern): not applicable here — this is a
  targeted bug fix, not a new feature gate.
- Entry #4064 (InferenceConfig dual-maintenance guard): confirms InferenceConfig already has all
  three crt-042 fields at all four coordinated sites. No new config fields are needed.
- Entry #4052 (crt-042 ADR unconditional config validation): confirmed — config validation is
  already correct and unconditional.

### Why Eval Results Are Identical (Not Just Wrong)

When `use_fallback = true`:
- Phase 0 (graph_expand) is skipped entirely: no expanded candidates.
- Phase 1 (PPR) is also skipped: no PPR personalization vector.
- Phase 2 (graph penalties via graph_penalty) uses FALLBACK_PENALTY (constant) for deprecated
  entries rather than graph-derived penalties.

Both `baseline.toml` and `ppr-expander-enabled.toml` produce identical outputs because both run
the identical cold-start fallback path — no graph traversal occurs in either profile.

## Proposed Approach

**Single-file change + TOML fix + one test:**

1. **`crates/unimatrix-server/src/eval/profile/layer.rs`** — Add a `TypedGraphState::rebuild()`
   call after Step 5 (store construction), before Step 6 (embed handle). Insert result into the
   `typed_graph_state` handle via write lock. On cycle detection or store error, log a warning and
   leave `use_fallback = true` (degraded mode, not abort). The `EvalServiceLayer::from_profile()`
   method currently constructs `ServiceLayer::with_rate_config()` in Step 13, which internally
   calls `TypedGraphState::new_handle()` and passes it to `SearchService`. The fix must call
   `rebuild()` before `with_rate_config()` and either (a) pass the pre-populated handle into
   `ServiceLayer`, or (b) write into the handle after construction. Option (b) is simpler — call
   `rebuild()` before `with_rate_config()`, store result in a local, then after the
   `ServiceLayer` is built, write the rebuilt state into the handle via
   `layer.inner.typed_graph_handle()`. This avoids any signature changes to `with_rate_config()`.

   Revised approach (preferred): call `TypedGraphState::rebuild(&store_arc)` before
   `with_rate_config()`. Construct a pre-populated handle via `Arc::new(RwLock::new(rebuilt_state))`
   instead of the cold-start `TypedGraphState::new_handle()`. Then pass this pre-populated handle
   into `with_rate_config()`. This requires adding a parameter to `with_rate_config()` — but
   `with_rate_config()` already creates `TypedGraphState::new_handle()` internally and cannot
   accept a pre-built handle without a signature change. The write-after-construction approach
   (option b) is therefore preferred as it avoids that signature change.

2. **`product/research/ass-037/harness/profiles/ppr-expander-enabled.toml`** — Add
   `[profile.distribution_targets]` with concrete thresholds derived from the crt-042 gate values:
   `cc_at_k_min`, `icd_min`, `mrr_floor`. Threshold values to be specified by the delivery agent
   based on current baseline metrics (see Open Questions).

3. **`crates/unimatrix-server/src/eval/profile/layer_tests.rs`** — Add one integration test:
   seed a SqlxStore with entries + graph edges, call `from_profile()`, assert that the `SearchService`'s
   `TypedGraphState` has `use_fallback = false` and a non-empty graph after construction. This
   requires `EvalServiceLayer` to expose the `TypedGraphStateHandle` (add an accessor method
   mirroring `embed_handle()` and `nli_handle()`).

**No changes needed to:**
- `InferenceConfig` (all three crt-042 fields are present)
- `ServiceLayer::with_rate_config()` signature
- `SearchService` or `graph_expand`
- `run_eval.py` Python harness
- `eval/runner/` or `eval/report/` code

## Acceptance Criteria

- AC-01: After `EvalServiceLayer::from_profile()` on a snapshot database containing graph edges,
  `TypedGraphState.use_fallback` is `false` and `TypedGraphState.typed_graph` is non-empty.
- AC-02: Running `unimatrix eval run` with `ppr-expander-enabled.toml` and a populated snapshot
  produces measurably different MRR/P@5 results from `baseline.toml` (the PPR+graph_expand path
  executes, not the fallback path).
- AC-03: Running `unimatrix eval run` with `ppr-expander-enabled.toml` does not fail at profile
  parse time — the TOML is valid (distribution_change=true with all three required target fields).
- AC-04: Running `unimatrix eval run` with `baseline.toml` (empty inference section) continues to
  produce the same results as before this fix — no regression in the baseline path.
- AC-05: `EvalServiceLayer::from_profile()` does not abort on rebuild failure (cycle detected or
  store I/O error) — it logs a warning, leaves `use_fallback = true`, and returns `Ok(layer)`.
- AC-06: Integration test in `layer_tests.rs` asserts `use_fallback = false` and non-empty
  `typed_graph` after `from_profile()` with a graph-seeded snapshot.
- AC-07: `cargo test --workspace` passes with no new failures.
- AC-08: All existing eval profile integration tests (layer_tests.rs, eval/profile/tests.rs)
  continue to pass — no behavioral regression in non-graph paths.

## Constraints

| ID | Constraint |
|----|-----------|
| C-01 | `TypedGraphState::rebuild()` is `async fn` — must be called from within the `async` `from_profile()` body. No spawn_blocking needed (rebuild is pure DB reads). |
| C-02 | Rebuild errors (cycle or store I/O) must not abort `from_profile()` — degrade to `use_fallback = true` with a tracing::warn. |
| C-03 | The `typed_graph_state` handle in `ServiceLayer` is created inside `with_rate_config()` via `TypedGraphState::new_handle()` and is not currently exposed as a constructor parameter. Post-construction write is the preferred approach to avoid signature changes. |
| C-04 | `EvalServiceLayer` must expose a `typed_graph_handle()` accessor (returns `TypedGraphStateHandle`) to enable testing — mirrors the existing `embed_handle()` and `nli_handle()` accessors. |
| C-05 | The snapshot database is read-only — `TypedGraphState::rebuild()` only reads from it, producing no writes. No WAL or locking concerns. |
| C-06 | `ppr-expander-enabled.toml` threshold values for `cc_at_k_min`, `icd_min`, and `mrr_floor` must be grounded in current baseline metrics. See Open Questions OQ-01. |
| C-07 | No changes to `ScenarioResult`, `ProfileResult`, or any runner/report type — the dual-type JSON boundary is unchanged (entry #3526). |
| C-08 | `EvalServiceLayer` holds `inner: ServiceLayer` as `pub(crate)`. The `typed_graph_handle()` accessor can be delegated: `self.inner.typed_graph_handle()` (the accessor is already `pub` on `ServiceLayer`). |

## Decisions (Human-Approved)

- **OQ-01 RESOLVED:** `ppr-expander-enabled.toml` — set `distribution_change = false`. Gate on:
  - `mrr_floor = 0.2651` (no regression from current baseline)
  - `p_at_5_min = 0.1083` (improvement — first run where P@5 should respond to cross-category entries)
  
  CC@k and ICD are future metrics — measure on first run, establish baselines, gate in subsequent runs. Do NOT invent floors we have never measured.

- **OQ-02 RESOLVED:** Log `TypedGraphState::rebuild()` at `info!` in eval context. The rebuild is the significant operation that makes the entire profile meaningful — visible without debug mode.

- **OQ-03/OQ-04:** Non-blocking. Post-construction `Arc<RwLock<_>>` write propagates to `SearchService`. Verify results by running the harness post-delivery.

## Tracking

https://github.com/dug-21/unimatrix/issues/499
