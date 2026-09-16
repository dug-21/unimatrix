# Agent Report — C2b Model CLI Ingress (Rust)

Agent: vnc-049-agent-3-c2b-model-cli-ingress (uni-rust-dev) | GH #986

## Summary

Closed the decomposition gap flagged in the C2 handoff: threaded the `--model` CLI
flag from the `unimatrix hook` subcommand into `HookInput.model_id`, validated via
`wire::is_valid_model_id` (fail-open, R-15), and wired it into every `ImplantEvent`
construction site on the CLI ingest path so C5's INSERT persists it. This completes
the CLI leg of AC-06's assembled path (CLI `--model` -> HookInput -> ImplantEvent ->
INSERT).

## Files modified

- `crates/unimatrix-server/src/main.rs`:
  - Added `#[arg(long)] model: Option<String>` to the `Hook` subcommand (next to
    `--provider`).
  - Threaded `model` through the `Command::Hook` match arm into `hook::run(...)`.
- `crates/unimatrix-server/src/uds/hook.rs`:
  - `run()` gained a `model: Option<String>` param; sets
    `hook_input.model_id = validate_cli_model_id(model)` before `build_request()`.
  - New private helper `validate_cli_model_id()` — fail-open filter through
    `is_valid_model_id`; invalid -> None + warn, absent -> None. (Extracted for unit
    testability; `run()` itself does UDS/stdin I/O.)
  - Imported `is_valid_model_id` from `unimatrix_engine::wire`.
  - Replaced the 8 `model_id: None` placeholders in the ImplantEvent construction
    literals (build_request / MultiEdit map / build_cycle_event_or_fallthrough /
    generic_record_event) with `input.model_id.clone()`. Only sites preceded by
    `provider: input.provider.clone(),` were touched; test HookInput fixtures and the
    parse_hook_input fallback default were left as None.
  - Added 5 unit tests.

## Tests

`cargo test -p unimatrix-server --lib model_id` -> 10 passed, 0 failed (includes my 5
new tests + the pre-existing C5 listener persistence + engine shape tests that prove
the INSERT leg).
`cargo test -p unimatrix-server --lib uds::hook` -> 206 passed, 0 failed, 1 ignored
(no regression in the hook module).

New tests:
- `validate_cli_model_id_valid_returns_some` — `ollama/qwen3-coder` -> Some.
- `validate_cli_model_id_invalid_returns_none` — uppercase/space/`@`, >128 chars, and
  empty string all -> None (R-15).
- `validate_cli_model_id_absent_returns_none` — None -> None (back-compat).
- `build_request_threads_model_id_into_implant_event` — HookInput.model_id ->
  ImplantEvent.model_id.
- `build_request_no_model_id_yields_none` — absent stays None.

`cargo build --workspace` clean. `cargo fmt` applied (my crate). `cargo clippy
-p unimatrix-server` — no warnings from main.rs / hook.rs. Pre-existing clippy
`-D warnings` in transcript_hold.rs/verbosity.rs/main.rs left untouched (out of scope);
no new ones added.

## Boundary notes

- Did NOT touch listener.rs (C5), observation(s).rs (C6), normalize.js (C3), or the
  plugin dir (C1). The ImplantEvent.model_id -> INSERT bind was already correct in C5;
  my job ended at populating HookInput.model_id from `--model` and threading it into
  the ImplantEvent build. Confirmed via the C5 persistence tests that pass unchanged.
- Shared-tree note: `edge_write_delete_agent_tests.rs` and
  `tests/project_routing_integration.rs` showed uncommitted changes in the working
  tree from concurrent agents; I staged ONLY my two files. Did not run/modify
  integration tests per scope.

## End-to-end path

CLL `--model` path is now closed end to end:
CLI `--model <id>` -> main.rs Hook arm -> `hook::run(event, provider, model, ...)` ->
`validate_cli_model_id` -> `HookInput.model_id` -> `build_request` ->
`ImplantEvent.model_id` -> (C5) listener INSERT. Verified by the assembled-path unit
tests plus the C5 `test_opencode_model_id_survives_wire_to_insert` passing.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing + context_search -- surfaced ADR/patterns
  #5751 (delegating-guard/KNOWN_PROVIDERS hoist), #5750 (ts-rs fixture regen under
  workspace), #4313 (touch every ImplantEvent/HookInput literal), #4839 (ts-rs flatten
  ripple), #4316 (KNOWN_* allowlist match guard). Applied #4313 (enumerated all 8
  literals) and the fail-open validation convention.
- Stored: entry #5753 "An additive wire carrier can be type-complete end-to-end yet
  dead: the ingress (value-origin) site is a separate touchpoint from the field
  declaration" via /uni-store-pattern (complements #4313; captures the false-green
  decomposition gap this task closed).
