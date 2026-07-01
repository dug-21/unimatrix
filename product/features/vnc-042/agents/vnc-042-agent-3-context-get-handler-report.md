# Agent Report — vnc-042 Component 1: context_get handler

**Agent:** vnc-042-agent-3-context-get-handler
**Status:** COMPLETE — build green, clippy clean, 19 new tests pass, no regressions (mcp::tools 291 pass / 0 fail).

## Files modified
- `crates/unimatrix-server/src/mcp/tools.rs`
  - `GetParams`: added `follow_supersessions: Option<bool>` with `#[serde(default)]` + doc (handler-owned default-on).
  - New `pub(crate)` seams: `PreNote` enum, `resolve_effective_id(store, id, follow) -> (u64, PreNote)` (reuses canonical `crate::mcp::graph_read::follow_to_current`), `finalize_note(pre, &EntryRecord) -> Option<ResolutionNote>`.
  - `context_get` handler: resolution branch BEFORE fetch; single fetch on `effective_id`; edges on same `effective_id`; format route (`format_single_entry` clean / `format_single_entry_with_note` on note). Audit/usage/session-record now use `effective_id`.
  - Added `pub(crate) const CONTEXT_GET_DESCRIPTION` (macro needs inline literal — const kept byte-identical for the test, per existing CONTEXT_GRAPH_DESCRIPTION convention) and updated the `#[tool(description=...)]` literal.
  - New `#[cfg(test)] mod get_resolution_tests` (19 tests).
- `crates/unimatrix-server/src/infra/validation.rs` — ONE mechanical struct-literal fill (`follow_supersessions: None`) in an unrelated negative-id test; required for compilation. FLAGGED below.

## Tests (19 pass / 0 fail)
- **R-02 behavioral default (highest value):** `test_get_handler_field_absent_resolves_to_terminal` — JSON omits the field ⇒ deserializes `None` ⇒ `resolve_effective_id` FOLLOWS a deprecated A→B chain to terminal B with `Followed{A,B}`. Proves default-ON behaviorally, not a field round-trip.
- Serde three-state (absent/true/false), quoted-scalar rejection (#3728), additive-field non-regression.
- Clean passthrough (no note → base formatter), follow=false as-stored + `AsStoredDeprecated{Some(B)}` footer, false-on-active no footer, orphaned pointerless footer `superseded_by:None` (AC-08/R-08).
- Dead-end fail-loud (returned id == requested id): orphaned terminal, quarantined terminal, >50 hops (cap not weakened), self-cycle, store-error→None (non-existent id).
- Threading: hop selects `effective_id==B`; orthogonality — `resolve_effective_id` takes neither `format` nor `include_edges`, so id selection is invariant across them.
- Tool-description documents param + default + escape hatch.

## Issues / flags
- **AUDIT-ID CHOICE (Gate 3b WARN, confirm):** `target_ids`, `record_access`, `record_confirmed_entry` now record **`effective_id`** (the entry actually returned). Audit `detail` names both when they differ (`retrieved entry #{effective_id} (requested #{id})`). This is the one WARN-level open choice from Gate 3a — recording the returned entry, per pseudocode recommendation.
- **ADJACENT MECHANICAL EDIT (not silent behavior fix):** `infra/validation.rs:1022` `GetParams { .. }` struct-literal needed `follow_supersessions: None` to compile. Purely additive, no behavior change; flagged per coordination rule.
- Wave-1 transient unused-import warning is now cleared (build clean).
- **ResolutionNote (Wave-1 type) has no `PartialEq`** — my tests match it structurally. Not a defect; noted in case Wave-1 wants to add the derive.
- Handler-level tests use extracted seam fns because no `RequestContext<RoleServer>` constructor exists in unit scope (end-to-end route/format proofs belong to the integration suite per test-plan §Integration).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced #3538 (phase-snapshot-first), #317 (handler identity/ctx boilerplate), #3728 (MCP integer/string serde fragility); applied phase-snapshot-first (unchanged) and plain `Option<bool>` (no string coercion).
- Stored: entry #5389 "Unit-test rmcp #[tool] handler logic via extracted pub(crate) seam fns" via context_store (pattern).
