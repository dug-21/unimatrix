# nxs-009: Acceptance Map

**Feature**: nxs-009 — Observation Metrics Normalization

---

## Acceptance Criteria → Test Mapping

| AC | Description | Test Type | Test Location | Risk |
|----|-------------|-----------|---------------|------|
| AC-01 | Schema has 23 columns, no BLOB; phase table exists | Unit | `sqlite_parity.rs` | — |
| AC-02 | Store roundtrip (21 universal + 3 phases + computed_at) | Unit | `sqlite_parity.rs` | R-01 |
| AC-03 | Store replace (phases ["3a","3b"] → ["3a","3c"]) | Unit | `sqlite_parity.rs` | R-01 |
| AC-04 | List all with correct phase attachment | Unit | `sqlite_parity.rs` | R-04 |
| AC-05 | Migration v8→v9 happy path | Integration | `sqlite_parity.rs` | R-02, R-05 |
| AC-06 | Migration corrupted blob → default MetricVector | Integration | `sqlite_parity.rs` | R-02 |
| AC-07 | Delete cascade removes phase rows | Unit | `sqlite_parity.rs` | — |
| AC-08 | Server retrospective output unchanged | Integration | `server/tests/` | — |
| AC-09 | Server status count unchanged | Integration | `server/tests/` | — |
| AC-10 | Bincode helpers removed from observe API | Build | `cargo build` | R-06 |
| AC-11 | Re-export compatibility | Build | `cargo build` | R-06 |
| AC-12 | Empty phases roundtrip | Unit | `sqlite_parity.rs` | — |
| AC-13 | SQL analytics query works | Unit | `sqlite_parity.rs` | — |

## Additional Risk-Driven Tests

| Risk | Test Description | Test Location |
|------|-----------------|---------------|
| R-03 | Column-field alignment structural assertion | `sqlite_parity.rs` |
| R-05 | Migration deserializer parity with production serializer | `sqlite_parity.rs` |

## Gate Criteria

- [ ] All 13 acceptance criteria pass
- [ ] All 6 risk-driven tests pass
- [ ] `cargo build --workspace` succeeds
- [ ] `cargo test --workspace` succeeds
- [ ] No TODO/unimplemented/todo! markers in new code
- [ ] Schema version is exactly 9
- [ ] Migration creates backup file
