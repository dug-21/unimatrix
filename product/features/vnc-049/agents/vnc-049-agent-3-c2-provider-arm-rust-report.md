# Agent Report — C2 Provider Normalization Arm (Rust)

Agent: vnc-049-agent-3-c2-provider-arm-rust (uni-rust-dev) | Wave 2 | GH #986

## Summary

Implemented the OpenCode Rust provider normalization arm per ADR-004 (provider arm) and
ADR-005 (new-module-with-thin-wiring). Substantive logic lives in the new module; the
over-cap `hook.rs` receives only minimal wiring.

## Files modified

- `crates/unimatrix-server/src/uds/hook/opencode.rs` (NEW, 180 lines incl. tests — under cap):
  OpenCode canonicalization module. `canonicalize()` (identity for the 7 canonical names,
  `__unknown__` sentinel otherwise), `normalize()` (returns the full `(name, provider)`
  contract), `is_opencode_event_alias()` (inert seam — false for all shared canonical names),
  `OPENCODE_CANONICAL_EVENTS`/`OPENCODE_PROVIDER`/`UNKNOWN_EVENT` consts, and a re-export of
  `is_valid_model_id` from the C4 wire contract (no duplication).
- `crates/unimatrix-server/src/uds/hook.rs` (MODIFY, minimal wiring):
  - Hoisted `KNOWN_PROVIDERS` from a fn-local const inside `run()` to a module-level const and
    added `"opencode"` (hoisting was required so the AC-02a membership test can reference it).
  - Declared `pub mod opencode;`.
  - Added one inference-path delegating guard at the top of `normalize_event_name` routing
    opencode aliases through the module (inert today; the safe seam for future aliases).
  - Added 3 tests to the existing hook.rs test module.

## Tests

`cargo test -p unimatrix-server --lib opencode` → 14 passed, 0 failed:
- 11 in `hook/opencode.rs` (7 per-event canonicalize + all-seven identity + unknown sentinel +
  alias-no-hijack + model_id re-export).
- 3 in `hook.rs` (`test_known_providers_contains_opencode`, `test_hook_rs_delegates_opencode_to_module`,
  `test_non_opencode_providers_unchanged`).

`cargo build -p unimatrix-server` passes. `cargo fmt` applied. `cargo clippy` — zero warnings from
my files (the 3 warnings reported are pre-existing in `transcript_hold.rs`/`main.rs`).

## AC coverage owned

- AC-02a: `"opencode" ∈ KNOWN_PROVIDERS` — asserted (`test_known_providers_contains_opencode`).
- Event-name→canonical correctness for all 7 events (incl. degraded/bus-derived legs) — asserted
  via the module's `normalize`/`canonicalize` tests returning `(name, "opencode")`.

## Scope decisions / boundary notes

- **Model_id `run()` plumbing NOT implemented (flagged, not silently dropped).** The C2 pseudocode
  §2 describes adding a `model: Option<String>` param to `hook::run()` and threading it to
  `HookInput.model_id`/`ImplantEvent.model_id`. My spawn task scoped me to KNOWN_PROVIDERS + the
  delegating arm + the opencode.rs module, with `hook.rs` restricted to MINIMAL wiring. Adding a
  `--model` CLI arg requires editing `main.rs` (outside my file boundary — the `Hook` subcommand
  at main.rs:316 has no `--model` arg today) and threading through ~16 `ImplantEvent` literals
  (C4 left them `model_id: None`). This is not minimal wiring and crosses my boundary. **Handoff:**
  someone must (a) add `--model` to the `main.rs` `Hook` subcommand + `run()` signature, and (b)
  set `hook_input.model_id = model.filter(|m| opencode::is_valid_model_id(m))` and copy it into
  every `ImplantEvent` build, for the AC-06 end-path (R-01) to carry a non-null `model_id`. The
  module already exposes `is_valid_model_id` for that use.
- Did not touch `listener.rs` (C5), `normalize.js` (C3), or the parity corpus (C8) per boundary.

## Shared-tree concurrency observations (not my regressions)

Wave 2 runs C2 + C5b in the same working tree concurrently. During validation I twice saw the
crate fail to compile from the C5 agent's in-flight uncommitted work (a missing `ObservationRow`
field at `listener.rs:1476`, later resolved; then `mod opencode_persistence;` at `listener.rs:10215`
with a test file missing `use sqlx::Row;`). My commit staged ONLY my two files. My tests passed
when the tree was in a compilable state. `test_run_scenarios_does_not_write_to_snapshot` is a known
pre-existing --workspace flake, not touched here.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing + context_search -- surfaced ADR-004 (#5741),
  three-touchpoints harness pattern (#5737), hook-normalization-at-ingest-boundary (#4298),
  canonical-names ADR (#4305). Applied all.
- Stored: entry #5751 "Provider inference-path delegating guard must return false for shared
  canonical event names" via /uni-store-pattern (gotcha: delegating guard hijacks claude-code
  inference if it claims shared names; + KNOWN_PROVIDERS hoist for testability).
