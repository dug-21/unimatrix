# Test Plan — `scripts/dogfood-switchover.sh`

> `promote` / `rollback` / `--dry-run`; repoints a TARGET settings file via the shipped
> `mergeSettings` required from the **installed** client. Operates on the live repo only when a
> human runs it; the harness drives it against a scratch root. Primary risks: **R-05** opaque
> require failure; **R-06** rollback drift; R-09 matcher delta; R-10 event-set / duplicates /
> foreign. Covers AC-02 (promote), AC-04 (rollback). Most behavioral assertions live in
> dogfood-effect (the executor); this file pins the switchover-script-specific expectations.

## Test Conventions for this Component

- The switchover script is exercised ONLY through `execFileSync("bash",[switchoverScript, …])`
  with explicit `--settings <scratch>` and `--client <installed>` — NEVER defaulting onto the
  live repo (R-08). All `--settings` paths under `os.tmpdir()`.
- Assertions are on the resulting scratch `settings.json` content + exit code + stderr message,
  never on the `.sh` source text.

## Behavior Test Expectations

### R-05 — `mergeSettings` require from installed client; opaque-failure guard (High)

- `promote_missing-client-dir_loud-actionable-nonzero`  ← R-05 core
  - Arrange: `--client` points at a non-existent / empty dir (no `lib/merge-settings.js`).
  - Act/Assert: exit is non-zero AND stderr contains an actionable message (e.g. "run
    dogfood-install.sh first" / "installed client not found"). NOT a raw require stack trace.
    This is tooling — it MAY be loud (distinct from C-7 hook fail-open).
- `promote_completeness-checked-before-require`
  - Assert: the script verifies `lib/merge-settings.js` + `lib/hook-client/index.js` +
    `lib/hook-client/config.js` exist before it `require`s `mergeSettings`. (Asserted via the
    missing-pieces variant producing the actionable message, not a Node `MODULE_NOT_FOUND`.)
- `promote_requires-INSTALLED-merge-settings`
  - Assert (via effect): promotion routes through the installed copy's `mergeSettings` (proven
    by the resulting settings carrying shipped semantics — see R-09/R-10). Validates the frozen
    copy's own merge logic is runnable (the integration seam).

### R-06 — Rollback reproduces exact Rust form, no bespoke revert string (High)

- `rollback_after-promote_restores-exact-rust-command-over-correct-events`  ← R-06 core
  - Arrange: scratch settings seeded with the real Rust-hook shape; run `promote` then
    `rollback` against it.
  - Assert: every Unimatrix command ==
    `LD_LIBRARY_PATH=<repo>/target/release <repo>/target/release/unimatrix hook <EVENT>` over
    the correct event set; emitted by the shipped `normalizeCommandSource` legacy arm (rollback
    passes a STRING command source), NOT a nan-016 bespoke string.
- `rollback_idempotent_twice-equals-once`
  - Assert: running rollback twice yields the same result (idempotent re-point).
- `rollback_preserves-foreign-and-ownership`
  - Assert: `isUnimatrixHook` still owns the rolled-back entries; a seeded foreign hook survives
    both promote and rollback unchanged.

### R-09 — Matcher narrowing asserted against IMPORTED constant (Medium)

- `promote_scratch-seeded-with-star_narrows-PreToolUse-to-imported-constant`  ← R-09 core
  - Arrange: scratch settings with PreToolUse matcher `"*"`.
  - Assert: after promote, the PreToolUse matcher equals the **imported**
    `PRETOOLUSE_CYCLE_MATCHER` from the installed `lib/merge-settings.js` — NOT a copy-pasted
    literal. (The constant is imported in the harness so a shipped change does not drift the
    assertion.) Runbook cross-check is in test-plan/runbook.md (AC-04).

### R-10 — Event-set actual opt-in state; duplicates; foreign (Medium)

- `promote_no-subagentstop-optin_registers-8-events`
  - Arrange: scratch root with NO `settings.local.json` opt-in.
  - Assert: exactly the 8 non-opt-in events are registered (asserted against the actual opt-in
    state, NOT a hardcoded 9).
- `promote_with-subagentstop-optin_registers-9-events`
  - Arrange: scratch root WITH `SubagentStop` opt-in in `settings.local.json`.
  - Assert: 9 events registered.
- `promote_existing-rust-commands_updated-in-place-no-duplicates`
  - Assert: promote UPDATES the existing Rust commands to node-client form in place; no
    duplicate Unimatrix entries appear.

### `--dry-run`

- `promote_dry-run_no-write-but-reports-actions`
  - Assert: with `--dry-run`, the scratch settings file is NOT modified, exit 0, and the
    reported actions reflect what would change (forwarded to `mergeSettings`'s `dryRun`).

## Security / Safety Assertions (path injection into emitted command)

- `promote_client-path-with-whitespace_quoted-exactly-as-buildHookClientCommand`
  - Arrange: `--client` under a temp dir whose path contains a space.
  - Assert: the emitted hook command quotes the path exactly as `buildHookClientCommand` does
    (quoted iff whitespace) — never interpolated unquoted into a shell command.
- `promote_settings-path-must-not-be-live`
  - Assert: the script (or the harness guard around it) rejects a `--settings` equal to the live
    repo settings path. (Primary guard test lives in dogfood-effect, R-08; noted here as the
    switchover-side contract.)

## Coverage Requirement (must all hold)

- R-05: missing/incomplete install → loud actionable tooling error; completeness checked before
  require; require targets the INSTALLED copy.
- R-06: rollback reproduces the exact Rust form over the correct events, idempotent, foreign
  preserved, via the shipped legacy arm — no bespoke revert string.
- R-09: matcher delta asserted against the IMPORTED `PRETOOLUSE_CYCLE_MATCHER`.
- R-10: event count asserted against actual opt-in state; no duplicates; foreign preserved.
