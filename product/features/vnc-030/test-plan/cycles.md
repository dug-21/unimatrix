# Test Plan — C1 `cycles.js` (cycle tracker module)

Source: ADR-001. ACs: AC-01, AC-08. Risks: R-02, R-03, R-08, R-15, R-22 + security (path traversal). File: `packages/unimatrix/test/hook-client/cycles.test.js` (NEW). Reuse `tempStateDir()` (state.test.js idiom), `config.walkToProjectRoot`/`computeProjectHash` (index.test.js `childStateDir`), adversarial bytes via `String.fromCharCode` (#4769). `npm test -- cycles`.

All functions are **never-throw** (F3 C-05). Sanitization happens **inside** `cycles.js` (#4772 — never pre-sanitize at call sites).

## Lifecycle (R-02, AC-01)

### test_writeCycle_creates_file_atomically (FR-01)
- `writeCycle(stateDir, sid, "vnc-030", "delivery")` → `true`; `cycles/{sanitizeSessionKey(sid)}.json` exists with `{topic:"vnc-030", phase:"delivery", declared_at:<secs>, updated:<secs>}`; written via temp+rename (no partial-file window).

### test_writeCycle_overwrites_last_writer_wins
- Two `writeCycle` calls, second topic differs → file holds the second topic (atomic overwrite, FR-07).

### test_readCycle_present_returns_topic_phase
- After `writeCycle`, `readCycle` → `{topic, phase}`.

### test_readCycle_missing_returns_null (FR-06)
- No file → `null`, no throw.

### test_updatePhase_rmw_bumps_phase_and_updated (FR-02)
- `writeCycle` then `updatePhase(..., "review")` → `true`; phase=="review"; `updated` >= prior; `topic` and `declared_at` unchanged.

### test_updatePhase_missing_file_noop_false_no_recreate (R-22, FR-02)
- `updatePhase` on a missing file → `false`; **no file created** (never recreate).

### test_deleteCycle_removes_file (FR-03)
- After `writeCycle`, `deleteCycle` → `true`; file gone. `deleteCycle` on missing file → `false`/no-throw.

## Lifecycle-event isolation (R-02 — the delete-on-close trap, CRITICAL)

### test_tracker_untouched_by_lifecycle_events (FR-04)
- The MODULE has no SessionStart/SessionClose/Stop entry point — assert at the **dispatch** layer (index-decoration.md) that none of these call `writeCycle`/`deleteCycle`. Here assert the module exposes only `read/write/updatePhase/delete/prune/anyOtherCycleFile`-style lifecycle-cycle-keyed ops; no `onSessionClose`-style export exists.
- Cross-ref `index-decoration.md` multi-turn test: cycle_start → 3×(Stop + RecordEvent) → file byte-unchanged after each Stop, stamp still attaches. (R-02 is killed only by the multi-turn assertion — Stop fires per assistant turn.)

## Prune (FR-05, AC-01)

### test_pruneCycles_removes_only_stale
- Write three trackers, set `updated` to now / now-6d / now-8d (inject the field directly). `pruneCycles` removes only the >7d file; the 6d and fresh survive. Deterministic via injected timestamps (no wall-clock dependence).

### test_pruneCycles_piggybacks_queue_prune_never_throws
- Prune on a missing `cycles/` dir → no-op, no throw.

## Crash + `--resume` (R-08, AC-01)

### test_resume_finds_tracker_same_session_key
- `writeCycle` for session S; simulate process restart (new module load, same `stateDir`+S — `--resume` reuses the id, claude 2.1.167); `readCycle(S)` → tracker found, stamping continues with zero gap. **Doc comment pins claude 2.1.167.**

## Worktree path routing (R-15, AC-08)

### test_worktree_tracker_under_main_root_hash (FR-23)
- Arrange: a worktree fixture — a `.git` **file** (not dir) pointing at the main gitdir (the F3 gitdir-port shape, `walkToProjectRoot`/`resolveGitFile`). cwd = worktree path.
- Act: resolve `stateDir` via `config.resolve(cwd).stateDir`, `writeCycle`.
- Assert: the tracker lands under the **main-root** hash; a subsequent worktree-cwd event resolves to the same `stateDir` and `readCycle` finds it.

### test_no_stamp_path_hashes_raw_cwd (C-11, R-15)
- Grep/assert in the test (or a source-audit assertion) that every tracker path derives from `config.resolve(cwd).stateDir`, never a raw-cwd hash. (No persisted raw-cwd discriminator exists to debug a violation — so the test is the only guard.)

## Fail-open injection (R-03, NFR-03 — per fs touchpoint)

For each of `readCycle`/`writeCycle`/`updatePhase`/`deleteCycle`/`anyOtherCycleFile`(readdir): inject EACCES / ENOENT / EROFS (mock `fs`):

### test_failopen_per_fs_touchpoint
- Each returns its never-throw degrade value (`null` for read, `false` for write/update/delete); exit 0 path preserved; **no throw**; **no stdout**; **no secret/path in stderr**.

### test_readCycle_corrupt_json_returns_null (R-03, FR-06)
- Tracker file with malformed/mistyped JSON → `readCycle` → `null`, event sent unstamped, no throw.

### test_writeCycle_disk_full_degrades_false
- ENOSPC on write during cycle_start → `false`; the cycle event is still sent (degrade, not abort).

## Security — path traversal via session_id (CRITICAL)

### test_sanitizeSessionKey_neutralizes_traversal
- Feed adversarial session_ids through `writeCycle`/`readCycle`/`deleteCycle`: `"../../etc/x"`, absolute path, embedded null byte (`String.fromCharCode(0)`), `"..\\..\\"`.
- Assert: every write/read/delete stays within `cycles/`; `sanitizeSessionKey` neutralizes the sequence; nothing is written/read/deleted outside the state dir. Blast radius if unmitigated = arbitrary file write/delete under `$HOME` with the hook's privileges.

## Coverage requirement
Every new fs/readdir touchpoint has a failure-injection test (NFR-03 on all new paths); lifecycle dispatch keys exclusively on CYCLE_* (no lifecycle-event write/delete); all tracker paths route through the project-root walk; sanitization is internal and traversal-proof.
