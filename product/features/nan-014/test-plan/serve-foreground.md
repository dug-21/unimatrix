# Test Plan: serve-foreground

## Component

`crates/unimatrix-server/src/main.rs` -- adds `foreground: bool` field to `Command::Serve` variant. When true, calls `tokio_main_daemon` directly (no launcher, no child spawn, no setsid).

## Risk Coverage

| Risk | Scenario | Test |
|------|----------|------|
| R-01 (High) | --foreground breaks existing --daemon path | `test_serve_daemon_flag_unchanged` + existing daemon tests |
| R-01 (High) | --foreground calls tokio_main_daemon directly | `test_serve_foreground_parsed` |
| R-01 (High) | foreground field defaults to false | `test_serve_bare_defaults_foreground_false` |
| R-05 (High) | SIGTERM as PID 1 | Shell: `docker stop` produces graceful shutdown logs |
| R-06 (Med) | Volume permission error | Unit: error message names path and required ownership |
| R-11 (Low) | --foreground + --daemon mutual exclusion | `test_foreground_conflicts_with_daemon` |
| R-11 (Low) | --foreground + --stdio mutual exclusion | `test_foreground_conflicts_with_stdio` |

## Unit Tests (Clap Parsing)

Located in `crates/unimatrix-server/src/main_tests.rs`.

### test_serve_foreground_parsed

**Arrange**: Parse `["unimatrix", "serve", "--foreground"]`.

**Act**: `Cli::try_parse_from(...)`.

**Assert**:
- `cli.command` matches `Some(Command::Serve { foreground: true, daemon: false, stdio: false })`

### test_serve_bare_defaults_foreground_false

**Arrange**: Parse `["unimatrix", "serve"]`.

**Act**: `Cli::try_parse_from(...)`.

**Assert**:
- `cli.command` matches `Some(Command::Serve { foreground: false, daemon: false, stdio: false })`

### test_serve_daemon_flag_unchanged

**Arrange**: Parse `["unimatrix", "serve", "--daemon"]`.

**Act**: `Cli::try_parse_from(...)`.

**Assert**:
- `cli.command` matches `Some(Command::Serve { daemon: true, foreground: false, stdio: false })`
- This test already exists in essence (`test_serve_daemon_flag_parsed` pattern). Verify it still passes unchanged after adding the `foreground` field. ANY modification to this test is a regression signal per R-01.

### test_foreground_conflicts_with_daemon

**Arrange**: Parse `["unimatrix", "serve", "--foreground", "--daemon"]`.

**Act**: `Cli::try_parse_from(...)`.

**Assert**:
- Returns `Err` (clap rejects conflicting flags)
- Error message mentions both flags

### test_foreground_conflicts_with_stdio

**Arrange**: Parse `["unimatrix", "serve", "--foreground", "--stdio"]`.

**Act**: `Cli::try_parse_from(...)`.

**Assert**:
- Returns `Err` (clap rejects conflicting flags)

## Unit Tests (Dispatch Logic)

### test_serve_foreground_dispatch_skips_setsid

This is a design-level verification, not easily unit-tested in isolation. The guarantee is architectural: the `--foreground` match arm calls `tokio_main_daemon(cli)` directly, without calling `prepare_daemon_child()`. Verified by code review and by the fact that `--foreground` dispatch arm does NOT contain `prepare_daemon_child`.

The existing daemon tests (which DO call `prepare_daemon_child`) serve as the regression gate: if `--foreground` accidentally enters the daemon-child path, daemon tests would still pass but foreground-specific behavior (no setsid) would be wrong. The container-level test (AC-06) catches this.

## Shell/Container Tests

### AC-06: Foreground mode as PID 1

**Arrange**: Build image, run container with `serve --foreground`.

**Act**: `docker top <container>` to verify PID 1.

**Assert**:
- PID column shows `1` for the `unimatrix` process
- No child processes spawned (single process, single PID)

### R-05: SIGTERM handling as PID 1

**Arrange**: Start container in foreground mode.

**Act**: `docker stop <container>` (sends SIGTERM, waits `stop_grace_period`).

**Assert**:
- `docker logs <container>` shows graceful shutdown messages (vector dump, DB compaction, PidGuard release)
- Container exit code is 0 (clean shutdown, not SIGKILL)

## Integration Tests

No new infra-001 tests. The infra-001 harness exercises the binary in stdio mode, which already calls `tokio_main_daemon` -- the same function `--foreground` calls. The existing `protocol`, `tools`, and `lifecycle` suites validate that `tokio_main_daemon` behavior is unchanged.

## Edge Cases

- **`serve` with no flags (bare serve)**: Should dispatch to stdio mode (existing behavior). `foreground` defaults to `false`.
- **`--foreground --project-dir /data`**: Both flags combined correctly. ProjectPaths resolves from `/data`.
- **Double SIGTERM**: Sending SIGTERM twice should not cause a panic. The `shutdown_signal()` handler is idempotent.
