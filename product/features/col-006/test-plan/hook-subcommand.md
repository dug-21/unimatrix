# Test Plan: hook-subcommand

## Risks Covered

| Risk | Severity | Test Coverage |
|------|----------|--------------|
| R-06 | High | Latency budget (< 50ms Ping round-trip) |
| R-12 | Medium | Stdin JSON parsing failures and graceful degradation |
| R-18 | High | No tokio runtime in hook path |
| R-20 | Low | cortical-implant bootstrap idempotency |

## Unit Tests

Location: `crates/unimatrix-server/src/hook.rs` (within `#[cfg(test)]` module)

### Request Building

1. **test_build_request_session_start**: `build_request("SessionStart", &input)` with `session_id: Some("abc")` and `cwd: Some("/tmp")` -> `HookRequest::SessionRegister { session_id: "abc", cwd: "/tmp", .. }`.

2. **test_build_request_stop**: `build_request("Stop", &input)` -> `HookRequest::SessionClose { .. }`.

3. **test_build_request_ping**: `build_request("Ping", &input)` -> `HookRequest::Ping`.

4. **test_build_request_unknown_event**: `build_request("UserPromptSubmit", &input)` -> `HookRequest::RecordEvent(ImplantEvent { event_type: "UserPromptSubmit", .. })`.

5. **test_build_request_session_id_fallback**: Input with `session_id: None` -> generated session_id starts with `"ppid-"`.

### CWD Resolution

6. **test_resolve_cwd_from_project_dir**: `resolve_cwd(input, Some("/override"))` -> `/override`.

7. **test_resolve_cwd_from_stdin**: `resolve_cwd(input_with_cwd("/from/stdin"), None)` -> `/from/stdin`.

8. **test_resolve_cwd_fallback_to_process**: `resolve_cwd(input_without_cwd, None)` -> current process cwd.

### Parse Failures

9. **test_parse_hook_input_empty_string**: `parse_hook_input("")` -> returns default `HookInput` (graceful degradation, not panic).

10. **test_parse_hook_input_invalid_json**: `parse_hook_input("{broken")` -> returns default `HookInput`.

11. **test_parse_hook_input_valid_minimal**: `parse_hook_input(r#"{"hook_event_name":"Ping"}"#)` -> `HookInput` with `hook_event_name: "Ping"`, rest `None`.

### Bootstrap (R-20)

12. **test_cortical_implant_bootstrap**: After `bootstrap_defaults()`, resolve `cortical-implant`. Verify trust level is `Internal`. Verify capabilities include `Read` and `Search` but not `Write` or `Admin`.

13. **test_cortical_implant_bootstrap_idempotent**: Call `bootstrap_defaults()` twice. Verify `cortical-implant` agent's `enrolled_at` is unchanged on second call.

## Integration Tests

Location: `crates/unimatrix-server/tests/hook_test.rs`

### Subprocess Execution

14. **test_hook_ping_pong_subprocess**: Start server with UDS listener. Spawn `unimatrix-server hook Ping` as subprocess with stdin pipe. Verify stdout contains Pong JSON. Verify exit code 0.

15. **test_hook_session_start_subprocess**: Pipe `{"hook_event_name":"SessionStart","session_id":"test-1","cwd":"/tmp"}` to stdin. Verify exit code 0. Verify no stderr errors.

16. **test_hook_stop_subprocess**: Pipe Stop JSON to hook subprocess. Verify exit code 0.

17. **test_hook_no_server_exits_0**: No server running. Spawn hook subprocess. Verify exit code 0.

18. **test_hook_no_server_queues_event**: No server running. Run hook SessionStart. Verify queue file created in `~/.unimatrix/{hash}/event-queue/`.

19. **test_hook_empty_stdin_exits_0**: Spawn hook subprocess with empty stdin. Verify exit code 0. Verify no panic.

### End-to-End (AC-13)

20. **test_session_start_stop_round_trip**: Start server. Hook SessionStart -> Ack. Hook Stop -> Ack. Both exit 0. Server logs confirm both events.

### Latency Benchmark (R-06, AC-04)

21. **test_ping_pong_latency** (`#[ignore]`): Start server. Spawn `unimatrix-server hook Ping` 10 times. Measure wall-clock time from process start to exit. Assert p95 < 50ms.

### No Tokio Verification (R-18)

22. **test_hook_no_tokio_imports**: Static analysis check. Verify `hook.rs` does not contain `tokio::` imports. Verify `main.rs` branches to hook path before `#[tokio::main]`. This is a code review check, not a runtime test. Can be implemented as a grep-based test.

## Edge Cases

- Stdin larger than expected (>1 MB of JSON) -> reads all, serde parses
- Hook invoked without stdin pipe (terminal input) -> `read_to_string` blocks; this is a user error. Document that hooks are invoked by Claude Code, not manually.
- Hook invoked with binary data on stdin -> parse fails, graceful degradation
- Project directory does not exist -> `compute_project_hash` on current dir fallback
- Socket file exists but is not a socket (regular file) -> connect fails, graceful degradation

## Assertions

- Exit code is always 0 (except truly unexpected panics)
- Stdout contains valid JSON response for synchronous hooks (Ping)
- Stdout is empty for fire-and-forget hooks (SessionStart, Stop)
- No stderr output on successful execution (or only diagnostic traces)
- Queue file exists after degraded fire-and-forget
- Latency < 50ms for Ping/Pong round-trip (p95)
- `build_request` produces correct `HookRequest` variant for each event type
