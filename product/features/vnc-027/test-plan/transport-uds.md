# Test Plan — transport-uds (`lib/hook-client/transport-uds.js`, NEW)

Component 1 / ADR-002, ADR-003 / FR-5..FR-11 / **AC-01, AC-03, AC-04, AC-05** / Risks R-01 (High), R-06 (High),
R-18 (Med), R-15 (Low), R-13 (Low). Contract: `post(config, frame, opts) -> Promise<SendResult>`, never rejects,
no stdout/stderr, no retry. `node --test`; oracle = `transport-http.js`. Stub listener via `net` for units;
live listener for the integration assertions (parity-corpus-uds.md).

## Framing — AC-01 / R-18 (`cargo wire.rs` is the byte authority)

- `test_frame_write_4byte_be_u32_prefix_plus_json` — TS-built frame byte-identical to committed Rust-generated framing fixtures (corpus layer a).
- `test_write_rejects_payload_over_1mib` — payload > 1,048,576 B → fail-open `{ok:false, status:0, failureClass:"http_4xx"}`, never sent, never thrown (R-18 s1).
- `test_write_accepts_exactly_1mib` — 1,048,576-byte payload accepted (boundary, matches wire.rs round-trip).
- `test_read_rejects_zero_declared_length` — declared length 0 → reject before allocating (`connect` class).
- `test_read_rejects_over_1mib_declared_length` — declared length 1,048,577 / 0xFFFFFFFF → reject BEFORE allocating the declared size (R-18 s2, hostile-prefix DoS guard).
- `test_read_accepts_exactly_1mib_response` — 1,048,576-byte response body accepted both directions.

## SendResult mapping — every ADR-002 §2 row (transport seam contract)

One unit per row (downstream `transform`/`state`/queue all key off these):
- `test_map_text_to_200_text_plain_buffer` — `Text{body}` → `{ok:true,200,"text/plain",Buffer(body),null}`.
- `test_map_ack_to_204_null` — sync empty injection `Ack` → `{ok:true,204,null,null,null}`.
- `test_map_fnf_flush_to_status_0` — FNF flushed (no read) → `{ok:true,0,null,null,null}` (non-HTTP status 0 pinned so breadcrumb consumers don't assume HTTP).
- `test_map_pong_to_200_application_json` — `Pong` → `{ok:true,200,"application/json",Buffer(json),null}`.
- `test_map_error_5xx_and_4xx` — `Error{code>=500}` → `failureClass:"http_5xx"`; `code<500` → `"http_4xx"`.
- `test_map_connect_failure` — ENOENT/ECONNREFUSED/EACCES → `{ok:false,0,...,"connect"}`.
- `test_map_deadline_timeout` — deadline exceeded → `failureClass:"timeout"`.
- `test_no_new_failureclass_values` — only the F3 set {auth,connect,timeout,http_4xx,http_5xx} ever appears.

## Socket lifecycle — ADR-003 / R-01 (FNF) & R-06 (sync)

- `test_fnf_uses_socket_end_not_destroy_before_finish` — instrument the socket: `socket.end(frame)` is called; `destroy()` is NEVER invoked before the `'finish'` event (R-01 s3, the frame-loss guard). Resolve success on `'finish'`.
- `test_fnf_never_reads_response` — FNF path issues no read; server EPIPE on its Ack is not an error (R-01 s5).
- `test_fnf_flush_timeout_resolves_ok_false` — post-connect flush timeout → `ok:false` → enqueued (at-least-once, duplicate accepted) — never dropped (R-01 s4).
- `test_sync_accumulates_chunked_response` — stub writes the 4-byte header and body in 1-byte and split-header chunks; client accumulates to the full declared length (R-06 s1).
- `test_sync_end_before_complete_frame_fails_connect` — `'end'` before declared length satisfied → `connect` class, no stdout (R-06 s2).
- `test_sync_deadline_mid_read_destroys_and_timeout` — deadline expiry mid-read → `destroy()` + `timeout`, no partial stdout (R-06 s4).
- `test_settle_once_clears_all_timers` — every resolution path calls `done()` once; timers cleared (R-06 s5).
- `test_no_process_exit_in_module` — grep-gate: the module contains no `process.exit(` (R-06 s5, #4768 pattern — grep, not stdout spy).
- `test_all_timers_unref` — connect/read/write deadline timers are `unref()`d so they cannot hold the event loop open.

## Fail-open & timeouts (FR-9, FR-10, R-15)

- `test_never_rejects` — every failure path resolves a SendResult; the promise never rejects (fuzz a range of socket errors).
- `test_no_stdout_no_stderr_from_transport` — transport writes nothing to stdout/stderr on any path (NFR-3; ownership is index/transform).
- `test_timeout_constants_are_40ms` — connect/sync/fnf deadline constants = 40 ms (sourced from Rust `HOOK_TIMEOUT`), asserted as constants (not load-bearing for p95).

## Integration (live listener — detailed in parity-corpus-uds.md)
AC-03 round-trip + sync-trio stdout goldens; AC-04 FNF 1 MiB truncation contract; AC-05 p95 < 20 ms over a live local
socket including project-root detection (FNF and sync measured separately; FNF pays the `'finish'` flush wait).

## Edge cases
- Stale socket file, no listener → ECONNREFUSED → `connect` → enqueue. Socket dir absent → ENOENT → `connect`. EACCES → `connect`. No throw on any.
- Unserializable frame (circular/BigInt) → `http_4xx` client-side reject, never sent.
- Concurrent spawns: connection-per-frame means no shared socket state (asserted at the replay layer).
- No-daemon enqueue bounds (R-13): enqueue-only path stays within 500 files / 5 MiB / 24 h (asserted via queue + config in parity-corpus-uds.md / config-transport-selection.md).
