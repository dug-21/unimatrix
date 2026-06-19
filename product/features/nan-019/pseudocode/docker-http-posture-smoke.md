# Component: `docker-http-posture-smoke.sh` — AC-05 grew-assertion (one bounded edit)

> File: `product/test/infra-001/scripts/docker-http-posture-smoke.sh`. **A different file from
> `release.yml`** — may be edited independently of the workflow jobs. This is the FOUNDATION
> component (OVERVIEW sequencing): the smoke jobs and the gate-logic test both key on this
> script's 0/1/3 exit contract and its terminal marker, so the marker MUST stay LAST (R-12).
> **One bounded assertion pair only.** No new script, no new scenarios, no YAML logic. (FR-05,
> AC-05, ADR-005, C-09/C-12, NFR-08)

## Purpose

Pin the data-landing check to the literal #783 symptom ("slug dir empty, hash dir populated"):
after the per-slug `204` write, assert the per-slug store **grew** AND the hash store did
**not** change. Uses the existing read-only `vol()` busybox sidecar — never `docker exec` into
the distroless runtime. (Guards N3.)

## Where the edit goes (anchoring within the existing script)

Current terminal region:
```
line 124–135  POST SessionRegister → expect 204            (GATE 2)
line 137–140  assert per-slug store file exists            (GATE 3)
line 142      log "ALL GATES PASSED — ..."                 (terminal run-marker — MUST stay LAST)
```

The grew-assertion is a NEW gate inserted **after GATE 3 (line ~140) and BEFORE the marker
(line 142)** — it becomes the last *assertion*, the marker stays the last *line* (R-12). The
"before" samples are taken **after register+restart, before the POST** (i.e. just before the
existing line ~125 POST); the "after" samples are taken **after the `204` is confirmed**.

## Pseudocode (insert; uses existing `vol()`, `fail()`, `$VOL`, `$SLUG`, `$HASH_DIR`)

```bash
# ---- helper: WAL-robust, non-decreasing size signal over a store dir -------
# Choose a WAL-INCLUSIVE signal (R-04 / ADR #329). The main unimatrix.db file
# is NOT monotone on one small committed write (autocheckpoint ~1000 pages):
# the write may sit in -wal and not enlarge the main file until checkpoint.
# Use `du` over the store DIRECTORY so the -wal/-shm bytes are counted.
store_size() {
  # $1 = absolute dir inside the volume (e.g. /data/.unimatrix/<slug>)
  # du -s reports total blocks for the dir incl. unimatrix.db, -wal, -shm.
  vol du -s "$1" | awk '{print $1}'
}

SLUG_DIR="/data/.unimatrix/${SLUG}"        # per-slug store dir
HASH_DB_DIR="${HASH_DIR}"                   # hash store dir (holds its own unimatrix.db)

# ---- BEFORE sample: after register+restart, BEFORE the POST ----------------
# (placed just before the existing "POST SessionRegister" block, line ~125)
SLUG_BEFORE="$(store_size "$SLUG_DIR")"
HASH_BEFORE="$(store_size "$HASH_DB_DIR")"

#   ... existing POST → require 204 (GATE 2) ...
#   ... existing per-slug store-file-exists check (GATE 3) ...

# ---- AFTER sample: after the 204 is confirmed ------------------------------
SLUG_AFTER="$(store_size "$SLUG_DIR")"
HASH_AFTER="$(store_size "$HASH_DB_DIR")"

# ---- GATE 4 (AC-05): per-slug grew; hash store unchanged -------------------
[ "$SLUG_AFTER" -gt "$SLUG_BEFORE" ] \
  || fail "per-slug store did not grow after the write (before=$SLUG_BEFORE after=$SLUG_AFTER) => write did not land in the per-slug store"

[ "$HASH_AFTER" -eq "$HASH_BEFORE" ] \
  || fail "hash store changed after a per-slug write (before=$HASH_BEFORE after=$HASH_AFTER) => write mis-routed to the hash dir (the #783 symptom)"

log "per-slug store grew and hash store unchanged => write landed correctly. PASS gate 4 (AC-05)"

#   ... existing line 142: log "ALL GATES PASSED — ..."  (UNCHANGED, STILL LAST) ...
```

