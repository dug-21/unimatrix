# Test Plan — Skills Installer (`installSkills`, `lib/init.js`)

> Component: `lib/init.js` `copySkills` → `installSkills(projectRoot, {force, dryRun}) -> string[]` (ADR-004)
> Runner: `node --test` · Test file: extend `test/init.test.js` (do not create isolated scaffolding — test infra is cumulative)
> Primary risks: **R-16** (install-if-absent first-run break), **R-13** (`--force` blast radius).
> ACs: AC-01 (never overwrite by default), AC-02 (`--force` skills-only, foreign untouched, zero wiring), AC-14 (dry-run), part of AC-12 (containment).

Drives the real `installSkills` entry point against temp `.git` project fixtures (matches the
existing `makeTempProject` idiom). Ownership set = the shipped `skills/` manifest; **foreign** files
under `.claude/skills/` are never read/written/deleted on any path.

## Install-if-absent default (R-16, AC-01)

- `test_installSkills_fresh_repo_installs_all_shipped` — bare fixture (no `.claude/skills/`) → every shipped skill on disk; each reported `installed`; return array lists them (first-run install works, NFR-06).
- `test_installSkills_existing_skill_kept_byte_for_byte` — pre-place a **modified** copy of a shipped skill; run default → byte-diff of that file = empty; reported `kept (exists)` (AC-01, the core copySkills-clobber regression).
- `test_installSkills_partial_install_fills_only_missing` — some shipped skills present (edited), some absent → absent ones installed, present ones untouched. Guards the "mis-detects fresh repo as already-installed" failure (R-16).
- `test_installSkills_foreign_skill_untouched_default` — a non-shipped file under `.claude/skills/` → never read/written; byte-identical.

## `--force` (R-13, AC-02)

- `test_installSkills_force_overwrites_owned_with_shipped` — modified owned skill + `force:true` → file == shipped version.
- `test_installSkills_force_never_touches_foreign` — foreign file under `.claude/skills/` + `force:true` → foreign byte-identical (ownership scoped to shipped manifest).
- `test_installSkills_force_scope_is_skills_only` — `.claude/protocols/` and `.claude/agents/` present → untouched on `force:true` (FR-09, SR-05 boundary). The wiring-surface half of AC-02 (`--force` re-asserts zero wiring) is asserted at the CLI/wire boundary in `cli-routing.md`.

## Dry-run (AC-14, R-14)

- `test_installSkills_dryRun_prints_actions_writes_nothing` — `dryRun:true` → return array carries the intended per-file actions; **zero** filesystem writes (assert dir mtime / file absence).
- `test_installSkills_dryRun_action_set_equals_real` — action set from `dryRun:true` == action set produced by the real run on the same fixture (NFR-10 fidelity).

## Containment / path-traversal (AC-12)

- `test_installSkills_rejects_path_traversal_filename` — a shipped filename attempting `../` escape is guarded (retained path-traversal guard, ADR-004 §6); no write outside resolved root.

## Coverage note

AC-01 and R-16 both discharged: first-run install AND re-run non-clobber asserted. `--force` proven definitions-only here (skills side); the zero-wiring-change half is proven end-to-end in `cli-routing.md` / integration (AC-02 full).
