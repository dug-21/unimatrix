# nan-019 Agent Report — AC-05 grew-assertion (docker-http-posture-smoke.sh)

**Agent:** nan-019-agent-4-ac05-smoke
**Scope:** ONE bounded edit (AC-05 / ADR-005 #5185) — assert per-slug store GREW and hash store did NOT.

## Files modified
- `product/test/infra-001/scripts/docker-http-posture-smoke.sh` (only)

## What changed (one bounded assertion pair + helper)
1. Added `store_size()` helper next to existing `vol()`: `vol du -s "$1" | awk '{print $1}'` — WAL-robust dir-size via the read-only busybox sidecar (no `docker exec`).
2. BEFORE sample (after register+restart, before the per-slug POST): `SLUG_BEFORE`/`HASH_BEFORE`.
3. AFTER sample (after the `204` confirmed) + GATE 4: `SLUG_AFTER -gt SLUG_BEFORE` else `fail`; `HASH_AFTER -eq HASH_BEFORE` else `fail` (the #783 mis-route symptom). Placed BEFORE the terminal `[783-smoke] ALL GATES PASSED` marker, which stays LAST (R-12). Exit contract (0/1/3) intact — failures route through existing `fail()` → exit 1.

## Signal chosen
`du -s` over the store DIRECTORY (not main-`.db` `stat`/`wc -c`). Rationale: under WAL autocheckpoint (~1000 pages, ADR #329) one small committed write lands in `unimatrix.db-wal` and does NOT enlarge the main `.db` until checkpoint — a main-file-only delta is the flaky form (OQ-6 forbids retry). `du -s` counts `unimatrix.db` + `-wal` + `-shm` → non-decreasing on a real write. The `db`+`-wal`+`-shm` byte-sum fallback was NOT needed; du-block granularity did not mask the delta.

## Local validation
- Docker AVAILABLE. `bash -n` syntax OK.
- Ran the full smoke **6×** (run 1 built the image; runs 2–6 reused it via `IMAGE=unimatrix:783-smoke`).
- **Monotone + deterministic:** every run — per-slug grew **356 -> 372** blocks; hash store unchanged **412**; all exit 0; marker printed LAST each time.
- Discrimination: the hash-unchanged half (`HASH_AFTER -eq HASH_BEFORE`) is what reds on a #783 mis-route; the grew half is what reds if the write never lands.

## Issues / blockers
- None. The grew-signal is WAL-robust pre-merge on amd64 (Linux). arm64 confirmation is the never-run path — deferred to the `workflow_dispatch` dry-run / first `v*` tag (Stage 3c / post-tag), as the test plan splits (R-13).

## Knowledge Stewardship
- Queried: `context_search` (decision/nan-019) → surfaced ADR-005 #5185 (grew-assertion surface), ADR-002 #5187, ADR-001 #5186, ADR-004 #5188; (pattern WAL) → SqlxStore dual-pool WAL entries (#2150-2270, background only). `context_get` #5185 for full ADR-005. Findings applied verbatim (du-over-dir signal, marker-last, vol() sidecar, strict -gt/-eq).
- Stored: entry **#5193** "WAL-robust store-grew signal: du -s over the store DIR, never a main-.db stat, for a single committed SQLite write" via context_store (pattern), with a `Supports` edge to ADR-005 #5185 — records the empirically-confirmed monotone deltas and the du-granularity-was-sufficient finding.

## Git
Per spawn instruction: did NOT run any git commands. Delivery Leader owns all git.
