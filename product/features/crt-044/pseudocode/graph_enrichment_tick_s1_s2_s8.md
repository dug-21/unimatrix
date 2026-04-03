# Component: graph_enrichment_tick_s1_s2_s8
# File: crates/unimatrix-server/src/services/graph_enrichment_tick.rs

## Purpose

Add a second `write_graph_edge` call per pair in `run_s1_tick`, `run_s2_tick`, and `run_s8_tick`
with swapped `source_id`/`target_id`. This makes forward writes bidirectional going forward,
matching the pattern established by `co_access_promotion_tick.rs`. The migration back-fills
historical edges; this change fixes the write path for all new pairs.

---

## Context: Unchanged Elements

The SQL query shapes in all three tick functions do NOT change. The `t2.entry_id > t1.entry_id`
and `e2.id > e1.id` join conventions, the `valid_ids` guard in S8, the watermark logic, and the
`EDGE_SOURCE_*` constants are all unchanged. Only the per-pair write loop body is extended.

The `write_graph_edge` function (imported from `nli_detection`) is unchanged. It is called twice
per pair — once per direction — with swapped `source_id`/`target_id`. All other arguments are
identical between the two calls.

---

## Three-Case Return Contract for `write_graph_edge` (entry #4041, ADR-002)

```
true  (rows_affected = 1)  → new row inserted; increment budget counter
false (rows_affected = 0, Ok path) → UNIQUE conflict via INSERT OR IGNORE; do NOT warn;
                                      do NOT increment counter; this is the normal steady-state
                                      after migration when the reverse edge already exists
false (Err path)           → SQL error; warn! emitted INSIDE write_graph_edge; do NOT
                              double-log; do NOT increment counter
```

Budget counters (`edges_written` in S1/S2, `pairs_written` in S8) are incremented ONLY on `true`
return from each call independently.

---

## Modified Function: `run_s1_tick`

### Location in file

The per-pair write loop body, lines 119-136 of the current source:

```rust
for row in &rows {
    let weight = f64::min(row.shared_tags as f64 * 0.1, 1.0) as f32;
    if write_graph_edge(store, row.source_id as u64, row.target_id as u64,
                        "Informs", weight, now_ts, EDGE_SOURCE_S1, "").await {
        edges_written += 1;
    }
    // <<< INSERT SECOND CALL HERE >>>
}
```

### Pseudocode for the second call

```
// Second direction: higher_id → lower_id (reverse of SQL query convention).
// Returns false for most pairs post-migration (UNIQUE conflict, silent).
// C-09: false return is correct here; do NOT warn or increment error counter.
if write_graph_edge(
    store,
    row.target_id as u64,   // swapped: was source_id
    row.source_id as u64,   // swapped: was target_id
    "Informs",
    weight,                  // same weight as first call
    now_ts,                  // same timestamp
    EDGE_SOURCE_S1,          // same source constant
    "",                      // same empty metadata
).await {
    edges_written += 1;      // C-06: count per-edge, not per-pair
}
```

### Full modified loop body (for clarity)

```
for row in &rows {
    let weight = f64::min(row.shared_tags as f64 * 0.1, 1.0) as f32;

    // First direction: lower_id → higher_id (existing, from SQL join convention)
    if write_graph_edge(
        store,
        row.source_id as u64,
        row.target_id as u64,
        "Informs",
        weight,
        now_ts,
        EDGE_SOURCE_S1,
        "",
    ).await {
        edges_written += 1;
    }

    // Second direction: higher_id → lower_id (new, crt-044)
    if write_graph_edge(
        store,
        row.target_id as u64,
        row.source_id as u64,
        "Informs",
        weight,
        now_ts,
        EDGE_SOURCE_S1,
        "",
    ).await {
        edges_written += 1;
    }
}
```

The trailing `tracing::info!(edges_written, ...)` log call is unchanged.

---

## Modified Function: `run_s2_tick`

### Location in file

The per-pair write loop body, lines 224-241 of the current source.

### Pseudocode for the second call

Identical pattern to S1, using `row.source_id`/`row.target_id` swapped, `EDGE_SOURCE_S2`,
`"Informs"`, and the S2-computed `weight`.

```
for row in &rows {
    let weight = f64::min(row.shared_terms as f64 * 0.1, 1.0) as f32;

    // First direction: lower_id → higher_id (existing, from e2.id > e1.id join convention)
    if write_graph_edge(
        store,
        row.source_id as u64,
        row.target_id as u64,
        "Informs",
        weight,
        now_ts,
        EDGE_SOURCE_S2,
        "",
    ).await {
        edges_written += 1;
    }

    // Second direction: higher_id → lower_id (new, crt-044)
    if write_graph_edge(
        store,
        row.target_id as u64,
        row.source_id as u64,
        "Informs",
        weight,
        now_ts,
        EDGE_SOURCE_S2,
        "",
    ).await {
        edges_written += 1;
    }
}
```

The trailing `tracing::info!(edges_written, candidates, vocabulary_size, ...)` log call is unchanged.

---

## Modified Function: `run_s8_tick`

### Location in file

Phase 5: the per-pair write loop body, lines 410-429 of the current source.

### Critical context

- The pair `(a, b)` is always `(min(ids), max(ids))` — constructed in Phase 3.
- The `valid_ids` guard (`if !valid_ids.contains(a) || !valid_ids.contains(b)`) covers BOTH
  directions: both `*a` and `*b` are validated in Phase 4. No additional guard is needed before
  the second call.
