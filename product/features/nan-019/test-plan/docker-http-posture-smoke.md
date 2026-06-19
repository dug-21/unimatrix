# Test Plan — `docker-http-posture-smoke.sh` (AC-05 grew-assertion)

> The only sanctioned script change in this feature: a **single bounded assertion pair**
> (FR-05 / ADR-005) added **before** the terminal `[783-smoke] ALL GATES PASSED` marker, via
> the existing read-only `vol()` busybox sidecar — never `docker exec` into distroless. This
> component owns **AC-05 grew-signal monotonicity + discrimination (R-04)** and **AC-05
> regression of the smoke/marker (R-12)**. R-04 is provable **pre-merge locally** via repeated
> full-smoke runs and **cannot be retried away** (OQ-6) — so it must be proven non-flaky
> before merge.

## Change under test

After the per-slug `204` write (current smoke lines 124–140), sample a WAL-robust,
non-decreasing size signal via `vol()` for both stores and assert with the existing `fail()`:

- per-slug **grew**: `SLUG_AFTER > SLUG_BEFORE` else `fail` — store
  `/data/.unimatrix/<slug>/unimatrix.db`.
- hash store **unchanged**: `HASH_AFTER == HASH_BEFORE` else `fail` — `$HASH_DIR/unimatrix.db`
  (the literal #783 mis-route symptom).

"Before" = after register+restart, before the POST; "after" = after the `204`. The pair sits
**before** line 142's marker so the marker stays last.

**Signal choice (R-04 / ADR #329):** the main `.db` file size is NOT monotone on a single
small committed write under WAL (autocheckpoint ~1000 pages). Use a **WAL-inclusive** signal —
`du -s` over the per-slug store dir, OR the sum of `unimatrix.db` + `-wal` + `-shm` — validated
monotone over ≥5 runs. A flaky signal cannot be papered over by retry (OQ-6).

---

## T3 — AC-05 grew-signal validation (R-04) — PRE-MERGE HARD GATE (local)

| Test fn | Method | Assertion | Risk |
|---------|--------|-----------|------|
| `test_ac05_signal_monotone_5x` | Run the **full smoke end-to-end ≥5×** locally with `IMAGE=` set | The grew-assertion passes **every** run — no intermittent "did not grow" | R-04 |
| `test_ac05_positive_control` | Healthy write lands in the per-slug store | `SLUG_AFTER > SLUG_BEFORE` holds | R-04 |
| `test_ac05_negative_control_misroute` | Simulate the #783 symptom (write lands in hash dir, not slug dir) | The assertion **MUST FAIL** — `SLUG_AFTER == SLUG_BEFORE` (grew-check fails) AND/OR `HASH_AFTER > HASH_BEFORE` (hash-unchanged fails) | R-04 (discrimination) |
| `test_ac05_uses_vol_not_exec` | Static read of the script | The measurement uses `vol()` busybox read-only (`:ro`); **no** `docker exec` into the distroless runtime image | R-12 / C-09 |
| `test_ac05_signal_is_wal_inclusive` | Static read of the script | The size signal is WAL-inclusive (`du -s` over the store dir or `db`+`-wal`+`-shm`), **not** a main-`.db`-file-only delta | R-04 |

**Coverage requirement (R-04):** the grew-signal is **WAL-robust** (validated monotone over
≥5 full runs), **discriminating** (the negative control fails on a hash-store mis-route — a
grew-check that can't fail on a mis-route is theater), and **sidecar-based**. No flaky signal
ships — because it cannot be retried away (OQ-6).

> **OQ-6 hard constraint for the impl/test agent:** if any of the ≥5 runs shows a non-monotone
> signal, the fix is to **change the signal** (make it WAL-inclusive / more robust), NOT to add
> a retry, a sleep-then-recheck loop, or a tolerance band that would also mask a real
> mis-route. Prove monotonicity before merge.

---

## T3 — AC-05 must not regress the smoke / marker (R-12)

| Test fn | Assertion | Risk |
|---------|-----------|------|
| `test_smoke_marker_still_last` | After FR-05, the smoke still emits `[783-smoke] ALL GATES PASSED` as the **LAST** gate line (the grew-assertion runs **before** it, not after) | R-12 |
| `test_smoke_baseline_3x_amd64` | The smoke still passes **3/3** locally on amd64 post-AC-05 (the #786 baseline holds; R-13) | R-12/R-13 |
| `test_ac05_change_is_bounded` | The change is **one** assertion pair, introduces **no** new script, re-implements **nothing** in YAML (NFR-08 / C-12), and adds no new scenario | R-12 |
| `test_ac05_marker_byte_unchanged` | The terminal marker string is byte-identical to what the gate greps for (`[783-smoke] ALL GATES PASSED`) — AC-05 did not perturb it (ties back to `smoke-amd64.md` T1 `test_gate_marker_byte_identical`) | R-12/R-03 |

**Coverage requirement (R-12):** post-AC-05, the smoke still passes 3/3 locally on amd64 and
still emits the marker **last**; the change is bounded and sidecar-based. A marker moved off
the true end silently breaks the run-marker the gate keys on — this is the cross-component
link to `smoke-amd64.md`/`smoke-arm64.md` T1.

---

## Pre-merge-provable vs post-tag split (this component)

- **PRE-MERGE, provable locally (HARD):** R-04 monotonicity (≥5 amd64 runs), R-04 discrimination
  (negative control), R-12 marker-last, R-12 boundedness, amd64 3/3 baseline. The grew-signal
  is the one Critical risk fully provable on local Linux — and it MUST be, because it cannot be
  retried away.
- **POST-DISPATCH / POST-TAG only:** the AC-05 assertions passing on **arm64** (never-run path,
  R-13) — confirmed on the dispatch dry-run then the first tag, treated as discovery.

## Edge case
- One-page write fully absorbed by `-wal` with no main-`.db` growth before checkpoint → the
  WAL-inclusive signal still grows. This is the exact failure mode of a naive main-file-only
  delta (ADR #329) and is why `test_ac05_signal_is_wal_inclusive` is a static gate, not just a
  runtime observation.
