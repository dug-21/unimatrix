# crt-018b Researcher v2 Report

## Agent ID
crt-018b-researcher-v2

## Summary

Re-researched the crt-018b problem space with a specific mandate to correct the trigger
mechanism for `EffectivenessState` updates. The prior scope draft was wrong in a subtle but
important way: it said "Updated by StatusService Phase 8 after every effectiveness report
computation" — which implied `context_status` calls would write to `EffectivenessState`. That
is incorrect. The corrected SCOPE.md reflects the true architecture.

## Key Findings

### Finding 1: context_status is strictly read-only (confirmed)

The MCP `context_status` handler (`mcp/tools.rs`) calls only `status_svc.compute_report()`.
It does not call `run_maintenance()`. It makes no database writes. The `maintain` parameter
was removed in a prior feature. This is verified by direct code inspection.

### Finding 2: Phase 8 runs inside compute_report(), not run_maintenance()

Effectiveness analysis (Phase 8, status.rs lines 661-736) is part of `compute_report()`. It
fires on EVERY call to `compute_report()` — both from `context_status` and from the background
tick's `maintenance_tick()`. Phase 8 results are stored only in `StatusReport.effectiveness`
for display. They are never written to any shared in-memory state by the existing code.

### Finding 3: ConfidenceState is written in run_maintenance(), not compute_report()

This is the authoritative pattern for crt-018b to follow:
- `ConfidenceState` writer = `StatusService::run_maintenance()` Step 2b
- `run_maintenance()` is called only by `background.rs::maintenance_tick()` (every 15 minutes)
- NOT called on `context_status` MCP invocations

This means `ConfidenceState` is a background-tick-only cache. `EffectivenessState` must
follow the exact same pattern.

### Finding 4: The correct EffectivenessState write path

`background.rs::maintenance_tick()` calls:
1. `compute_report(None, None, false)` — Phase 8 runs here, returns `StatusReport` with
   `effectiveness: Option<EffectivenessReport>` populated
2. `run_maintenance(...)` — writes `ConfidenceState`

`EffectivenessState` should be written in step 1's aftermath (after `compute_report()` returns,
before or inside `run_maintenance()`), by extracting classifications from
`report.effectiveness`. No additional SQL queries needed — the data is already computed.

### Finding 5: EffectivenessState does not exist in the codebase yet

Confirmed by grepping all of `crates/unimatrix-server/src/`. Zero matches for
`EffectivenessState`, `effectiveness_state`, or `EffectivenessStateHandle`. crt-018b must
create this component from scratch.

### Finding 6: unimatrix-engine crate has effectiveness module

`crates/unimatrix-engine/src/effectiveness/mod.rs` contains all the pure computation
functions and types from crt-018. `EffectivenessState` with `EffectivenessStateHandle` will
live in `crates/unimatrix-server/src/services/` as a new file (mirroring `confidence.rs`).

## What Changed from v1 Scope

| Section | v1 (Wrong) | v2 (Corrected) |
|---------|-----------|----------------|
| AC-01 writer trigger | "Phase 8 after every effectiveness report computation" (implied context_status writes) | "background tick loop after compute_report() call" — context_status calls do NOT write |
| Change 1 rationale | "StatusService already pays that cost on every context_status call" — accurate but misleading | Background tick is the sole writer; context_status is read-only |
| Auto-quarantine "N cycles" semantics | Ambiguous — could mean N context_status calls | Explicitly N consecutive background tick passes (minimum 45 minutes apart) |
| AC-09 counter increment | "incremented by Phase 8 run" | "incremented by background tick write — NOT by context_status calls" |
| AC-17 integration test | "background tick with known data" | Same, but now explicit that test must verify no EffectivenessState mutation from context_status call |
| Constraint 5 | "3 cycles = 3 status calls" | "3 cycles = 3 background tick passes = minimum 45 minutes" |

## Authoritative Trigger Mechanism

`EffectivenessState` is written exclusively by the background maintenance tick
(`background.rs::maintenance_tick()`), every 15 minutes. The write reads the
`EffectivenessReport` already present in the `StatusReport` returned by `compute_report()`,
extracts the per-entry classification map, and writes it to `Arc<RwLock<EffectivenessState>>`
under a write lock. This is the same pattern as `ConfidenceState` — background tick is the
writer, search/briefing paths are readers.

## Knowledge Stewardship

- Queried: /uni-query-patterns for "effectiveness classification trigger" -- Unimatrix MCP
  tools were not available in this agent context; queried codebase directly instead.
- Stored: nothing novel to store -- the finding (background tick is the authoritative writer
  for in-memory state caches; context_status is read-only) is already captured in the
  ConfidenceState pattern. The corrected SCOPE.md is the artifact; no generalizable new
  pattern was discovered beyond confirming existing conventions apply here too.
