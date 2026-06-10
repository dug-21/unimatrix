# Gate 3a Report: nan-016

> Gate: 3a (Component Design Review)
> Date: 2026-06-10
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Architecture alignment | PASS | 4 pseudocode files map 1:1 to the ARCHITECTURE Component Breakdown; all 5 ADRs honored; OQ-1/2/3 resolved consistently |
| 2. Specification coverage | PASS | FR-1..FR-15, NFR-1..NFR-8, AC-01..AC-06 all have corresponding pseudocode; no scope additions |
| 3. Risk coverage | PASS | All 15 architecture risks (4 Critical) map to test scenarios; R-01/R-04 negative controls MANDATORY; all 9 SR-XX traced |
| 4. Interface consistency | PASS | Frozen mergeSettings/buildHookClientCommand/config.js contract in OVERVIEW verified byte-exact against shipped 0.7.2; consistent across files |
| 5. Knowledge stewardship | PASS | architect/spec/testplan reports all carry `## Knowledge Stewardship` with Queried + Stored/"nothing novel -- {reason}" |

**Feature-specific load-bearing checks (all PASS):**

| Focus check | Status | Evidence |
|-------------|--------|----------|
| Effect harness real re-fired hook (not string-diff) | PASS | `dogfood-effect.md` T1 + `reFire` helper use `execFileSync("node",[installedIndexJs,EVENT],{cwd:scratchRoot,input:JSON})`; T1b negative control mandatory (R-01) |
| AC-03 = code-freeze, not state-dir; throwaway-copy edit | PASS | T3 edits a throwaway copy under `os.tmpdir()`; explicit "NOT state separation, shared {hash} by design (#4923)"; live tree never edited (R-13) |
| Scratch hash asserted DISTINCT from live | PASS | `makeScratchRoot` asserts `scratchHash !== computeProjectHash(repoRoot)` + no scratch socket (R-03) |
| No write to live settings; tmpdir guard | PASS | tmpdirGuard + pre/post live-settings hash; T4 negative guard test; live read-only (R-08) |
| Honors C-6/C-7/C-8/C-04/C-9 | PASS | non-symlink assert (C-6); fail-open re-fire (C-7); no lib/ edits (C-8); size + zero-deps regression gates (C-04/C-9) |

## Detailed Findings

### 1. Architecture alignment
**Status**: PASS
**Evidence**: Pseudocode `OVERVIEW.md` enumerates exactly the four ARCHITECTURE components at the same committed paths (`scripts/dogfood-install.sh`, `scripts/dogfood-switchover.sh`, `packages/unimatrix/test/dogfood-effect.test.js`, `RUNBOOK.md`). All five ADRs are reflected: ADR-001 (`npm pack`+extract, clean-replace — `dogfood-install.md` `pack`/`extract`/`clean_replace`); ADR-002 (fixed `~/.unimatrix/dogfood-client/` default + container-durability note); ADR-003 (both promote object-arm and rollback string-arm route through installed `mergeSettings` — `dogfood-switchover.md`); ADR-004 (no daemon lifecycle, explicit); ADR-005 (scratch-root effect harness). The three architecture Open Questions are pinned: OQ-1 (throwaway-copy edit), OQ-2 (`before`-hook install to temp `--target`), OQ-3 (shell-wrapping-Node, justified against committed `.sh` filenames in SPEC/SCOPE). No component boundary or technology choice contradicts the architecture.

### 2. Specification coverage
**Status**: PASS
**Evidence**: Each FR has pseudocode: FR-1/2/3/4 (`dogfood-install.md` pack/extract/clean_replace/assert_complete/smoke), FR-5/6/7/8/9 (`dogfood-switchover.md` promote/rollback one-liners + no-daemon note), FR-10/11/12/13 (`dogfood-effect.md` T1/T1c/T3/T4 + before/after), FR-14 (`runbook.md` sections 0-6 mapping a-e), FR-15 (regression gates in `runbook` test-plan). NFRs are addressed (NFR-1 idempotency byte-compare, NFR-3 hash-invariance, NFR-4 zero-write pre/post hash, NFR-5/6 size+zero-deps gates, NFR-7 fixed dir). No pseudocode implements unrequested features — every routine traces to an FR/AC/risk. The "delivered, not executed" boundary (live flip out of scope) is explicit in switchover and runbook pseudocode.

### 3. Risk coverage
**Status**: PASS
**Evidence**: Test-plan `OVERVIEW.md` Risk-to-Test Mapping covers all 15 risks. The four Critical risks each have non-vacuous coverage: R-01 (real re-fire + MANDATORY negative control T1b), R-02 (mutate-then-reinstall byte-compare + stray-gone + staged-mv evidence), R-03 (scratch hash distinct + no socket + realpath-mirrors-config.js), R-04 (behavior-changing throwaway-copy edit + byte/behavior invariance + non-symlink + MANDATORY leak-detecting negative control). High/Medium risks (R-05..R-15) all mapped. All nine SR-XX scope risks trace through to at least one R-ID and a scenario. Risk priorities are reflected — the effect harness (the Critical-risk carrier) gets the deepest test plan.

