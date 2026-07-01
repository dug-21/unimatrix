# Agent Report — vnc-042-agent-1-pseudocode

## Deliverables

Per-component pseudocode under `product/features/vnc-042/pseudocode/`:
- `OVERVIEW.md` — component interaction, data flow, shared types (`ResolutionNote`, `ResolutionStatus` json contract), sequencing.
- `context-get-handler.md` — Component 1: `GetParams` field, resolution branch, effective_id threading, formatter route, tool-desc.
- `response-formatter.md` — Component 2: `ResolutionNote` enum + `format_single_entry_with_note`; `format_single_entry` unchanged.
- `follow-to-current-reexport.md` — Component 3: `pub(super)`→`pub(crate)` widen + re-export.

## Components covered

1. `context_get` handler + `GetParams` + tool description (`mcp/tools.rs`)
2. response formatter (`mcp/response/entries.rs`)
3. `follow_to_current` visibility (`mcp/graph_read_neighbors.rs`, `mcp/graph_read.rs`)

Matches the architecture Component Breakdown and IMPLEMENTATION-BRIEF Component Map exactly.

## Source grounding

All signatures verified against live code: `follow_to_current` (`graph_read_neighbors.rs:36`,
`pub(super) async fn(&Store, u64) -> Option<u64>`), `format_single_entry` (`entries.rs:24`),
`format_store_success_with_note` precedent (`entries.rs:189`), handler body (`tools.rs:950-1052`),
`Status` enum (`schema.rs:10`: Active=0/Deprecated=1/Proposed=2/Quarantined=3). No invented names.

## Open questions / gaps flagged

1. **Deprecated footer predicate vs quarantined status.** Footer scoped to
   `entry.status == Status::Deprecated`. A requested entry with *status* `Quarantined` (3),
   read via the escape hatch, gets NO footer under FR-07's "when deprecated" wording. AC-08's
   "orphaned/quarantined deprecated" reads as deprecated-status entries with `superseded_by IS NULL`.
   Confirm no separate footer is expected for a Quarantined-*status* requested entry. Low risk;
   default assumption documented in `context-get-handler.md`.
2. **Audit/usage id.** Handler currently records `id` in `target_ids`/usage/confirmed-entry.
   Pseudocode recommends recording `effective_id` (the entry actually returned) but this is not
   covered by any AC. Reviewer to confirm desired audit semantics.
3. **`graph_read.rs` re-export idiom.** Pseudocode says mirror the module's existing re-export
   style (e.g. `pub(crate) use ...`) rather than pin an exact line — implementer should match
   the idiom already used for sibling symbols like `handle_graph`.

No blocking gaps. No TODO/placeholders in the pseudocode.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_search` (pattern) → #298 generic parameterized-formatter,
  #4474 exact-tool-description-before-pseudocode; `context_search` (decision, topic vnc-042) →
  #5388/#5387/#5385 (the three vnc-042 ADRs, consistent with the ADR files read). Applied the
  parameterized-formatter pattern to Component 2 delegation rule and the exact-tool-desc rule to
  Component 1 §1b.
- Deviations from established patterns: none. `_with_note` mirrors the existing
  `format_store_success_with_note` split; visibility-widen follows Pattern #4436 fully-qualified
  call path.
