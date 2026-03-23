# crt-027 Architect Report

**Agent:** crt-027-agent-1-architect
**Status:** Complete

## Outputs

- `product/features/crt-027/architecture/ARCHITECTURE.md`
- `product/features/crt-027/architecture/ADR-001-contextsearch-source-field.md` — Unimatrix #3242
- `product/features/crt-027/architecture/ADR-002-subagentstart-routing-and-word-guard.md` — Unimatrix #3243
- `product/features/crt-027/architecture/ADR-003-indexbriefingservice-replaces-briefingservice.md` — Unimatrix #3244
- `product/features/crt-027/architecture/ADR-004-compaction-payload-flat-index-migration.md` — Unimatrix #3245
- `product/features/crt-027/architecture/ADR-005-indexentry-typed-wa5-contract.md` — Unimatrix #3246

## Key Design Decisions

### WA-4a: SubagentStart Hook Routing

SubagentStart is routed to `HookRequest::ContextSearch` with `source: "SubagentStart"` via
a new arm in `build_request` (hook.rs). The `source` field is added to `ContextSearch` with
`#[serde(default)]` — backward-compatible wire protocol addition. `dispatch_request` uses
the field for the `ObservationRow.hook` column instead of the hardcoded "UserPromptSubmit"
literal. WA-2 histogram boost applies automatically via the parent session_id.

Additionally: `MIN_QUERY_WORDS: usize = 5` constant added to hook.rs. UserPromptSubmit
prompts with < 5 words fall through to `generic_record_event` (no injection noise).

### WA-4b: BriefingService → IndexBriefingService

Full replacement. `IndexBriefingService::new()` takes `EffectivenessStateHandle` as a
required non-optional parameter (same pattern as ADR-004 crt-018b, entry #1546). k=20
hardcoded. `UNIMATRIX_BRIEFING_K` deprecated. Both callers migrated in this feature.
Service is not gated by `mcp-briefing` feature flag.

### CompactPayload Migration

`CompactionCategories` deleted. `format_compaction_payload` rewritten to accept
`Vec<IndexEntry>`. Session context header and histogram block retained. Section structure
(Decisions/Injections/Conventions headers) removed. WA-5 can prepend without parsing.

### IndexEntry as WA-5 Contract

`IndexEntry { id, topic, category, confidence, snippet }` + `format_index_table()` +
`SNIPPET_CHARS = 150` defined in `mcp/response/briefing.rs`. Not feature-gated. WA-5
depends on these by name, not by parsing rendered string output.

## SR-01 Resolution: SubagentStart stdout injection

Unconfirmed. Architecture degrades gracefully: if Claude Code ignores SubagentStart stdout,
the observation row and topic_signal are still recorded — strictly better than current state.
No error, no non-zero exit. Post-delivery manual validation step recommended for spec.

## SR-03 Resolution: EffectivenessStateHandle wiring

`IndexBriefingService::new()` takes `EffectivenessStateHandle` as required parameter. Passed
as `Arc::clone(&effectiveness_state)` in `ServiceLayer::with_rate_config()`. Missing wiring
is a compile error. Cached snapshot initialized internally (`EffectivenessSnapshot::new_shared()`).

## SR-04 Resolution: format_compaction_payload test invariants

10 existing tests must be rewritten not deleted. 8 of 10 invariants survive. Two do not:
- Section ordering (decisions before injections) → replaced by confidence-desc sort
- Deprecated indicator → replaced by active-entries-only invariant

Full invariant-to-new-test mapping in ARCHITECTURE.md and ADR-004.
