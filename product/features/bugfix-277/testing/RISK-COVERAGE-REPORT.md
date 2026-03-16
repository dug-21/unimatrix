# Risk Coverage Report: bugfix-277 / GH #278

## Bug Summary

Contradiction scan ran O(N) ONNX inference on every `context_status` call and on every
15-minute maintenance tick. At >100 entries this caused the tick to exceed its timeout.

Fix: introduced `ContradictionScanCacheHandle` (`Arc<RwLock<Option<ContradictionScanResult>>>`).
The background tick writes the cache every 4 ticks (~60 min at default interval). `StatusService`
reads from the cache without running ONNX. Cold-start reports `contradiction_scan_performed: false`.

Commit: `255da37` `fix(server): cache contradiction scan result in background tick (#278)`

---

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Cold-start cache is `None` (no scan yet) | `test_contradiction_cache_cold_start_is_none` | PASS | Full |
| R-02 | Cache write then read returns correct data | `test_contradiction_cache_write_then_read` | PASS | Full |
| R-03 | Tick interval constant gates correctly (0, 4, 8 fire; 1–3, 5–7 skip) | `test_contradiction_scan_interval_constant` | PASS | Full |
| R-04 | `u32` tick counter wraps at `u32::MAX` without panic | `test_tick_counter_u32_max_wraps_without_panic` | PASS | Full |
| R-05 | `ContradictionScanResult` is cloneable (needed for cache write) | `test_contradiction_scan_result_clone` | PASS | Full |
| R-06 | `context_status` does not run ONNX on interactive call | `test_status_empty_db`, `test_status_with_entries`, `test_contradiction_scan_in_status` (integration) | PASS | Full |
| R-07 | Contradiction detection still works end-to-end after caching | `test_contradiction_detected`, all 12 contradiction suite tests | PASS | Full |
| R-08 | Status lifecycle reflects contradiction results correctly | `test_status_reflects_lifecycle_changes` (lifecycle suite) | PASS | Full |
| R-09 | No regression in contradiction false-positive resistance | `test_false_positive_compatible_entries`, `test_false_positive_different_aspect` | PASS | Full |
| R-10 | Contradiction cache handles quarantined entries correctly | `test_quarantine_effect_on_scan` | PASS | Full |

---

## Test Results

### Unit Tests (contradiction_cache module — new tests)

Tests in `crates/unimatrix-server/src/services/contradiction_cache.rs`:

| Test | Result |
|------|--------|
| `test_contradiction_cache_cold_start_is_none` | PASS |
| `test_contradiction_cache_write_then_read` | PASS |
| `test_contradiction_scan_interval_constant` | PASS |
| `test_tick_counter_u32_max_wraps_without_panic` | PASS |
| `test_contradiction_scan_result_clone` | PASS |

- Total (module): 5
- Passed: 5
- Failed: 0

### Unit Tests (full workspace)

- Total: 2538
- Passed: 2538
- Failed: 0
- Ignored: 18

### Clippy

`cargo clippy -p unimatrix-server -- -D warnings`: **0 errors in unimatrix-server**.

Pre-existing clippy errors in other crates (`unimatrix-engine`, `unimatrix-observe`,
`patches/anndists`) are unrelated to this bug fix. Not fixed here per triage protocol.

### Integration Tests

#### Smoke Suite (`-m smoke`) — Mandatory Gate

- Total selected: 20
- Passed: 19
- XFailed (pre-existing): 1 (`test_store_1000_entries` — GH#111)
- Failed: 0

Gate: **PASS**

#### Contradiction Suite (`suites/test_contradiction.py`)

- Total: 12
- Passed: 12
- Failed: 0

Covers: negation detection, incompatible directives, false positive resistance,
contradiction scan in status, quarantine effect, scan at 100 entries, empty/single-entry
edge cases, multiple pair detection, embedding consistency.

#### Tools Suite — Status tests (`suites/test_tools.py -k status`)

- Total selected: 8
- Passed: 7
- XFailed (pre-existing): 1 (`test_status_includes_observation_fields`)
- Failed: 0

#### Lifecycle Suite (`suites/test_lifecycle.py`)

- Total: 25
- Passed: 23
- XFailed (pre-existing): 2
  - `test_multi_agent_interaction` — GH#238
  - `test_auto_quarantine_after_consecutive_bad_ticks` — pre-existing tick env issue
- Failed: 0

**Integration Total: 65 selected, 61 passed, 0 failed, 4 xfailed (all pre-existing)**

---

## Gaps

None. All risks from the bug scope are covered:

- Cache data structure correctness: covered by new module unit tests.
- Tick interval gating: covered by `test_contradiction_scan_interval_constant` and `test_tick_counter_u32_max_wraps_without_panic`.
- End-to-end MCP behavior: covered by contradiction suite (12 tests) and status lifecycle test.
- No ONNX on interactive calls: validated by status tools tests completing within timeout
  and `test_context_status_does_not_advance_consecutive_counters` (lifecycle suite).

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01: Contradiction scan must not run ONNX on `context_status` calls | PASS | `StatusService::compute_report()` reads from `ContradictionScanCacheHandle` only; all status integration tests pass |
| AC-02: Background tick gates scan to every 4th tick | PASS | `CONTRADICTION_SCAN_INTERVAL_TICKS=4`; `test_contradiction_scan_interval_constant` verifies gate logic |
| AC-03: Cold-start returns `contradiction_scan_performed: false` | PASS | `test_contradiction_cache_cold_start_is_none`; `test_status_empty_db` passes on fresh server |
| AC-04: `u32` tick counter wraps safely | PASS | `test_tick_counter_u32_max_wraps_without_panic` |
| AC-05: Contradiction detection results remain correct | PASS | All 12 `test_contradiction.py` integration tests pass |
| AC-06: No regression in smoke tests | PASS | 19/20 smoke tests pass; 1 xfail is pre-existing GH#111 |

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for testing procedures (contradiction cache, verification) — found entry #487 (workspace test procedure), entry #750 (pipeline validation tests); no directly relevant contradiction-cache testing procedures found.
- Stored: nothing novel to store — the contradiction cache test patterns follow the established `Arc<RwLock<_>>` cache handle pattern already documented in prior entries. The integration suite selection (contradiction + status tools + lifecycle) is standard per the suite selection table.