### 4. Interface consistency
**Status**: PASS
**Evidence**: The frozen contract in `pseudocode/OVERVIEW.md` was verified byte-exact against shipped `packages/unimatrix/lib/merge-settings.js` and `config.js` (0.7.2): exports `mergeSettings,isUnimatrixHook,buildHookClientCommand,normalizeCommandSource,subagentStopEnabled,HOOK_EVENTS,EVENT_MATCHERS,UNIMATRIX_PATTERNS,PRETOOLUSE_CYCLE_MATCHER`; `PRETOOLUSE_CYCLE_MATCHER === "context_cycle|mcp__unimatrix__context_cycle"`; `HOOK_EVENTS` = the 9-event list (SubagentStop opt-in); `buildHookClientCommand("/x/...","SessionStart") === "node /x/... SessionStart"` (bare) and quoted on whitespace; `computeProjectHash`/`socketPathFor`/`walkToProjectRoot` present in config.js; `package.json` `name="@dug-21/unimatrix"`, `files=["bin/","lib/","skills/","postinstall.js","protocols/"]`. Component 2 (promote object-arm, rollback string-arm), Component 3 (imported constants, 8-events-on-no-opt-in), and the shared types in OVERVIEW are mutually consistent. The tarball-name-by-glob (#4328) and matcher-by-imported-constant (no literal drift) decisions are coherent across pseudocode and test plans.

### 5. Knowledge stewardship compliance
**Status**: PASS
**Evidence**: The active-storage architect report has a `## Knowledge Stewardship` block with `Queried:` (context_briefing + targeted context_get on #4923) and `Stored:` (#4924-#4928 ADRs). The read-only spec report has `Queried:` plus a reasoned "no directly relevant entries" justification. The testplan report has `Queried:` (context_briefing + search surfacing #2928/#4796/#4915/#4328) and a reasoned "nothing novel to store -- {patterns already captured by #2928/#4796/#4328}". All blocks present with reasons after "nothing novel" — no WARN.

## Feature-specific verification (per spawn prompt)

- **Effect harness is non-vacuous (R-01/R-04, the load-bearing risks):** PASS. `dogfood-effect.md` T1 re-fires the installed entrypoint via `execFileSync` and asserts exit-0/empty-stdout; T1b is a MANDATORY negative control that points the re-fire at a broken path and asserts it FAILS; T3 proves isolation with a behavior-changing edit and a MANDATORY leak-detecting negative control. This is a real re-fired hook + negative controls, NOT a settings string-diff. The test-plan OVERVIEW explicitly states "a green harness with NO `execFileSync` re-fire and NO negative control is a FAILED gate."
- **AC-03 code-freeze framing, throwaway copy, no live-tree edit (R-13):** PASS. T3 edits a throwaway copy of `packages/unimatrix` under `os.tmpdir()`, re-packs to a second temp install, and asserts the original installed bytes/behavior are invariant; explicitly framed as code-freeze NOT state-dir separation (#4923 cited); `after` asserts the working tree is clean even on failure.
- **Scratch hash DISTINCT from live (R-03):** PASS. `makeScratchRoot` realpaths the root (mirrors config.js, #4796 symlink guard) and asserts `scratchHash !== computeProjectHash(repoRoot())`, plus no scratch socket before re-fire.
- **No write to live `/workspaces/unimatrix/.claude/settings.json` (R-08):** PASS. tmpdirGuard rejects the live path (T4 negative test); pre/post-suite live-settings hash asserts zero writes; live opened read-only for shape only.
- **C-6/C-7/C-8/C-04/C-9:** PASS. Non-symlink assertion both at install (`assert_complete`) and harness (T3); fail-open re-fire (C-7); no `lib/` behavioral edits, with git-diff regression check (C-8); size + zero-deps gates as pure regression guards (C-04/C-9).

## Minor observations (non-blocking, no rework required)

- `dogfood-effect.md` "Gaps/flags" leaves the implementer to pick the lighter of two ways to make the throwaway-copy repo a git root for re-pack (copy `packages/unimatrix` + `git init` tmpRepo, vs a `--pkg-dir` affordance). Both keep the live tree untouched; this is an implementation-detail choice, correctly deferred to 3b, not a design gap.
- Test-plan flags an optional `--keep-staging`/`--print-target`/`--dry-run` script affordance for directly asserting staged-mv (R-02) and the default fixed target (R-12) without writing the real dir. Effect-level coverage exists regardless; the affordance is a "SHOULD," correctly flagged for 3b.

## Rework Required

None.

## Scope Concerns

None. The design correctly preserves the "delivered, not executed" boundary; the deferred live flip and the F6 follow-up issue are explicitly flagged for the human, not implemented.
