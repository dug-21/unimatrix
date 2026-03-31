## ADR-001 (crt-035): Bidirectional Tick Updates Are Eventually Consistent, Not Atomic Per Pair

### Context

The promotion tick writes both `(a→b)` and `(b→a)` CoAccess edges per qualifying pair.
Each direction requires three SQL operations: INSERT OR IGNORE, weight fetch (on no-op),
and conditional UPDATE. The infallible-tick contract (SCOPE.md §Constraints, crt-034) requires
that a failure on any single SQL operation is logged at `warn!` and the tick continues — a
per-pair SQL error must not abort the batch.

SR-01 from the scope risk assessment asks whether forward + reverse weight updates must be
atomic (same transaction per pair) or can be eventually consistent (independent SQL calls
that converge on the next tick).

SR-07 notes that oscillating pairs (weight flips above/below threshold between ticks) could
leave one direction updated and the other stale if a partial failure occurs mid-pair.

Two options were considered:

**Option A — Eventual consistency (independent SQL calls):**
- Each direction is an independent INSERT/fetch/UPDATE sequence.
- A failure on the reverse direction logs `warn!` and the loop continues to the next pair.
- On the next tick, the reverse direction's INSERT is a no-op (edge exists) and the
  weight-fetch + UPDATE path detects and corrects any delta > 0.1.
- No per-pair transaction wrapping required.

**Option B — Atomic per pair (transaction per pair):**
- Wrap both directions in a `BEGIN`/`COMMIT` per pair.
- A failure rolls back both directions for that pair; the pair is retried next tick.
- Simpler consistency model, but: SQLite nested transactions via `SAVEPOINT` add overhead;
  the infallible contract must abort the pair rather than the direction, which changes the
  blast radius of failures; and the tick's batch parallelism cannot be recovered easily.

### Decision

Option A: eventual consistency. Forward and reverse weight updates are independent SQL calls.
Both directions use the same `new_weight` (derived from the same `co_access.count` and the
same `max_count`). A partial failure (reverse INSERT succeeds, reverse UPDATE fails) leaves
the reverse edge at its previous weight until the next tick detects the delta and corrects it.
Convergence time is one tick interval (configurable, default ~60s).

The `promote_one_direction` helper returns `(inserted: bool, updated: bool)`. Failure to
insert logs `warn!` and returns `(false, false)`. Failure to update logs `warn!` but the
pair is not retried within the same tick. The batch continues.

### Consequences

**Easier:**
- Infallible-tick contract is preserved: a broken reverse direction does not abort the pair
  or the batch.
- Implementation is straightforward: `promote_one_direction` called twice per pair, totals
  accumulated.
- No SAVEPOINT or per-pair transaction overhead.

**Harder:**
- During the one-tick convergence window, forward and reverse edges may have different
  weights if a partial failure occurred. PPR will see slightly asymmetric scores for that
  pair for one tick interval.
- Oscillating pairs (SR-07) can exhibit transient asymmetry if weight updates straddle the
  delta threshold across ticks. Both directions normalize to the same `new_weight` on the
  tick where both updates pass the delta guard, so asymmetry is bounded.

**Not changed:**
- `CO_ACCESS_WEIGHT_UPDATE_DELTA` (0.1) applies per direction independently.
- The delta guard semantics (strictly greater than) are unchanged.
- The `INSERT OR IGNORE` idempotency mechanism is unchanged.
