## ADR-003: `edges_carried` Count Contract — Count Actual Inserts, Own the Write Loop

### Context

AC-11 requires the response ack to carry an `edges_carried` integer — **count only**, no edge
content, **omitted when zero**. SR-02 is the trap: `write_graph_edge` returns `bool` via
`rows_affected() > 0` (pattern #4041), where:

- `true`  — new row inserted.
- `false` (Ok path) — UNIQUE conflict, `INSERT OR IGNORE` silently discarded (idempotent).
- `false` (Err path) — SQL error, warned internally, **not** propagated.

If the carry loop counts *attempted* writes instead of *successful inserts*, `edges_carried`
and idempotency (AC-08) go wrong: an edge already on B (because `params.edges` wrote the same
triple in step 8b, ADR-001) is a UNIQUE conflict and must **not** be counted, and a SQL-failed
write must **not** be counted as carried.

The existing `validate_and_write_edges` (`edge_write.rs:152`) **discards** the per-edge bool
(`let _inserted = write_graph_edge(...)`). It cannot report a carry count. So the carry loop
cannot simply delegate the whole batch to it and read a number back.

### Decision

`run_carry_forward_loop` **owns its write loop** and keys the count off `write_graph_edge`'s
`true` return — counting actual inserts, never attempts:

```
for row in eligible_outgoing_rows:
    inserted = write_graph_edge(store, B, row.target_id, &row.relation_type,
                                1.0, row.created_at, EDGE_SOURCE_AGENT, "")
    if inserted:            // true  → genuinely new carried edge
        carried += 1
    // false → UNIQUE conflict (already on B from step 8b, or duplicate row) OR SQL error.
    //         For SQL error, write_graph_edge already warned; increment `failed`.
    //         The two false-cases are distinguished only by whether a warn fired — the loop
    //         cannot see that. Per ADR-002 posture both are non-fatal; `carried` excludes both.
```

`edges_carried = CarrySummary.carried` = the number of `true` returns = genuinely new edges
placed on B by carry-forward. Idempotency follows directly: re-correcting, or passing an edge
in `params.edges` that A already had, yields a UNIQUE conflict → `false` → not counted (SR-02,
AC-08).

`CarrySummary { found, carried, failed }`:
- `found`   — eligible outgoing rows returned by `query_outgoing_edges`.
- `carried` — `true` returns (the `edges_carried` ack value).
- `failed`  — distinguished SQL-error writes (the SR-01 observable signal, ADR-002).

**Distinguishing `false` UNIQUE-conflict from `false` SQL-error:** `write_graph_edge` collapses
both to `false`. To populate `failed` precisely the carry loop needs the SQL-error signal. Two
admissible implementations — **the developer chooses**, the contract is the count semantics:
(a) accept that `failed` may be approximate and rely on `write_graph_edge`'s internal `warn!`
as the SR-01 signal (count `found - carried` as "not newly inserted", `failed` derived only
where a pre-check confirms the edge was absent); or (b) add a thin count-returning wrapper /
extend `write_graph_edge` to surface the three-case distinction. Recommendation: **(a)** — do
not change `write_graph_edge`'s signature; the SR-01 test asserts *correction + already-carried
edges persist on a forced write failure*, which the internal warn + non-abort posture
satisfies regardless of whether `failed` is exact. `carried` (the ack) is always exact because
it keys strictly off `true`.

**Ack rendering (AC-11):** append `edges_carried` to the response only when `carried > 0`
(omit at zero, mirroring vnc-017's zero-edge silence, ADR-004 vnc-017). Count only — never edge
identities or content (OQ-03: no provenance marker; the ack is the sole awareness channel).

### Consequences

Easier: `edges_carried` is provably exact (keys off `true` only), idempotent re-passes are
correct by construction, and the count rides the existing #4041 contract with no new
primitive. No schema/provenance marker (OQ-03).

Harder: the carry loop cannot reuse `validate_and_write_edges` wholesale for non-`Contradicts`
edges because that function discards the bool — it writes its own loop (and delegates only
`Contradicts` bidirectional handling, ADR-005). `failed` exactness is bounded by
`write_graph_edge` collapsing UNIQUE-conflict and SQL-error to `false`; accepted — `carried` is
exact and the SR-01 guarantee (non-abort) does not depend on `failed` precision.

Related: ADR-001 (8b before 8b′ so re-passed edges conflict, not double-count), ADR-004 (upsert
composition), ADR-002 (posture). Pattern #4041 (rows-affected contract — load-bearing here).
