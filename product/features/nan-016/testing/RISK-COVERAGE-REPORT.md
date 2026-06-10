# Risk Coverage Report: nan-016

> Stage 3c test execution for the UDS dogfooding re-release capability + (delivered,
> unexecuted) switchover. **Surfaces are JS/Node + shell only — there is NO Rust/pytest/
> infra-001 surface** (C-8: zero `lib/` changes, zero `crates/` changes). "Integration" here =
> `node --test` effect harness + the existing init/merge `node --test` suites + the two gate
> scripts. `cargo test --workspace` and the infra-001 pytest smoke harness are **not in scope**
> for this feature and were not run (they exercise no nan-016 code path).
>
> The F6 (#682) soak is NOT started by this feature, and the live flip against this repo's
> `.claude/settings.json` was NOT executed. The live settings file is byte-identical pre/post
> this entire test run (sha256 `55795a44…22267` unchanged).

## Why no Rust/pytest surface applies

nan-016 adds two POSIX shell scripts (`scripts/dogfood-install.sh`,
`scripts/dogfood-switchover.sh`), one `node --test` harness
(`packages/unimatrix/test/dogfood-effect.test.js`), and a runbook. It changes **no**
`packages/unimatrix/lib/**` bytes and **no** `crates/**` Rust code (C-8, verified by an empty
`git diff main...feature/nan-016 -- packages/unimatrix/lib packages/unimatrix/package.json`).
There is therefore nothing for `cargo test` or the infra-001 MCP pytest harness to exercise that
this feature introduces. Running them would only re-validate untouched upstream code, not the
nan-016 deliverable. All risk coverage is delivered by the `node --test` + shell surface below.

## Coverage Summary

| Risk ID | Risk Description | Test(s) / Check | Result | Coverage |
|---------|-----------------|-----------------|--------|----------|
| R-01 | Vacuous effect verification (string-diff regression) | effect T1 (real `execFileSync` re-fire, exit-0/empty-stdout) + **T1b re-fire negative control** (broken install path FAILS) | PASS | Full |
| R-02 | Non-atomic / overlay clean-replace | shell D5 install + D6 mutate-stray-then-reinstall (stray GONE, tree byte-fresh); staged-`mv` clean_replace in install.sh | PASS | Full |
| R-03 | Scratch-hash collision with live state | effect T1 (scratch root with real `.git/` under tmpdir, distinct hash, no scratch socket, daemon-absent re-fire) | PASS | Full |
| R-04 | Weak isolation proof (cannot detect leak) | effect T3 (behavior-changing in-repo edit in **throwaway copy**, installed byte/behavior invariant, non-symlink entrypoint, C-6) | PASS | Full |
| R-05 | `mergeSettings` require from installed client throws opaquely | switchover D9 (missing `--client` → loud exit 5, actionable message); install.sh `assert_complete`; switchover completeness gate before require | PASS | Full |
| R-06 | Rollback drifts from true pre-flip Rust form | effect T2 (promote→rollback round-trip = exact Rust legacy form over correct events, no stale node-client hook, idempotent, foreign preserved, shipped legacy arm); shell D8c rollback dry-run emits legacy form | PASS | Full |
| R-07 | Daemon-absent re-fire exits non-zero (C-7) | effect T1 (re-fire exit 0/empty stdout) + T1c (malformed/empty stdin exit 0); install.sh `smoke` fail-open | PASS | Full |
| R-08 | Live-settings mutation (deferred-flip boundary breach) | effect T4 (tmpdir guard rejects live path); suite-level pre/post sha256 of live `.claude/settings.json` byte-identical; all shell exercises used scratch `--settings` only | PASS | Full |
| R-09 | Matcher-narrowing delta not asserted / drifts | effect T1 (PreToolUse asserted == **imported** `PRETOOLUSE_CYCLE_MATCHER`); shell D7 promote prunes the stale `"*"` Rust uni hook; RUNBOOK §4 documents delta | PASS | Full |
| R-10 | Event-set 8-vs-9 / dup / foreign | effect T1 (event count vs actual opt-in; foreign survives, no dupes); shell D7 emitted 8 events (no `SubagentStop` opt-in), foreign `Bash` hook not pruned | PASS | Full |
| R-11 | `npm pack` drift / postinstall side-effect | shell D5 (full `files`-array tree: `bin lib skills protocols postinstall.js`; platform binary ABSENT; postinstall present-but-inert — extraction is pure file copy); mechanism is `npm pack` | PASS | Full |
| R-12 | Container-rebuild non-durability | shell D1 (`--print-target` resolves fixed `~/.unimatrix/dogfood-client`); rollback emits fixed absolute `<repo>/target/release/unimatrix` path | PASS | Full |
| R-13 | In-repo edit not restored | effect T3 (edit in throwaway copy, never live tree); post-suite `git status --porcelain packages/unimatrix/lib` clean | PASS | Full |
| R-14 | Init / size / zero-deps regression | init.test 12, init-integration.test 8, init-remote.test 37, merge-settings.test 48 all green; size + zero-deps gates exit 0; empty frozen-surface diff | PASS | Full |
| R-15 | `before`-hook install disturbs human-staged install | effect harness installs into test-scoped tmpdir `--target` only; post-suite `~/.unimatrix/dogfood-client` absent (no real install created) | PASS | Full |

All 15 risks (4 Critical, 5 High, 6 Medium) have full coverage. **Both mandatory negative
controls are present and green:** R-01 re-fire negative control (`T1b` — broken install path
FAILS the assertion) and the prune negative control (`T1d` — `mergeSettings`-alone no-prune
post-state FAILS the same clean-state helper). R-04's isolation proof uses a behavior-changing
edit, not a no-op bytes check.

### Scope Risk (SR) traceability — all covered via mapped R-IDs

| SR | Via | Result |
|----|-----|--------|
| SR-01 (incomplete tree / postinstall) | R-11, R-05 | PASS |
| SR-02 (stale shadow / non-idempotent) | R-02, R-12 | PASS |
| SR-03 (build reproducibility) | R-02, R-14 | PASS |
| SR-04 (vacuous verification) | R-01 | PASS (re-fire + negative control) |
| SR-05 (matcher delta) | R-09 | PASS |
| SR-06 (live-flip read as executed) | R-08 | PASS (pre/post hash unchanged) |
| SR-07 (isolation = code-freeze) | R-04 | PASS |
| SR-08 (fail-open / daemon-absent) | R-07, R-03 | PASS |
| SR-09 (init / size-gate regression) | R-14 | PASS |

## Test Results

### Unit / Effect Tests (`node --test`)

| Suite | Tests | Passed | Failed | Skipped |
|-------|-------|--------|--------|---------|
| `test/dogfood-effect.test.js` (new harness) | 7 | 7 | 0 | 0 |

The harness re-fires the **installed** entrypoint via `execFileSync` (non-vacuous, R-01) and
carries both mandatory negative controls. Suites/sub-tests:
- T1: promote → CLEAN post-state (every uni hook on installed entrypoint, stale `"*"` Rust hook
  count 0) + matcher delta + fail-open re-fire — PASS
- T1d: prune NEGATIVE CONTROL (no-prune post-state FAILS the same clean-state helper) — PASS
- T1b: re-fire NEGATIVE CONTROL (broken install path FAILS the assertion, R-01) — PASS
- T1c: fail-open on malformed/empty stdin (R-07) — PASS
- T2: promote→rollback → CLEAN Rust legacy form, idempotent, foreign preserved (R-06) — PASS
- T3: in-repo edit in throwaway copy → installed bytes/behavior unchanged, non-symlink (R-04,
  R-13, C-6) — PASS
- T4: tmpdirGuard rejects the live settings path (R-08) — PASS

### Integration / Regression Tests (`node --test`, frozen-API suites — AC-05 / R-14)

| Suite | Tests | Passed | Failed | Skipped |
|-------|-------|--------|--------|---------|
| `test/merge-settings.test.js` | 48 | 48 | 0 | 0 |
| `test/init.test.js` | 12 | 12 | 0 | 0 |
| `test/init-integration.test.js` | 8 | 8 | 0 | 0 |
| `test/init-remote.test.js` | 37 | 37 | 0 | 0 |
| **Regression total** | **105** | **105** | **0** | **0** |

**Grand total (effect + regression): 112 `node --test` tests, 112 passed, 0 failed, 0 skipped.**

### Gate Scripts (AC-06 / NFR-5 / NFR-6 / C-9)

| Gate | Command | Exit | Result |
|------|---------|------|--------|
| Hook-client size | `node test/check-hook-client-size.js` | 0 | PASS (stripped 76597/100000, raw 129550/160000) |
| Zero-deps | `node test/check-zero-deps.js` | 0 | PASS (no runtime deps; 16 modules require only Node built-ins / relative) |

### Shell Component Behavior (`scripts/*.sh`, exercised against tmpdir scratch only)

| # | Exercise | Expected | Observed | Result |
|---|----------|----------|----------|--------|
| D1 | `dogfood-install.sh --print-target` | default `~/.unimatrix/dogfood-client`, exit 0 | `/home/vscode/.unimatrix/dogfood-client`, exit 0 | PASS |
| D2 | install `--dry-run --target <tmpdir>` | resolves target, no write, exit 0 | resolved, exit 0 | PASS |
| D3 | install `--target relative/path` | guard reject, exit 3 | exit 3 (must be absolute) | PASS |
| D4 | install `--target /workspaces/unimatrix` | guard reject repo, exit 3 | exit 3 (refusing forbidden) | PASS |
| D5 | REAL install into tmpdir `--target` | complete tree, non-symlink entrypoint, no binary, exit 0 | all confirmed, exit 0 | PASS |
| D6 | mutate + stray-file, re-install | stray GONE (clean-replace, not overlay), exit 0 | stray GONE, exit 0 | PASS |
| D7 | `promote --dry-run` on live-shaped scratch settings | 8 events (no opt-in), stale `"*"` Rust hook pruned (count 1), foreign kept, no write | pruneCount 1, foreign kept, scratch unchanged, exit 0 | PASS |
| D8 | `rollback --dry-run` (default client absent) | loud R-05 exit 5 | exit 5, actionable message | PASS |
| D8c | `rollback --dry-run` (client present) | emits Rust legacy form, no write, exit 0 | legacy form actions, dryRun true, exit 0 | PASS |
| D9 | `promote --client <missing>` | loud R-05 exit 5 | exit 5 | PASS |
| D10 | bad mode `frobnicate` | exit 2 | exit 2 | PASS |

All shell exercises used scratch `--settings` under `/tmp` and tmpdir `--target` dirs; temp
dirs were cleaned up. No real `~/.unimatrix/dogfood-client/` was created (confirmed absent
post-run). Live `.claude/settings.json` never opened for write.

### Frozen-surface diff (AC-05 / C-8 / R-14)

`git diff main...feature/nan-016 -- packages/unimatrix/lib packages/unimatrix/package.json`
→ **empty** (exit 0). Zero behavioral change to `lib/init.js`, `lib/merge-settings.js`,
`lib/hook-client/config.js`, or `package.json` runtime.

## Gaps

**None.** All 15 architecture risks and all 9 scope risks (SR-01..SR-09) have full test
coverage with passing results. Both mandatory negative controls (R-01 re-fire, the prune)
are present, green, and share the assertion helper with their positive counterparts (so a
regression to a vacuous check would surface). No `xfail` markers were needed (no pytest
surface). No integration tests were deleted or commented out. No pre-existing failures were
encountered, so no GH Issues were filed.

### Boundary statements (deliberately NOT done by this feature)

- The **F6 (#682) soak is NOT started** by this test run — re-running `dogfood-install.sh` is
  the documented soak-reset point, but it was only exercised against a tmpdir `--target`, never
  the fixed `~/.unimatrix/dogfood-client/`.
- The **live flip was NOT executed** — `promote`/`rollback` ran only against scratch settings
  under `/tmp`. This repo's `.claude/settings.json` is byte-identical pre/post (R-08).

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | Shell D1/D2 (target resolution), D5 (real install: complete `files`-array tree, non-symlink entrypoint, platform binary absent), D6 (idempotent clean-replace, stray file gone); staged-`mv` clean_replace; postinstall inert (pure file-copy extraction). R-02, R-11, R-12. |
| AC-02 | PASS | Effect T1 — real `promote` on live-shaped scratch seed: every uni command → installed entrypoint, stale `"*"` Rust hook pruned (count 0), PreToolUse == imported `PRETOOLUSE_CYCLE_MATCHER`, foreign preserved/no dupes, 8 events; `execFileSync` re-fire exit-0/empty-stdout (scratch hash distinct); T1b negative control + T1d prune negative control. No live-settings write. R-01/03/07/08/09/10. |
| AC-03 | PASS | Effect T3 — behavior-changing in-repo edit in a throwaway copy leaves installed bytes/behavior unchanged; `lstatSync(...).isSymbolicLink() === false` (C-6); working tree clean post-test (R-13). Code-freeze, not state-dir separation. R-04, R-13. |
| AC-04 | PASS | RUNBOOK.md present; grep confirms all five FR-14 items (a soak-reset, b rollback→Rust hook.rs via mergeSettings, c deferred flip/no-active-feature window, d PreToolUse matcher-narrowing delta, e daemon assumed-running/fail-open). Effect T2 + shell D8c prove rollback restores exact Rust legacy form via the shipped `normalizeCommandSource` arm (no bespoke revert string), idempotent, foreign preserved. R-06. |
| AC-05 | PASS | `init.test.js` (12), `init-integration.test.js` (8), `init-remote.test.js` (37), `merge-settings.test.js` (48) all green; empty frozen-surface `git diff`. R-14. |
| AC-06 | PASS | `check-hook-client-size.js` exit 0 (stripped 76597 ≤ 100000, raw 129550 ≤ 160000); `check-zero-deps.js` exit 0 (zero runtime deps, all 16 modules built-in/relative only). R-14. |

All six acceptance criteria PASS.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — surfaced ADR-005 (#4928, effect-via-scratch-root
  re-fire), ADR-001 (#4924, npm pack copy-install), #4930 (seeding a scratch settings with a
  Rust-hook shape to exercise switchover), #4781 (Stage 3c pre-existing-failure triage protocol),
  #2928/#4796 reasoning carried in the risk strategy. All applied to verify the negative-control
  and scratch-hash mandates were actually satisfied by the harness, not just named.
- Stored: nothing novel to store. The load-bearing testing patterns here (effect harness must
  re-fire not string-diff; deferred-action boundary guarded with pre/post live-settings hash +
  tmpdir guard; switchover scratch-seed shape) are already captured by #2928, #4796, and the
  nan-016-specific #4930. No 2+-feature reusable pattern emerged that is not already present.
