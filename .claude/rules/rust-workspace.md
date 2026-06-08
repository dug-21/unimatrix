---
paths:
  - "**/*.rs"
  - "**/Cargo.toml"
---

# Rust Workspace


## Build Commands

```bash
# Build: first error + summary (truncate to prevent context bloat)
cargo build --workspace 2>&1 | grep -A5 "^error" | head -20
cargo build --workspace 2>&1 | tail -3

# Test: hardened workspace run (see "Hardened cargo test convention" below)
# CARGO_TEST_TIMEOUT_SECS: hard ceiling so an interrupted run cannot orphan cargo children.
# `setsid -w` is REQUIRED: without -w, setsid returns its own fork status (0) instead of
# the inner command's exit code, producing false-green gates (GH#709). rc=124 = killed at ceiling.
setsid -w timeout "${CARGO_TEST_TIMEOUT_SECS:-600}" cargo test --workspace > /tmp/uni-test.$$.log 2>&1; rc=$?; tail -30 /tmp/uni-test.$$.log; rm -f /tmp/uni-test.$$.log; exit $rc

# Clippy: first warnings only
cargo clippy --workspace -- -D warnings 2>&1 | head -30
```

## Hardened cargo test convention (canonical — copy byte-identical)

`cargo test --workspace` MUST be invoked through this exact single-line form. It is
the single source of truth; every agent/protocol copy must match it byte-for-byte so
the regression lint (`product/test/infra-002/check-cargo-test-convention.sh`) passes.

```bash
# CARGO_TEST_TIMEOUT_SECS: hard ceiling so an interrupted run cannot orphan cargo children.
# `setsid -w` is REQUIRED: without -w, setsid returns its own fork status (0) instead of
# the inner command's exit code, producing false-green gates (GH#709). rc=124 = killed at ceiling.
setsid -w timeout "${CARGO_TEST_TIMEOUT_SECS:-600}" cargo test --workspace > /tmp/uni-test.$$.log 2>&1; rc=$?; tail -30 /tmp/uni-test.$$.log; rm -f /tmp/uni-test.$$.log; exit $rc
```

Why each piece exists (do NOT rewrite it as a pipeline — `cargo test ... | tail`):

- **`setsid -w`** runs `cargo` (and its `rustc`/test-binary descendants) in a NEW session
  and process group. If the Bash tool call is interrupted/timed-out, the harness can
  signal the whole group instead of orphaning the cargo subtree to PID 1 (root cause
  of #122 — orphans hold `target/.cargo-lock` and test `.db` handles, hanging later runs).
  The **`-w` flag is mandatory**: bare `setsid` forks and returns the *fork's* status
  (always 0), so `rc=$?` would capture 0 even when `cargo test` FAILS or is killed at the
  ceiling — a silent false-green that INVERTS the gate-integrity guarantee #122 exists to
  protect (GH#709). `-w` makes `setsid` wait for the child and propagate its real exit code.
- **`timeout` + named constant** `CARGO_TEST_TIMEOUT_SECS` (default **600s**, NOT a bare
  magic number) imposes a hard ceiling and kills the entire child tree on expiry. 600s
  fits a cold full-workspace build+test; integration-heavy crates may approach it on a
  cold cache. Tune in ONE place via the env var.
- **`> /tmp/uni-test.$$.log 2>&1`** writes to a PID-namespaced file, NOT a live pipe.
  A pipe makes `cargo` the upstream writer of a pipeline the harness cannot kill as a
  unit; a file removes that orphan-writer problem while preserving output truncation.
- **EXIT-CODE GUARD (load-bearing — gate integrity):** the capture+report is a SINGLE
  NON-PIPELINED statement. `rc=$?` captures the status of `cargo test` (via `timeout`),
  NOT `tail`. NEVER rewrite this as `... | tail` and NEVER background it (`&`) — either
  makes `$rc` the wrong process's status and gates read FALSE-GREEN. `tail` runs AFTER
  `rc` is captured, so it cannot clobber the result; `exit $rc` propagates the real status.
- **`rm -f /tmp/uni-test.$$.log`** cleans the log on every success/failure path before
  `exit`. An interrupted run may leave at most one PID-stamped file (acceptable); files
  do not accumulate per successful run.

### rc=124 means KILLED at the ceiling — not a test failure

`timeout` returns exit code **124** when it kills the run at `CARGO_TEST_TIMEOUT_SECS`.
Treat **124 as "the run was killed for hanging" (investigate the hang)** — it is FAIL,
but it is NOT an ordinary parseable red suite and MUST NOT be auto-retried as if a test
flaked. On 124: investigate the hang (likely an orphan/lock from a prior run), then
re-run per-crate (`cargo test -p {crate} --lib`) rather than blindly retrying `--workspace`.

## Naming Conventions

| Element | Convention | Example |
|---------|------------|---------|
| Modules | snake_case | `http_polling_source.rs` |
| Structs | PascalCase | `HttpPollingSource` |
| Functions | snake_case | `fetch_data()` |
| Constants | SCREAMING_SNAKE | `DEFAULT_TIMEOUT` |
| Traits | PascalCase | `ResponseParser` |

## Code Quality

- `cargo fmt` before commit
- `cargo clippy` — no warnings
- Error handling uses the project error type with `.map_err()` context
- Logging uses `tracing` macros (info!, error!, debug!)
- No `.unwrap()` in non-test code
- No hardcoded secrets (use env vars)
- **Max 500 lines per file** — split into modules when approaching this limit. Focused, single-responsibility files over monolithic ones.

## Dependencies

- Minimal dependency footprint — prefer std where possible
- `cargo audit` must pass before merge (no known CVEs)
- Pin major versions for security-sensitive crates (serde, bincode, redb)
- No `unsafe` in dependencies without explicit review and ADR justification
