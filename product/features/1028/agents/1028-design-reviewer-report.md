# Design Review: 1028-design-reviewer

**Assessment: APPROVED WITH NOTES**

The diagnosis holds. I checked it against tracing-subscriber 0.3.22 `fmt_layer.rs:1005-1018`, the `main.rs` panic hook (543-556), all five `fmt()` builders (810/823/832/1623/2073) and `infra/shutdown.rs`.

## Findings

| # | Severity | Finding |
|---|---|---|
| F1 | High (constraint) | **`log_internal_errors(false)` does the actual fixing. The hook change is defense in depth.** With only the hook change, the panic from `eprintln!` in `on_event` no longer aborts, but it still unwinds out of `graceful_shutdown` and `main`. Exit is 101 and the vector dump, adaptation save and compact are still skipped. "Either change alone fixes it" is true for SIGABRT only, not for data loss. Ship both. The test must assert that persistence happened, not just that the exit code is not -6. |
| F2 | Medium (test design) | **Closing stderr is unreliable while `UnimatrixClient._drain_stderr` is blocked in `readline`.** In CPython, `close()` on a BufferedReader waits for the lock held by the blocked read. A raw `os.close(fd)` also fails: the blocked `read()` still holds a reference to the pipe, so the server never gets EPIPE. Required approach: extend the harness, for example `UnimatrixClient(..., drain_stderr=False)`. When it is false, `wait_until_ready()` reads stderr on the calling thread up to the ready marker, using `select` with a timeout, and a new `close_stderr()` then closes it. No other thread ever owns the pipe. Do not hand-roll a separate Popen in the test file, because test infrastructure is meant to be cumulative. |
| F3 | Medium (test design) | Red before green: the new test must fail on main (rc -6, no `unimatrix-vector.meta`). Leave the default `RUST_LOG=info`, because the bug needs at least one shutdown event to pass the filter (lesson #4601: test with the production filter). Optionally add a second case that also closes the stdout read end, which matches a real client death more closely. |
| F4 | Low | The hook (#733) must keep its existing contract: write to stderr and not to tracing, do not call `abort()`, and keep the format `PANIC at {location}: {payload}`. Use `let _ = writeln!(std::io::stderr(), ...)`. Pattern #733 currently says "Write to eprintln!", which is the defect. Correct #733 after merge. |
| F5 | Low | Turning off `log_internal_errors` loses nothing operators use. Every builder's fallback writes to the same sink that just failed. On the daemon file writer (`daemon.rs:93`), stderr is the same log file, so on ENOSPC the fallback fails too. The other side effect is that FormatEvent-internal errors are no longer written. Those only happen on a formatter bug, and the event is lost anyway. Keep the #4600 `Mutex<LineWriter<File>>` writer unchanged. No custom MakeWriter is needed. |
| F6 | Info | Other panic paths: none on the shutdown path. The remaining `eprintln!`/`println!` calls run at startup or in sync subcommands only (`main.rs:819`, `http/token.rs:171` first-boot notice, `health.rs`, `export.rs`, and others). The only other `eprintln!` in tracing-subscriber is `fmt_layer.rs:846` (on_new_span field formatting into an in-memory buffer, not I/O). F4 makes it non-aborting. There are no non-fmt layers and no `registry().with(...)`. `panic = "abort"` is not set in any profile. |
| F7 | Info | Hot path: `log_internal_errors` adds one bool check per event and the hook is never hot. Neither costs anything measurable. |
| F8 | Info | Blast radius: logging init in every serve mode plus the bridge, and the process-wide panic hook. Behaviour changes only when a log write fails. The hunks are 100+ lines away from #1027 (~666/690), so the merge-conflict risk is low. |
| F9 | None | Security: no new input, auth or trust surface. The audit trail is a DB hash chain, not tracing, so it is unaffected. The fix stops the abort skipping destructors (PidGuard, socket guards), so the stale PID and socket files left behind today go away. |

## Shutdown ordering (persistence before logging)
Out of scope; do not reorder. Once a failed log write can no longer panic, ordering has nothing to do with correctness. The log lines placed before each step are what operators use to find which step hung or failed. No follow-up issue needed.

## Revised approach
No change to the proposal. Add these constraints:
1. Panic hook: `let _ = writeln!(std::io::stderr(), "PANIC at {location}: {payload}");`.
2. `.log_internal_errors(false)` on all 5 `fmt()` builders (810/823/832/1623/2073).
3. Test in `test_lifecycle.py`. Use the harness extension from F2 (`drain_stderr=False` + `close_stderr()`). The test must fail on main. Assert `returncode == 0` and that `vector/unimatrix-vector.meta` exists. Optionally restart and confirm the entry can be found.
4. After merge: correct pattern #733 (non-panicking stderr write) and store a pattern: "all tracing fmt subscribers set log_internal_errors(false); fallback eprintln! to a dead sink panics".

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing. Relevant: #733 (panic hook pattern, the source of the `eprintln!`), #4600 (daemon LineWriter + stderr fallback), #4601 (verify with the production RUST_LOG), #5895 (mechanism lesson from #1003).
- Stored: nothing novel to store. #5895 already records the mechanism. The #733 correction and the new log_internal_errors pattern are deferred until the fix is verified (F4, approach item 4). No ADR: a bugfix inside existing patterns, with no architectural decision changed.

Unimatrix entries consulted: #733, #4600, #4601, #5895
