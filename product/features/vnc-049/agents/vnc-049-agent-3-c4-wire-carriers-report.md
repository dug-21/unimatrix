# Agent Report — vnc-049 C4 Wire Carriers

**Agent:** vnc-049-agent-3-c4-wire-carriers (uni-rust-dev) | **Wave:** 1 (foundation)
**Status:** COMPLETE | **Commit:** 0bf4680a

## Scope delivered
Added the `model_id: Option<String>` wire carrier to `HookInput` and `ImplantEvent` per
pseudocode/c4-wire-carriers.md + ADR-002, regenerated ts-rs bindings, and kept the workspace green.

## Files modified
- `crates/unimatrix-engine/src/wire.rs` — `model_id` on `HookInput` (`#[serde(default)]`) and
  `ImplantEvent` (`#[serde(default, skip_serializing_if="Option::is_none")]`, mirrors `provider`);
  new `pub fn is_valid_model_id` + `MODEL_ID_MAX_LEN`; 6 new unit tests; `model_id: None` threaded
  through the crate's own ImplantEvent literals + the `cycle_stamp_event` shorthand helper.
- `crates/unimatrix-engine/bindings/HookInput.ts`, `ImplantEvent.ts`, `HookRequest.ts` — regenerated
  (HookRequest via the `RecordEvent` `#[serde(flatten)]` ripple, per pattern #4839). Fixtures
  unchanged (None omitted by skip_serializing_if).
- `crates/unimatrix-server/src/uds/hook.rs`, `uds/listener.rs`, `uds/parity_corpus_uds.rs`,
  `uds/listener/tests/foreign_domain.rs`, `uds/listener/tests/stamp_read.rs`,
  `http/router/tests.rs` — purely additive `model_id: None,` in every ImplantEvent/HookInput struct
  literal (43 lines, 0 removed) to keep `cargo build --workspace` green. These are the None
  placeholders; **C2 (hook.rs) and C5 (listener.rs) populate the production sites in later waves.**

## Decisions resolved live
- **Validation-helper location (C4 vs C2/C5 open decision):** placed `is_valid_model_id` in C4
  (`wire.rs`), next to the field, per pseudocode. Call sites (C2 hook::run / C5 bind) import it.
- **Carrier charset (Gate 3a WARN reconciliation):** implemented `^[a-z0-9._/-]{1,128}$` — ALLOWS
  `/` and `.` (e.g. `ollama/qwen3-coder`). Distinct from the source_domain contract
  `^[a-z0-9_-]{1,64}$`. Unit test `test_model_id_format_contract` asserts this (accepts
  `ollama/qwen3-coder`, `anthropic/claude-3.5-sonnet`; rejects empty, >128, `a;drop`, uppercase,
  whitespace, control chars).
- **serde back-compat (R-10):** frames without `model_id` deserialize to `None` for both structs
  (dedicated tests). None is omitted on the wire → frozen fixtures byte-stable.

## Tests
- `cargo test -p unimatrix-engine --lib`: 488 passed, 0 failed (incl. 6 new C4 tests + bindings gate
  `test_export_bindings_all_seven_written_and_nonempty`).
- `cargo test --workspace`: 5980 passed, **1 failed** —
  `eval::scenarios::tests::test_run_scenarios_does_not_write_to_snapshot`. Verified PRE-EXISTING
  FLAKE, not caused by C4: passes 3/3 in isolation; it byte-compares a full SQLite DB file across a
  read-only run (fragile under `--workspace` parallelism — the flake class rust-workspace.md warns
  about). C4 adds only None-defaulted fields (byte-identical serialization) and touches no DB/eval
  code, so it cannot affect this test.
- `cargo build --workspace`: clean. `cargo clippy -p unimatrix-engine --all-targets`: no warnings.
  `cargo fmt -p unimatrix-engine` applied; did NOT format the server crate (pre-existing unrelated
  fmt drift in `edge_write_delete_agent_tests.rs` — left untouched per #4839).

## Blockers / handoffs
- None blocking. Handoff: C2 changes hook.rs `model_id: None` → `input.model_id.clone()` at run
  sites; C5 binds `event.model_id` at listener insert sites; C8 adds a parity case carrying
  model_id; the client→listener→DB crossing test (lesson #5670) is owned by C5.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search + context_get(#4839,#4313,#4722) --
  found the ImplantEvent/HookInput literal blast-radius pattern (#4313), the 3-binding flatten
  ripple (#4839), and ts-rs cfg(test) codegen mechanics (#4722); applied all three.
- Stored: entry #5750 "Regenerate ts-rs bindings AND fixtures under the workspace build, not
  isolated -p unimatrix-engine (serde_json preserve_order unification)" via context_store (pattern;
  edge Supports #4839).
