# Gate Report: bugfix-381 (Bug Fix Validation)

> Gate: bugfix-gate (381-gate-bugfix)
> Date: 2026-03-25
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Both root causes addressed | FAIL | LP-1, LP-2, LP-3 use `debug!` not `info!` — invisible at default level; defeats purpose of the fix |
| No todo!/unimplemented!/TODO/FIXME introduced | PASS | Pre-existing TODO comments in main.rs, none introduced by this commit |
| All tests pass | PASS | 2047 unit tests pass, 0 failures |
| No new clippy warnings in changed files | PASS | No warnings from main.rs or listener.rs |
| No unsafe code introduced | PASS | No unsafe blocks in diff |
| Fix is minimal — no unrelated changes | WARN | Protocol and skill files (.claude/protocols/uni/, .claude/skills/) bundled in fix commit |
| No new behavioral tests required — confirmed acceptable | PASS | debug! additions have no behavioral impact; acceptance criteria states no test changes required |
| No xfail markers added | PASS | No xfail changes |
| Investigator report has Knowledge Stewardship block | PASS | Present with Queried + Stored entries |
| Rust-dev (agent-1-fix) report has Knowledge Stewardship block | PASS | Present with Queried + Stored entries |
| Goal-absent log (LP-4b) present | FAIL | Investigator and GH Issue both required this; not implemented |
| Acceptance criteria satisfied | FAIL | AC-1 explicitly requires `RUST_LOG=info` default visibility; `debug!` fails this |

---

## Detailed Findings

### Check 1: Both Root Causes Addressed

**Status**: FAIL

**Evidence — Root Cause 2 (RUST_LOG) — PASS**:
The `EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level))` fix is correctly applied at all three `tokio_main_*` sites in `main.rs`. Comments added at each site. This part is correct.

**Evidence — Root Cause 1 (Missing log points) — FAIL**:
The GH Issue title is "obs: UDS injection visibility — query, injection content, SubagentStart context at **INFO level**". The issue body states explicitly:

> All visibility must be at `INFO` log level (default). No debug logging required to observe normal operation.

Acceptance Criterion #1:
> Running with `RUST_LOG=info` (default) shows query text, result count, and routing decision for every SubagentStart event

All three new log points in `listener.rs` use `tracing::debug!`:
- LP-3 (SubagentStart received): `tracing::debug!` — invisible at INFO
- LP-1 (ContextSearch executed): `tracing::debug!` — invisible at INFO
- LP-2 (injecting entries): `tracing::debug!` — invisible at INFO

LP-4 (goal-present branch, existing `debug!`) gained `target: "unimatrix_server::obs"` but remains `debug!` — the investigator's Step 3 proposed promoting it to `info!`; this was not done.

At the default runtime level (no `RUST_LOG` set → filter is `info`), none of the four observation log points will fire. The RUST_LOG fix is working correctly but the log points it is supposed to expose are at the wrong level. The combined effect: the server still shows no observation logs under normal operation, which is exactly the reported symptom.

**Issue**: Change LP-1, LP-2, LP-3 from `tracing::debug!` to `tracing::info!`. Promote LP-4 from `debug!` to `info!` (or confirm the approved spec allows keeping LP-4 at `debug!` given the `target:` addition, with documented rationale).

The approved design reviewer spec (`381-design-reviewer-report.md`) also used `tracing::info!` in all revised code examples. The rust-dev report documents `debug!` without acknowledging the deviation.

---

### Check 2: Goal-Absent Log (LP-4b) Missing

**Status**: FAIL

**Evidence**: The investigator's Step 3 explicitly specifies adding an `else` branch after the `if let Some(ref goal_text) = maybe_goal` block:

```rust
} else {
    tracing::info!(target: "unimatrix_server::obs", session_id = ?session_id,
        "col-025: SubagentStart goal-absent — falling through to ContextSearch");
}
```