- The `pairs_written` counter now counts per-edge (individual INSERT attempts returning true).
  A new pair where neither direction exists increments the counter by 2. This is the documented
  semantic change from crt-041's per-pair counting (C-06, ADR-002 SR-01).

### Pseudocode for the second call

```
for (a, b) in &pairs {
    if !valid_ids.contains(a) || !valid_ids.contains(b) {
        pairs_skipped += 1;
        continue;
    }

    // First direction: a → b (min → max, existing)
    if write_graph_edge(
        store,
        *a,
        *b,
        "CoAccess",
        0.25_f32,
        now_ts,
        EDGE_SOURCE_S8,
        "",
    ).await {
        pairs_written += 1;
    }

    // Second direction: b → a (max → min, new, crt-044)
    // false return is correct when reverse edge already exists post-migration (C-09).
    if write_graph_edge(
        store,
        *b,
        *a,
        "CoAccess",
        0.25_f32,
        now_ts,
        EDGE_SOURCE_S8,
        "",
    ).await {
        pairs_written += 1;
    }
}
```

The Phase 6 watermark update and trailing `tracing::info!(pairs_written, pairs_skipped_quarantined,
new_watermark, ...)` log call are unchanged.

---

## `pairs_written` Semantic Change (C-06, ADR-002 SR-01)

After this change, `pairs_written` in `run_s8_tick` counts individual edge INSERT attempts that
return `true`, not logical pairs. Behavioral implications:

| State | Before crt-044 | After crt-044 |
|-------|---------------|--------------|
| New pair, neither direction exists | `pairs_written += 1` | `pairs_written += 2` |
| Pair where one direction exists | `pairs_written += 0 or 1` (edge-dependent) | `pairs_written += 1` (new direction only) |
| Steady-state (both directions exist post-migration) | `pairs_written += 0 or 1` | `pairs_written += 0` (both calls return false) |

This matches `edges_written` semantics in `run_s1_tick` and `run_s2_tick` and the pattern in
`co_access_promotion_tick.rs`. The PR description MUST document this change per AC-12.

---

## Error Handling

- Neither tick function's infallible contract changes. Both functions remain `async fn ... -> u64`.
- `write_graph_edge` returning `false` on the second call (UNIQUE conflict) is NOT an error.
  No warning is emitted. The budget counter is not incremented. Processing continues to the next
  pair (C-09, FR-T-05).
- `write_graph_edge` returning `false` on the Err path emits `warn!` inside `write_graph_edge`
  itself. The tick does NOT double-log. The budget counter is not incremented (FR-T-06).
- A failed first call does NOT skip the second call. Both calls are independent attempts.

---

## Key Test Scenarios (for tester agent)

### Bidirectionality tests (per-source regression guards, R-03, AC-03/04/05, AC-10)

Each test uses a two-entry fixture with the appropriate signal (shared tags for S1, shared terms
for S2, co-retrieved entries in audit_log for S8). After one tick run, queries GRAPH_EDGES directly.

1. **S1 bidirectionality** — Two entries sharing ≥3 tags. Run `run_s1_tick`. Assert both
   `(a→b, source='S1', relation_type='Informs')` and `(b→a, source='S1', relation_type='Informs')`
   exist. Counter = 2. (R-03, AC-03, AC-10)

2. **S2 bidirectionality** — Two entries sharing ≥2 vocabulary terms. Run `run_s2_tick`. Assert
   both directions with `source='S2'`. Counter = 2. (R-03, AC-04, AC-10)

3. **S8 bidirectionality** — Two entries co-retrieved in an audit_log event. Run `run_s8_tick`.
   Assert both `(a→b, source='S8', relation_type='CoAccess')` and
   `(b→a, source='S8', relation_type='CoAccess')` exist. Counter = 2. (R-03, AC-05, AC-10)

### False-return steady-state test (R-04, AC-13)

4. **S8 steady-state** — Pre-insert both `(a→b)` and `(b→a)` CoAccess S8 edges. Run
   `run_s8_tick`. Assert: (a) no warn-level log entries, (b) `pairs_written` does not increment
   (both calls return false), (c) no error counter increments. Verifies C-09 and FR-T-05.

### pairs_written counter assertions (R-05, AC-12)

5. **S8 new pair counter** — Single new pair, neither direction exists. Run `run_s8_tick`. Assert
   `pairs_written = 2` (both directions inserted). (R-05)

6. **S8 partial pair counter** — One direction exists, one does not (simulate single write_graph_edge
   returning true, second returning false). Assert `pairs_written = 1`. (R-05)

---

## Constraints Traced

| Constraint | How Satisfied |
|-----------|--------------|
| C-06 | `pairs_written` incremented per-edge (each `write_graph_edge` call independently) |
| C-09 | Second call returning `false` is not treated as error; no warn; counter unchanged |
| FR-T-01 | `run_s1_tick` calls `write_graph_edge` twice per pair with swapped IDs |
| FR-T-02 | `run_s2_tick` calls `write_graph_edge` twice per pair with swapped IDs |
| FR-T-03 | `run_s8_tick` calls `write_graph_edge` twice per pair with swapped IDs |
| FR-T-04 | `pairs_written` counts per-edge (individual INSERT attempts returning true) |
| FR-T-05 | `false` return on second direction call not treated as error |
| FR-T-06 | Budget counters incremented only on `true` return, independently per call |
| ADR-002  | Two `write_graph_edge` calls per pair; SQL query shapes unchanged |
