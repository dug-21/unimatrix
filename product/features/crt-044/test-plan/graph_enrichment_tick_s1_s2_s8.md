# Test Plan: graph_enrichment_tick_s1_s2_s8

## Component

`crates/unimatrix-server/src/services/graph_enrichment_tick.rs` — second `write_graph_edge` call
added to `run_s1_tick`, `run_s2_tick`, and `run_s8_tick`.

## Test File

**Extend** existing file:
`crates/unimatrix-server/src/services/graph_enrichment_tick_tests.rs`

Add new tests after the existing S1, S2, and S8 test blocks. Do NOT create a new file —
the existing file has all required helpers and is cumulative per project convention.

## Available Helpers (already exist in test file)

```rust
seed_entry(store, id, status)                       // Insert entry row
seed_entry_with_content(store, id, title, content)  // Insert entry with text
seed_tag(store, entry_id, tag)                      // Insert entry_tags row
seed_audit_row(store, event_id, op, outcome, ids)   // Insert audit_log row
count_edges_by_source(store, source)                // COUNT(*) WHERE source = ?
fetch_edge(store, source_id, target_id, relation)   // Optional<(weight, source, created_by, bootstrap_only)>
read_s8_watermark(store)                            // Read s8_audit_log_watermark counter
make_config()                                       // Default InferenceConfig
make_config_s8(interval, cap)                       // S8-specific config
```

No new helpers are required for the new tests.

---

## S8 Fixture Pattern for Co-Access Signal

`run_s8_tick` drives from co-access signal data in `co_access` table (or audit_log). To generate
an S8 qualifying pair, both entries must be active (status=0) and their IDs must appear together
in enough audit_log sessions (or be present in the co_access table above threshold). Consult
the existing S8 test setup in the file (`test_s8_basic_coaccess_edge_written`) and replicate
its fixture pattern for the new tests.

---

## Test Cases

### TICK-S1-U-10: run_s1_tick writes both directions per pair (AC-03, AC-10, R-03)

```rust
#[tokio::test]
async fn test_s1_both_directions_written()
```

This is the **per-source regression guard** for run_s1_tick (SR-06). If the second
`write_graph_edge` call is removed from run_s1_tick, exactly this test fails.

**Arrange**:
- Two entries (id=1, id=2), both active (status=0).
- Three shared tags: `"tag_a"`, `"tag_b"`, `"tag_c"` — qualifies under S1 threshold (≥3).

```rust
seed_entry(&store, 1, 0).await;
seed_entry(&store, 2, 0).await;
for tag in &["tag_a", "tag_b", "tag_c"] {
    seed_tag(&store, 1, tag).await;
    seed_tag(&store, 2, tag).await;
}
```

**Act**: `run_s1_tick(&store, &make_config()).await`

**Assert**:
```rust
// Forward direction (lower_id → higher_id — as written by query shape).
let fwd = fetch_edge(&store, 1, 2, "Informs").await;
assert!(fwd.is_some(), "forward (1→2) S1 Informs edge must exist");
let (_, src, _, bootstrap_only) = fwd.unwrap();
assert_eq!(src, "S1");
assert_eq!(bootstrap_only, 0);

// Reverse direction (higher_id → lower_id — new second call).
let rev = fetch_edge(&store, 2, 1, "Informs").await;
assert!(rev.is_some(), "reverse (2→1) S1 Informs edge must exist (AC-03)");
let (_, src_rev, _, _) = rev.unwrap();
assert_eq!(src_rev, "S1", "reverse edge must carry source='S1'");

// Total S1 edges: exactly 2.
assert_eq!(count_edges_by_source(&store, "S1").await, 2,
    "one pair must produce exactly 2 S1 Informs edges");
```

**Risks covered**: R-03 (S1 omits second call), AC-03, AC-10.

---

### TICK-S2-U-10: run_s2_tick writes both directions per pair (AC-04, AC-10, R-03)

```rust
#[tokio::test]
async fn test_s2_both_directions_written()
```

This is the **per-source regression guard** for run_s2_tick (SR-06).

**Arrange**:
- Two entries (id=3, id=4), both active.
- Content and title including S2 vocabulary terms to qualify. Use `make_config_s2()` with
  a vocabulary containing terms present in both entries' title/content.

```rust
seed_entry_with_content(&store, 3, "graph neural network architecture", "knowledge").await;
seed_entry_with_content(&store, 4, "neural network graph topology", "context").await;
let cfg = make_config_s2(vec!["graph", "neural", "network"], 10);
```

