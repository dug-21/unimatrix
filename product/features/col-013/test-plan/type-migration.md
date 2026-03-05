# Test Plan: type-migration (Wave 1)

## Unit Tests

### T-TM-01: trust_score("auto") returns 0.35 (AC-17)
- Input: trust_score("auto")
- Expected: 0.35
- Location: crates/unimatrix-engine/src/confidence.rs

### T-TM-02: Existing trust_score values unchanged (AC-17b)
- Input: trust_score("human"), trust_score("system"), trust_score("agent"), trust_score("unknown")
- Expected: 1.0, 0.7, 0.5, 0.3 (unchanged)
- Location: crates/unimatrix-engine/src/confidence.rs

### T-TM-03: check_entry_contradiction detects conflict (AC-18)
- Input: opposing content ("Always use X" vs existing entry "Never use X")
- Setup: Store with one active entry containing "Never use X"
- Expected: Returns Some(ContradictionPair) with conflict_score > 0
- Location: crates/unimatrix-server/src/infra/contradiction.rs
- Note: Requires test-support feature (mock embedding)

### T-TM-04: check_entry_contradiction returns None for compatible (AC-18)
- Input: compatible content that doesn't conflict
- Expected: Returns None
- Location: crates/unimatrix-server/src/infra/contradiction.rs

## Compilation Tests

### T-TM-05: Type migration compiles (AC-20a)
- Command: cargo check --workspace
- Expected: Clean compilation with no errors

### T-TM-06: All existing tests pass (AC-20b)
- Command: cargo test --workspace
- Expected: All existing 1025+ unit and 174+ integration tests pass

## Risk Coverage

| Risk | Tests |
|------|-------|
| R-03 (CRT regressions) | T-TM-01, T-TM-02 |
| R-06 (type migration) | T-TM-05, T-TM-06 |
