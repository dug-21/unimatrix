# Agent Report: crt-039-agent-3-background

**Component**: `crates/unimatrix-server/src/background.rs`
**Feature**: crt-039 — Tick Decomposition: Decouple Structural Graph Inference from NLI Gate

---

## Changes Made

File modified: `crates/unimatrix-server/src/background.rs`

Three changes per pseudocode spec:

1. **Ordering invariant comment** added before the contradiction scan block (lines 661–667). Reflects the full canonical sequence: compaction → promotion → graph-rebuild → contradiction_scan → extraction_tick → structural_graph_tick.

2. **Named section comment** on contradiction scan block (lines 669–674). Replaces the old single-line `GH #278 fix:` comment with a labeled block that makes the condition explicit (`embed adapter available && tick_multiple_of_interval`) and its independence from the structural graph tick clear. Zero behavioral change — NFR-07 zero-diff constraint satisfied (only comment additions, no condition mutations).

3. **Removed `if inference_config.nli_enabled` gate** around `run_graph_inference_tick` call (lines 759–769 before). Replaced with a named `--- Structural graph tick (always) ---` comment block and unconditional call. The function signature is unchanged; internal gating of Phase 8 remains inside `run_graph_inference_tick` via `get_provider()`.

Commit: `605f985c` — `impl(background): remove nli_enabled gate; add ordering invariant and contradiction scan labels (crt-039)`

---

## Test Results

- **Build**: `cargo build --workspace` — zero errors, 16 pre-existing warnings in unimatrix-server
- **Tests**: 2569 passed, 1 failed

The 1 failure is `test_phase4b_uses_nli_informs_cosine_floor_not_supports_threshold` in `nli_detection_tick.rs`. This is a pre-existing failure caused by the cosine floor change (0.45 → 0.50) in `config.rs` — it asserts the old value 0.45 and is listed as a "Test to Update" (TC-U) in the implementation brief. It is out of scope for this agent; it will be addressed by the agent working on `nli_detection_tick.rs` / `config.rs`.

---

## AC Verification

- **AC-01** (gate removal): `run_single_tick` no longer contains `if inference_config.nli_enabled { ... run_graph_inference_tick ... }`. Call is unconditional.
- **AC-07** (ordering invariant comment): Present with exact text matching pseudocode spec.
- **AC-06** (contradiction scan zero-diff): `git diff` on the contradiction scan block shows only line additions (new comment lines). Condition `if current_tick.is_multiple_of(CONTRADICTION_SCAN_INTERVAL_TICKS)` and inner `if let Ok(adapter) = embed_service.get_adapter().await` are unchanged. No bracket, condition, or assignment mutations.

---

## Issues / Blockers

None. The pre-existing test failure is a dependency on the `config.rs` / `nli_detection_tick.rs` agent completing the cosine floor update and test adjustments.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — no briefing call made (used direct `context_search` per spawn prompt instructions). Found ADR-001 (#4017), ADR-002 (#4018), ADR-003 (#4019) confirming the control-flow split design. Pattern #3897 (helper extraction for infallible tick writes) confirmed this tick remains infallible (`run_graph_inference_tick` returns `()`).
- Stored: nothing novel to store — the changes to `background.rs` are comment additions and a gate removal. The pattern of unconditional tick calls with internal gating is well-established in this codebase and already captured in existing Unimatrix entries.
