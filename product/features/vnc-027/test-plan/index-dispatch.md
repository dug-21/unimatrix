# Test Plan — index-dispatch (`lib/hook-client/index.js`)

Component 3 / ADR-002 §4, ADR-004, ADR-006 / FR-14, FR-27, FR-30 / **AC-04, AC-08, AC-10, AC-12** /
Risks R-11 (High), R-14 (Med). Selects transport once per spawn from `config.mode`, injects `post` into
`queue.replay`; `null` request → immediate return; SessionClose delete removed; canonical-event flag to
`runFireAndForget`; `pruneOffsets` wired alongside `queue.prune`. `node --test` on `index.test.js`.

## Transport selection (FR-14, ADR-002 §4)

- `test_selects_uds_transport_when_mode_uds` — `config.mode==="uds"` → `transport-uds.post` chosen once and passed to `queue.replay`.
- `test_selects_http_transport_when_mode_http` — `mode==="http"` → `transport-http.post`, F3 behavior unchanged.
- `test_transport_selected_once_per_spawn` — exactly one selection; the same `post` flows to both the carrying send and `queue.replay`.
- `test_queue_replay_receives_injected_post` — `queue.replay(config, post)` is called with the selected transport (cross-transport replay works by construction — frames carry no transport state).

## Null-sentinel handling — AC-08 / R-11 (ADR-004)

- `test_null_request_returns_before_transport_selection` — a `null` request (non-cycle PreToolUse sentinel) → immediate return: no network, no stdout, no queue entry, exit 0 (R-11 s4). Transport selection is NOT reached.
- `test_null_request_no_config_resolve_side_effects` — assert no send-outcome breadcrumb is written for a null spawn.

## FR-16 rekey wiring — AC-10 / AC-12 / R-14 (ADR-006)

- `test_sessionclose_delete_removed` — a `Stop`→SessionClose successful FNF does NOT delete the offset file (the removed per-turn delete; assertable negative, ADR-006).
- `test_canonical_event_flag_passed_to_fire_and_forget` — `runFireAndForget` receives the canonical event name (or `isTaskCompleted` flag) from index.js, NOT the frame type (Stop and TaskCompleted both build SessionClose frames).
- `test_taskcompleted_event_deletes_offset_after_successful_send` — a spawn whose canonical event is `TaskCompleted` deletes the offset after a successful carrying send (the retained-but-unreachable branch, proven functional).
- `test_pruneoffsets_wired_alongside_queue_prune` — `pruneOffsets(config.stateDir)` runs at the top of `runFireAndForget` next to `queue.prune` (R-14 s2).
- `test_pruneoffsets_fnf_path_only` — `pruneOffsets` runs on the FNF path only; the sync trio gains NO file I/O (NFR-4).
- `test_pruneoffsets_fail_open` — unreadable dir / ENOENT → spawn proceeds, no throw (R-14 s2).

## Fail-open dispatch invariants (NFR-3)

- `test_exit_zero_on_all_paths` — sync, FNF, null, connect-failure all exit 0; never throw to host.
- `test_no_stdout_on_failure` — no stdout on any failure path; sync stdout only when `ok && status===200 && text/plain && body.length>0`.

## Integration cross-references
- AC-04 cross-transport replay (both directions) — parity-corpus-uds.md.
- AC-10 multi-turn offset persistence — state-offset-rekey.md + Layer 2.
- AC-12 full F3 delta suite green with only delete-timing assertion changes — state-offset-rekey.md.

## Edge cases
- Stale settings.json firing PreToolUse `*` for an ordinary tool → null sentinel → clean no-op (R-11 s5).
- Replay-before-send order preserved per transport; a failed replay leaves the frame queued (best-effort, FR-26).
