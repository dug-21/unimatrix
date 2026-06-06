# Test Plan: index.js (entry / dispatch)

Oracle: `hook.rs::run`, `read_stdin`, `parse_hook_input`, `resolve_cwd`. Risks: R-14 (Critical),
R-13, AC-02/AC-03/AC-08/AC-09/AC-13. Suite: `test/hook-client/index.test.js` + spawn-level tests
that run the real entry via `child_process` with controlled stdin/env.

## Unit / Spawn Tests

### Stdin reading (FR-01, R-14 — runs on Linux AND macOS AND Windows CI runners)
- `test_stdin_piped_json_parses` — piped JSON via fd 0 → parsed HookInput.
- `test_stdin_empty_yields_empty_input` — empty stdin / EOF-immediately → empty HookInput, no throw, exit 0.
- `test_stdin_exactly_1mib_accepted` and `test_stdin_1mib_plus_1_capped` — cap behavior parity with Rust (assert built request reflects capped read; no throw).
- `test_stdin_malformed_json_defensive_parse` — `{not json` → empty HookInput, pipeline continues (normalize + dispatch still run), exit 0.
- Explicit assertion the source uses `fs.readFileSync(0)` — grep-gate: `'/dev/stdin'` MUST NOT appear in `lib/hook-client/` (closed gate-note 1; obligation is Windows execution, not the mechanism).
- `test_windows_spawn_produces_post` — R-14 smoke: on the Windows runner, a full spawn with piped stdin + stub server receives exactly one POST.

### Dispatch split (mirror of `hook.rs:244-251`)
- `test_dispatch_fnf_set` — SessionRegister/SessionClose/RecordEvent/RecordEvents → fire-and-forget path (no `Accept: text/plain`, no stdout ever).
- `test_dispatch_sync_set` — ContextSearch/CompactPayload/Ping → sync path (`Accept: text/plain`, stdout only on 200 text body).
- `test_argv_event_passed_to_normalize` — `argv[2]` event name reaches normalize; missing argv event → defensive (generic observation or exit 0 per pseudocode; assert no throw).

### Exit-0 / no-stdout guarantee (FR-05, C-05)
- `test_exit0_matrix` — spawn the real entry under each induced failure: malformed stdin, missing config, ECONNREFUSED, timeout, 401, 500, unwritable state dir, throwing transcript path. Assert exit code 0 AND stdout is byte-empty in every cell; stderr carries exactly one `unimatrix: <class>: <message>` line when a send was attempted.
- `test_stdout_purity` — no test cell ever sees a stray byte on stdout (canary for `console.log` leakage; integration-risk row "client ↔ host CLI").

### Sync-path isolation (AC-08 amended wording, R-13)
- `test_sync_no_delta_io` — sync events with non-empty `transcript_path`: fs spy asserts NO stat/span-read/offset persistence and NO `queue/` I/O. Exactly one POST.
- `test_subagentstart_tail_read_exempt` — SubagentStart performs the single RQ-6 12 KB tail read (query derivation) and nothing else; still one POST.

### Pipeline ordering (FNF path)
- `test_fnf_order_replay_then_event_then_delta` — stub-server request log shows queued replays before the carrying event; delta POST issued concurrently with carrying event (`Promise.allSettled` — see delta.md for independence).

## Benchmark (AC-13, R-13)

- Harness ≥50 iterations + warmup (ass-068 Q1 method), server stubbed, measuring entry + parse +
  build + transform + state-dir hash + root walk + health.json write. Targets p50 ≤ ~12 ms,
  p95 ≤ 20 ms. Results committed to `product/features/vnc-026/testing/` (shell-verified AC).

## ass-071 freebie (advisory, no assertion)

When the spawn-level suite drives a SubagentStop event, dump the raw stdin payload to
`product/features/vnc-026/testing/subagentstop-stdin-dump.json`. Feeds ass-071/crt-052.

## Edge Cases

- `cwd` resolution: stdin `cwd` non-empty wins over `process.cwd()`; empty stdin `cwd` falls back (feeds config.md split-brain test).
- Unknown event name → `__unknown__` handling produces generic observation (raw name preserved) — corpus-covered, asserted here at dispatch level.
- Top-level synchronous throw injected into any pipeline stage → caught, exit 0, breadcrumb best-effort.
