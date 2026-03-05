# Implementation Brief: col-013 Extraction Rule Engine

## Summary

col-013 adds rule-based knowledge extraction from observation data, automatic background maintenance, and targeted CRT refactors. Five extraction rules identify knowledge gaps, implicit conventions, dead knowledge, recurring friction, and file dependencies from cross-feature observation patterns. A quality gate pipeline ensures extracted entries meet trust thresholds before storage. A background tick replaces manual maintenance, making `context_status` read-only.

## Implementation Waves

### Wave 1: Type Migration + CRT Refactors (Foundation)

**Goal**: Move shared observation types to unimatrix-core and complete CRT refactors that other waves depend on.

**Changes**:
1. Move `ObservationRecord`, `HookType`, `ParsedSession`, `ObservationStats` from `unimatrix-observe::types` to `unimatrix-core`
2. Add re-exports in `unimatrix-observe` for backward compatibility
3. Add `serde_json` dependency to `unimatrix-core/Cargo.toml`
4. Update imports in ~14 files across unimatrix-observe and unimatrix-server
5. In `unimatrix-engine/src/confidence.rs`: add `"auto" => 0.35` to `trust_score()` match
6. In `unimatrix-server/src/infra/contradiction.rs`: extract `check_entry_contradiction()` from `scan_contradictions()`
7. Add `unimatrix-store` dependency to `unimatrix-observe/Cargo.toml`

**Tests**: `cargo test --workspace` passes. New unit test for trust_score("auto"). New unit test for check_entry_contradiction().

**Estimated**: ~120 lines changed (mostly import rewrites), ~20 lines new test code.

**Gate**: All existing tests pass. New tests pass. `cargo check --workspace` clean.

### Wave 2: Extraction Rules + Quality Gate

**Goal**: Implement the ExtractionRule trait, 5 rules, and quality gate pipeline in unimatrix-observe.

**Changes**:
1. Create `crates/unimatrix-observe/src/extraction/mod.rs` -- `ExtractionRule` trait, `ProposedEntry`, `QualityGateResult`, `ExtractionContext`, `default_extraction_rules()`, `run_extraction_pipeline()`, `quality_gate()`
2. Create rule files: `knowledge_gap.rs`, `implicit_convention.rs`, `dead_knowledge.rs`, `recurring_friction.rs`, `file_dependency.rs`
3. Quality gate pipeline implementation (6 checks in order per ADR-005)
4. Re-export extraction module from `unimatrix-observe::lib.rs`

**Tests**: Unit tests for each rule with synthetic observation data. Unit tests for each quality gate check. Test for `default_extraction_rules()` returning 5 rules.

**Estimated**: ~400 lines new code, ~200 lines test code.

**Gate**: All extraction rule tests pass. Quality gate tests pass for each rejection path.

### Wave 3: Background Tick + Maintenance Relocation

**Goal**: Implement the background tick loop, relocate maintenance operations, make context_status read-only.

**Changes**:
1. Create `crates/unimatrix-server/src/background.rs` -- `background_tick_loop()`, `maintenance_tick()`, `extraction_tick()`, tick metadata tracking
2. Refactor `StatusService::run_maintenance()` -- extract body into `maintenance_tick()` function, callable from background loop
3. Add `coherence_by_source: HashMap<String, f64>` computation to status service
4. Update `StatusReport` with new fields: `last_maintenance_run`, `next_maintenance_scheduled`, `extraction_stats`, `coherence_by_source`
5. In `server.rs`: launch background tick task during `UnimatrixServer::new()` or `serve()` initialization
6. In `mcp/tools.rs`: silently ignore `maintain` parameter (keep in struct, don't act on it)
7. Wire extraction pipeline (from Wave 2) into `extraction_tick()`

**Tests**: Integration test for background tick firing. Integration test for maintenance via tick. Integration test for extraction pipeline end-to-end. Unit test for context_status ignoring maintain. Unit test for coherence_by_source.

**Estimated**: ~200 lines new code (background.rs), ~100 lines refactored (status.rs, tools.rs), ~100 lines test code.

**Gate**: Background tick fires and runs maintenance + extraction. context_status reports new fields. All existing tests pass.

## Key Files (Expected)

| File | Action | Wave |
|------|--------|------|
| `crates/unimatrix-core/src/lib.rs` | Add observation types | W1 |
| `crates/unimatrix-core/Cargo.toml` | Add serde_json dep | W1 |
| `crates/unimatrix-observe/src/types.rs` | Re-export from core | W1 |
| `crates/unimatrix-observe/src/lib.rs` | Add extraction module | W1, W2 |
| `crates/unimatrix-observe/Cargo.toml` | Add unimatrix-store dep | W1 |
| `crates/unimatrix-engine/src/confidence.rs` | trust_score "auto" | W1 |
| `crates/unimatrix-server/src/infra/contradiction.rs` | Extract function | W1 |
| `crates/unimatrix-observe/src/extraction/mod.rs` | Trait + pipeline | W2 |
| `crates/unimatrix-observe/src/extraction/knowledge_gap.rs` | Rule 1 | W2 |
| `crates/unimatrix-observe/src/extraction/implicit_convention.rs` | Rule 2 | W2 |
| `crates/unimatrix-observe/src/extraction/dead_knowledge.rs` | Rule 3 | W2 |
| `crates/unimatrix-observe/src/extraction/recurring_friction.rs` | Rule 4 | W2 |
| `crates/unimatrix-observe/src/extraction/file_dependency.rs` | Rule 5 | W2 |
| `crates/unimatrix-server/src/background.rs` | Background tick | W3 |
| `crates/unimatrix-server/src/services/status.rs` | Maintenance refactor | W3 |
| `crates/unimatrix-server/src/server.rs` | Launch tick | W3 |
| `crates/unimatrix-server/src/mcp/tools.rs` | Ignore maintain | W3 |

## Architecture Decisions (from ARCHITECTURE.md)

- ADR-001: Extraction rules in unimatrix-observe with Store dependency
- ADR-002: Observation types to unimatrix-core
- ADR-003: Background tick architecture (15-min interval, single coordinator)
- ADR-004: Extraction watermark pattern (incremental observation processing)
- ADR-005: Quality gate pipeline order (cheapest rejections first)
- ADR-006: Single-entry contradiction check extraction

## Risk Mitigations Built Into Implementation

| Risk | Built-in Mitigation |
|------|-------------------|
| Low-quality entries | Quality gate (6 checks), trust_score 0.35, cross-feature validation |
| Tick starvation | Async coordinator, spawn_blocking for work |
| Write contention | Same locking pattern, sequential batch writes |
| Type migration breakage | Re-exports preserve public API |
| CRT regressions | Isolated changes, dedicated tests |
| Observation table growth | Watermark pattern, 90-day retention |

## Estimated Size

| Wave | New Lines | Refactored Lines | Test Lines |
|------|-----------|-----------------|------------|
| W1 (Foundation) | 20 | 100 | 20 |
| W2 (Extraction) | 400 | 0 | 200 |
| W3 (Background) | 200 | 100 | 100 |
| **Total** | **~620** | **~200** | **~320** |

Total: ~620 new + ~200 refactored = ~820 implementation lines, ~320 test lines. Slightly above the ASS-015 estimate of ~675 (due to the type migration adding ~100 lines of import changes not originally counted).
