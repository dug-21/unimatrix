# Agent Report: crt-029-agent-6-background-call-site

**Feature**: crt-029 — Background Graph Inference (Supports Edges)
**Component**: background-call-site
**GH Issue**: #412

---

## Task

Add the `run_graph_inference_tick` call site to `background.rs`, gated on `inference_config.nli_enabled`, immediately after the existing `maybe_run_bootstrap_promotion` block.

---

## Files Modified

- `crates/unimatrix-server/src/background.rs`

---

## Changes Made

### Import added (line 46)

```rust
use crate::services::nli_detection_tick::run_graph_inference_tick;
```

Mirrors the existing `maybe_run_bootstrap_promotion` import pattern.

### Call site added (after line 669, inside `run_single_tick`)

```rust
// crt-029: Background graph inference (recurring, cap-throttled via max_graph_inference_per_tick).
// Runs after bootstrap promotion so bootstrap-promoted edges are visible to the tick's
// pre-filter HashSet. Must remain after maybe_run_bootstrap_promotion (sequencing invariant).
if inference_config.nli_enabled {
    run_graph_inference_tick(store, nli_handle, vector_index, ml_inference_pool, inference_config).await;
}
```

Sequencing invariant satisfied: `maybe_run_bootstrap_promotion` at line 669, `run_graph_inference_tick` at line 676.

---

## Static Checks (Pre-Merge Gates)

| Check | Result |
|-------|--------|
| Import present | PASS — `use crate::services::nli_detection_tick::run_graph_inference_tick;` at line 46 |
| Ordering invariant | PASS — bootstrap promotion line 669 < inference tick line 676 |
| `nli_enabled` guard present | PASS — `if inference_config.nli_enabled` wraps the call |
| `pub mod nli_detection_tick;` in mod.rs | PASS — confirmed at `services/mod.rs:33` |

---

## Build & Test Results

- **Build**: clean — `Finished dev profile` with zero errors
- **Tests**: all pass — no failures across workspace
  - 2226 unit tests + integration suites all ok

---

## Issues / Blockers

None. The `nli_detection_tick` module was already created by agent-5 and declared in `services/mod.rs` prior to this agent running. All prerequisites were in place.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned entries #3656 (ADR-001 module split), #3653 (single rayon spawn pattern), #3655 (source candidate bound), #3658 (ADR-003 bound derived). Entry #3653 confirmed the single-rayon-dispatch constraint in the tick; confirms this call site correctly defers all NLI work to the tick module.
- Stored: nothing novel to store — the call site is a two-line addition following an established pattern already documented in the briefing results (#3213: Arc startup resource threading). No new patterns emerged from this component.
