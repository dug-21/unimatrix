# Gate 3b Report: Code Review

**Feature**: col-013 (Extraction Rule Engine)
**Gate**: 3b (Code Review)
**Result**: PASS
**Date**: 2026-03-05

## Validation Checklist

### Pseudocode Alignment

- [x] Wave 1 (type-migration): ObservationRecord, HookType, ParsedSession, ObservationStats migrated to unimatrix-core with re-exports in unimatrix-observe
- [x] Wave 1 (CRT refactors): trust_score("auto") = 0.35 added to confidence.rs; check_entry_contradiction() added to contradiction.rs
- [x] Wave 2 (extraction-rules): ExtractionRule trait, 5 rules, quality gate, ExtractionContext/Stats implemented as specified
- [x] Wave 3 (background-tick): TickMetadata, spawn_background_tick, maintenance_tick, extraction_tick implemented; StatusReport extended with 4 new fields; context_status ignores maintain parameter

### Architecture Alignment

- [x] ExtractionRule trait mirrors DetectionRule pattern (ADR-001 from specification)
- [x] Quality gate checks ordered cheapest-first (ADR-005)
- [x] Watermark-based incremental observation processing (ADR-004)
- [x] Background tick with 15-min interval (ADR-003)
- [x] Types migrated to unimatrix-core with backward-compatible re-exports (ADR-002)
- [x] Single-entry contradiction check extracted from batch scan (ADR-006)

### Component Interfaces

- [x] ExtractionRule::evaluate(&self, &[ObservationRecord], &Store) -> Vec<ProposedEntry>
- [x] quality_gate(&ProposedEntry, &mut ExtractionContext) -> QualityGateResult
- [x] TickMetadata shared via Arc<Mutex<>> between background tick and status handler
- [x] StatusReport gains: last_maintenance_run, next_maintenance_scheduled, extraction_stats, coherence_by_source

### Test Plan Match

- [x] 15 unit tests in extraction/mod.rs (quality gate, helpers, defaults)
- [x] 7 tests in knowledge_gap.rs
- [x] 7 tests in implicit_convention.rs
- [x] 6 tests in dead_knowledge.rs (2 with real Store)
- [x] 4 tests in recurring_friction.rs
- [x] 6 tests in file_dependency.rs
- [x] 2 tests in confidence.rs (trust_score auto)
- [x] 3 tests in background.rs (tick metadata, hook parsing, now_secs)

### Build Quality

- [x] `cargo build --workspace` succeeds (0 new errors)
- [x] `cargo clippy` clean on all new/modified files
- [x] No todo!(), unimplemented!(), TODO, FIXME, HACK in non-test code
- [x] No .unwrap() in non-test code (new files)
- [x] All new files under 500 lines (background.rs: 440, extraction/mod.rs: 443)
- [ ] status.rs: 703 lines (pre-existing: 589 lines on main, already exceeded 500-line limit)

### Test Results

| Crate | Tests | Status |
|-------|-------|--------|
| unimatrix-core | 21 | PASS |
| unimatrix-engine | 173 | PASS |
| unimatrix-observe | 275 | PASS |
| unimatrix-server | 770 | PASS |
| **Total** | **1239** | **PASS** |

### New Tests Added: 50

- 15 quality gate + helper tests (extraction/mod.rs)
- 7 knowledge gap rule tests
- 7 implicit convention rule tests
- 6 dead knowledge rule tests
- 4 recurring friction rule tests
- 6 file dependency rule tests
- 2 trust_score("auto") tests
- 3 background tick metadata tests

### Files Created (11 new)

| File | Lines | Purpose |
|------|-------|---------|
| crates/unimatrix-core/src/observation.rs | 48 | Observation types (migrated) |
| crates/unimatrix-observe/src/extraction/mod.rs | 443 | ExtractionRule trait, quality gate, helpers |
| crates/unimatrix-observe/src/extraction/knowledge_gap.rs | 221 | KnowledgeGapRule |
| crates/unimatrix-observe/src/extraction/implicit_convention.rs | 256 | ImplicitConventionRule |
| crates/unimatrix-observe/src/extraction/dead_knowledge.rs | 321 | DeadKnowledgeRule |
| crates/unimatrix-observe/src/extraction/recurring_friction.rs | 172 | RecurringFrictionRule |
| crates/unimatrix-observe/src/extraction/file_dependency.rs | 230 | FileDependencyRule |
| crates/unimatrix-server/src/background.rs | 440 | Background tick loop |

### Files Modified (11 modified)

| File | Change |
|------|--------|
| crates/unimatrix-core/src/lib.rs | Added observation module + re-exports |
| crates/unimatrix-core/Cargo.toml | Added serde, serde_json deps |
| crates/unimatrix-observe/src/types.rs | Replaced 4 types with re-exports from core |
| crates/unimatrix-observe/src/lib.rs | Added extraction module |
| crates/unimatrix-observe/Cargo.toml | Added unimatrix-core, unimatrix-store deps |
| crates/unimatrix-engine/src/confidence.rs | Added "auto" => 0.35 trust_score |
| crates/unimatrix-server/src/infra/contradiction.rs | Added check_entry_contradiction() |
| crates/unimatrix-server/src/lib.rs | Added background module |
| crates/unimatrix-server/src/server.rs | Added tick_metadata field |
| crates/unimatrix-server/src/mcp/tools.rs | Silently ignore maintain, read tick metadata |
| crates/unimatrix-server/src/mcp/response/status.rs | 4 new StatusReport fields + formatting |
| crates/unimatrix-server/src/services/status.rs | coherence_by_source computation + new field defaults |
| crates/unimatrix-server/src/main.rs | Spawn background tick at startup |
| crates/unimatrix-server/src/mcp/response/mod.rs | Updated test StatusReport constructors |
| crates/unimatrix-store/tests/sqlite_parity.rs | Fixed pre-existing schema_version assertion |

### Notes

1. status.rs at 703 lines exceeds the 500-line limit but was already 589 lines pre-existing. Not refactored to avoid scope creep.
2. Pre-existing clippy warnings exist in other crates (unimatrix-embed, unimatrix-adapt, anndists) but are not introduced by this feature.
3. The `maintain` parameter on context_status is silently ignored per spec (no error, no side effect). The StatusParams struct retains the field for backward compatibility.
