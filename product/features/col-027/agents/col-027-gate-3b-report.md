# Agent Report: col-027-gate-3b

**Agent ID:** col-027-gate-3b
**Gate:** 3b (Code Review)
**Feature:** col-027
**Result:** REWORKABLE FAIL

## Work Completed

Ran all Gate 3b checks against the 6 changed files for col-027:
- `crates/unimatrix-core/src/observation.rs`
- `crates/unimatrix-server/src/uds/hook.rs`
- `crates/unimatrix-server/src/uds/listener.rs`
- `crates/unimatrix-observe/src/detection/friction.rs`
- `crates/unimatrix-observe/src/detection/mod.rs`
- `crates/unimatrix-observe/src/metrics.rs`

## Checks Run

1. `cargo build --workspace` — PASS (0 errors)
2. `cargo clippy --workspace -- -D warnings` — 60 errors, ALL pre-existing (confirmed via git stash; count identical before col-027 changes)
3. `cargo test --workspace` — PASS (0 failures)
4. Placeholder scan — PASS (no todo!/unimplemented!/TODO/FIXME in changed files)
5. hook_type::POSTTOOLUSEFAILURE constant usage — FAIL (1 inline string in listener.rs:2602 production code)
6. extract_error_field() not called from extract_response_fields() — PASS
7. ToolFailureRule registered in default_rules() — PASS
8. Rule count = 22 — PASS
9. Knowledge stewardship (3 agent reports) — PASS

## Finding

One rework-required issue:

`crates/unimatrix-server/src/uds/listener.rs` line 2602 uses `"PostToolUseFailure"` as an inline string literal in the production match arm inside `extract_observation_fields()`. The `hook_type` module is not imported at the file's top-level imports (only in test scope at line 4433).

Fix: add `use unimatrix_core::observation::hook_type;` to listener.rs top-level imports and change the match arm to `hook_type::POSTTOOLUSEFAILURE =>`.

## Knowledge Stewardship

- Stored: nothing novel to store -- the "use constants not inline strings in match arms" rule is an existing project convention. The finding is a single missed application, not a new pattern.
