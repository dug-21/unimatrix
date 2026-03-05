# Acceptance Map: col-013 Extraction Rule Engine

## Wave 1: Type Migration + CRT Refactors

| AC | Criterion | FR | Test Method | Gate |
|----|-----------|-----|-------------|------|
| AC-17 | trust_score("auto") returns 0.35 | FR-11.1 | Unit: assert trust_score("auto") == 0.35 | 3a |
| AC-17b | Existing trust_score values unchanged | FR-11.1 | Unit: assert human/system/agent/other values unchanged | 3a |
| AC-18 | check_entry_contradiction() extracted and usable | FR-11.2 | Unit: opposing content returns Some, compatible returns None | 3a |
| AC-20a | Type migration compiles | FR-12.1-12.4 | cargo check --workspace | 3a |
| AC-20b | All existing tests pass after Wave 1 | FR-12, FR-11 | cargo test --workspace | 3a |

## Wave 2: Extraction Rules + Quality Gate

| AC | Criterion | FR | Test Method | Gate |
|----|-----------|-----|-------------|------|
| AC-01 | ExtractionRule trait defined | FR-01.1 | Compilation + custom rule test | 3a |
| AC-02 | KnowledgeGapRule: gap entries from 2+ feature zero-result searches | FR-02 | Unit: synthetic observations with zero-result context_search across 2 features | 3a |
| AC-02b | KnowledgeGapRule: no output from single feature | FR-02 | Unit: zero-result search in 1 feature only | 3a |
| AC-03 | ImplicitConventionRule: convention entries from 100% patterns in 3+ features | FR-03 | Unit: synthetic file access observations consistent across all features | 3a |
| AC-03b | ImplicitConventionRule: no output from partial consistency | FR-03 | Unit: pattern in 80% of features | 3a |
| AC-04 | DeadKnowledgeRule: deprecation signals for access-cliff entries | FR-04 | Unit: synthetic entry accessed in features 1-3, absent in 4-8 | 3a |
| AC-04b | DeadKnowledgeRule: no output for recently accessed entries | FR-04 | Unit: entry accessed in latest feature | 3a |
| AC-05 | RecurringFrictionRule: entries from hotspots in 3+ features | FR-05 | Unit: same detection rule fires in 3 feature datasets | 3a |
| AC-06 | FileDependencyRule: entries from read-before-edit chains in 3+ features | FR-06 | Unit: Read(A)->Edit(B) pattern in 3 feature datasets | 3a |
| AC-09 | Quality gate: rate limit rejects after 10/hour | FR-07.1 | Unit: 11th entry rejected | 3a |
| AC-10 | Quality gate: cross-feature validation per rule minimum | FR-07.3 | Unit: single-feature entry rejected for each rule | 3a |
| AC-11 | Quality gate: confidence floor rejects < 0.2 | FR-07.4 | Unit: entry with 0.15 confidence rejected | 3a |
| AC-10b | Quality gate: content validation rejects short entries | FR-07.2 | Unit: title < 10 chars rejected | 3a |
| AC-20c | All existing tests pass after Wave 2 | -- | cargo test --workspace | 3a |

## Wave 3: Background Tick + Maintenance Relocation

| AC | Criterion | FR | Test Method | Gate |
|----|-----------|-----|-------------|------|
| AC-13 | Background tick starts at server startup | FR-09.1 | Integration: tick_metadata.last_run is set after startup + interval | 3b |
| AC-14 | Maintenance tick performs confidence refresh, co-access cleanup, compaction, session GC | FR-09.2 | Integration: run maintenance_tick with stale entries, verify refreshed | 3b |
| AC-15 | Extraction pipeline triggers on tick | FR-09.3 | Integration: insert observations, wait for tick, verify auto-entry created | 3b |
| AC-16 | context_status read-only: reports maintenance, ignores maintain | FR-10.1-10.2 | Unit: maintain=true produces no side effects; last_maintenance_run present | 3b |
| AC-19 | StatusReport includes coherence_by_source | FR-11.3 | Unit: entries with different trust_sources produce per-source lambda | 3b |
| AC-07 | Quality gate: near-duplicate rejection (cosine >= 0.92) | FR-07.5 | Integration: store entry, propose near-duplicate, verify rejection | 3b |
| AC-08 | Quality gate: contradiction rejection | FR-07.6 | Integration: store contradicting entry, propose conflicting, verify rejection | 3b |
| AC-12 | Auto-extracted entries have trust_source="auto" and provenance tags | FR-08 | Integration: run extraction pipeline, verify stored entry metadata | 3b |
| AC-20d | All existing tests pass after Wave 3 | -- | cargo test --workspace | 3b |

## Delivery Gates

### Gate 3a: Wave 1+2 Complete
- [ ] All AC items tagged "3a" pass
- [ ] `cargo test --workspace` clean
- [ ] No new clippy warnings
- [ ] Type migration complete, re-exports verified
- [ ] 5 extraction rules implemented with unit tests
- [ ] Quality gate unit tests for each rejection path

### Gate 3b: Wave 3 Complete (Feature Complete)
- [ ] All AC items tagged "3b" pass
- [ ] Background tick fires and runs maintenance + extraction
- [ ] context_status reports new fields correctly
- [ ] End-to-end: observations -> extraction -> quality gate -> stored entry
- [ ] `cargo test --workspace` clean (all existing + new tests)
- [ ] No new clippy warnings

### Gate 3c: Final Verification
- [ ] All 22 AC items pass
- [ ] Test count: existing tests + new tests all pass
- [ ] Risk coverage report: all R-01 through R-07 mitigated
- [ ] No TODO, unimplemented!(), or placeholder code

## Cross-Reference

| AC | SCOPE.md AC | SPEC FR | Risk | ADR |
|----|-------------|---------|------|-----|
| AC-01 | AC-01 | FR-01 | -- | ADR-001 |
| AC-02 | AC-02 | FR-02 | R-01 | -- |
| AC-03 | AC-03 | FR-03 | R-01 | -- |
| AC-04 | AC-04 | FR-04 | R-01 | -- |
| AC-05 | AC-05 | FR-05 | R-01 | -- |
| AC-06 | AC-06 | FR-06 | R-01 | -- |
| AC-07 | AC-07 | FR-07.5 | R-01 | ADR-005 |
| AC-08 | AC-08 | FR-07.6 | R-01 | ADR-006 |
| AC-09 | AC-09 | FR-07.1 | R-07 | -- |
| AC-10 | AC-10 | FR-07.3 | R-01 | -- |
| AC-11 | AC-11 | FR-07.4 | R-01 | -- |
| AC-12 | AC-12 | FR-08 | R-01, R-05 | -- |
| AC-13 | AC-13 | FR-09.1 | R-02 | ADR-003 |
| AC-14 | AC-14 | FR-09.2 | R-02 | ADR-003 |
| AC-15 | AC-15 | FR-09.3 | R-04 | ADR-004 |
| AC-16 | AC-16 | FR-10.1-10.2 | R-02 | ADR-003 |
| AC-17 | AC-17 | FR-11.1 | R-03 | -- |
| AC-18 | AC-18 | FR-11.2 | R-03 | ADR-006 |
| AC-19 | AC-19 | FR-11.3 | R-03 | -- |
| AC-20 | AC-20 | FR-12 | R-06 | ADR-002 |
