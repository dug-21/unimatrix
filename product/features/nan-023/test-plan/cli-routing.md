# Test Plan — CLI Routing (`bin/unimatrix.js`)

> Component: arg routing for `init`, `init --force`, `wire`, `wire --harness <name>`, `--dry-run` (ADR-005)
> Runner: `node --test`, driving the CLI end-to-end (spawn `bin/unimatrix.js` or call its exported entry) against temp `.git` fixtures. Test file: extend `test/init.test.js` / new `test/cli-routing.test.js`.
> Primary risks: **R-12** (intent-gate bypass), **R-13** (`--force` reaches wire), **R-14** (dry-run/containment).
> ACs: AC-03 (wire = zero definition writes), AC-11 (new-entry opt-in), AC-14 (dry-run), AC-16 (ambiguity → help), AC-02 (`--force` never re-asserts wiring).

These tests drive the **real user command** (the entry point), not a writer beneath it — per the
RISK-STRATEGY behavioral-outcome lens, a writer unit test does NOT discharge these rows.

## `wire` verb isolation (AC-03, ADR-005 §1)

- `test_wire_writes_zero_definition_files` — multi-harness fixture; run `wire` → wiring surfaces change AND `.claude/skills/`, `.claude/protocols/`, `.claude/agents/` trees byte-identical (no `installSkills`, no DB).
- `test_init_runs_installSkills_and_wire` — `init` runs both `installSkills` and `wire()`; wire performs no definition/DB work whether reached via `init` or standalone (integration-risk boundary).

## `--harness` routing + intent gate (AC-11, AC-16, R-12)

- `test_wire_no_harness_new_entry_skipped_intent` — opencode/codex config lacking the entry, no `--harness` → `action:"skipped-intent"` + help line naming the exact command; **no write** (AC-11 negative arm).
- `test_wire_with_harness_writes_new_entry` — `wire --harness codex-cli` → the new `[mcp_servers.unimatrix]` IS written (AC-11 positive arm).
- `test_wire_additive_into_existing_no_optin` — surface the user already maintains → additive merge proceeds without `--harness` (AC-11 exemption).
- `test_wire_claude_mcp_not_intent_gated` — claude-code `.mcp.json` common path is ensured without opt-in (backward compat, ADR-005 §3).
- `test_wire_unknown_harness_prints_help` — `--harness bogus` → help/usage, **no** fall-through write (AC-16).
- `test_cli_ambiguous_invocation_prints_help` — conflicting/unknown flags → help, never a default install (#960, AC-16).

## `--force` blast radius (AC-02, R-13, ADR-005 §5 / ADR-004 §4)

- `test_force_gates_installSkills_only` — `init --force` on a fixture with pre-existing wiring → every wiring surface **byte-identical to pre-run** (zero wiring changes); skills == shipped (AC-02 full — the wiring half not provable in `skills-installer.md`).
- `test_wire_rejects_force` — `wire --force` → `--force` is a no-op/usage note for wire; wiring stays additive, not re-asserted (guarantees `--force` never reaches `wire()`).

## Dry-run (AC-14, R-14)

- `test_init_dryRun_zero_writes_action_set_matches` — `init --dry-run` → `[dry-run]`-prefixed output, zero filesystem changes, action set == real-path action set.
- `test_wire_dryRun_zero_writes_action_set_matches` — same for `wire --dry-run` (dry-run prints from the same manifest, ADR-001 §4).

## Containment / global-path absence (AC-12, R-14)

- `test_cli_no_writes_to_global_paths` — run `init`/`wire` in a sandbox with `HOME` redirected; assert **zero** writes to `~/.codex/`, `~/.claude/`, `~/.gemini/`, `~/.config/` (SR-12, NFR-04).

## Coverage note

Every test here drives the actual CLI verb and asserts the observed outcome (files on disk, printed action set), satisfying the behavioral-outcome lens rows for `init`, `init --force`, `wire`, `wire --dry-run`.
