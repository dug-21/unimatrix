# Acceptance Map: col-012 Data Path Unification

## AC-to-Risk-to-Wave Traceability

| AC-ID | Criterion | Risk IDs | Wave | Verification |
|-------|-----------|----------|------|-------------|
| AC-01 | Schema migration v6->v7 creates observations table | R-02 | 1 | Integration test: open v6 DB, verify table + indexes |
| AC-02 | RecordEvent persists all hook events | R-01, R-04 | 1 | Integration test: send event, query table |
| AC-03 | RecordEvents batch persists in single transaction | R-07 | 1 | Integration test: batch insert, verify atomicity |
| AC-04 | Retrospective reads from SQLite | R-03, R-10 | 3 | Integration test: populate observations, run retrospective |
| AC-05 | Session discovery uses SESSIONS table | R-05 | 2 | Unit test: mock ObservationSource |
| AC-06 | Feature attribution uses SESSIONS.feature_cycle | R-05 | 2 | Integration test: sessions with feature_cycle |
| AC-07 | All 21 detection rules produce findings from SQL data | R-03, R-10 | 3 | Integration test: seed trigger data, verify findings |
| AC-08 | JSONL write path removed from hooks | R-08 | 4 | Manual review + grep verification |
| AC-09 | JSONL parsing removed from unimatrix-observe | -- | 4 | Compilation succeeds without parser.rs/files.rs |
| AC-10 | context_retrospective produces valid report | R-03 | 3 | Integration test: full pipeline |
| AC-11 | context_status stats from observations table | R-09 | 3 | Integration test: verify stats fields |
| AC-12 | All tests pass; new tests cover SQL path | -- | All | CI: cargo test --workspace |
| AC-13 | Net code reduction | -- | 4 | diff stat: net negative lines |

## Risk Coverage

| Risk ID | Risk | Covered By ACs | Test Type |
|---------|------|---------------|-----------|
| R-01 | Payload field extraction | AC-02 | Integration |
| R-02 | Schema migration failure | AC-01 | Integration |
| R-03 | SQL-to-Record mapping fidelity | AC-04, AC-07, AC-10 | Integration |
| R-04 | spawn_blocking write failure | AC-02 | Integration + error path |
| R-05 | NULL feature_cycle | AC-05, AC-06 | Unit + Integration |
| R-06 | Timestamp overflow | AC-02 | Boundary test |
| R-07 | Batch partial failure | AC-03 | Integration |
| R-08 | Hook script breakage | AC-08 | Manual review |
| R-09 | Status response schema change | AC-11 | Integration |
| R-10 | Input field type mismatch | AC-04, AC-07 | Integration |

## Wave Dependencies

```
Wave 1: Schema + Event Persistence
    │   AC-01, AC-02, AC-03
    │
    └─► Wave 2: ObservationSource Trait + SQL Implementation
         │   AC-05, AC-06
         │
         └─► Wave 3: Retrospective Pipeline Migration
              │   AC-04, AC-07, AC-10, AC-11
              │
              └─► Wave 4: JSONL Removal
                   AC-08, AC-09, AC-13

AC-12 spans all waves.
```

## Gate Criteria

Before marking col-012 complete:
- [ ] All 13 ACs verified (integration tests pass)
- [ ] All 10 risks covered by at least one test
- [ ] `cargo test --workspace` passes
- [ ] Net code reduction confirmed via diff stat
- [ ] No TODO/unimplemented!() in new code
