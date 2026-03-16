# Fix Agent Report: 279-agent-1-fix

## Bug
GH #279 — extraction tick holds `Mutex<Connection>` for up to 10,000 observation
rows in a single `spawn_blocking`, blocking MCP handlers for 2–5 seconds at high
hook event volume.

---

## Files Modified

- `crates/unimatrix-server/src/background.rs`

---

## Changes Made

### 1. Constant added (line 56–62)

```rust
const EXTRACTION_BATCH_SIZE: i64 = 1000;
```

Placed alongside `DEFAULT_TICK_INTERVAL_SECS` and `AUTO_QUARANTINE_CYCLES_MAX` per
the approved fix approach.

### 2. `fetch_observation_batch()` helper extracted (lines 865–943)

The observation query logic was factored out of `extraction_tick()` into a
synchronous helper function `fetch_observation_batch(store, watermark)` that:
- Uses `LIMIT ?2` with `EXTRACTION_BATCH_SIZE` as the bind parameter (replacing
  the hardcoded `LIMIT 10000`)
- Returns `(Vec<ObservationRecord>, new_watermark)`
- Is directly unit-testable without requiring the ONNX model or async runtime

### 3. `extraction_tick()` Step 1 simplified (lines 958–964)

The first `spawn_blocking` closure now delegates to `fetch_observation_batch`:
```rust
let (observations, new_watermark) =
    tokio::task::spawn_blocking(move || fetch_observation_batch(&store_clone, watermark))
        .await
        .map_err(|e| ServiceError::Core(CoreError::JoinError(e.to_string())))??;
```
No other extraction logic was changed. Watermark update, extraction rules,
quality gate, and entry store paths are all unmodified.

---

## New Tests

All 6 are in `background::tests`, extending the existing test module.

| Test function | What it verifies |
|--------------|-----------------|
| `test_fetch_observation_batch_first_batch_capped_at_batch_size` | AC-01: 1,200-row backlog → first call returns exactly 1,000 rows; watermark advances to 1,000 |
| `test_fetch_observation_batch_second_call_advances_watermark` | AC-02: second call on 2,200-row backlog advances watermark by another 1,000 |
| `test_fetch_observation_batch_remainder_processed_on_third_tick` | AC-03: 1,200 rows → second call returns the 200-row remainder; watermark reaches 1,200 |
| `test_fetch_observation_batch_empty_store_returns_empty` | AC-04: empty store returns empty records; watermark stays 0 |
| `test_fetch_observation_batch_no_reprocessing_past_watermark` | AC-05: after consuming 50 rows, second call returns nothing; watermark unchanged |
| `test_extraction_batch_size_constant_value` | AC-06: constant is exactly 1000 — guards against silent regression |

---

## Test Results

```
test background::tests::test_extraction_batch_size_constant_value ... ok
test background::tests::test_fetch_observation_batch_empty_store_returns_empty ... ok
test background::tests::test_fetch_observation_batch_no_reprocessing_past_watermark ... ok
test background::tests::test_fetch_observation_batch_first_batch_capped_at_batch_size ... ok
test background::tests::test_fetch_observation_batch_remainder_processed_on_third_tick ... ok
test background::tests::test_fetch_observation_batch_second_call_advances_watermark ... ok

test result: ok. 1323 passed; 0 failed; 0 ignored
```

All pre-existing tests continue to pass. No regressions.

Clippy: no warnings in `background.rs`. Pre-existing clippy errors in
`unimatrix-engine/src/auth.rs` are unrelated to this fix.

---

## Issues / Blockers

None.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server extraction tick batch size mutex contention` — Unimatrix MCP server unavailable in this context; proceeded without results (non-blocking).
- Stored: nothing novel to store — the extraction batch size pattern (named constant + bind parameter for LIMIT) is already captured in entry #1736 "Extraction tick batch size controls mutex hold duration" stored by the investigator agent (279-investigator). The refactoring of the fetch logic into a testable helper is a minor structural choice, not a generalizable gotcha.
