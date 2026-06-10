# Component 4 — `product/features/nan-016/RUNBOOK.md` (Runbook content structure)

## Purpose

Define the committed runbook's content structure. AC-04 requires the five FR-14 items (a–e)
present and a rollback cross-check that the steps actually reproduce the Rust binary command form
via `mergeSettings`. This file is a content spec, not code — the implementation agent writes the
prose `RUNBOOK.md` to this structure.

## Required sections (the five FR-14 items + framing)

### Section 0 — Header / boundary statement
- nan-016 delivers and proves the mechanism; it does **NOT** execute the live flip and does
  **NOT** start the F6 (#682) soak clock. The flip is a deliberate later human action in a
  no-active-feature window. (SCOPE / SPEC objective; ALIGNMENT human-awareness item 1.)
- FLAGGED FOR THE HUMAN: nan-016 does not create the follow-up flip-tracking issue on #682.

### Section 1 — Promotion (= re-release = F6 soak-reset point)  [FR-14a]
- Command: `scripts/dogfood-install.sh` (optional `--target <dir>`; default
  `~/.unimatrix/dogfood-client/`).
- States: builds (`npm pack`) `packages/unimatrix`, clean-replace installs the frozen tree to the
  fixed dir; idempotent (re-run = byte-identical tree; clean-replace, no overlay).
- States explicitly: **re-running promotion IS the F6 soak-reset point** — it resets the soak to
  the newly installed bytes.
- Notes: fixed dir chosen over npm global prefix for container-rebuild durability (NFR-7).

### Section 2 — Switchover (the deferred live flip)  [FR-14c]
- Command: `scripts/dogfood-switchover.sh promote [--settings <path>] [--client <dir>]`.
- States: repoints hooks to `node <client>/lib/hook-client/index.js <EVENT>` via the installed
  `mergeSettings`.
- States explicitly: running this against this repo's live `.claude/settings.json` is the
  **deferred flip**, to be done by a human in a no-active-feature window, and **doing so starts
  the F6 soak clock**. nan-016 never runs this against live settings.
- `--dry-run` available to preview actions without writing.

### Section 3 — Rollback (revert to the Rust hook)  [FR-14b]
- Command: `scripts/dogfood-switchover.sh rollback [--settings <path>]`.
- States: reverts hooks to `LD_LIBRARY_PATH=<repo>/target/release <repo>/target/release/unimatrix
  hook <EVENT>` over `HOOK_EVENTS`, via `mergeSettings`'s legacy (string) arm — the same engine in
  reverse, no bespoke revert logic.
- **Rollback cross-check (AC-04 verification):** document that the rollback command form is
  produced by `normalizeCommandSource`'s legacy arm (passing the binary path STRING to
  `mergeSettings`), i.e. it reproduces the pre-switchover Rust command form; idempotent; foreign
  hooks preserved.

### Section 4 — PreToolUse matcher-narrowing (intended behavioral delta)  [FR-14d]
- States: promotion applies shipped `EVENT_MATCHERS`, narrowing PreToolUse from the live `"*"` to
  `context_cycle|mcp__unimatrix__context_cycle` (`PRETOOLUSE_CYCLE_MATCHER`). This is an
  **intended** delta the operator will observe at flip time (vnc-027 shipped behavior, not a
  nan-016 change).
- Also note: `SubagentStop` is opt-in via `settings.local.json` — a fresh target registers 8
  events; 9 only with opt-in.

### Section 5 — Daemon posture / fail-open  [FR-14e]
- States: the local UDS daemon is **assumed already running**; nan-016 does **not** start/stop/
  probe it (no daemon lifecycle management).
- States: hooks **fail open** — the emitted node-client command exits 0 / empty stdout on every
  path including daemon-absent (C-7). The switchover introduces no host-breaking hook path.

### Section 6 — What this capability does NOT do
- Does not execute the live flip / does not start the F6 clock.
- Does not retire or modify Rust `hook.rs`.
- Does not modify the client (`lib/hook-client/`, `lib/init.js`, `lib/merge-settings.js`,
  `lib/hook-client/config.js`, `package.json` runtime) — C-8.
- Does not append the CLAUDE.md knowledge block (uni-init's job).

## Acceptance mapping

| FR-14 item | Section | AC-04 check |
|------------|---------|-------------|
| a — promotion = re-install = F6 reset | 1 | present |
| b — rollback = revert to Rust hook.rs | 3 | present + cross-check reproduces Rust form via mergeSettings |
| c — flip deferred to no-active-feature window | 0, 2 | present |
| d — PreToolUse matcher-narrowing is intended delta | 4 | present |
| e — daemon assumed running / fail-open if absent | 5 | present |

## Data flow

- IN: the behaviors delivered by Components 1–3 (commands, flags, deltas).
- OUT: a committed `RUNBOOK.md` with sections 0–6; consumed by the human operator at flip time and
  by Gate 3c / AC-04 verification.

## Error handling

N/A (documentation). The runbook documents the scripts' loud-error posture (tooling) vs. the
hooks' fail-open posture (C-7) so the operator interprets failures correctly.

## Key test scenarios (hints)

1. File exists and contains each of the five FR-14 items (a–e). (AC-04)
2. Rollback section's documented command form matches what `mergeSettings` legacy arm emits
   (cross-check, not a copy-pasted literal that can drift).
3. Matcher-narrowing section names `PRETOOLUSE_CYCLE_MATCHER` / the narrowed value and frames it
   as intended.
4. Boundary statements present: flip deferred, soak clock not started, no follow-up issue created.

## Gaps / flags

- None blocking. The runbook must NOT instruct running the flip against live settings as part of
  delivery — it documents the procedure for a future human action only.