**Act**: `run_s2_tick(&store, &cfg).await`

**Assert**:
```rust
// Forward direction.
assert!(fetch_edge(&store, 3, 4, "Informs").await.is_some(),
    "forward (3→4) S2 Informs edge must exist");

// Reverse direction — new second call.
let rev = fetch_edge(&store, 4, 3, "Informs").await;
assert!(rev.is_some(), "reverse (4→3) S2 Informs edge must exist (AC-04)");
let (_, src_rev, _, _) = rev.unwrap();
assert_eq!(src_rev, "S2", "reverse edge must carry source='S2'");

// Total S2 edges: exactly 2.
assert_eq!(count_edges_by_source(&store, "S2").await, 2,
    "one pair must produce exactly 2 S2 Informs edges");
```

**Risks covered**: R-03 (S2 omits second call), AC-04, AC-10.

---

### TICK-S8-U-10: run_s8_tick writes both directions per pair (AC-05, AC-10, R-03)

```rust
#[tokio::test]
async fn test_s8_both_directions_written()
```

This is the **per-source regression guard** for run_s8_tick (SR-06).

**Arrange**: Two active entries and a co-access signal qualifying them as a pair. Use the same
fixture pattern as `test_s8_basic_coaccess_edge_written` (existing test) — replicate its setup
for entries id=101, id=102 or similar values not conflicting with other tests.

**Act**: `run_s8_tick(&store, &make_config_s8(1, 10)).await`

**Assert**:
```rust
// Forward direction (a=min(ids), b=max(ids) — written by existing call).
assert!(fetch_edge(&store, 101, 102, "CoAccess").await.is_some(),
    "forward (101→102) S8 CoAccess edge must exist");

// Reverse direction (*b, *a — new second call).
let rev = fetch_edge(&store, 102, 101, "CoAccess").await;
assert!(rev.is_some(), "reverse (102→101) S8 CoAccess edge must exist (AC-05)");
let (_, src_rev, _, _) = rev.unwrap();
assert_eq!(src_rev, "S8", "reverse edge must carry source='S8'");

// Total S8 edges: exactly 2.
assert_eq!(count_edges_by_source(&store, "S8").await, 2,
    "one pair must produce exactly 2 S8 CoAccess edges");
```

**Risks covered**: R-03 (S8 omits second call), AC-05, AC-10.

---

### TICK-S8-U-11: pairs_written counter per-edge — new pair (AC-05, AC-12, R-05)

```rust
#[tokio::test]
async fn test_s8_pairs_written_counter_per_edge_new_pair()
```

**Arrange**: Same two-entry fixture as TICK-S8-U-10. Neither direction exists in GRAPH_EDGES
before the tick runs (clean store).

**Act**: Call `run_s8_tick` and capture its return value or internal counter. Since
`run_s8_tick` is `async fn` with no return value, instrument the assertion via GRAPH_EDGES
row count to infer the counter value. The implementation test approach:

Note: if `run_s8_tick` returns a `TickResult` struct or logs the counter, capture it. If it is
fire-and-forget with no observable return, verify via edge count:
- 2 edges written → both calls returned `true` → counter incremented by 2.

```rust
run_s8_tick(&store, &make_config_s8(1, 10)).await;

let s8_count = count_edges_by_source(&store, "S8").await;
assert_eq!(s8_count, 2,
    "new pair: both write_graph_edge calls return true → 2 rows inserted");
// 2 rows = counter incremented by 2 (per-edge semantics, AC-12)
```

If the tick function has a structured return with `pairs_written` field:
```rust
let result = run_s8_tick(&store, &make_config_s8(1, 10)).await;
assert_eq!(result.pairs_written, 2,
    "new pair: pairs_written must be 2 (per-edge, AC-12, C-06)");
```

**Risks covered**: R-05 (counter stays per-pair), AC-05 counter assertion, AC-12.

---

### TICK-S8-U-12: false return on pre-existing reverse — no warn, counter += 1 (AC-13, R-04)

```rust
#[tokio::test]
async fn test_s8_false_return_on_existing_reverse_no_warn_no_increment()
```

This test simulates **steady-state post-migration** where the reverse edge already exists (the
common case after v19→v20 migration back-fills the reverse). It validates that:
1. The `false` return from the second `write_graph_edge` call does NOT trigger a warn/error log.
2. The counter increments by 1 (first call true → +1, second call false → +0).
3. The tick completes successfully.

