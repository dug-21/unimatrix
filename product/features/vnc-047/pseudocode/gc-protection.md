# C10 — GC protection by OMISSION + regression test

**File:** `crates/unimatrix-store/src/retention.rs` (NO change to DELETE paths; EXTEND `test_gc_protected_tables_regression` :522)
**ADR:** ADR-005. **Risks:** R-07. **AC:** AC-04.

## Purpose

`cycle_tags` must survive GC. Protection is **by OMISSION**, not by a protected-set registration —
there is NO protected-set data structure in `retention.rs`. The load-bearing work is NOT production
code (there is none to add); it is EXTENDING the regression test so a future GC change that starts
deleting `cycle_tags` is caught.

## Production code: DO NOTHING to the DELETE paths

```
# gc_cycle_activity (:116) deletes ONLY: observations -> query_log -> injection_log -> sessions.
# gc_unattributed_activity (:202) deletes ONLY the same enumerated set (unattributed variant).
# Adding cycle_tags to EITHER enumeration would DESTROY the source of truth.
# => Make NO edit to any `DELETE FROM …` in retention.rs. cycle_tags is protected by not appearing.
```

- There is no "register cycle_tags as protected" call — do not invent one (ADR-005, implementer note 1).
- `cycle_tags` joins `cycle_events` / `cycle_review_index` as an omitted-and-therefore-protected table;
  contrast the purgeable `sessions`.

## Test extension: `test_gc_protected_tables_regression` (:522) — BOTH surfaces, positive control

```
EXTEND the existing regression test (do not create isolated scaffolding — test infra is cumulative):

  # --- surface 1: gc_cycle_activity ---
  seed cycle_tags rows for a feature_cycle FC
  seed a cycle_review_index row for FC          # makes the cycle "purgeable"
  seed sessions rows for FC                      # POSITIVE CONTROL (must be purged)
  run gc_cycle_activity(FC)
  assert cycle_tags rows for FC UNCHANGED (intact)          # protection proven
  assert sessions rows for FC purged                        # proves GC actually ran (not vacuous)

  # --- surface 2: gc_unattributed_activity ---
  seed cycle_tags rows (for an unattributed / NULL-feature_cycle scenario as that path targets)
  seed sessions rows matching the gc_unattributed_activity predicate  # POSITIVE CONTROL
  run gc_unattributed_activity()
  assert cycle_tags rows UNCHANGED
  assert the positive-control sessions rows purged
```

### Why the positive control is mandatory (R-07)

Without asserting that SOMETHING (`sessions`) IS purged in the same pass, the "cycle_tags survived"
assertion can pass vacuously (e.g. if GC no-oped on a mis-seeded cycle). The positive control proves
the DELETE paths actually executed against this data.

## Error handling

N/A (test-only component; no production behavior change).

## Key test scenarios (hints)

1. **gc_cycle_activity (AC-04, R-07.1):** cycle_tags intact after a full purge of a purgeable cycle;
   sessions for that cycle purged (positive control).
2. **gc_unattributed_activity (R-07.2):** cycle_tags intact; matching sessions purged.
3. Both surfaces covered in ONE extended regression test (both DELETE surfaces named in the
   integration surface must be exercised).