## Signal-choice rationale (R-04 / ADR #329 — load-bearing)

- **Why a directory `du -s`, not a main-file `stat`/`wc -c`:** under WAL with autocheckpoint
  (~1000 pages), one small committed write often lands in `unimatrix.db-wal` and does NOT grow
  `unimatrix.db` until checkpoint. A main-file-only delta is the **flaky** form — it
  false-fails a healthy write, and `|| retry` is forbidden (OQ-6). `du -s` over the store dir
  counts `unimatrix.db` + `-wal` + `-shm`, giving a non-decreasing signal on a real write.
- **Acceptable equivalent:** sum of `unimatrix.db` + `-wal` + `-shm` sizes via `vol`. The impl
  agent MUST pick whichever is validated **monotone over ≥5 local runs** (R-04 coverage). Do
  NOT ship a signal not so validated.
- **Strict `-gt` / `-eq`:** grew = strictly greater (a write must add bytes); hash unchanged =
  exactly equal (any delta there is the mis-route symptom). Both via the existing `fail()`
  (exit 1) so a violation flows through the gate's `case` as a red.

## Constraints honored

- **C-09:** all sampling via `vol()` (`-v "$VOL:/data:ro"` busybox) — never `docker exec` into
  distroless. Read-only mount: inspection cannot tamper with the artifact under test.
- **C-12 / NFR-08:** one bounded gate added in place; no new script; nothing re-implemented in
  YAML; existing `fail()`/`log()`/`vol()`/`$HASH_DIR` reused.
- **R-12:** the grew-assertion runs BEFORE `ALL GATES PASSED`; the marker remains the literal
  terminal line `[783-smoke] ALL GATES PASSED` so the gate's anchored grep still matches.

## Data Flow

- **In:** running container's data volume `$VOL` (read-only via `vol()`); `$SLUG`, `$HASH_DIR`.
- **Transform:** dir-size sample BEFORE the POST and AFTER the 204 → two integer deltas.
- **Out:** PASS (continue to the marker) or `fail()` → exit 1 (consumed by the smoke job's `case`).

## Error Handling

| Condition | Result |
|-----------|--------|
| Per-slug dir did not grow | `fail()` → exit 1 → job red (write didn't land) |
| Hash dir changed | `fail()` → exit 1 → job red (#783 mis-route) |
| `vol du` errors (dir missing) | non-numeric → `[ -gt ]`/`[ -eq ]` errors under `set -e` → exit 1 |

## Key Test Scenarios (hints — full plan delegated to tester)

- **Monotonicity (R-04):** run the full smoke end-to-end ≥5× locally; the grew-assertion passes
  every time — no intermittent "did not grow".
- **Positive control:** write lands in per-slug store → grew passes.
- **Negative control / discriminating power:** if the write were mis-routed to the hash dir,
  the hash-unchanged half FAILS. A grew-check that cannot fail on a mis-route is theater (R-04).
- **Marker-last (R-12):** post-edit, the smoke still emits `[783-smoke] ALL GATES PASSED` AS THE
  LAST line; the gate's anchored grep still matches; smoke still passes 3/3 on amd64.

## Open Questions

- **OQ-C (resolved here, confirm empirically):** signal = `du -s` over the per-slug/hash store
  dir (WAL-inclusive). The impl/tester agent MUST validate monotonicity over ≥5 runs against the
  shipped DB config before merge; if `du` granularity (block-rounding) masks a small single-write
  delta on the shipped config, fall back to the explicit `unimatrix.db`+`-wal`+`-shm` byte sum
  (same WAL-inclusive intent). Flagged, not a blocker for pseudocode.
