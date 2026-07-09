# C13 — Freeze-outcome trace (best-effort, NON-GATING)

**File:** emitted INSIDE `insert_cycle_start_with_tags` (C2, `unimatrix-store/src/db.rs`)
**ADR:** ADR-007 §(b). **Risks:** R-16 (LOW, non-gating). **AC:** AC-09 (best-effort). **FR-12.**

## Purpose

Give operators log-only visibility into the whole-set-once outcome: whether this start WROTE the set
or hit a FROZEN-SKIP (EXISTS guard found the set already frozen). This is the ONLY observation point
for frozen-skip — it is NOT returned to the caller (would need a new interface; out of scope).

## Placement decision (reconciling the fixed signature)

The wrote-set vs frozen-skip distinction is known ONLY inside the C2 transaction (the EXISTS result).
The C2 public signature is fixed to `-> Result<()>` (ARCHITECTURE Integration Surface), so it CANNOT
return the outcome to the listener. ADR-007 §(b) explicitly permits emitting the trace "in
`insert_cycle_start_with_tags` **or** its caller" — therefore the trace is emitted INSIDE C2, right
after `COMMIT`, using the `wrote_set` boolean the guard computed. See store-write-primitive.md.

> This keeps the Component-Map "C13 = listener step-5" placement approximate: the trace physically
> lives in the store method to preserve the fixed `Result<()>` signature. The listener (C5) may
> optionally emit a coarser "routing to tag write" line, but the authoritative wrote-set/frozen-skip
> distinction is the store-method trace. NON-GATING either way.

## Pseudocode (the two lines inside C2, post-COMMIT)

```
if wrote_set {
    tracing::info!(feature_cycle = %cycle_id, n = tags.len(),
        "cycle_tags: recorded N labels for feature_cycle");
} else {
    tracing::info!(feature_cycle = %cycle_id, n = tags.len(),
        "cycle_tags: set already frozen for feature_cycle, N submitted labels ignored");
}
```

- `tracing` (not `eprintln!`) — this runs inside the tokio fire-and-forget spawn, on the store side.
- Exact wording is illustrative (ADR-007 §b); operators need only the wrote-set vs frozen-skip
  distinction and the `feature_cycle`.

## Error handling

None — logging only; cannot affect the txn result (emitted after COMMIT).

## Key test scenarios (hints — NON-GATING)

1. First tag-bearing start → a wrote-set trace line is emitted (log-assertion or manual observation).
2. A later start on the same FC (frozen) → a frozen-skip trace line is emitted.
> The frozen-skip outcome is NOT caller-returnable — NO assembled-path proof is required for it
> (R-16/AC-09). A listener tracing line is the only observation point. Do NOT block delivery.
