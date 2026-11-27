# Risk-Based Test Strategy: 1036

Inputs:
- SCOPE.md (D-005) and DECISIONS.md D-001..D-030. D-025..D-030 are binding: D-025..D-029 amend D-023, AC-3, AC-1 and AC-2, and D-030 keeps R4/RT-03 (In-3 as written).
- ARCHITECTURE.md, including the §6a canonical contract names and the §8 test infrastructure.
- ADR-INDEX.md: #5937, #5960, #5962, #5959, #5955, #5943, #5961; nan-024 amended #5956.
- SPECIFICATION.md and SCOPE-RISK-ASSESSMENT.md (SR-01..SR-17).
- Evidence: nan-024 ADR-002 #5900, and lesson #4473 (warn+continue hides failure-path tests).

Binding decisions applied here:
- **D-025:** enforced modes are re-tightened on every write for `remote.json`, `install.json`, `trust.json`, the root marker and `bin/*`. Other existing files keep their mode. New files follow D-023.
- **D-026:** a project-file link is written through only if its real target is inside the realpath of the project root. Otherwise init exits 1 naming the link. User/global links behave as AC-3 says.
- **D-027:** AC-2 gets a real-`opencode` check.
- **D-028:** with `CLAUDE_CONFIG_DIR` set, the default `~/.claude.json` entry is kept, including `projects[root]`, and `status` lists it.
- **D-029:** an unparseable project `opencode.json` still exits 1.
- **D-030:** with `CLAUDE_CONFIG_DIR` set, owned hooks in the default `~/.claude/settings.json` are still removed (R4/RT-03). Hooks live in the project `.claude/settings.local.json`, which the override does not affect, so the removal is harmless. Foreign hooks in that file are untouched. The removal prevents duplicate firing (AC-6, #5803).

Real-binary checks (D-012, D-027):
- Codex is file-level only, tracked in #1053.
- AC-1(c) runs real `claude mcp get`. AC-2(b) runs real `opencode debug config --standalone`; `opencode mcp list` was measured unusable. Both run in the devcontainer and skip with a stated reason when the binary is absent.

Priority = Severity x Likelihood. Critical = High x High, or a security regression with no self-heal path.

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---|---|---|---|---|
| R-01 | **Enforced-mode re-tightening (D-025).** The writer must chmod `remote.json`, `install.json`, `trust.json`, the root marker and `bin/*` (0600 for files, 0700 for `bin/*`). It can miss when content is unchanged: the `unchanged` short-circuit skips the write, and a mode-only drift survives. It can also miss for a caller that does not pass the enforced flag. The opposite failure is enforcing modes on harness or project files, which must keep their mode | High | Low | Med |
| R-02 | Fixpoint verify (`mutate(readBack) === null`) fails for a non-idempotent mutate (indent, key order, trailing newline, TOML re-emit). The result is a spurious `verify-failed` exit 1, or a leg that reports `updated` on every run (breaks AC-5) | High | Med | High |
| R-03 | **Project-link containment (D-026).** Scope is `settings.local.json`, project `settings.json`, `.mcp.json`, project `opencode.json`, `.codex/*` and `.claude/skills/*`. Failure modes: (a) the containment check is wrong: prefix match without a separator boundary (`/proj` vs `/proj-x`), comparing against an unresolved root, case folding on macOS/win32, a parent dir link (`.claude -> /elsewhere`); (b) a link swapped between the check and the rename; (c) the rule applied to user/global links (breaks AC-3) or to the exclude file | High | Med | High |
| R-04 | `wire` and `status`/`mcpNode`/hook-state resolve different files: a private path survives, or `process.env` is read instead of the injected `env` (SR-09) | High | Med | High |
| R-05 | D-007 same-file identity is wrong for the remaining legacy twins (`~/.claude/settings.json` R4, kept by D-030; `~/.config/opencode/opencode.json` R10). For R10, a false "different" deletes the live OpenCode entry and the harness goes dark. For R4, it deletes owned hooks from the live user settings file when `CLAUDE_CONFIG_DIR=<home>`; this is harmless because the project file carries the hooks. A false "same" keeps a twin (AC-6) | High | Med | High |
| R-06 | Migrating the install-store/trust.js private writers to the core regresses rename retry, new-file 0700/0600, the win32 `recordTrustIdentity` posture, or the D-021 symlinked-`trust.json` refusal (SR-03) | High | Med | High |
| R-07 | Dry-run is not side-effect-free or not leg-identical. The writer `mkdir -p`s parents or opens a temp before the dryRun check. Gated-removal, `git-exclude`, mode-only or store legs differ. The resolved lines or the D-013 notice differ (SR-15) | Med | High | High |
| R-08 | AC-5 idempotency fails on store or skill legs: a timestamp in `install.json`, a CRLF skill byte-compare, a root marker rewrite, the package-copy leg not `unchanged`, the D-013 notice on run 2, or a mode re-assert reported as a change when the mode is already correct | Med | High | High |
| R-09 | Test env leakage. The devcontainer or CI sets the four override vars, so tests read or write real config (SR-10, #5911) | High | Med | High |
| R-10 | Implementation departs from the ARCHITECTURE §6a canonical contract: a stale name, a status word, or a `SafeWriteError` subclass that wrappers fail to rethrow | Med | Low | Low |
| R-11 | Write-before-remove gating is bypassed. A gated removal (R1/R4/R10) runs after the destination leg is `skipped-*`, or a wrapper catches a throw and later removals still run, leaving the harness dark (SR-11) | High | Low | Med |
| R-12 | A Codex or OpenCode wrapper swallows a `SafeWriteError` into `skipped*`, so the run exits 0 with a harness unwired (#4473) | Med | Med | Med |
| R-13 | A spurious CAS conflict from a concurrent writer (hook-time `trust.json`, Claude writing `.claude.json`) aborts init with exit 1 | Med | Med | Med |
| R-14 | Git-exclude decision errors: leaked `GIT_*` env, the tracked case appending, a `!` override re-appending, prefix escaping, a hand-built worktree/submodule path, a git hang | Med | Med | Med |
| R-15 | JSON fidelity loss on rewrite: key order, indent, trailing newline, a dropped `__proto__` key, integers above 2^53 (D-022) | Med | Med | Med |
| R-16 | Parse-failure posture applied to the wrong file. User `opencode.json` must skip with exit 0 (D-024). Project `opencode.json` must exit 1 (D-029), as must `.claude.json` and the settings files | Med | Med | Med |
| R-17 | Override edge values (`""`, whitespace, `~/x`, relative, NUL, trailing separator), and init's env differing from the harness env (SR-13/SR-17). D-028 lowers the impact: the default `~/.claude.json` entry is no longer deleted, so a Claude session started without the var keeps working | Med | Low | Low |
| R-18 | The status Claude local-scope check uses the wrong key (`resolveProjectKey`, win32 case, worktree root), or reports `wired` from the default `~/.claude.json` when `CLAUDE_CONFIG_DIR` is set (SR-16). Or that default entry is not listed as another profile (D-028) | High | Low | Med |
| R-19 | D-065 foreign-at-key is overwritten at a resolved or twin location, or the G2 removal runs when W2 is `skipped-foreign` | High | Low | Med |
| R-20 | Legacy plugin predicate: a false positive removes a foreign plugin; a false negative leaves G3 (SR-12) | Med | Low | Low |
| R-21 | Package-copy swap: concurrent inits, an incomplete destination renamed aside, `.tmp-` prune racing a build, `install.json` swapped before the dir rename | Med | Low | Low |
| R-22 | Temp leak or naming: the EEXIST regenerate cap, cleanup when mutate throws, `.pid` names left in a caller | Med | Low | Low |
| R-23 | Writer core inside `trust.js` (D-021) grows the hook hot-path cost, gains a non-builtin `require`, or the installed copy drifts from source | Med | Low | Low |
| R-24 | Win32 branches are proven only with an injected `fs`: rename retry, skipped dir fsync, junction realpath, identity normalisation, and the case-insensitive D-026 containment compare (SR-04) | Med | Med | Med |
| R-25 | Real-binary proof is partial. Claude and OpenCode are proven only where the binary is present; in CI only the self-referential verify read remains. Codex is file-level (D-012, #1053). The `opencode debug config` output format is unversioned, and running without `--standalone` attaches to the user's background service and real config | Med | Med | Med (Codex gap accepted) |

## Risk-to-Scenario Mapping

### R-01: Enforced-mode re-tightening (D-025)
**Impact**: a missed re-tighten leaves a credential world-readable, or a trust-set file the trust check refuses on every hook event. An over-broad chmod changes harness or project files the user owns.

**Test Scenarios**, driven through spawned `init` / `init --bundle`, with each fixture seeded from a completed install:
1. `remote.json` at 0644, credential changed: content updated, mode 0600.
2. `remote.json` at 0644, credential unchanged: mode 0600, bytes unchanged. This is a mode-only repair, which the writer performs with chmod; the leg reports `updated` (ARCHITECTURE §3.2), not `unchanged`.
3. `bin/shim-entry.js` at 0775: mode 0700 after init. The next hook event through the installed shim passes the trust check.
4. Repeat scenario 3 for `bin/trust.js` and the shim.
5. `install.json` at 0664 → 0600. `trust.json` at 0644 → 0600. Root marker at 0644 → 0600. The trust check passes afterwards.
6. Mode-keep control: `.claude/settings.local.json` at 0644, `~/.claude.json` at 0644, `opencode.json` at 0664 and a `.claude/skills/*` file at 0644 all keep their mode after a content change and after an unchanged run.
7. New files under umask 000, 002 and 022: `bin/*` 0700, enforced files 0600, other new files 0600.
8. A second init with every mode already correct does no chmod and no rename (AC-5).
9. Win32 with injected `platform`: no POSIX chmod path. The ACL flow is unchanged.

**Coverage Requirement**: scenarios 1-6 must exist by name (#4473). Scenario 2 is the mode-only path and is the most likely to be missed.

### R-02: Non-idempotent mutate
**Test Scenarios**:
1. Run every mutate (W1-W5, R1-R12, X1, store JSON) twice on its own output. The second call returns `null`. Fixtures: tab, 4-space and 2-space indent; no trailing newline; CRLF; non-canonical key order.
2. TOML with comments and odd spacing: write, then re-mutate, gives `null`.
3. Double init: zero renames on run 2 and every leg `unchanged`.

### R-03: Project-link containment (D-026)
**Test Scenarios**, driven through spawned `init`, in both dry-run and real run. For each failure case, assert init exits 1 with `SafeWriteError` code `outside-project` naming the link and its target, that the link and target are unchanged, and that no temp is left:
1. **Inside:** `.claude/settings.local.json -> ./local/settings.json` (relative) and an absolute link inside R. Written through; the link is still a link and the target holds the change.
2. **Chained inside:** a 2-hop chain that ends inside R is written through.
3. **Outside:** `settings.local.json`, `.mcp.json`, `opencode.json`, `.codex/hooks.json` and `.codex/config.toml` each linked to a valid-JSON or TOML file outside R. Each one fails.
4. **Escape-and-return:** a chain that hops outside R and ends inside R. The decision is made on the final realpath; assert the documented result.
5. **Root is a symlink:** R is reached through a symlinked path (`/work/link -> /real/proj`) and the target is inside `/real/proj`. Written through, because the root realpath is resolved first. A target in `/real/proj-x` (prefix sibling) fails.
6. **Parent dir link:** `.claude -> /outside/dir`. The real target is outside, so it fails.
7. **Dangling:** a dangling project link fails with code `dangling-symlink` naming the link.
8. **Skills:** `.claude/skills/<name>/SKILL.md -> ~/victim.txt` fails and the victim is unchanged. A skill link inside R is written through.
9. **Swap:** the link is retargeted outside R at the `before-rename` fault seam. The write is refused (re-check before rename) and no bytes reach the outside file.
10. **Unchanged:** user/global links (`~/.claude.json`, `$CODEX_HOME/*`, the user `opencode.json`, `~/.unimatrix/install.json`) pointing outside home are still followed per AC-3.
11. **Exclude file:** with `--project-dir <submodule>`, the exclude file is at `<super>/.git/modules/<name>/info/exclude`, outside R. Spec FR-22a classifies it as git metadata, not a project file, so the append succeeds with no `outside-project` error.
12. **Case:** on an injected win32/darwin platform, a target differing only in case from R is treated as inside.
13. **Lexical guard removed:** the old `isWithinProject` checks on these writes are gone (grep test), so the realpath check is the single gate.

### R-04: init and status path divergence
**Test Scenarios**: for each override (none, each var, all vars), `status.collect(ctx).configDirs` equals the `resolved:` paths in init's manifest. A grep test (FR-01) finds no private home joins and no direct `process.env` reads. Spawned `status` with `CLAUDE_CONFIG_DIR=<d>` prints `<d>/.claude.json`.

### R-05: Same-file identity (remaining legacy twins)
**Test Scenarios**: assert no removal leg in each of these fixtures:
- `XDG_CONFIG_HOME=<home>/.config`;
- `OPENCODE_CONFIG_DIR=<home>/.config/opencode`;
- a trailing-slash variant;
- an override dir that is a symlink to the default dir;
- the only-D-exists realpath form;
- `CLAUDE_CONFIG_DIR=<home>`, which makes `<home>/settings.json` vs `~/.claude/settings.json` (R4) a different-file case, while `.claude.json` is the same file.

Different-file controls remove the R4 and R10 twins. Run with injected win32 and mixed-case drive letters.

### R-06: Writer migration regression
**Test Scenarios**:
1. The existing install-store, trust and credstore tests run green against the core before the private writers are deleted (FR-33).
2. A symlinked `trust.json` (live and dangling) exits 1 with the §6a text; the link and target are unchanged.
3. A hook-time `trust.json` symlink is skipped, non-fatal.
4. Symlinked `install.json` / `bin/*` are refused with the §6a trust-file message.

### R-07: Dry-run side effects and parity
**Test Scenarios**:
1. The tree hash (including absent parents and a missing `info/`) is unchanged by `init --dry-run`.
2. The AC-9 parity harness over every AC fixture, including the R-01 mode-drift fixtures: the dry run reports the same mode-repair legs and does no chmod.
3. A gated removal with a skipped destination emits `skipped-dest-unwired` in both modes.
4. A D-026 outside-root link fails with exit 1 in dry-run too, naming the link.

### R-08: Store and skill idempotency
**Test Scenarios**: AC-5 double init with and without overrides and `--bundle`. Assert mtime, bytes and mode are unchanged for every `~/.unimatrix/` file, `.claude/skills/*`, the exclude file and the twin locations. No D-013 notice. The package-copy leg is `unchanged`.

### R-09: Test env leakage
**Test Scenarios**: run the full suite with all four vars set to a sentinel dir that is not tmp; the sentinel is untouched. `assertTmpHome` rejects an injected override outside tmp.

### R-10: §6a conformance
**Test Scenarios**: `status --json` asserts the §6a fields and value sets exactly. The symlink messages match §6a byte for byte. A grep test finds none of the replaced names.

### R-11: Gating bypass / dark harness
**Test Scenarios**:
1. AC-7 verify-failed on W1. Assert exit 1, the legacy `~/.claude/settings.json` hooks still present, and R4 did not run.
2. W2 `skipped-foreign`: R1 does not run.
3. W5 `skipped-malformed`: the R10 twin is kept.
4. W2 verify-failed with `CLAUDE_CONFIG_DIR` set: the default `~/.claude.json` entry is still intact (it is never removed, D-028).

### R-12: SafeWriteError swallowed
**Test Scenarios**: inject a rename failure into W3, W4 and W5 through `init`. Assert exit 1 naming the file, not a `skipped*` leg.

### R-13: CAS conflict
**Test Scenarios**:
1. A one-time mutation at `before-rename`: attempt 2 succeeds and both changes are present.
2. A mutation on every attempt: exit 1 `conflict`, no temp left.
3. A hook event during `init` does not fail it on POSIX.

### R-14: Git-exclude branches
**Test Scenarios**: one real-git fixture per FR-42 row (the AC-8 list). In addition:
- `GIT_DIR` set to another repo in the parent env;
- a subdirectory R named `a*b[1]!`;
- `git diff --exit-code` and `git diff --cached --exit-code` in every case.

### R-15: JSON fidelity
**Test Scenarios**: round-trip fixtures with a `__proto__` key, an integer above 2^53, nested key order and each indent style. A file with no owned change is never rewritten.

### R-16: Parse-failure posture
**Test Scenarios**: one fixture per §3.4 row, through `init`. Expected results:
- unparseable user `opencode.json`: exit 0, a warning, and a `status` line naming it as leaving OpenCode unwired;
- unparseable project `opencode.json` during the removal: exit 1 naming the file, file unchanged (D-029);
- `.claude.json` and the settings files: exit 1, file unchanged.

### R-17: Override edge values / env mismatch
**Test Scenarios**:
1. Each var x {`""`, `"  "`, `~/x`, `cfg`, a NUL value, `<abs>/`}: exactly one warning for the invalid values, and the same destinations as with the var unset.
2. When an override is in effect, the FR-51 note is printed.
3. With `CLAUDE_CONFIG_DIR=<d>` set, a `claude mcp get` run without the var still finds the default entry (D-028).

### R-18: status local-scope check and the default profile
**Test Scenarios**: each `claudeMcp.status` value is checked in text and JSON:
1. `wired`: an owned entry in the resolved file.
2. `missing`: after a simulated `claude purge`. The line names the resolved file, gives the init hint, and the exit code is 0.
3. `foreign`: a non-owned entry.
4. `unreadable`: an unparseable resolved file, exit 0.
5. `n/a`: outside a project, with `file` and `key` null.
6. With `CLAUDE_CONFIG_DIR=<d>` and an owned entry only in the default `~/.claude.json` (`projects[root]` and/or top-level), status reads `missing` for `<d>`, never `wired`. That entry appears as an `other profile:` line and in `alternateProfiles` (D-028), not in `staleLegacy`.
7. Case 1 from a linked worktree and with a win32-normalised key.

### R-19: Foreign-at-key
**Test Scenarios**: every AC-4 row also seeds a foreign value at the `unimatrix` key. It is kept, with a warning naming the file and key.

### R-20 to R-25
- **R-20:** FR-15 positive and negative plugin fixtures. Order is preserved and an empty array stays `[]`.
- **R-21:** a fault between staging and rename leaves no partial `pkg/<ver>/`. A concurrent complete destination is kept. `install.json` swaps only after the rename.
- **R-22:** an `openSync` spy checks `wx` and the name regex. A collision forces regeneration. A throwing mutate leaves no temp.
- **R-23:** `trust-selfcontained.test.js` passes. The installed copy is byte-identical. Hook-path latency is re-measured against the nan-024 ADR-005 budget.
- **R-24:** with injected win32, rename retries ×10 on EPERM/EBUSY/EACCES, a dir-fsync throw is ignored, and the containment compare is case-insensitive.
- **R-25:**
  - The AC-1(c) and AC-2(b) skip messages name the missing binary and #1053.
  - AC-2(b), run once with `OPENCODE_CONFIG_DIR=<tmp>/cfg` and once with only `XDG_CONFIG_HOME=<tmp>/xdg`:
    1. Run init, then `opencode debug config --standalone` with `HOME=<tmp>` and cwd = R. `--standalone` is mandatory; a test helper asserts the flag is present.
    2. Parse the JSON array. Assert that the document whose `path` equals the resolved `opencode.json` holds `mcp.servers.unimatrix` (or `mcp.unimatrix`) equal to the written entry.
    3. Assert that no document from `~/.config/opencode` holds an owned entry.
    4. Match the path and entry leniently, because the output format is unversioned. A parse failure fails the test with the raw output printed and is never treated as a pass.
    5. Use a bounded timeout, so the check fails with a reason and never hangs.
  - Precedence check: with both vars set, the `OPENCODE_CONFIG_DIR` document holds the entry, and `$XDG_CONFIG_HOME/opencode` is untouched and listed as AP-1.

## Behavioral-Outcome Coverage (from the scope behavioral lens)

Each scenario drives the CLI (`bin/unimatrix.js init|status` spawned, or `init()`/`wire()` with injected env). A writer or resolver unit test does not discharge an outcome.

| Outcome (from scope lens, as amended) | Entry point | Required scenario |
|---|---|---|
| Each harness wired; abandoned and shadowing entries gone; foreign unchanged | `init`, no overrides | Spawn init on a fixture seeded with G2, G3, `.mcp.json` and the legacy hook entries plus foreign siblings. Assert one owned entry per surface and foreign deep-equal. Devcontainer: real `claude mcp get unimatrix`, and `opencode debug config --standalone` shows the entry in the resolved document |
| A harness started with the same env sees the entry. The legacy OpenCode twin and the legacy hooks twin (D-030) are gone unless they are the same file; foreign hooks there are untouched. The default `~/.claude.json` entry is kept and listed (D-028) | `init` with each override var | Spawn init per var. Assert the resolved file holds the entry. R4/R10 twins removed, except in the same-file fixtures. The `~/.claude.json` default entry is byte-unchanged and `status` lists it as another profile. AC-1(c) `claude mcp get` with `CLAUDE_CONFIG_DIR=<d>`; AC-2(b) `opencode debug config --standalone` with `OPENCODE_CONFIG_DIR`/`XDG_CONFIG_HOME` |
| A warning names the ignored var; outcome = no-override case | `init` with a relative value | Spawn init with `CLAUDE_CONFIG_DIR=cfg`. Assert the warning, and the manifest equals the no-override manifest |
| User/global link kept and target changed. A project link inside the root is kept; one outside the root fails loud. No temp left | `init` on symlinked configs | R-03 scenarios 1-10 through spawned init. AC-3 user/global links: absolute, relative and 2-hop. Symlinked `trust.json`: exit 1 with the §6a text |
| Exit 1 names the file; harness keeps old or new wiring | `init` that cannot verify | `init()` with a verify-failed fault on W1/W2. Assert exit 1 naming the file, and the old wiring is still present |
| Every leg `unchanged`; no mtime or mode change, including the exclude file | second `init` | Spawn init twice (git repo, overrides, `--bundle`). Every leg is `unchanged`, mtimes and modes are equal, no D-013 notice |
| Lists exactly the real run's writes, removals, twins, mode repairs and exclude append; nothing changed | `init --dry-run` | Spawn `init --dry-run`, then `init`. Tree hash and modes unchanged after the dry run; manifest and changed-set parity |
| `git status` never shows settings.local.json; no tracked change; outside git the skip is reported | `init` in a repo / outside git | Spawn init in a repo, a linked worktree, a submodule (`--project-dir`), a non-repo dir and with no git on PATH. Assert porcelain, a clean diff and the skip leg |
| Prints resolved files; names a missing local-scope entry with the init hint; lists other profiles; exits 0 | `status` | R-18 scenarios 2 and 6 via spawned `status`, plus an unparseable user `opencode.json` |
| Re-run/status advice printed | `init` while Claude runs | Spawn init where `.claude.json` changes: the FR-52 notice is printed once, is absent when unchanged, and is prefixed in dry-run |
| A loose mode on credential or trust input is repaired | `init`, then a hook event via the installed shim | R-01 scenarios 1-5 through spawned init and the real shim |

## Integration Risks

- **Plan order across harnesses (R-11).** A throw in a Codex step ends the run after the verified Claude steps. An OpenCode failure leaves the Claude and Codex wiring complete.
- **W2 + R1 in one mutate.** Two legs come from one write. Verify sees the entry present and the user-scope key absent. Dry-run emits both legs.
- **Enforced-mode flag plumbing (R-01).** Each enforced caller (install-store, credstore, the root marker, bin copy) must pass the flag. A missing caller passes the content tests and fails only the mode-drift test.
- **D-026 vs the D-007 identity helper.** Both use realpath logic. They must share the root-realpath and ancestor resolution so they cannot disagree.
- **credstore lazy require.** The hook hot path must not load `safe-write.js`. Check the require cache after a hook event.
- **`trust.js` dual role.** It is both writer core and a trust-set member. Rewrite it by rename only.

## Edge Cases

- `CLAUDE_CONFIG_DIR` points to a dir that does not exist yet: parents are created. Record the dir mode.
- `.claude.json` above 1 MB: NFR-04 timing, with a hash per attempt.
- `plugin` present but not an array, or `[]`: no leg.
- An exclude file with CRLF or no trailing newline.
- R as a repo subdirectory with prefix P. R itself reached through a symlink (R-03 scenario 5).
- A hard-linked target: the inode is replaced. On win32 the next event runs a full trust re-check.
- `""` override: unset, no warning. Whitespace: warns.
- An enforced file owned by another uid (restored backup): chmod fails, which raises a `SafeWriteError` `io`, exit 1 naming the file.

## Security Risks

| Untrusted input | Damage | Blast radius | Covered by |
|---|---|---|---|
| Repo-committed symlinks at project paths | Writes steered outside the project | Limited to inside R by D-026; outside, exit 1 | R-03 |
| Env override values | Writes redirected (same user only) | User-writable dirs | R-17. Absolute-only, NUL rejected |
| Foreign JSON/TOML (`__proto__`, big ints, JSONC) | Pollution or silent alteration | The rewritten file | R-15, R-16 |
| `git --show-prefix` dir names | Ignore-pattern injection | That repo's ignores | R-14, FR-44 |
| `GIT_*` env | Wrong exclude file | Another repo's `info/exclude` | R-14, FR-41 |
| Credential/trust-file modes | Disclosure or trust refusal | Local users / all hook events | R-01, enforced by D-025 |
| Temp names | Pre-creation | Closed by `wx` + random name | R-22, AC-10 |

## Failure Modes

- **Writer error** (`SafeWriteError` of any code, including `outside-project` (D-026) or `io` from a chmod failure on an enforced file): exit 1 naming the file or link, no temp left, no later removal, previous wiring kept.
- **Parse failures:**
  - unparseable `.claude.json`, project settings files or project `opencode.json` (D-029): exit 1, file untouched;
  - unparseable user `opencode.json` or Codex files: warning, `skipped-malformed`, exit 0;
  - `status` names an unparseable user `opencode.json`.
- **Git** unavailable, not a repo, or a git error: a reported skip leg, exit code unaffected.
- **Trust failure after store:** exit 1. A mode drift on the enforced files is repaired before the trust check (D-025), so this is limited to ownership or ACL faults.
- **Crash mid-run:** a leftover twin is recoverable on the next init. A `.tmp-` file is pruned by pattern.

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|---|---|---|
| SR-01 | R-25 | Measured layout in ADR-001 #5937. AC-1(c) real `claude` |
| SR-02 | R-03, R-22 | FR-23 S1-S12. Dangling links fail loud. Temp in the real target dir. Project-link containment (D-026) |
| SR-03 | R-06, R-01 | FR-33 migrate tests first. Enforced modes on trust inputs and the credential (D-025) |
| SR-04 | R-24 | Best-effort dir fsync off on win32. Injected-fs tests |
| SR-05 | R-15, R-16 | §3.4 per-file posture (D-016/D-024/D-029). D-022 |
| SR-06 | R-14 | FR-42 steps 2 and 5 |
| SR-07 | R-14 | git `cwd = R`, `--git-path`, exit-code table, GIT_* scrub |
| SR-08 | R-05, R-11, R-10 | One plan table (ADR-004 #5959 / FR-10). §6a names. D-028 removes the `~/.claude.json` legacy removal |
| SR-09 | R-04 | One resolver (ADR-002 #5960). Parity test |
| SR-10 | R-09 | `scrubbedEnv` plus the `assertTmpHome` override check |
| SR-11 | R-11 | Gated removals, a throw ends the run, `skipped-dest-unwired` |
| SR-12 | R-20 | FR-15 forms plus negative fixtures |
| SR-13 | R-17 | FR-50/FR-51 lines. D-028 keeps the default Claude entry, so a Claude without the var still works |
| SR-14 | R-25 | AC-1(c) `claude mcp get` and AC-2(b) `opencode debug config --standalone` (D-027). Codex file-level (D-012, #1053) |
| SR-15 | R-07 | One planning path. Parity harness. FR-42 step-7 exception |
| SR-16 | R-18 | Status reads only the resolved file. The default `~/.claude.json` is listed as another profile (D-028) |
| SR-17 | R-17 | Warning naming the var, repeated by status |

## AC Coverage Mapping (as amended by D-026, D-027, D-028)

| AC | Risks | Required scenarios |
|---|---|---|
| AC-1 (D-028) | R-04, R-18, R-25 | With `CLAUDE_CONFIG_DIR=<d>`, MCP is written only to `<d>/.claude.json`. The default `~/.claude.json` entry (including `projects[root]`) is kept byte-unchanged and listed by `status`. There is no `removed` leg for it. Real `claude mcp get` (skips with reason) |
| AC-2 (D-027) | R-05, R-17, R-25 | One case per chosen-by. The `~/.config/opencode` twin is removed, except in the same-file fixtures. FR-13 migration. `opencode debug config --standalone` shows the entry in the resolved document and none in `~/.config/opencode` (skips with reason) |
| AC-3 (D-026) | R-03, R-06 | User/global links followed (absolute, relative, chained). Project links inside the root are followed; outside the root, exit 1 with `outside-project` naming the link. Dangling: exit 1. Symlinked `trust.json`: exit 1 with the §6a text |
| AC-4 | R-15, R-19, R-20 | RT table rows (minus RT-05, D-028) with foreign siblings and foreign-at-key. Plugin fixtures |
| AC-5 | R-02, R-08, R-01 | Double init with overrides, twins, `--bundle` and git. mtime, bytes and mode everywhere. Zero renames |
| AC-6 | R-05, R-11 | Duplicate seeding plus a counting scanner. Alternate profiles, including the default `~/.claude.json` under the override, are excluded |
| AC-7 | R-11, R-13 | Fault seam: conflict once, conflict always, verify-failed. Old wiring kept, no temp |
| AC-8 | R-14, R-03 | One real-git fixture per FR-42 row. Worktree; submodule exclude outside R accepted. Prefix, symlinked exclude, dry-run |
| AC-9 | R-07 | Parity over all fixtures, including mode-drift and outside-root links. Tree hash and modes unchanged |
| AC-10 | R-22, R-06 | `wx` + name regex. Collision. Failure at each stage for each FR-20 caller. No `.pid` temp names |

## Open Items for the Human

None.
- OI-1 (R4/RT-03, owned hooks in the default `~/.claude/settings.json` under `CLAUDE_CONFIG_DIR`) is resolved by D-030. R4 is kept, and the R-05 R4 same-file fixtures and R-11 scenario 1 stand as normal scenarios.
- Earlier items were closed by D-025 (modes) and D-026 (repo-controlled symlinks).

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|---|---|---|
| Critical | 0 | 0 |
| High | 8 (R-02..R-09) | 30 |
| Medium | 11 (R-01, R-11..R-16, R-18, R-19, R-24, R-25) | 31 |
| Low | 6 (R-10, R-17, R-20..R-23) | 10 |
