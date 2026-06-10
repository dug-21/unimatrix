# nan-016 Architecture — UDS Dogfooding Re-Release Capability

> Slice A only (rescoped 2026-06-10). Delivers the build+copy-install of the in-repo TS
> hook client to a stable external path, the switchover mechanism, an effect-verification
> harness, and a runbook. **The live flip is DELIVERED, NOT EXECUTED here** — it is a
> deferred human action that also starts the F6 (#682) soak clock.

## System Overview

This repo currently dogfoods the **Rust** hook: every entry in `.claude/settings.json`
points at `/workspaces/unimatrix/target/release/unimatrix hook <EVENT>`. The F6 (#682)
Rust-hook retirement soak needs to run this repo's own hooks on the **TS hook client**
(`packages/unimatrix/lib/hook-client/`) — but there is no reproducible, isolated local
re-release of that client to switch to.

nan-016 closes exactly that gap. It adds **build/runbook tooling around an unmodified
client** (C-8): scripts that build + copy-install the client to a fixed external path, a
switchover script that repoints hooks via the shipped `mergeSettings`, a harness that
proves both by effect against a scratch project, and a runbook. Nothing in `lib/` changes.

Where it sits in the larger system:

- **Consumes** the F3/F4a shipped client (`lib/hook-client/**`, `lib/merge-settings.js`)
  as a frozen contract. nan-016 imports `mergeSettings`/`buildHookClientCommand`; it does
  not alter them.
- **Enables** F6: re-running the build+install script is the named soak-reset point; the
  switchover script (when eventually run by a human) starts the soak clock.
- **Does not touch** the Rust `hook.rs` or the `npx … init` local path (AC-05 / C-8).

### Transport / state grounding (verified in code)

`lib/hook-client/config.js` derives the project root by walking up to `.git` from the
hook's **cwd** (`walkToProjectRoot`), hashes that root (`computeProjectHash`), and from
the hash derives **both** the UDS socket (`~/.unimatrix/{hash}/unimatrix.sock`) and the
client state dir (`~/.unimatrix/{hash}/hook-client/`). The hash keys on the **project
root, not the client install location** (Unimatrix #4923). Therefore a client copy-installed
*anywhere*, run from this repo's cwd, resolves the **same** socket and state as the in-repo
client or the Rust binary would. This is the load-bearing fact behind three decisions:

1. No separate state/socket config is needed for the dogfood client (it shares
   `~/.unimatrix/{hash}/` with the running Rust daemon — by design, per #4923).
2. "Isolation" (AC-03) is **code-freezing** of the copied `lib/` bytes, **NOT**
   state-dir separation (addresses **SR-07**).
3. The switchover does no daemon lifecycle management; the local UDS daemon already runs
   and the client fail-opens if it is absent (addresses **SR-08**, honors C-7).

## Component Breakdown

Four components, all net-new, all outside `packages/unimatrix/lib/` (C-8). All committed
shell/JS lives under `scripts/`; the harness lives under `packages/unimatrix/test/` so it
runs inside the existing `node --test` infrastructure (cumulative test rule).

| # | Component | File (committed) | Responsibility |
|---|-----------|------------------|----------------|
| 1 | Build + copy-install script | `scripts/dogfood-install.sh` | Build/pack `packages/unimatrix`, clean-replace install a frozen client tree to `~/.unimatrix/dogfood-client/`. Idempotent. The F6 soak-reset point. |
| 2 | Switchover script | `scripts/dogfood-switchover.sh` | Repoint a target repo's hooks to the installed client via `mergeSettings`; provide rollback to the Rust hook. Operates on the live repo only when a human runs it; the harness runs it against a scratch root. |
| 3 | Effect-verification harness | `packages/unimatrix/test/dogfood-effect.test.js` | Prove AC-02 (switchover repoints + matcher delta) and AC-03 (code-freeze isolation) by **effect** against a scratch project root + scratch `settings.json` and the **real installed path**. Never touches this repo's live settings. |
| 4 | Runbook | `product/features/nan-016/RUNBOOK.md` | Document promotion (= re-run install = F6 reset), switchover, rollback, the matcher-narrowing delta, and that the flip is deferred to a no-active-feature window. |

### Component 1 — Build + copy-install (`scripts/dogfood-install.sh`)

**Invocation:** `scripts/dogfood-install.sh` (no args; optional `--target <dir>` defaulting
to `~/.unimatrix/dogfood-client`). Run from anywhere; resolves repo root via `git rev-parse`.

**Mechanism (ADR-001): `npm pack` + extract.** Steps:

1. `npm pack` in `packages/unimatrix` → produces `dug-21-unimatrix-<version>.tgz` in a
   temp dir. `npm pack` honors the `files` array, so the tarball is exactly the runtime
   asset set: `bin/ lib/ skills/ postinstall.js protocols/` — **and crucially excludes the
   platform binary** (it is an `optionalDependency`, not bundled). The hook client needs
   only `node` + `lib/hook-client/**`; it never spawns the binary (addresses **SR-01**:
   the frozen tree is complete *for the client's needs*, and asserted below).
2. **Clean-replace** the target: remove `~/.unimatrix/dogfood-client/` if present, then
   extract the tarball's `package/` into it. Replace-not-overlay makes re-runs deterministic
   and prevents a stale prior install shadowing a new build (addresses **SR-02**). The
   removal+extract is staged (extract to a sibling temp dir, then atomic `mv` over the
   target) so a partially-extracted tree is never observable as "the installed client."
3. **No postinstall runs.** Extraction is a file copy; `postinstall.js` is copied as an
   inert file but never executed. This is deliberate — `postinstall` only downloads the
   ONNX model for the *binary*, which the client does not use. No host side-effects (SR-01).
4. **Assert completeness:** after extract, verify `lib/hook-client/index.js` and the full
   `lib/hook-client/*.js` set + `lib/merge-settings.js` + `lib/hook-client/config.js`
   exist and that `node <target>/lib/hook-client/index.js SessionStart </dev/null` exits 0
   (a smoke fail-open check — see SR-08). Missing-asset → loud non-zero exit (this script
   is tooling, not a hook; it may be loud, unlike the fail-open hooks).

**Idempotency / re-run:** clean-replace ⇒ second run yields a byte-identical tree from the
same source (build is dependency-free, C-9 — addresses **SR-03**). Re-running IS the F6
soak-reset (AC-01).

**Why a fixed dir, not npm global prefix:** the npm global prefix under nvm is node-version
-pinned (`.../node/vX.Y.Z/...`) and does not survive a container rebuild that bumps node
(node is currently v24). A fixed `~/.unimatrix/dogfood-client/` gives a rebuild-stable hook
command path (ADR-002, OQ-1, #4923).

### Component 2 — Switchover (`scripts/dogfood-switchover.sh`)

**Invocation:**
- `scripts/dogfood-switchover.sh promote [--settings <path>] [--client <dir>]` — repoint
  hooks to the installed client.
- `scripts/dogfood-switchover.sh rollback [--settings <path>]` — revert hooks to the Rust
  binary.
- `--dry-run` on either, forwarded to `mergeSettings`'s `dryRun`.

Defaults: `--settings` = `<repo>/.claude/settings.json`, `--client` =
`~/.unimatrix/dogfood-client`. **The harness always passes explicit scratch paths** so no
test ever defaults onto the live repo (addresses **SR-06**).

**How promote repoints (ADR-003):** the script shells a tiny Node one-liner that requires
the **shipped** `lib/merge-settings.js` from the *installed* client and calls:

```
mergeSettings(settingsPath, {
  events: HOOK_EVENTS,
  commandForEvent: (event) =>
    buildHookClientCommand(path.join(clientDir, "lib/hook-client/index.js"), event)
}, { dryRun })
```

This is the same call shape `initRemote` uses (verified in `lib/init.js` step 4). Consequences,
all intentional:

- The emitted command is `node <clientDir>/lib/hook-client/index.js <EVENT>` (path quoted
  iff it contains whitespace — `buildHookClientCommand`). `~/.unimatrix/dogfood-client` has
  no whitespace, so it is emitted bare and matched by the `UNIMATRIX_PATTERNS` node-client
  arm — re-points are idempotent across command forms.
- Because it routes through `mergeSettings` (not a string swap), the eventual soak exercises
  **shipped matcher semantics**: `EVENT_MATCHERS` narrows `PreToolUse` from the live `"*"`
  to `context_cycle|mcp__unimatrix__context_cycle` (`PRETOOLUSE_CYCLE_MATCHER`). This is a
  deliberate behavioral delta vs. today's settings (addresses **SR-05**); the harness
  asserts it and the runbook calls it out.
- `isUnimatrixHook` recognizes the existing Rust-binary commands, so promote **updates them
  in place** (Rust → node) rather than appending duplicates. Foreign hooks are untouched.
- `SubagentStop` is opt-in via `settings.local.json` (`mergeSettings` filters it unless
  enabled). On a scratch root with no opt-in, the registered set is the 8 non-opt-in events
  — the harness asserts this rather than assuming 9.

**How rollback repoints (ADR-003):** call `mergeSettings(settingsPath, <rustBinaryPath
string>, {dryRun})`. Passing a **string** triggers `normalizeCommandSource`'s legacy arm,
which emits `LD_LIBRARY_PATH=<binDir> <binary> hook <EVENT>` over the full `HOOK_EVENTS`
set — byte-identical to the pre-F5 local form (modulo the two events FR-21 added and the
narrowed PreToolUse matcher, which are shipped client behavior, not nan-016 deltas). The
rust binary path is `<repo>/target/release/unimatrix`. Rollback is therefore the same
mechanism in reverse; there is no bespoke revert logic to drift.

**No daemon lifecycle (ADR-004, OQ-1d):** the script never starts/stops/probes the daemon.
The client fail-opens if the socket is absent (C-7). This keeps the switchover safe even
when the daemon is down (addresses **SR-08**).

### Component 3 — Effect-verification harness (`test/dogfood-effect.test.js`)

A `node --test` file that makes AC-02/AC-03 **real proofs, not string-diffs** (addresses
**SR-04**). Pattern: build a throwaway project root in a temp dir, run the real scripts
against it, then assert on resulting state and re-fired hook behavior.

Fixtures per test (all under `os.tmpdir()`, cleaned up):

- A scratch project root: a temp dir with a `.git/` **directory** (so `walkToProjectRoot`
  treats it as a real root and hashes it to its own `~/.unimatrix/{scratchHash}/`, isolated
  from this repo's hash) and a scratch `.claude/settings.json` seeded with the current
  Rust-hook shape (`"*"` PreToolUse) plus a **foreign hook** to prove preservation.
- The **real installed client** at `~/.unimatrix/dogfood-client/` (test depends on
  Component 1 having run; the harness invokes `dogfood-install.sh` in a `before` hook, or
  skips with a clear message if the install is absent).

**AC-02 proof (switchover by effect):**
1. Run `dogfood-switchover.sh promote --settings <scratch> --client <installed>`.
2. Parse the resulting scratch `settings.json` and assert: every Unimatrix event command is
   `node <installed>/lib/hook-client/index.js <EVENT>`; the **PreToolUse matcher equals
   `PRETOOLUSE_CYCLE_MATCHER`** (the SR-05 delta); the foreign hook survives; no duplicate
   Unimatrix entries.
3. **Re-fire a real hook against the installed path:** `execFileSync("node",
   [installedIndexJs, "SessionStart"], { cwd: scratchRoot, input: JSON.stringify({...}) })`
   and assert **exit 0 and empty stdout** — proving the emitted command actually runs the
   installed client and fail-opens (the daemon-absent case, since the scratch hash has no
   socket — addresses **SR-08**). This is the non-vacuous core of SR-04.

**AC-03 proof (copy-install isolation = code freeze, SR-07):**
1. Capture the installed `lib/hook-client/index.js` bytes (hash) after install.
2. Append a marker (e.g., a comment or `process.stderr.write`) to the **in-repo**
   `packages/unimatrix/lib/hook-client/<somefile>.js` *in a temp copy of the repo tree* —
   OR, to avoid mutating the working tree at all, assert the installed bytes are unchanged
   after a no-op and document that editing in-repo source cannot reach the frozen copy
   because the path is an external absolute copy. The chosen proof: re-read the installed
   file's hash and assert it equals the captured hash, AND assert the installed path is NOT
   a symlink (`fs.lstatSync(installedIndexJs).isSymbolicLink() === false`) — the latter is
   the structural guarantee that no `npm link` symlink leak exists (C-6). Isolation is
   framed explicitly as **byte/behavior freeze of the installed `lib/`**, NOT state-dir
   separation (#4923 cited inline).
3. Re-fire the hook against the installed path after an in-repo edit attempt and assert
   identical exit-0/empty-stdout behavior.

**Rollback proof:** run `rollback` against the same scratch settings and assert every
command reverts to `LD_LIBRARY_PATH=… <rust> hook <EVENT>` and `isUnimatrixHook` still
owns them (idempotent re-point).

**Hard constraint (SR-06):** the harness reads `<repo>/.claude/settings.json` only to
**copy its shape into the scratch fixture**; it never writes to it. A guard asserts the
`--settings` arg passed to the script is under `os.tmpdir()`.

### Component 4 — Runbook (`RUNBOOK.md`)

Documents (AC-04): **promotion** = re-run `dogfood-install.sh` (= F6 reset point);
**switchover** = run `dogfood-switchover.sh promote` (the deferred human flip, in a
no-active-feature window — this starts the F6 clock); **rollback** = `dogfood-switchover.sh
rollback`; the **PreToolUse matcher-narrowing** as an intended delta the operator will see;
and the C-7 fail-open posture / no daemon management. Explicitly states nan-016 does **not**
execute the flip and does **not** start the soak clock.

## Component Interactions / Data Flow

```
dogfood-install.sh ──npm pack──▶ tarball ──extract(clean-replace)──▶ ~/.unimatrix/dogfood-client/
                                                                            │ (frozen lib/ tree)
                                                                            ▼
dogfood-switchover.sh promote ──require(installed lib/merge-settings.js)──▶ mergeSettings(scratchOrLive settings, node-client commandSource)
                                                                            │
                                                                            ▼
                                                              settings.json hooks repointed to
                                                              `node <client>/lib/hook-client/index.js <EVENT>`
                                                                            │
test/dogfood-effect.test.js ──invokes both scripts on SCRATCH root──▶ asserts settings shape + re-fires hook (exit 0)
```

The switchover deliberately requires `mergeSettings` from the **installed** client (not the
in-repo one), so promotion validates that the frozen copy's merge logic is itself runnable.

## Technology Decisions

| ADR | Decision |
|-----|----------|
| ADR-001 | Copy-install via `npm pack` + extract (not `npm install --prefix`, never `npm link`) |
| ADR-002 | Fixed install dir `~/.unimatrix/dogfood-client/` (not npm global prefix) |
| ADR-003 | Switchover repoints through shipped `mergeSettings`, both promote and rollback |
| ADR-004 | No daemon lifecycle management; rely on client fail-open (C-7) |
| ADR-005 | Effect-verification via scratch project root + re-fired hook; never touch live settings |

## Integration Points

- **`lib/merge-settings.js`** (frozen, C-8): `mergeSettings`, `buildHookClientCommand`,
  `HOOK_EVENTS`, `EVENT_MATCHERS`, `PRETOOLUSE_CYCLE_MATCHER`, `isUnimatrixHook`,
  `normalizeCommandSource`. nan-016 imports from the **installed** copy.
- **`lib/hook-client/config.js`** (frozen): project-root-hash derivation — explains why the
  installed client shares state with the daemon (#4923).
- **`lib/hook-client/index.js`** (frozen): the hook entry the emitted command invokes;
  guarantees exit-0/empty-stdout fail-open (the SR-08 anchor).
- **`packages/unimatrix/package.json`** `files` array: defines what `npm pack` freezes.
- **`<repo>/.claude/settings.json`**: the live target the *human* flip mutates; the harness
  only reads it to copy its shape.
- **`<repo>/target/release/unimatrix`**: the Rust binary rollback reverts to.
- **`packages/unimatrix/test/check-hook-client-size.js`** (C-04): unchanged; nan-016 adds
  no `lib/hook-client/**` bytes, so the gate keeps passing (addresses SR-09 / AC-06).

## Integration Surface

| Integration Point | Type / Signature | Source |
|-------------------|------------------|--------|
| `mergeSettings(filePath, commandSource, options)` | `(string, {events:string[], commandForEvent:(e)=>string} \| string, {dryRun:boolean}) → {actions:string[], content:object}` | `lib/merge-settings.js` |
| `buildHookClientCommand(clientPath, event)` | `(string, string) → "node <quoted-path> <event>"` | `lib/merge-settings.js` |
| `normalizeCommandSource(string)` legacy arm | string → `LD_LIBRARY_PATH=<binDir> <binary> hook <event>` over `HOOK_EVENTS` | `lib/merge-settings.js` |
| `HOOK_EVENTS` | `string[]` (9 events incl. opt-in `SubagentStop`) | `lib/merge-settings.js` |
| `EVENT_MATCHERS.PreToolUse` | `"context_cycle\|mcp__unimatrix__context_cycle"` (narrowed from `"*"`) | `lib/merge-settings.js` |
| `isUnimatrixHook(entry)` node-client arm | matches `node <path>/hook-client/index.js <EVENT>` (quoted/bare, `/` or `\`) | `lib/merge-settings.js` |
| Installed client entry | `node ~/.unimatrix/dogfood-client/lib/hook-client/index.js <EVENT>` | emitted by switchover |
| Installed client path | `~/.unimatrix/dogfood-client/` (fixed; from tarball `package/` root) | ADR-002 |
| Rust rollback command | `LD_LIBRARY_PATH=<repo>/target/release <repo>/target/release/unimatrix hook <EVENT>` | rollback path |
| Project-root hash | `sha256(realpath(projectRoot)).slice(0,16)` → `~/.unimatrix/{hash}/` | `lib/hook-client/config.js` |
| Hook fail-open contract | exit code 0, empty stdout on every path incl. daemon-absent | `lib/hook-client/index.js` |
| `npm pack` tarball contents | `files` array: `bin/ lib/ skills/ postinstall.js protocols/`; NO platform binary | `package.json` |

## Risk Coverage Summary

| Risk | Where addressed |
|------|-----------------|
| SR-01 (incomplete frozen tree / postinstall side-effect) | ADR-001: `npm pack` honors `files`; postinstall copied-but-not-run; post-extract completeness + smoke assertion |
| SR-02 (stale install shadowing) | ADR-001: clean-replace via staged extract + atomic mv |
| SR-03 (build reproducibility) | C-9 dependency-free build ⇒ byte-stable; clean-replace |
| SR-04 (vacuous verification) | ADR-005 / Component 3: scratch root + real `mergeSettings` + re-fired hook (exit-0), not string-diff |
| SR-05 (matcher-narrowing delta) | ADR-003: harness asserts `PRETOOLUSE_CYCLE_MATCHER`; runbook calls it out |
| SR-06 (transient live-flip read as "executed") | ADR-005: scratch-only; tmpdir guard; live settings read-only |
| SR-07 (isolation = state vs code) | Component 3 AC-03: byte/behavior freeze + non-symlink assertion; #4923 cited; explicitly not state separation |
| SR-08 (fail-open / daemon-absent) | ADR-004 + Component 3: re-fired hook on scratch hash (no socket) asserts exit-0 |
| SR-09 (init / size-gate regression) | C-8 freeze; no `lib/` edits; AC-06 gate untouched |

## Open Questions

1. **Repo-tree mutation in the AC-03 isolation proof.** The cleanest non-vacuous proof of
   "editing in-repo source doesn't change the installed copy" literally edits in-repo
   source. To honor "no working-tree perturbation," the harness should perform the edit in
   a *throwaway copy* of `packages/unimatrix/lib/hook-client/` (or a git-stash-and-restore),
   not the live tree. Pseudocode/test-design should pin which. (Leaning: temp-copy edit +
   re-pack into a *second* scratch install dir, then assert the original installed bytes are
   unchanged — fully avoids the working tree.)
2. **Harness dependency on a prior install.** Component 3 needs Component 1 to have run.
   Decide whether the test runs `dogfood-install.sh` in a `before` hook (slower, hermetic)
   or skips when the install is absent (faster, but coverage-gappy in CI). Recommend the
   `before`-hook install into a **test-scoped temp dir** (not the real `~/.unimatrix/
   dogfood-client`) so the suite never disturbs a human-staged dogfood install.
3. **`scripts/` shell vs Node.** The scripts are POSIX shell wrappers around Node one-liners
   for the `mergeSettings`/pack logic. If the team prefers a single committed Node CLI
   (e.g. `scripts/dogfood.js`) over shell, the architecture is unaffected — same calls,
   same files. Flagged for the human / pseudocode agent.
4. **Follow-up issue for the flip.** Per SCOPE Tracking, the deferred live flip + F6
   soak-clock start should be a checklist item on #682 or a small follow-up issue. nan-016
   does not create it — flagged for the human.
