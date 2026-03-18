# Test Plan: unimatrix-observe Migration (dead_knowledge.rs)

**Component**: `crates/unimatrix-observe/src/extraction/dead_knowledge.rs` + `extraction/mod.rs` + 4 other extraction rules
**Risks**: R-08 (ExtractionRule block_on bridge panic)
**ACs**: No direct AC number; covered by AC-05 (no spawn_blocking), AC-13 (no rusqlite), AC-14 (test count)
**Spec reference**: FR-14, ADR-006

---

## Resolution Context (ADR-006)

ADR-006 chose Option A: convert `ExtractionRule::evaluate()` to `async fn` across all 5 extraction rules. The `spawn_blocking` wrapper around `run_extraction_rules` in `background.rs` is removed. The call site becomes `.await`.

Key facts for test planning:
- 5 extraction rules are affected (`dead_knowledge`, `knowledge_gap`, `implicit_convention`, `recurring_friction`, `file_dependency`)
- 21 detection rules (`DetectionRule::detect()`) are NOT affected — different trait, no store access
- Dynamic dispatch for `Vec<Box<dyn ExtractionRule>>` uses either: explicit enum over 5 rule types OR `async_trait` macro — delivery-level choice
- `dead_knowledge.rs` has real logic rewritten (async sqlx query on `read_pool`)
- The 4 others gain `async` keyword on `evaluate()` signature only

---

## Unit Tests (`#[tokio::test]` in `unimatrix-observe/tests/` or inline)

### OB-U-01: `test_dead_knowledge_rule_evaluate_returns_from_async_context` — (R-08)
- **Arrange**: Create a `SqlxStore` with `PoolConfig::test_default()`; insert several active entries with `access_count > 0`; create a `DeadKnowledgeRule`
- **Act**: Call `rule.evaluate(&observations, &store).await` from within a `#[tokio::test]` (i.e., inside a tokio async runtime — the panic scenario)
- **Assert**: Returns without panic; returns a `Vec<ProposedEntry>` (may be empty or non-empty depending on entries)
- **Teardown**: `store.close().await`
- **Risk**: R-08 (the critical test — must NOT panic with "cannot start a runtime from within a runtime")

### OB-U-02: `test_dead_knowledge_rule_queries_active_entries`
- **Arrange**: Open store; insert 3 active entries (access_count > 0); insert 1 deprecated entry
- **Act**: `rule.evaluate(&[], &store).await` (empty observations — isolates the store query path)
- **Assert**: Returns proposed entries that correspond only to the active entries with access_count > 0
- **Assert**: Does not include the deprecated entry
- **Teardown**: `store.close().await`
- **Risk**: R-08 (ADR-006 correctness)

### OB-U-03: `test_knowledge_gap_rule_evaluate_async_no_panic` — (R-08)
- **Arrange**: Open store; create `KnowledgeGapRule`
- **Act**: `rule.evaluate(&sample_observations, &store).await`
- **Assert**: Returns without panic; no rusqlite call site
- **Teardown**: `store.close().await`
- **Risk**: R-08 (async signature propagated to 4 non-store rules)

### OB-U-04: `test_run_extraction_rules_all_5_rules_execute` — (R-08)
- **Arrange**: Open store; create all 5 extraction rules via `default_extraction_rules()` (or equivalent factory); insert sample active entries
- **Act**: `run_extraction_rules(&observations, &store, &rules).await`
- **Assert**: Returns without panic; all 5 rules' evaluate methods were invoked (verify via output or tracing)
- **Teardown**: `store.close().await`
- **Risk**: R-08

---

## Integration Tests

### OB-I-01: `test_observe_crate_compiles_without_rusqlite` — (AC-13)
- **Verification**: `cargo build -p unimatrix-observe` succeeds
- **Assert**: Crate builds with zero rusqlite references; no `pub use rusqlite` accessible
- **Risk**: SR-07, AC-13

### OB-I-02: `test_background_task_extraction_rules_no_spawn_blocking` — (AC-05, R-08)
- **Arrange**: Build unimatrix-server with the migrated background.rs
- **Assert (static)**: `grep -n "spawn_blocking.*run_extraction_rules\|spawn_blocking.*dead_knowledge" crates/unimatrix-server/src/background.rs` returns zero matches
- **Assert (runtime)**: Full server integration test suite runs without "cannot start a runtime from within a runtime" panic in test output
- **Risk**: R-08

---

## Static Verification

### OB-S-01: No rusqlite in observe crate — (AC-13)
- **Check**: `grep -r "rusqlite" crates/unimatrix-observe/Cargo.toml crates/unimatrix-observe/src/` returns zero matches
- **Risk**: SR-07

### OB-S-02: All 5 ExtractionRule impls have `async fn evaluate`
- **Check**: `grep -n "fn evaluate" crates/unimatrix-observe/src/extraction/*.rs` — all 5 files must show `async fn evaluate`
- **Risk**: R-08

### OB-S-03: `run_extraction_rules` is `async fn`
- **Check**: `grep -n "fn run_extraction_rules" crates/unimatrix-observe/src/extraction/mod.rs` shows `async fn`
- **Risk**: R-08

### OB-S-04: `background.rs` call site uses `.await`, not `spawn_blocking`
- **Check**: `grep -n "run_extraction_rules" crates/unimatrix-server/src/background.rs` shows `.await` at the call site and no `spawn_blocking` wrapper
- **Risk**: R-08

### OB-S-05: No `lock_conn()` in observe crate
- **Check**: `grep -rn "lock_conn" crates/unimatrix-observe/src/` returns zero matches
- **Risk**: AC-03, R-08

---

## Test Count Notes

The existing synchronous tests for `DeadKnowledgeRule` (if any) must be rewritten as `#[tokio::test]` using `SqlxStore` (ADR-006 consequence). They count as preserved (same test, async conversion — TC-01). Verify that no observe-crate tests were deleted.

---

## Notes

- OB-U-01 is the most critical test for this component. A panic in this test indicates the block_on problem is present and ADR-006 Option A was not correctly implemented.
- The choice of dynamic dispatch mechanism (enum vs async_trait macro) for `Vec<Box<dyn ExtractionRule>>` is a delivery decision. OB-U-04 exercises whichever mechanism is chosen.
- If the delivery agent chose the explicit enum approach (recommended in ADR-006), OB-U-04 implicitly tests that all 5 enum variants are present by calling `default_extraction_rules()`.
- OB-I-02 combines a grep check with a runtime assertion. Both must pass.
