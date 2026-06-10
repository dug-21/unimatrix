# nan-016 — UDS Dogfooding Re-Release Capability — IMPLEMENTATION BRIEF

> Slice A only (rescoped 2026-06-10). Compiles the approved Session 1 design into an
> implementation-ready brief for Session 2 delivery. Architecture is the technical ground
> truth; where SPEC and ARCHITECTURE differ on the install mechanism, ADR-001 (`npm pack` +
> extract) is RESOLVED.

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/nan-016/SCOPE.md |
| Scope Risk Assessment | product/features/nan-016/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/nan-016/architecture/ARCHITECTURE.md |
| ADR-001 (copy-install via npm pack) | product/features/nan-016/architecture/ADR-001-copy-install-npm-pack.md |
| ADR-002 (fixed install dir) | product/features/nan-016/architecture/ADR-002-fixed-install-dir.md |
| ADR-003 (switchover via mergeSettings) | product/features/nan-016/architecture/ADR-003-switchover-via-mergesettings.md |
| ADR-004 (no daemon lifecycle) | product/features/nan-016/architecture/ADR-004-no-daemon-lifecycle.md |
| ADR-005 (effect-verification scratch harness) | product/features/nan-016/architecture/ADR-005-effect-verification-scratch-harness.md |
| Specification | product/features/nan-016/specification/SPECIFICATION.md |
| Risk Strategy | product/features/nan-016/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/nan-016/ALIGNMENT-REPORT.md |
| Acceptance Map | product/features/nan-016/ACCEPTANCE-MAP.md |

## Goal

Deliver a reproducible, idempotent build + copy-install of the in-repo TS hook client
(`packages/unimatrix`) to a fixed external path `~/.unimatrix/dogfood-client/`, plus a
committed switchover script (promote/rollback/dry-run) that repoints a target repo's hooks
at that installed client via the shipped `mergeSettings`, plus an effect-verification harness
and a runbook. The capability is **delivered and proven by effect but NEVER executed against
this repo's live `.claude/settings.json`** — the live flip and the F6 (#682) soak-clock start
are a deferred, deliberate human action in a no-active-feature window.

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| Build + copy-install (`scripts/dogfood-install.sh`) | pseudocode/dogfood-install.md | test-plan/dogfood-install.md |
| Switchover (`scripts/dogfood-switchover.sh`) | pseudocode/dogfood-switchover.md | test-plan/dogfood-switchover.md |
| Effect-verification harness (`packages/unimatrix/test/dogfood-effect.test.js`) | pseudocode/dogfood-effect.md | test-plan/dogfood-effect.md |
| Runbook (`product/features/nan-016/RUNBOOK.md`) | pseudocode/runbook.md | test-plan/runbook.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

