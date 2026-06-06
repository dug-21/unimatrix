# Test Plan: state.js (state dir, atomic writes, breadcrumb)

ADR-003 (layout, atomic writes, sanitization), ADR-005 (breadcrumb). Risks: R-10 (High), R-14,
R-16, R-19. Suite: `test/hook-client/state.test.js` (temp HOME per test).

## Breadcrumb Accuracy (R-10 — "a wrong breadcrumb is worse than none")

- `test_failure_class_matrix` — drive spawns through ECONNREFUSED→`connect`, timeout→`timeout`, 401 and 403→`auth`, 404/413→`http_4xx`, 500→`http_5xx`; assert `health.json.failure_class` exact per row.
- `test_consecutive_failures_counter` — increments across 3 failing spawns, resets to 0 on success; `last_success`/`last_failure` timestamps update on the right transitions.
- `test_queue_depth_truthful` — `queue_depth` equals actual file count in `queue/` at write time (seed 0, 3, 500 files).
- `test_sync_failures_update_breadcrumb` — sync-trio send failures also write the breadcrumb (every send-attempting spawn, ADR-005).
- `test_w4_transition_sequence` — outage → N failures → recovery: breadcrumb state at each step matches the W4 workflow (state-transition test, R-10 coverage requirement).
- `test_content_free` — across ALL failure classes: breadcrumb contains no token substring, no payload fragment, no transcript bytes, no full URL — `url_host` is host only (R-16).
- `test_breadcrumb_write_failure_nonfatal` — read-only state dir: spawn still exits 0, no stdout, send still attempted (R-10 scenario 3).

## Atomic Writes

- `test_offset_write_temp_plus_rename` — fs spy: write to temp name then `renameSync`; no direct write to final path; concurrent reader never observes partial JSON (loop-read during hammered writes).
- `test_breadcrumb_atomic` — same mechanism for `health.json`.

## Offset Persistence Lifecycle

- `test_offset_file_shape` — `offsets/{session_key}.json` == `{"offset": N, "updated": <unix secs>}`.
- `test_offset_prune_7days` — offset files with `updated` >7 days pruned; a pruned-mid-session file → treated as offset 0 (safe re-ship; joint with delta.md corrupt-offset case).
- `test_offset_deleted_on_sessionclose_success` — successful SessionClose send deletes the session's offset file; failed SessionClose leaves it.

## Session-Key Sanitization (R-19 / security table)

- `test_key_passthrough` — `^[A-Za-z0-9_-]{1,64}$` ids used verbatim.
- `test_key_hashed_otherwise` — traversal corpus: `../../etc/passwd`, absolute paths, ids with `/`, NUL bytes, 65+ chars, Unicode → `sha256(id).slice(0,16)`; resulting path stays inside `offsets/` (resolved-path prefix assertion).

## State Dir Creation (R-14)

- `test_dir_modes` — dirs created 0700, files 0600 (POSIX); on Windows runner: chmod no-ops must not throw.
- `test_no_home_env` — `HOME`/`USERPROFILE` unset → no throw; spawn exits 0, send still attempted (state ops degraded best-effort).
- `test_full_disk_all_writes_fail` — offset + queue + breadcrumb writes all failing → exit 0 (edge-case row).