**Arrange**: Two active entries with a co-access qualifying signal. Pre-insert BOTH directions
into GRAPH_EDGES to simulate post-migration state:

```rust
// Pre-insert forward edge (what the first write_graph_edge call will produce UNIQUE conflict for).
// NOTE: pre-insert the REVERSE edge (what the second call will conflict on).
// The tick's first call inserts forward; if forward also pre-exists, both calls conflict.
// To test "second call conflicts": pre-insert only the REVERSE direction.
sqlx::query(
    "INSERT INTO graph_edges (source_id, target_id, relation_type, weight, \
     created_at, created_by, source, bootstrap_only) \
     VALUES (?1, ?2, 'CoAccess', 0.25, 0, 'migration', 'S8', 0)"
)
.bind(102i64)  // reverse: higher → lower
.bind(101i64)
.execute(store.write_pool_server()).await.unwrap();
```

With the reverse pre-inserted and the forward absent, when the tick runs:
- First `write_graph_edge(101, 102, ...)` → inserts → returns `true` → counter += 1.
- Second `write_graph_edge(102, 101, ...)` → UNIQUE conflict (pre-inserted row) → returns
  `false` (Ok path, no warn).

**Act**: `run_s8_tick(&store, &make_config_s8(1, 10)).await`

**Assert**:
```rust
// 2 edges total: 1 pre-inserted reverse + 1 newly-inserted forward.
// (No new edges from the second call — it conflicted.)
assert_eq!(count_edges_by_source(&store, "S8").await, 2,
    "pre-existing reverse + newly-inserted forward = 2 total S8 edges");

// Counter == 1: only first call returned true.
// Verify via edge count: exactly 1 new row inserted during this tick.
// (If tick returns pairs_written, assert == 1.)
```

For log-level verification (no warn/error): if the tick test harness captures log output via
`tracing_test` or similar, assert that no WARN-level message containing "graph_edge" or
"UNIQUE" appears. If no log capture is available, this assertion is covered by code review
confirming C-09.

**AC-13 assertion summary**:
- `pairs_written` increments by 1 (not 0, not 2).
- No error counter increments (verified structurally — `write_graph_edge` false return is the
  Ok path, not the Err path).
- Tick completes without panicking.

**Risks covered**: R-04 (false return triggers warn), AC-13, R-05 (partial counter).

---

## Independence of Per-Source Tests

The three bidirectionality tests (TICK-S1-U-10, TICK-S2-U-10, TICK-S8-U-10) are **independent
regression guards**:

- Each test runs in its own `tempfile::TempDir` with a fresh store — no shared state.
- If a future change removes the second `write_graph_edge` call from `run_s1_tick` only,
  exactly `test_s1_both_directions_written` fails. The S2 and S8 tests continue to pass.
- This per-source independence is the SR-06 requirement from RISK-TEST-STRATEGY.md.

---

## Tests NOT Needed

| Scenario | Reason |
|----------|--------|
| run_s1_tick idempotency with pre-existing both directions | Already covered by existing `test_s1_idempotent` (second run returns false on both calls — same mechanism). |
| run_s2_tick and run_s8_tick idempotency separately | Existing tests cover idempotency for each tick via single-call idempotency. Two-call idempotency follows from `INSERT OR IGNORE` contract (no new assertion needed). |
| Weight field correctness on reverse edge | Architecture specifies same weight as forward edge (`g.weight` copied in migration; same `weight` arg in second tick call). The per-source tests verify the reverse row exists with `source` field correct — weight is implicitly correct by the identical argument pattern. Explicit weight assertion is not a risk-justified addition. |

---

## Test Count Summary

| Test ID | Function | AC | Risk |
|---------|----------|-----|------|
| TICK-S1-U-10 | `test_s1_both_directions_written` | AC-03, AC-10 | R-03 |
| TICK-S2-U-10 | `test_s2_both_directions_written` | AC-04, AC-10 | R-03 |
| TICK-S8-U-10 | `test_s8_both_directions_written` | AC-05, AC-10 | R-03 |
| TICK-S8-U-11 | `test_s8_pairs_written_counter_per_edge_new_pair` | AC-05, AC-12 | R-05 |
| TICK-S8-U-12 | `test_s8_false_return_on_existing_reverse_no_warn_no_increment` | AC-13 | R-04 |

**Total: 5 new tests**, all `#[tokio::test]`, all added to the existing test file.

---

*Authored by crt-044-agent-2-testplan (claude-sonnet-4-6). Written 2026-04-03.*
