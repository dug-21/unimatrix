# crt-018 Pseudocode Overview

## Components

| Component | Crate | File(s) | Purpose |
|-----------|-------|---------|---------|
| effectiveness-engine | unimatrix-engine | `src/effectiveness.rs`, `src/lib.rs` | Pure classification, calibration, aggregation |
| effectiveness-store | unimatrix-store | `src/read.rs` | SQL queries returning pre-aggregated data |
| status-integration | unimatrix-server | `src/services/status.rs`, `src/mcp/response/status.rs` | Phase 8 orchestration + 3-format output |

## Data Flow

```
StatusService::compute_report()
  |
  | Phase 8 (spawn_blocking)
  |
  +-> Store::compute_effectiveness_aggregates()    [4 SQL queries, 1 lock_conn()]
  |     returns EffectivenessAggregates { entry_stats, active_topics, calibration_rows, data_window }
  |
  +-> Store::load_entry_classification_meta()      [1 SQL query on entries table]
  |     returns Vec<EntryClassificationMeta>
  |
  +-> Build HashMap<u64, EntryInjectionStats> from aggregates.entry_stats
  |
  +-> For each EntryClassificationMeta:
  |     classify_entry(...) -> EntryEffectiveness
  |
  +-> build_report(classifications, calibration_rows, data_window) -> EffectivenessReport
  |     internally calls: aggregate_by_source(), build_calibration_buckets()
  |     caps lists: top 10 ineffective, top 10 unmatched, all noisy
  |
  +-> StatusReport.effectiveness = Some(report)
  |
  +-> format_status_report() renders effectiveness in summary/markdown/JSON
```

## Shared Types Crossing Boundaries

**Store -> Engine boundary** (defined in unimatrix-store, consumed by engine logic in server):
- `EffectivenessAggregates` -- raw SQL output
- `EntryInjectionStats` -- per-entry injection/outcome counts
- `EntryClassificationMeta` -- entry metadata for classification

**Engine -> Server boundary** (defined in unimatrix-engine, consumed by server):
- `EffectivenessCategory` (enum)
- `EntryEffectiveness` -- per-entry classification result
- `SourceEffectiveness` -- per-trust-source aggregates
- `CalibrationBucket` -- confidence vs actual success
- `DataWindow` -- session coverage metadata
- `EffectivenessReport` -- top-level container

**Engine constants** (defined in unimatrix-engine, used by engine functions):
- `INEFFECTIVE_MIN_INJECTIONS: u32 = 3`
- `OUTCOME_WEIGHT_SUCCESS: f64 = 1.0`
- `OUTCOME_WEIGHT_REWORK: f64 = 0.5`
- `OUTCOME_WEIGHT_ABANDONED: f64 = 0.0`
- `NOISY_TRUST_SOURCES: &[&str] = &["auto"]`

## Sequencing Constraints

1. **effectiveness-store first** -- defines `EffectivenessAggregates`, `EntryInjectionStats`, `EntryClassificationMeta` structs that the engine's caller (server) needs to map into engine function arguments.
2. **effectiveness-engine second** -- defines all engine types and pure functions. Depends on no other crt-018 code.
3. **status-integration last** -- wires store + engine together in Phase 8, extends StatusReport, extends formatting. Depends on both other components.

Note: effectiveness-store and effectiveness-engine have no compile-time dependency on each other. The server is the integration point that calls both.