Note: pseudocode and test-plan files are produced in Session 2 Stage 3a. The four components
above are fixed by the architecture (Component Breakdown table); actual file paths are filled
during delivery.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|------------|--------|----------|
| Install mechanism (SPEC OQ-B) | `npm pack` + extract with clean-replace (staged extract to sibling temp dir, atomic `mv` over target). NOT `npm install --prefix`; NEVER `npm link`. **Treat OQ-B as RESOLVED by ADR-001.** | ARCHITECTURE Component 1 | architecture/ADR-001-copy-install-npm-pack.md |
| Install location (OQ-1) | Fixed dir `~/.unimatrix/dogfood-client/` (overridable via `--target` for harness temp installs). Not the npm global prefix (node-version-pinned, breaks on container rebuild). | ARCHITECTURE Component 1 | architecture/ADR-002-fixed-install-dir.md |
| Switchover mechanism (OQ-1b/OQ-1c) | Repoint via shipped `mergeSettings` (require the **installed** copy's `lib/merge-settings.js`), both promote and rollback. Not a string swap. | ARCHITECTURE Component 2 | architecture/ADR-003-switchover-via-mergesettings.md |
| Daemon lifecycle (OQ-1d) | No start/stop/probe. Rely on unmodified client fail-open (C-7). | ARCHITECTURE Component 2 | architecture/ADR-004-no-daemon-lifecycle.md |
| Verification approach (SR-04) | Effect harness: scratch project root (real `.git/` dir under `os.tmpdir()`) + scratch `settings.json` + re-fired hook against the real installed path. Never touches live settings. | ARCHITECTURE Component 3 | architecture/ADR-005-effect-verification-scratch-harness.md |
| Switchover mode (OQ-2) | UDS-local. | SCOPE rescope header | — |
| CLAUDE.md block (OQ-4) | init does NOT append the knowledge block (uni-init's job). Out of scope. | SCOPE rescope header | — |
| Script location (OQ-A) | Repo-root `scripts/` for the two `.sh` scripts; harness under `packages/unimatrix/test/` (cumulative `node --test` infra). | ARCHITECTURE Component Breakdown | — |

### Open Questions Routed to Pseudocode/Test-Design (not blocking)

- **OQ-C (re-fire mechanics):** how the harness invokes the installed-path entrypoint with a
  synthetic hook payload + scratch project root so it resolves a daemon-absent UDS without
  contacting the live daemon while still proving fail-open. Architecture leaning:
  `execFileSync("node", [installedIndexJs, "SessionStart"], {cwd: scratchRoot, input: JSON})`,
  assert exit 0 / empty stdout. (RISK R-01, R-07.)
- **OQ-D / ARCH OQ-1 (AC-03 edit restoration):** perform the in-repo edit in a **throwaway
  copy** of the tree (or git stash/restore), never the live working tree; assert the working
  tree is clean after the test including on failure paths. (RISK R-13.)
- **ARCH OQ-2 (harness install target):** `before`-hook installs into a **test-scoped temp
  dir** via `--target`, never the real `~/.unimatrix/dogfood-client/`. (RISK R-15.)
- **ARCH OQ-3 (shell vs Node):** scripts are POSIX shell wrappers around Node one-liners for
  pack/mergeSettings. A single Node CLI is acceptable if the team prefers — same calls, same
  files. Pseudocode agent to pin.

## Files to Create/Modify

| File | Action | Summary |
|------|--------|---------|
| `scripts/dogfood-install.sh` | Create | `npm pack` `packages/unimatrix`; clean-replace install frozen tree to `~/.unimatrix/dogfood-client/`; assert completeness + smoke. Idempotent. F6 soak-reset point. |
| `scripts/dogfood-switchover.sh` | Create | `promote` / `rollback` / `--dry-run`; repoint a target settings file via the installed `mergeSettings`. Accepts `--settings` and `--client`. |
| `packages/unimatrix/test/dogfood-effect.test.js` | Create | `node --test` effect harness proving AC-02/AC-03 by re-fired hook against the installed path on a scratch root. Never touches live settings. |
| `product/features/nan-016/RUNBOOK.md` | Create | Documents promotion/switchover/rollback, matcher-narrowing delta, fail-open posture, deferred-flip boundary. |

**Frozen — MUST NOT be modified (C-8):** `packages/unimatrix/lib/hook-client/**`,
`lib/init.js`, `lib/merge-settings.js`, `lib/hook-client/config.js`, `package.json` runtime
behavior.

## Data Structures

- **Installed client tree** at `~/.unimatrix/dogfood-client/` — the extracted tarball
  `package/` root. Contains the `files`-array set: `bin/`, `lib/` (incl. `lib/hook-client/**`),
  `skills/`, `postinstall.js`, `protocols/`. **Excludes** the platform binary (it is an
  `optionalDependency`, not bundled). Entrypoint: `~/.unimatrix/dogfood-client/lib/hook-client/index.js`.
- **Scratch fixture** (per harness test, under `os.tmpdir()`): a temp dir with a real `.git/`
  **directory** (so `walkToProjectRoot` hashes it to its own isolated `~/.unimatrix/{scratchHash}/`)
  and a scratch `.claude/settings.json` seeded with the current Rust-hook shape (`"*"`
  PreToolUse) plus a foreign hook to prove preservation.
- **Emitted hook command (promote):** `node <clientDir>/lib/hook-client/index.js <EVENT>`
  (path quoted iff it contains whitespace).
- **Emitted hook command (rollback):** `LD_LIBRARY_PATH=<repo>/target/release <repo>/target/release/unimatrix hook <EVENT>`.
- **Project-root hash:** `sha256(realpath(projectRoot)).slice(0,16)` → `~/.unimatrix/{hash}/`
  (keys on project root, NOT client install location — #4923).

## Function Signatures (shipped, imported from the INSTALLED client — frozen contract)

| Signature | Source |
|-----------|--------|
| `mergeSettings(filePath, commandSource, options) → {actions:string[], content:object}` where `commandSource` is `{events:string[], commandForEvent:(e)=>string}` (promote) OR `string` (rollback, legacy arm), `options` is `{dryRun:boolean}` | `lib/merge-settings.js` |
| `buildHookClientCommand(clientPath, event) → "node <quoted-path> <event>"` | `lib/merge-settings.js` |
| `normalizeCommandSource(string)` legacy arm → `LD_LIBRARY_PATH=<binDir> <binary> hook <event>` over `HOOK_EVENTS` | `lib/merge-settings.js` |
| `isUnimatrixHook(entry)` — recognizes Rust-binary, legacy `unimatrix-server`, and node-client command forms (idempotent re-point) | `lib/merge-settings.js` |
| `computeProjectHash(root) → 16-hex`; `socketPathFor` → `~/.unimatrix/{hash}/unimatrix.sock`; `resolve(cwd)` walks to project root | `lib/hook-client/config.js` |

Constants: `HOOK_EVENTS` (9 events incl. opt-in `SubagentStop`); `EVENT_MATCHERS.PreToolUse =
PRETOOLUSE_CYCLE_MATCHER = "context_cycle|mcp__unimatrix__context_cycle"` (narrowed from `"*"`).

**Promote call shape** (same as `initRemote`, `lib/init.js` step 4):
```
mergeSettings(settingsPath, {
  events: HOOK_EVENTS,
  commandForEvent: (event) =>
    buildHookClientCommand(path.join(clientDir, "lib/hook-client/index.js"), event)
}, { dryRun })
```
**Rollback call shape:** `mergeSettings(settingsPath, "<repo>/target/release/unimatrix", { dryRun })`.

## Constraints

- **C-6 (copy-install only):** `npm pack` + extract; NEVER `npm link` (symlink reintroduces the
  source-mutation leak). Assert the installed entrypoint is NOT a symlink.
- **C-7 (fail-open hook posture):** the emitted node-client command must exit 0 / empty stdout
  on every path incl. daemon-absent. Switchover introduces no host-breaking hook path; no
  daemon dependency. (Tooling scripts MAY be loud on error; hooks may NOT.)
- **C-8 (no client changes):** do not modify `lib/hook-client/` logic nor the runtime behavior
  of `lib/init.js`, `lib/merge-settings.js`, `lib/hook-client/config.js`, `package.json`.
- **C-9 (dependency-free client):** shipped JS stays dependency-free (`test/check-zero-deps.js`).
- **C-04 (size gate):** comment-stripped ≤ 100 KB / raw ≤ 160 KB over `lib/hook-client/**/*.js`
  (`test/check-hook-client-size.js`). Regression guard, not a budget — C-8 adds no `lib/` bytes.
- **No live-settings mutation:** no test/harness writes `/workspaces/unimatrix/.claude/settings.json`.
  Live settings read-only (shape-copy into fixture); a tmpdir guard rejects the live path.
- **Clean-replace safety:** validate `--target` is non-empty and resolves under `~/.unimatrix/`
  (or an explicit absolute `--target`) before any removal; never degrade to `rm -rf` of a parent
  or `$HOME`.

## Dependencies

- **Tooling:** Node ≥ 18 (container is node v24), `npm pack`, POSIX shell.
- **External services:** local UDS daemon (assumed running; NOT managed by nan-016; hooks
  fail-open if absent).
- **Frozen code surfaces (held / not regressed):** `lib/merge-settings.js`, `lib/init.js`,
  `lib/hook-client/config.js`, `lib/hook-client/index.js`, `package.json` (`files` array,
  `postinstall`, version), `test/check-hook-client-size.js`, `test/check-zero-deps.js`,
  existing init tests (`test/init.test.js`, `test/init-integration.test.js`,
  `test/merge-settings.test.js`).
- **`<repo>/target/release/unimatrix`** — the Rust binary rollback reverts to.
- **Upstream features:** F3 (#679, shipped), F4a/vnc-027 (#680, shipped). Enables but does NOT
  start F6 (#682).
- **No new npm dependencies** (C-9).

## Risk Coverage Anchors (for test-design — RISK-TEST-STRATEGY.md)

| Priority | Risks | Mandate |
|----------|-------|---------|
| Critical | R-01 (vacuous verification), R-02 (non-atomic/overlay replace), R-03 (scratch-hash collision), R-04 (weak isolation proof) | Non-vacuous proof required; R-01 and R-04 REQUIRE a negative control that fails when the install/leak is broken. |
| High | R-05, R-06 (rollback drift), R-07 (daemon-absent non-zero), R-08 (live-settings breach), R-11 (`npm pack` drift / postinstall) | Loud tooling error on missing install; rollback round-trip; daemon-absent exit-0; tmpdir guard + pre/post live-settings hash; postinstall proven inert. |
| Medium | R-09 (matcher delta), R-10 (event-set 8-vs-9), R-12 (container durability), R-13 (in-repo edit restore), R-14 (init/size regression), R-15 (before-hook install scope) | Matcher asserted against the **imported** `PRETOOLUSE_CYCLE_MATCHER` (not a literal); event count asserted against actual opt-in state; working tree clean after isolation test; install only into test-scoped temp dir. |

All nine scope risks SR-01..SR-09 trace to ≥1 R-ID (RISK-TEST-STRATEGY Scope Risk Traceability).

## NOT in Scope

- Executing the live switchover / flipping this repo's hooks (deferred human action; starts the
  F6 clock).
- Starting or defining the F6 (#682) soak pass/fail duration.
- Retiring or modifying Rust `hook.rs` (→ F6).
- Any change to `lib/hook-client/` logic, transports, or parity (owned by vnc-027/#680).
- `npm link`-based dogfooding (forbidden, C-6).
- macOS local mode, darwin packages, `build-darwin-*` jobs (CUT — Mac is client-only).
- `init --remote` unification, skills-in-remote, remote-install size gate (DEFERRED → #725).
- Container HTTPS serving + network security posture (→ #726).
- Appending the knowledge block to CLAUDE.md (uni-init's job; OQ-4).
- **Creating the follow-up flip-tracking issue on #682** (flagged for the human; nan-016 does
  not create it).
- Daemon lifecycle management (start/stop/probe).

## Alignment Status

ALIGNMENT-REPORT.md: **all six checks PASS** (Vision Alignment, Milestone Fit, Scope Gaps,
Scope Additions, Architecture Consistency, Risk Completeness). No variances require approval.

Two human-awareness items carried from the source docs (NOT new variances):

1. **Deferred live flip / F6 soak-clock start follow-up is NOT created by nan-016.** The human
   must track the eventual flip on #682 or a small follow-up issue. nan-016 delivers and proves
   the mechanism only. **FLAGGED FOR THE HUMAN.**
2. **Doc-consistency nit (now resolved in this brief):** SPEC OQ-B lists both install
   mechanisms as open while ARCHITECTURE ADR-001 already pins `npm pack` + extract. Architecture
   is the technical ground truth — **install mechanism is RESOLVED to `npm pack` + extract
   (OQ-B resolved-by-ADR-001).** Session 2 implements `npm pack`; do not treat the mechanism as
   open.
