# Agent Report: crt-038-agent-1-architect

## Task
Design architecture for crt-038: conf-boost-c formula application and NLI dead-code removal.

## Artifacts Produced

### ARCHITECTURE.md
`/workspaces/unimatrix/product/features/crt-038/architecture/ARCHITECTURE.md`

### ADR Files
| File | Unimatrix ID |
|------|-------------|
| `architecture/ADR-001-effective-zero-nli-short-circuit.md` | #4005 |
| `architecture/ADR-002-nlistoreconfig-complete-deletion.md` | #4006 |
| `architecture/ADR-003-implementation-ordering.md` | #4007 |
| `architecture/ADR-004-nli-detection-module-merge-deferred.md` | #4008 |

## Key Decisions

**ADR-001 (#4005)**: `FusionWeights::effective()` must short-circuit when `self.w_nli == 0.0` and return `*self` unchanged, before the `nli_available` branch. Re-normalizing zero NLI weight is a correctness error that silently shifts the formula to `w_sim'≈0.588, w_conf'≈0.412`. This is the highest-priority change and must precede any eval run.

**ADR-002 (#4006)**: `NliStoreConfig` is deleted entirely (AC-14 is authoritative over the SCOPE.md Background ambiguity). No fields retained. `InferenceConfig` retains same-named fields independently for `run_graph_inference_tick`.

**ADR-003 (#4007)**: Mandatory ordering — effective() short-circuit first, formula defaults second, eval gate third. Dead-code removals may follow in any order. The eval gate (MRR ≥ 0.2913) is only valid when run on the correct scoring path.

**ADR-004 (#4008)**: Module merge of `nli_detection.rs` into `nli_detection_tick.rs` deferred to Group 2. The three retained helpers (`format_nli_metadata`, `write_nli_edge`, `current_timestamp_secs`) stay in place. `write_edges_with_cap` is deleted alongside the dead functions (no callers remain after removal).

## Critical Finding: Shared Helper Enumeration (SR-06 Resolution)

`nli_detection_tick.rs` line 34 imports exactly three symbols from `nli_detection.rs`:
```rust
use crate::services::nli_detection::{current_timestamp_secs, format_nli_metadata, write_nli_edge};
```

These must NOT be deleted. All other functions in `nli_detection.rs` that are not in this import list are either being explicitly removed (the three dead functions) or are callerless after removal (`write_edges_with_cap`) and should also be deleted.

## Pre-existing Constraint Acknowledgement

`background.rs` at 4,229 lines is a pre-existing 500-line rule violation. Removal of ~60 NLI lines does not resolve it. This is not a gate condition for crt-038.

## Open Questions
None blocking delivery. See ARCHITECTURE.md for notes on w_util/w_prov signal zeroing (SR-05, low risk) and the deferred module merge.
