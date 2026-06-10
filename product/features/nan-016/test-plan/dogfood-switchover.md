# Test Plan — `scripts/dogfood-switchover.sh`

> `promote` / `rollback` / `--dry-run`; repoints a TARGET settings file via the shipped
> `mergeSettings` required from the **installed** client, then PRUNES stale uni-owned hook groups
> so the post-state is CLEAN (rework: clean-switch). Operates on the live repo only when a human
> runs it; the harness drives it against a scratch root. Primary risks: **R-05** opaque require
> failure; **R-06** rollback drift; R-09 matcher delta; R-10 event-set / duplicates / foreign.
> Covers AC-02 (promote), AC-04 (rollback). Most behavioral assertions live in dogfood-effect
> (the executor); this file pins the switchover-script-specific expectations.
>
> **Rework (clean-switch):** `mergeSettings` alone keys every op on `EVENT_MATCHERS[event]` and so
> leaves a stale `"*"` PreToolUse Rust uni hook in place (#4930). The switchover now additionally
> PRUNES uni-owned hook groups (per shipped `isUnimatrixHook`) that are NOT in the post-operation
> target form: promote prunes stale Rust groups (incl. the `"*"` PreToolUse one); rollback prunes
> stale node-client groups. Foreign hooks are NEVER pruned. The prune is idempotent and reported
> (not written) under `--dry-run`.

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

- `rollback_after-promote_restores-exact-rust-command-over-correct-events-clean`  ← R-06 core
  - Arrange: scratch settings seeded with the real **live-shaped** Rust-hook shape (incl. the
    `"*"` PreToolUse Rust uni hook) + a foreign hook; run `promote` then `rollback` against it.
  - Assert: (a) every uni-owned command (per shipped `isUnimatrixHook`, over ALL groups) ==
    `LD_LIBRARY_PATH=<repo>/target/release <repo>/target/release/unimatrix hook <EVENT>` over the
    correct event set; (b) NO stale node-client uni hook survives (count of uni-owned hooks still
    in `node <installed>/.../index.js <EVENT>` form == 0 — the promote-side node-client group is
    pruned on rollback); emitted by the shipped `normalizeCommandSource` legacy arm (rollback
    passes a STRING command source), NOT a nan-016 bespoke string.
- `rollback_idempotent_twice-equals-once`
  - Assert: running rollback twice yields the same result (idempotent re-point + idempotent
    prune — a stale node-client group is gone after the first rollback and stays gone).
- `rollback_preserves-foreign-and-ownership`
  - Assert: `isUnimatrixHook` still owns the rolled-back entries; a seeded foreign hook survives
    both promote and rollback unchanged (never pruned).

### R-09 — Matcher narrowing asserted against IMPORTED constant (Medium)

- `promote_scratch-seeded-with-star_narrows-PreToolUse-to-imported-constant-and-prunes-star`  ← R-09 core
  - Arrange: scratch settings with the `"*"` PreToolUse Rust uni hook.
  - Assert: after promote, (a) the surviving uni PreToolUse group's matcher equals the **imported**
    `PRETOOLUSE_CYCLE_MATCHER` from the installed `lib/merge-settings.js` — NOT a copy-pasted
    literal (constant imported so a shipped change does not drift the assertion); (b) the original
    `"*"` Rust uni PreToolUse group is PRUNED (count == 0) — the clean-switch state, replacing the
    prior "stale `"*"` survives as a documented delta" reality (#4930). Runbook cross-check is in
    test-plan/runbook.md (AC-04).

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

### Stale-uni-hook PRUNE (rework: clean-switch) — unit + effect

> Proves the prune is real, scoped to uni-owned hooks, and idempotent. Unit-level assertions
> exercise the prune helper on a hand-built settings object; effect-level assertions run the real
> `promote`/`rollback` and re-parse the scratch settings (the live-shaped seed is the executor's
> in dogfood-effect.md).

- `promote_prunes-stale-rust-uni-groups_count-zero`  ← prune core (promote side)
  - Arrange: live-shaped seed with the `"*"` PreToolUse Rust uni hook + foreign hook.
  - Assert: after promote, the count of uni-owned hooks (per shipped `isUnimatrixHook`) still in
    the Rust command form == 0; the only uni groups present are the installed-entrypoint
    node-client groups; foreign hook preserved.
- `promote_prune_NEGATIVE-CONTROL_no-prune-leaves-stale-rust-hook-FAILS`  ← **prune negative control (MANDATORY)**
  - Purpose: prove the prune is not vacuous — the assertion FAILS if the prune step is removed.
  - Arrange: same live-shaped seed; produce the no-prune post-state (mergeSettings alone, no
    prune) and feed it to the SAME prune assertion helper.
  - Assert: the `count of stale Rust uni hooks == 0` assertion FAILS against the no-prune state
    (the `"*"` Rust uni hook is still present), proving the prune step is load-bearing.
- `rollback_prunes-stale-node-uni-groups_count-zero`  ← prune core (rollback side)
  - Arrange: promote then rollback on the live-shaped seed.
  - Assert: after rollback, the count of uni-owned hooks still in the node-client
    (`node <installed>/.../index.js`) form == 0; foreign hook preserved.
- `prune_preserves-foreign-hooks`
  - Assert: a seeded foreign hook (NOT matched by `isUnimatrixHook`) is never pruned on either
    promote or rollback — byte-unchanged, no duplicate, no reorder-induced loss.
- `prune_idempotent_second-run-no-additional-change`
  - Assert: re-running the same operation (promote again after promote, rollback again after
    rollback) produces byte-identical settings — the prune removes nothing on the second pass
    (nothing stale remains) and adds no churn.

### `--dry-run`

- `promote_dry-run_no-write-but-reports-actions-and-planned-prunes`
  - Assert: with `--dry-run`, the scratch settings file is NOT modified (byte-identical to the
    seed), exit 0, AND the reported actions reflect BOTH what `mergeSettings` would change
    (forwarded `dryRun`) AND the planned prunes — i.e. the stale `"*"` Rust uni group is named as
    a planned removal in the dry-run report, without being written. A `--dry-run` that omits the
    planned prune from its report is a FAIL (the operator would not see the clean-up that the real
    run performs).

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
  preserved, via the shipped legacy arm — no bespoke revert string; NO stale node-client uni hook
  survives (rollback-side prune).
- R-09: matcher delta asserted against the IMPORTED `PRETOOLUSE_CYCLE_MATCHER`; the stale `"*"`
  Rust uni group is PRUNED (not left as a documented delta).
- R-10: event count asserted against actual opt-in state; no duplicates; foreign preserved.
- **Prune (clean-switch):** promote prunes stale Rust uni groups (incl. `"*"` PreToolUse),
  rollback prunes stale node-client uni groups; foreign never pruned; idempotent; `--dry-run`
  reports planned prunes without writing. A prune NEGATIVE CONTROL proves the prune is real (the
  count-zero assertion FAILS against a no-prune post-state).
