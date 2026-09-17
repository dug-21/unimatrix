# Agent Report — Skills Installer (nan-023, Wave A / A3)

Agent: nan-023-agent-3-skills-installer · Role: uni-js-dev · Stage 3b

## Task

Refactor `copySkills` → `installSkills(projectRoot, {force, dryRun}) -> string[]` in
`lib/init.js` per ADR-004 (non-destructive definition install).

## Files Modified

- `packages/unimatrix/lib/init.js` — `copySkills` replaced by `installSkills`;
  both call sites (`init` Step 5, `initRemote` Step 5) threaded with `force`;
  `copySkills` kept as a thin install-if-absent alias for the legacy 2-arg
  surface; `installSkills` added to `module.exports`. File is 748 lines (< 800).
- `packages/unimatrix/test/init.test.js` — added `describe("installSkills")`
  with 11 component tests (extended the existing file; cumulative test infra).

No new modules. `package.json` / `package-lock.json` unchanged (zero new deps).

## Behavior (ADR-004)

- Default = install-if-absent: shipped skill file written only when destination
  is absent; an edited owned skill survives byte-for-byte (AC-01).
- `--force` = overwrite Unimatrix-owned skills with shipped versions (AC-02).
- Foreign files under `.claude/skills/` are structurally unreachable — the loop
  iterates the SHIPPED source tree (`__dirname/../skills`), never the destination
  tree, on any path including `--force`.
- Scope = skills only; a boundary action line
  ("Definition scope: skills only (protocols/agents not installed)") closes every
  action list (C-13, SR-05).
- Dry-run writes nothing (does not even mkdir `.claude/skills`); per-file
  decision set is identical to the real run.
- Path-traversal guard on shipped filenames retained (throws — package-integrity
  defect, not consumer input).

## Tests

- Component suite `test/init.test.js`: **23 pass / 0 fail** (11 new for
  `installSkills`, covering AC-01 byte-diff, AC-02 force + foreign-untouched +
  skills-only scope, AC-14 dry-run + action-set-equals-real, AC-12 traversal).
- Size gate (`test/check-hook-client-size.js`): **PASS** — stripped 105243/110000
  (~4.7 KB headroom), raw 191416/200000. `installSkills` lives in `lib/init.js`,
  outside the hook-client dir, so it does not consume the gate.

## Seams / Handoffs (not mine to implement)

1. **`--force` CLI parse (Wave C — cli-routing.md).** `bin/unimatrix.js:25`
   builds `init({ dryRun, projectDir, remote, token, bundle, slug })` and does
   NOT yet pass `force`. `init`/`initRemote` already read `options.force` and
   thread it into `installSkills`; cli-routing must add
   `force: args.includes("--force")` to that options object. Per C-12/SR-04,
   `--force` gates `installSkills` only and is never forwarded to `wire()`.
2. **Thin `wire()` call in init (Wave C — wire-orchestrator.md).** Not added
   here per task scope. The existing `maybeProvisionOpenCode` call site remains;
   Wave C routes claude-code/opencode/codex writers through `wire.js`.

## Blocker (Stage 3c handoff — expected, per ADR-004 + pseudocode)

`test/init-integration.test.js` `describe("copySkills")` has **3 failing tests**
(`test_copies_skill_dirs`, `test_overwrites_existing_unimatrix_skills`,
`test_dry_run_does_not_copy_skills`). They assert the retired legacy behavior:
blanket overwrite-every-run and the old wording ("Copied skill:",
"[dry-run] Would copy skill:"). ADR-004 intentionally removes that behavior
(AC-01). Integration tests are Stage-3c-owned; I did not modify them per the
spawn instruction. Stage 3c must update these to the `installSkills` semantics
(install-if-absent default; `--force` for overwrite; new per-file action lines).
The `describe("init (integration with mocks)")` block (5 tests) still passes.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced ADR-004 (#5766, the
  decision I implemented) plus adjacent packaging/distribution decisions
  (#4336 three-location distribution, #4330 nan-011 protocols dir). Applied the
  ADR-004 ownership model (shipped-manifest = owned) directly.
- Stored: entry #5771 "Non-destructive definition install via source-tree
  iteration (structural foreign-untouched)" via /uni-store-pattern.
