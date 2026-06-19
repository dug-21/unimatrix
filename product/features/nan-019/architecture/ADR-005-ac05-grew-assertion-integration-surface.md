## ADR-005: AC-05 Grew-Assertion — Per-Slug DB Grew, Hash Store Did Not, via the Busybox Sidecar

### Context

Gate 3 of the smoke proves only that `/data/.unimatrix/<slug>/unimatrix.db`
*exists* — but `project register` creates that file anyway, so a future
*different-mechanism* mis-route (the write silently lands in the path-hash store
again, the literal #783 symptom: "slug dir empty/static, hash dir populated")
could still pass Gate 3 green. OQ-4 is DECIDED: fold the grew-assertion in as a
required AC (AC-05). The change is bounded (SR-05 — one assertion pair, no smoke
rewrite, no new scenarios) and MUST use the established read-only `busybox`
sidecar pattern — never `docker exec` into the distroless runtime image.

This ADR defines the **integration surface** of the change (what the smoke must
assert and via what mechanism), not the code — the smoke is edited by a
downstream agent, in the `infra-001` script, cumulatively.

### Decision

Extend `docker-http-posture-smoke.sh` to assert, around the per-slug HTTPS write
(Gate 2's `204` POST), that the **per-slug store grew** and the **hash store did
not** — pinning the data-landing assertion to the literal #783 symptom. The
surface:

1. **Two measurement points exist already in the script's vocabulary:**
   - per-slug store: `SLUG_DB="/data/.unimatrix/${SLUG}/unimatrix.db"` (Gate 3).
   - hash store: `HASH_DIR` (discovered at script line ~96), whose DB is
     `"$HASH_DIR/unimatrix.db"`.

2. **Measure a monotonic size signal before and after the write**, via the
   existing `vol()` busybox sidecar (read-only `-v "$VOL:/data:ro"`). Use a
   stat-based byte size (busybox `stat -c %s <path>`), captured for both stores:
   - `SLUG_BEFORE` / `SLUG_AFTER` for `SLUG_DB`
   - `HASH_BEFORE` / `HASH_AFTER` for `"$HASH_DIR/unimatrix.db"`
   "Before" is sampled after register+restart but **before** the per-slug POST;
   "after" is sampled after the `204` is confirmed. (SQLite size growth on a
   committed write is the simplest monotone signal; if WAL mode makes the main
   `.db` size lag, measure the `-wal`/total dir size via busybox `du -s` instead —
   the downstream agent picks whichever is reliably monotone for the shipped DB
   config. The surface requirement is: a non-decreasing per-store size signal.)

3. **Assert the pair** as a new gate (between Gate 2 and the existing Gate 3, or
   folded into Gate 3), using the script's existing `fail()`:
   - per-slug store **grew**: `SLUG_AFTER > SLUG_BEFORE` else
     `fail "per-slug store did not grow after per-slug write => write did not land in the per-slug store"`.
   - hash store **did not grow**: `HASH_AFTER == HASH_BEFORE` else
     `fail "hash store grew after a per-slug write => write was mis-routed to the path-hash store (the #783 symptom)"`.

4. **The terminal run-marker is unchanged.** `ALL GATES PASSED` (line 142) still
   prints only after this new assertion passes — so ADR-003's positive
   run-marker now also certifies the grew-assertion ran. No change to the smoke's
   exit-code contract (`0`/`1`/`3`): a grew-assertion failure is a `fail()` →
   `exit 1`, indistinguishable to the gate from any other behavioral failure
   (which is correct — it IS a behavioral failure).

5. **Constraints the change MUST keep:** read-only busybox sidecar only (no
   `docker exec` into distroless); no new scripts; no new scenarios beyond this
   assertion pair (SR-05); reuses `SLUG`, `HASH_DIR`, `VOL`, `vol()`, `fail()`
   already defined.

### Consequences

- Easier: The smoke now fails on the literal #783 symptom (slug empty / hash
  populated) even if the route returns `204` via some future different mechanism
  — closing the gap that Gate 3's file-exists check left open. Guards N3.
- Easier: The assertion rides the run-marker contract for free — no new gate
  exit code, no new wiring in `release.yml` (ADR-003 already certifies "all gates
  passed" end-to-end).
- Harder: The grew signal depends on the shipped DB's on-disk behavior (WAL vs
  rollback journal, page allocation). The downstream agent must pick a signal
  that is reliably monotone for a single committed write — a flaky size signal
  would make the gate itself flaky (which, per OQ-6, must not be papered over
  with a retry). The surface explicitly allows `du -s` on the store dir as the
  fallback if main-file size is non-monotone.
- Harder: Two extra sidecar runs per smoke (before/after) — negligible cost.
- Related: ADR-003 (the run-marker that now certifies this assertion),
  lesson #5130 (the #783 root cause this assertion pins to its symptom).
