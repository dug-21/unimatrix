# nan-016 — UDS Dogfooding Re-Release Runbook

> Operator runbook for the in-repo TS hook client dogfooding capability. nan-016
> **delivers and proves** the build/install + switchover/rollback mechanism. It does
> **NOT execute the live flip** against this repo's `.claude/settings.json`, and it does
> **NOT start the F6 (#682) soak clock**.

## 0. Boundary statement (read first)

- The **live flip is DEFERRED** to a no-active-feature window. nan-016 delivers and proves
  the mechanism by effect (scratch fixtures) only; it never repoints this repo's live
  `.claude/settings.json`.
- Running the switchover against this repo's live settings is a deliberate **later human
  action**. **Doing so starts the F6 (#682) soak clock.**
- **FLAGGED FOR THE HUMAN:** nan-016 does **not** create the follow-up flip-tracking issue
  on #682. Tracking the eventual flip (and the soak pass/fail window) is the human's to own.
- nan-016 changes nothing under `packages/unimatrix/lib/` — the client is frozen (C-8). All
  behavior below is produced by the **shipped** `mergeSettings` engine.

## 1. Promotion — re-release the client (F6 soak-reset point) [FR-14a]

```sh
scripts/dogfood-install.sh [--target <dir>]
```

- Default `--target`: `~/.unimatrix/dogfood-client/` (a fixed external path).
- Builds the in-repo client via `npm pack` of `packages/unimatrix`, then **clean-replace**
  installs the frozen tree (staged extract + atomic `mv`) into the target. Replace, not
  overlay — a stale prior install never shadows a new build.
- **Idempotent:** the build is dependency-free, so a re-run yields a byte-identical tree from
  the same source.
- **Re-running promotion IS the F6 soak-reset point.** Each run of `scripts/dogfood-install.sh`
  resets the soak to the newly installed bytes. This is the named F6 (#682) soak-reset.
- This script is **tooling, not a hook** — it is loud on error (non-zero exit, message) if an
  asset is missing or the smoke check fails.
- Fixed dir is chosen over the npm global prefix because the global prefix under nvm is
  node-version-pinned and does not survive a container rebuild that bumps node (NFR-7).

## 2. Switchover — promote (the deferred live flip) [FR-14c]

```sh
scripts/dogfood-switchover.sh promote [--settings <path>] [--client <dir>] [--dry-run]
```

- Defaults: `--settings` = `<repo>/.claude/settings.json`, `--client` =
  `~/.unimatrix/dogfood-client/`.
- Repoints the target's Unimatrix hooks to the installed client command form
  `node <client>/lib/hook-client/index.js <EVENT>`, via the **installed** copy's
  `lib/merge-settings.js` (`mergeSettings`). Existing Rust-binary entries are updated in place
  (no duplicates); foreign hooks are preserved.
- `--dry-run` previews the actions without writing the settings file.
- **DEFERRED LIVE FLIP:** running `promote` against this repo's live `.claude/settings.json`
  is the deferred flip, to be performed by a **human in a no-active-feature window**. **Doing
  so starts the F6 soak clock.** nan-016 never runs this against live settings; the effect
  harness runs it only against a scratch root under `os.tmpdir()`.

## 3. Rollback — revert to the Rust `hook.rs` binary [FR-14b]

```sh
scripts/dogfood-switchover.sh rollback [--settings <path>] [--dry-run]
```

- Reverts the target's Unimatrix hooks to the Rust `hook.rs` binary command form:
  `LD_LIBRARY_PATH=<repo>/target/release <repo>/target/release/unimatrix hook <EVENT>` over
  `HOOK_EVENTS`.
- **Same engine in reverse — no bespoke revert logic.** Rollback passes the Rust binary path
  as a **string** to `mergeSettings`, which triggers `normalizeCommandSource`'s legacy arm to
  emit the command form above. This reproduces the pre-switchover Rust command form exactly.
- **Cross-check (AC-04):** the rollback command form documented here is the one produced by
  the `normalizeCommandSource` legacy arm — it is not a copy-pasted literal that can drift, and
  it matches what the `dogfood-effect.test.js` rollback test proves by effect (R-06). Rollback
  is idempotent; foreign hooks are preserved.

## 4. PreToolUse matcher-narrowing — intended behavioral delta [FR-14d]

- When `promote` runs, it applies the shipped `EVENT_MATCHERS`, which **narrows the PreToolUse
  matcher from the live `"*"` to `context_cycle|mcp__unimatrix__context_cycle`**
  (`PRETOOLUSE_CYCLE_MATCHER`).
- This is an **INTENDED behavioral delta** the operator will observe at flip time. It is
  shipped vnc-027 client behavior, **not** a nan-016 change. Expect PreToolUse to match the
  narrowed pattern after the flip, not `"*"`.
- Note: `SubagentStop` is **opt-in** via `settings.local.json`
  (`unimatrix.hooks.subagent_stop === true`). A fresh target with no opt-in registers **8**
  events; the full **9** events register only with opt-in.

## 5. Daemon posture / fail-open [FR-14e]

- The local UDS daemon is **assumed to be already running**. nan-016 does **no daemon
  lifecycle management** — it never starts, stops, or probes the daemon.
- Hooks **fail open**: the emitted node-client command **exits 0 with empty stdout on every
  path, including when the daemon is absent** (C-7). If the daemon's UDS socket is not present,
  the hook degrades to context-loss only; it never breaks or hangs the host session.
- The switchover introduces no host-breaking hook path and no daemon dependency.
- Interpreting failures: the **scripts** (`dogfood-install.sh`, `dogfood-switchover.sh`) are
  tooling and are loud on error (non-zero exit). The **hooks** they install are fail-open and
  are silent on failure (exit 0). A loud script error means a setup problem to fix; a silent
  hook means context-loss, never a broken session.

## 6. What this capability does NOT do

- Does **not** execute the live flip and does **not** start the F6 (#682) soak clock.
- Does **not** create the follow-up flip-tracking issue on #682 (human's to track).
- Does **not** retire or modify the Rust `hook.rs`.
- Does **not** modify the client: `lib/hook-client/`, `lib/init.js`, `lib/merge-settings.js`,
  `lib/hook-client/config.js`, or `package.json` runtime behavior (C-8).
- Does **not** append the CLAUDE.md knowledge block (that is uni-init's job).

## Acceptance mapping (FR-14)

| FR-14 item | Section | Check |
|------------|---------|-------|
| a — promotion = re-run install = F6 soak-reset | 1 | present |
| b — rollback = revert to Rust `hook.rs` form via `mergeSettings` | 3 | present + cross-checked by effect test |
| c — flip deferred to no-active-feature window; clock not started | 0, 2 | present |
| d — PreToolUse matcher-narrowing is an intended delta | 4 | present |
| e — daemon assumed running / fail-open if absent | 5 | present |
