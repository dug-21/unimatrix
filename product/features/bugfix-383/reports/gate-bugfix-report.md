# Gate Bugfix Report: bugfix-383

> Gate: Bug Fix Validation
> Date: 2026-03-25
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Root cause addressed | PASS | All user-visible identifiers renamed; struct, rule_name, claim, recommendation, remediation text updated |
| No todo!/unimplemented!/FIXME placeholders | PASS | One intentional `// TODO(col-028):` comment on deferred field rename — documents deferred work, not a code placeholder |
| All tests pass (fix-specific + existing) | PASS | 422 observe tests pass, 2068 server tests pass in isolation; concurrent workspace run shows pre-existing flaky col018 tests (confirmed by stash test) |
| No new clippy warnings | PASS | Build completes with pre-existing warnings only |
| No unsafe code introduced | PASS | No unsafe blocks in changed files |
| Fix is minimal (no unrelated changes) | WARN | 15 additional `remediation_for_rule()` arms added for non-renamed rules — caught by the new contract test. In-scope assessment: ACCEPTABLE — the contract test required these arms to pass, and they fix a latent defect (all 22 rules fell through to generic fallback). Not an unrelated change. |
| New tests would have caught original bug | PASS | `test_default_rules_names` now asserts `orphaned_calls`; contract test asserts all 22 rules have non-fallback arms |
| Skill documentation updated | FAIL | `.claude/skills/uni-retro/SKILL.md` and `packages/unimatrix/skills/retro/SKILL.md` still reference `permission_retries` at line 39 — agents reading this skill get incorrect guidance |
| Knowledge stewardship — rust-dev | PASS | `383-agent-1-fix-report.md` in worktree has `## Knowledge Stewardship` with `Queried:` and `Stored:` entries |
| Knowledge stewardship — investigator | FAIL | No investigator report found anywhere; required by bugfix protocol and spawn prompt |

## Detailed Findings

### Root Cause Addressed
**Status**: PASS
**Evidence**: `OrphanedCallsRule` struct in `friction.rs` line 22; `fn name() -> "orphaned_calls"` line 26; claim text `"Tool '{tool}' had {retries} orphaned call(s) (Pre–terminal differential)"` line 82; `recommendation_for()` match arm on `"orphaned_calls"` in `report.rs` line 65; `remediation_for_rule()` match arm on `"orphaned_calls"` in `recurring_friction.rs` line 109. The full rename chain is complete in source code.

### No Placeholders
**Status**: PASS
**Evidence**: `// TODO(col-028): rename field to orphaned_call_events once metric migration is complete` at `metrics.rs:85` is a documentation comment referencing a separate tracked issue for a deferred SQLite migration. No `todo!()`, `unimplemented!()`, or `FIXME` markers present.

### Tests Pass
**Status**: PASS
**Evidence**:
- `cargo test -p unimatrix-observe`: 422 passed, 0 failed
- `cargo test -p unimatrix-server --lib`: 2068 passed, 0 failed (in isolation)
- Concurrent workspace run shows 2-3 `col018_*` failures that are pre-existing flaky tests: confirmed by stashing fix, running on main, and seeing them fail there too under concurrent load. The fix agent's report also documents this as pre-existing.

### No New Clippy Warnings
**Status**: PASS
**Evidence**: `cargo build --workspace` completes with "12 warnings (run `cargo fix` to apply 1 suggestion)" — pre-existing, not introduced by this fix.

### Fix Minimality — 15 Remediation Arms
**Status**: WARN
**Evidence**: The changed files include 15 new match arms in `remediation_for_rule()` for rules unrelated to the rename (e.g., `lifespan`, `file_breadth`, `session_timeout`, etc.). The fix agent's report explains these were discovered necessary to pass the new contract test `test_all_default_rules_have_non_fallback_recommendation_and_remediation`. This is a latent defect correction gated by the new test — not scope creep. The arms are low-risk string literals with no logic.

Assessment: WARN (not FAIL) — the additions are semantically coupled to the same module and directly required by the new contract test. They address a real quality gap.

### Skill Documentation Not Updated
**Status**: FAIL
**Evidence**:
- `/workspaces/unimatrix/.claude/skills/uni-retro/SKILL.md` line 39: `` - `permission_retries` → settings.json allowlist may need updating ``
- `/workspaces/unimatrix/packages/unimatrix/skills/retro/SKILL.md` line 39: same stale text

Both files provide guidance to agents interpreting retrospective reports. After this fix, the rule name is `orphaned_calls`, not `permission_retries`. Any agent reading this skill would look for `permission_retries` in a report and never find it, and would not recognize `orphaned_calls` as requiring action. The fix scope explicitly stated it updated "user-visible identifiers" but missed these two skill documentation files.

### Knowledge Stewardship — rust-dev
**Status**: PASS
**Evidence**: `product/features/bugfix-383/agents/383-agent-1-fix-report.md` (in worktree `agent-a611dddf`) contains:
```
## Knowledge Stewardship
- Queried: `/uni-query-patterns` for "detection rules renaming" — no prior matching patterns found
- Stored: entry #3485 "Renaming a detection rule requires updating 5+ string-keyed match arms across two crates" via `/uni-store-pattern`
```

### Knowledge Stewardship — investigator
**Status**: FAIL
**Evidence**: No investigator report exists anywhere in the workspace or worktrees. The bugfix protocol (`.claude/protocols/uni/uni-bugfix-protocol.md`) lines 489-491 require both investigator and rust-dev reports to contain `## Knowledge Stewardship` blocks. The spawn prompt confirms this requirement. Zero investigator reports found for bugfix-383.

## Rework Required

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| Stale `permission_retries` in skill docs | uni-rust-dev (or directly) | Update line 39 in `.claude/skills/uni-retro/SKILL.md` and `packages/unimatrix/skills/retro/SKILL.md`: change `permission_retries` to `orphaned_calls` and update the description from "settings.json allowlist may need updating" to "tool invocations with no terminal event — check context overflow or parallel call management" |
| Missing investigator report | uni-bug-investigator or retrospective | Create `product/features/bugfix-383/agents/383-investigator-report.md` documenting root cause diagnosis with a `## Knowledge Stewardship` section |

## Knowledge Stewardship

- Stored: nothing novel to store -- gate findings for this specific bug are feature-specific; the skill-doc staleness pattern is a new candidate but requires a second occurrence to classify as recurring
