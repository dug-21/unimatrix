# Test Plan: queue.js (disk event queue)

ADR-003 mini-spec. Risks: R-08, R-13, R-16; AC-15 (AMENDED — delta arm asserts NO queue file).
Suite: `test/hook-client/queue.test.js` (temp state dir per test) + spawn-level lifecycle tests.

## Lifecycle (AC-15 amended letter)

- `test_lifecycle_fail_enqueue_recover_replay_drain` — stub down: non-delta FNF frame enqueued as `queue/{ts_ms}-{pid}-{seq}.json` (O_EXCL); stub up: next FNF spawn replays in lexicographic order BEFORE its own frame (stub request log order), each file deleted only after 2xx, queue drained.
- `test_sync_trio_never_queued` — ContextSearch/CompactPayload/Ping failures → no queue file, ever.
- `test_delta_never_queued` — delta send failure → NO file in `queue/` (joint with delta.md; the load-bearing at-rest guarantee — also run as a directory scan after the full AC-09 failure matrix: zero files whose content contains `transcript_delta`).
- `test_replay_never_on_sync_path` — sync spawn with queued frames present → fs spy shows zero `queue/` reads (SR-03, R-13).

## Bounds + Eviction (FR-14)

- `test_enqueue_501st_file_drops_oldest` — 500 files present → enqueue → oldest (lexicographic min) deleted, count == 500.
- `test_enqueue_over_5mib_drops_oldest` — size bound triggers independently of count.
- `test_age_prune_24h` — frames with mtime/ts older than 24 h deleted at enqueue/replay time, NOT replayed (stub never receives them).
- `test_same_ms_same_pid_seq_bump` — O_EXCL collision → `seq` increments; both frames persist; order preserved.

## Replay Budget (FR-15)

- `test_replay_caps_32_frames` — 40 queued → exactly 32 replayed + new event; 8 remain.
- `test_replay_caps_256kib` — large frames → stops at 256 KiB even under 32 frames.
- `test_stop_at_first_failure` — stub: succeed 3, then 500 → frames 1–3 deleted, frame 4+ remain (failed frame NOT deleted), new event still attempted per pseudocode.
- `test_poison_pill` — corrupt (unparseable) frame file mid-queue → deleted, replay continues with the rest; exit 0.

## Concurrency (R-08)

- `test_concurrent_recovering_spawns_double_send` — two spawns replay the same frame before either deletes it → at-most-duplicate delivery; Layer 2 variant asserts the server tolerates the duplicate observation (no error, no corruption).

## Failure Isolation (FR-15 / C-05)

- `test_full_disk_swallowed` — enqueue/delete failures injected (EACCES/ENOSPC via read-only dir) → exit 0, no stdout, send still attempted, breadcrumb + stderr only.
- `test_queue_dir_missing_recreated` — `queue/` deleted between spawns → recreated 0700.

## Security Posture (R-16 / FR-16)

- `test_modes` — dir 0700, files 0600 (POSIX runners; Windows: no throw — R-14).
- `test_no_auth_header_in_frames` — queued frame content scan: no `Authorization`/token string (frames are bare HookRequest JSON).
- `test_distinct_dirs_from_rust_queue` — path is `hook-client/queue/`, never `event-queue/` (integration-risk row: no cross-format reads).

## Concrete Assertions

- `enqueue(frame)` uses `{flag:"wx"}`; never throws outward.
- `replay(send, budget={frames:32, bytes:256*1024})` returns `{sent, remaining}`; deletes only after 2xx.
