# Agent Report — nan-016-agent-2-testplan (Stage 3a, Test Plan Design)

## Deliverables
- product/features/nan-016/test-plan/OVERVIEW.md
- product/features/nan-016/test-plan/dogfood-install.md
- product/features/nan-016/test-plan/dogfood-switchover.md
- product/features/nan-016/test-plan/dogfood-effect.md
- product/features/nan-016/test-plan/runbook.md

Test-plan files map 1:1 to the architecture Component Breakdown (4 components) plus OVERVIEW.

## Critical-mandate encoding (verified present)
- R-01 + R-04 negative controls: dogfood-effect.md has `refire_negative-control_broken-install-path-FAILS-the-assertion` and `isolation_negative-control_symlink-or-overlay-leak-WOULD-be-detected`, both marked MANDATORY.
- R-02: dogfood-install.md `reinstall_mutated-and-stray-file_byte-identical-to-fresh-and-stray-gone` (mutate-then-reinstall byte-compare) + staged-temp+atomic-mv evidence test.
- R-03: dogfood-effect.md asserts scratch hash DISTINCT from live hash, no scratch socket, realpath-mirrors-config.js (#4796 tmpdir-symlink guard).
- R-08: dogfood-effect.md `live-settings_pre-post-suite-hash-unchanged` + tmpdir guard negative test, read-only shape copy.
- R-09/R-10: matcher asserted against IMPORTED `PRETOOLUSE_CYCLE_MATCHER`; event count against actual opt-in state (8 vs 9).
- R-13: working-tree-clean-on-failure-path test (restore in teardown).
- AC-05/AC-06: runbook.md cross-cutting gates — existing init/merge suites green + size/zero-deps exit 0 + no frozen-surface diff.

## Integration suite plan (no Rust/pytest surface)
"Integration" = node --test harness + existing packages/unimatrix node --test suite + size/zero-deps gate scripts. infra-001/cargo NOT exercised by nan-016. OVERVIEW Integration Harness Plan lists Stage 3c order: (A) new dogfood-effect.test.js harness, (B) existing init/merge/init-remote suites, (C) check-hook-client-size.js + check-zero-deps.js, (D) git-diff frozen-surface check. Gate semantics: green harness with no re-fire / no negative control = FAILED gate.

## Open questions (for pseudocode/3b)
1. OQ-C re-fire mechanics pinned to `execFileSync("node",[installedIndexJs,"SessionStart"],{cwd:scratchRoot,input:JSON})`; pseudocode should confirm payload shape per event.
2. R-04 negative-control fixture (leaky symlink/overlay install) construction — pseudocode to specify how the leaky fixture is built without npm link in the real tree.
3. R-02 staged-mv assertion needs a script affordance (e.g. `--keep-staging` debug flag or staging-path env) to assert sibling-temp+atomic-mv directly; otherwise only effect-level (no-partial-at-target) is testable. Flagged for pseudocode/3b.
4. R-12 default-target assertion needs a non-destructive way to read the resolved default (e.g. `--print-target` / `--dry-run`) so CI never writes the real `~/.unimatrix/dogfood-client/`.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search -- surfaced ADR-001..005 (#4924, #4925, #4926, #4928), #2928 (effect-over-string-diff / per-rule isolation), #4796 (tmpdir-symlink state-dir split; un-executed-AC-as-fact), #4915 (manifest completeness needs code-derived cross-check). Applied to negative-control and scratch-hash mandates.
- Stored: nothing novel at plan time -- the load-bearing patterns (effect harness must re-fire not string-diff; deferred-action boundary needs a negative-guard test; matcher asserted against imported constant) are already captured by #2928 / #4796 / #4328. A reusable "scratch project-root hash isolation fixture" pattern is a candidate for Stage 3c once the implementation yields a generalizable helper.
