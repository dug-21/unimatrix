# Security Review: 1028-security-reviewer

## Risk Level: low

## Summary
The change set is a hardening fix. It adds no new input surface, trust boundary, dependency or secret. A failed log write can no longer panic or abort the process, so graceful shutdown persistence and destructors (PidGuard, socket guards) now run. The four findings below are low severity: one is informational and three are test-harness hygiene. None blocks merge.

## Verification
- Read the full `git diff main...HEAD`: 5 files, +156/-3.
- `tracing_subscriber::fmt()` builders in non-test code: exactly 5, all in main.rs (812/826/836/1628/2079), and all set `log_internal_errors(false)`. There is one `set_hook` in the crate (main.rs:544).
- `log_internal_errors` exists in tracing-subscriber 0.3.22 (Cargo.lock). No dependency changes.
- `cargo check --bin unimatrix`: clean.
- `infra/shutdown.rs` and `infra/pidfile.rs` contain no `eprintln!` or `println!`, so no other panicking print is on the shutdown path.
- The panic hook keeps the #733 contract: it writes to stderr, does not route through tracing, keeps the same format and does not call `abort()`.

## OWASP / Trust Boundary
| Check | Result |
|---|---|
| Input validation | No new inputs. Unchanged. |
| Injection / path traversal | None. The test builds its paths from `tmp_path`. |
| Deserialization | None. |
| Error handling | Improved. A write error on the log sink is now dropped instead of turning into a panic and then an abort. |
| Access control | Unchanged. |
| Dependencies | None added. |
| Secrets | None. |
| Audit/integrity | The audit trail is a DB hash chain, not tracing. Muting internal log errors does not affect it. |

## Findings

### F1: Log-write failures are now silent
- **Severity**: low (informational)
- **Location**: crates/unimatrix-server/src/main.rs:813, 827, 837, 1629, 2080
- **Description**: With `log_internal_errors(false)`, a failing sink (ENOSPC on the daemon log file, EPIPE on stderr) drops events without any signal. That costs some forensic visibility, for example log loss while the disk is full. Before this change the process aborted in the same situation, and the fallback wrote to the same dead sink anyway, so there was no usable signal before either.
- **Recommendation**: None for this PR.
- **Blocking**: no

### F2: Test helper leaks the server process on non-timeout exceptions
- **Severity**: low (test hygiene)
- **Location**: product/test/infra-001/suites/test_lifecycle.py, `_run_shutdown_with_closed_stderr`
- **Description**: Only `subprocess.TimeoutExpired` triggers `client.shutdown()`. If `initialize`, `wait_until_ready` (ServerDied or TimeoutError) or `context_store` raises, the server is left running with no stderr drain. It can then block on a full stderr pipe and orphan. That is the same class of debris as lesson #5895.
- **Recommendation**: Use `try/finally` (or `except BaseException`) and call `client.shutdown()`. `shutdown()` already returns early if the process has exited.
- **Blocking**: no

### F3: `RLIMIT_CORE` hard limit lowered for the whole pytest process
- **Severity**: low (test hygiene)
- **Location**: test_lifecycle.py, `resource.setrlimit(resource.RLIMIT_CORE, (0, 0))`
- **Description**: This sets the hard limit to 0, which an unprivileged process cannot raise again. Every later test and child in the same pytest session loses core dumps. It does no harm and arguably helps (#5895), but it is a hidden global side effect.
- **Recommendation**: Optionally set only the soft limit and restore it afterwards, or pass `preexec_fn` through the harness. Either change keeps the effect to this one child.
- **Blocking**: no

### F4: Undrained stderr when `drain_stderr=False`
- **Severity**: low (test robustness)
- **Location**: product/test/infra-001/harness/client.py, `_wait_until_ready_inline`
- **Description**: After the ready marker, nothing reads stderr. If a future caller of `drain_stderr=False` makes the server log more than about 64KB before calling `close_stderr()`, the server blocks on its stderr write. The current test logs little, so it is not affected. The docstring states that this mode exists for the close-stderr flow.
- **Recommendation**: None needed now. Consider one line in the docstring warning that the mode is only for short close-stderr flows.
- **Blocking**: no

## Blast Radius Assessment
The change touches logging init in every serve mode and the bridge, plus the process-wide panic hook. Behavior differs only when a log write fails or a panic happens while stderr is dead. Worst case if there is a subtle bug: the panic hook's own diagnostic line is lost when stderr is broken. That fails safe, because the panic still unwinds normally. With the default `panic = "unwind"` (no profile sets `abort`), no data-corruption path is introduced. The fix removes an existing integrity loss: shutdown no longer skips the vector dump, adaptation-state save, compaction and the guard destructors.

## Regression Risk
Low. When stderr is healthy, logging output is unchanged. Existing harness callers keep the default `drain_stderr=True`, and their code path is byte-identical apart from the extra `if`. The new `close_stdin`, `close_stdout` and `wait_exit` methods are additive.

## PR Comments
- Posted 1 review comment on PR #1048.
- Blocking findings: no

## Knowledge Stewardship
- Stored: nothing novel to store. The log_internal_errors/panic-hook lesson is already earmarked post-merge (#733 correction plus a new pattern), and the test hygiene notes are specific to this PR.

Unimatrix entries consulted: #733, #5895, #4600, #4601
