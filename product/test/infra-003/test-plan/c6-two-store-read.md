# Test Plan — C6: Cross-store negative read + two-store read primitive

> Pseudocode: `pseudocode/c6-two-store-read.md`. Risks: **R-04** (WAL false-empty
> on the OTHER store — the dangerous direction), **R-18** (substring cross-match),
> R-12, R-07 (missing-B INFRA), R-11. ACs: **AC-04**, **AC-08**, **AC-12**.

C6 is the genuine cross-store negative read, parameterized over `(store_dir, table,
predicate)`: each store must hold **only its own** marker. This is where a real
leak is converted into RED. The test of C6 proves the negative read operates on a
WAL-complete copy of the **other** store (a leaked row hiding in the other store's
uncopied `-wal` is the false-GREEN trap), reads on-disk via `vol` (no
cross-credential), and that the `LIKE` predicate cannot cross-match.

## What C6 must do (behavior under test)

- After a direction's positive reaches PRESENT (C5), a **single** read of the
  **other** store for this marker; assert **0 rows**.
  - Observe: A must not hold `obs-b`; B must not hold `obs-a`.
  - MCP: A must not hold `mcp-b`; B must not hold `mcp-a`.
- Each store read **on-disk via `vol`** (db + `-wal` + `-shm`) — no slug's
  credential reads another's store, no over-the-wire B read (AC-12, #4950).
- Built fresh — does **not** extend the removed D6 dir-count/`other_count`
  heuristic (a latent false-RED trap, ass-084 OoS#2); content reads only.

## Verification tier 1 — off-Docker gate-logic test (stub-driven, primary teeth)

- `test_c6_marker_in_wrong_store_is_red` (**fault-injection teeth**, R-04/R-18) —
  stub the two-store read so `obs-b`/`mcp-b` is PRESENT in **A's** store → assert
  the gate exits **RED** at the cross-store cell. Symmetric: `obs-a`/`mcp-a` in
  **B's** store → RED. This proves the gate is not a vacuous GREEN (R-02/R-03).
- `test_c6_clean_other_store_passes` — stub all four cross-cells absent → no RED
  from C6 (the negative controls pass).
- `test_c6_no_count_heuristic` (R-04/C-11) — grep/inspection: the negative read is
  a content `WHERE` predicate, never a `du`/dir-count/`other_count` (the removed
  D6 logic is replaced, not extended).
- `test_c6_missing_other_db_is_infra` (R-07) — stub the other store's main db
  absent → **INFRA**, not a 0-row "clean" pass that would mask a missing store.

## Verification tier 2 — live run

- `test_c6_other_store_wal_copied` (R-04, the dangerous direction) — confirm the
  **other** store's `-wal`/`-shm` are copied before the negative query, so a
  genuinely cross-routed row sitting in the other store's uncopied WAL cannot
  false-GREEN the leak. All four marker reads operate on WAL-complete copies.
- `test_c6_on_disk_vol_read_only` (AC-12, security) — each read uses the `vol`
  sidecar mounted `:ro`; assert no `docker exec`, no `:rw`, and the shared write
  token is never repurposed as a read proof.
- `test_c6_like_no_cross_match` (R-18) — with all four markers present in their own
  stores, each cross-cell `LIKE '%<marker>%'` returns 0, confirming the mutually
  non-substring set never cross-matches.

## Coverage requirement

All four cross-cells read a WAL-complete copy of the **other** store on-disk via
`vol` (R-04/AC-12); a marker found in the wrong store is RED (teeth); a missing
other-store db is INFRA, never a 0-row clean pass (R-07); `LIKE` cannot cross-match
(R-18); no count heuristic (C-11).