The GH Issue body also specifies this companion log for the goal-absent path: "Plus add companion for goal-absent path (currently no log at any level)."

Reading `listener.rs` lines 1024–1026 confirms no `else` branch was added — only the comment `// goal absent or empty → fall through to existing ContextSearch dispatch`.

The rust-dev report lists "Four log points in listener.rs" but the description only accounts for LP-3, LP-4 (target: addition), LP-1, LP-2 — the goal-absent log (LP-4b) is absent from the report and from the code.

**Issue**: Add `else` branch after `if let Some(ref goal_text) = maybe_goal` at line 1024 with the goal-absent `tracing::info!` log.

---

### Check 3: Unrelated Files in Fix Commit

**Status**: WARN

**Evidence**: `git diff HEAD~1 --stat` shows 5 non-code files changed in the same commit as the bug fix:
- `.claude/protocols/uni/uni-bugfix-protocol.md`
- `.claude/protocols/uni/uni-delivery-protocol.md`
- `.claude/protocols/uni/uni-design-protocol.md`
- `.claude/skills/uni-init/SKILL.md`
- `.claude/skills/uni-retro/SKILL.md`

These are protocol and skill file updates that have no relationship to the UDS logging bug. Bundling them into the bug fix commit violates the principle of minimal, targeted fixes and makes the commit harder to bisect or revert in isolation.

This is a WARN rather than FAIL because the fix commit (`d4ef9cb`) is already merged to the branch and the code changes are correct in isolation. The unrelated changes do not introduce any bugs.

---

### Check 4: No Placeholders or Stubs

**Status**: PASS

Two pre-existing `TODO(W2-4)` comments in `main.rs` at lines 614 and 1001 exist in `HEAD~1` — confirmed not introduced by this commit.

---

### Check 5: Tests Pass

**Status**: PASS

`cargo test -p unimatrix-server --lib`: 2047 passed, 0 failed.

---

### Check 6: No New Clippy Warnings in Changed Files

**Status**: PASS

`cargo clippy -p unimatrix-server` produces no errors or warnings attributed to `src/main.rs` or `src/uds/listener.rs`. Pre-existing collapsible-if errors in `unimatrix-engine/src/auth.rs` are unrelated.

---

### Check 7: Knowledge Stewardship

**Status**: PASS

- Investigator report (`381-investigator-report.md`): `## Knowledge Stewardship` present; Queried (2 entries) and Stored (entry #3453 lesson) documented.
- Rust-dev report (`381-agent-1-fix-report.md`): `## Knowledge Stewardship` present; Queried and Stored (entry #3457 pattern) documented.
- Verify-agent report (`381-agent-2-verify-report.md`): `## Knowledge Stewardship` present; Queried and "nothing novel" with reason documented.

---

## Rework Required

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| LP-1, LP-2, LP-3 use `debug!` — must be `info!` per GH Issue AC-1 and approved design | uni-rust-dev | Change `tracing::debug!` to `tracing::info!` for LP-1 (`UDS: ContextSearch executed`), LP-2 (`UDS: injecting entries`), LP-3 (`UDS: SubagentStart received`) in `listener.rs` |
| LP-4 goal-present branch not promoted to `info!` | uni-rust-dev | Promote existing `debug!` at line 964 to `info!` (or document explicit rationale for keeping it at `debug!` if that was intentional) |
| LP-4b goal-absent log missing entirely | uni-rust-dev | Add `else` branch after `if let Some(ref goal_text) = maybe_goal` block (after line 1024) with `tracing::info!(target: "unimatrix_server::obs", session_id = ?session_id, "col-025: SubagentStart goal-absent — falling through to ContextSearch")` |

---

## Knowledge Stewardship

- Stored: nothing novel to store -- the log-level deviation pattern (implementation using `debug!` when `info!` was specified) is a one-off issue for this fix, not a recurring systemic pattern warranting a lesson entry. The root cause pattern (tracing-subscriber EnvFilter) was already stored by the investigator as entry #3453.
