# vnc-042 — Implementation Brief

`context_get` resolves superseded (deprecated) entries to their current version by default.

**Tracking:** GH #843 (product behavior + AC-01..AC-07 LOCKED) · **Feature ID:** vnc-042

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/vnc-042/SCOPE.md |
| Scope Risk Assessment | product/features/vnc-042/SCOPE-RISK-ASSESSMENT.md |
| Specification | product/features/vnc-042/specification/SPECIFICATION.md |
| Architecture | product/features/vnc-042/architecture/ARCHITECTURE.md |
| ADR-001 (param name + default) | product/features/vnc-042/architecture/ADR-001-follow-supersessions-param-default.md |
| ADR-002 (dead-end fail-loud) | product/features/vnc-042/architecture/ADR-002-dead-end-fail-loud.md |
| ADR-003 (response construction) | product/features/vnc-042/architecture/ADR-003-response-construction.md |
| Risk / Test Strategy | product/features/vnc-042/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-042/ALIGNMENT-REPORT.md |
| Acceptance Map | product/features/vnc-042/ACCEPTANCE-MAP.md |

## Component Map

Session 2 Stage 3a produces per-component pseudocode + test-plan files. Expected components
from the architecture (paths filled during delivery):

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| `context_get` handler (resolution branch, `GetParams`, tool-desc) | pseudocode/context-get-handler.md | test-plan/context-get-handler.md |
| response formatter (`format_single_entry_with_note`, `ResolutionNote`) | pseudocode/response-formatter.md | test-plan/response-formatter.md |
| `follow_to_current` visibility widen + re-export | pseudocode/follow-to-current-reexport.md | test-plan/follow-to-current-reexport.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

`context_get(id)` today performs a raw by-ID read (`entry_store.get(id)`) with no status
check, silently returning stale content when `id` points at a deprecated entry. Add one
parameter, `follow_supersessions: Option<bool>` (default-on via handler), so a deprecated id
resolves to its active terminal by default — full content, same shape — while `false` returns
the entry exactly as stored for lookback/audit. A surgical single-tool contract change in the
MCP server crate reusing existing supersession machinery: no schema, no new SQL, no other-tool
changes.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|------------|--------|----------|
| Parameter name + shape + default | `follow_supersessions: Option<bool>`, `#[serde(default)]`, semantic default `true`. Handler OWNS the default (`None ⇒ follow`) — a bare `#[serde(default)] bool` defaults OFF and is FORBIDDEN. | OQ-1 / SR-06 / FR-01 | architecture/ADR-001-follow-supersessions-param-default.md |
| Graph-vs-get naming/default divergence | ACCEPTED. `context_graph.resolve_supersessions` defaults `false`; `context_get.follow_supersessions` defaults `true`. Distinct verb `follow_*` signals the flipped default; shared noun `supersessions` keeps it greppable. No cross-tool standardization now. | OQ-C / SR-06 | architecture/ADR-001-follow-supersessions-param-default.md |
| Dead-end path (`follow_to_current → None`) — which entry returned | Return the ORIGINALLY-REQUESTED id (`effective_id = id`) with a loud `DeadEnd` flag — never empty, never silent. No stop-id surfacing, no new walk. Covers orphaned/quarantined terminal, >50 hops, and internal store error. | OQ-2 / SR-05 / FR-08 | architecture/ADR-002-dead-end-fail-loud.md |
| Notice injection point | New `format_single_entry_with_note` wrapper. NEVER inject inside `format_single_entry` (protects byte-identity canary). Clean passthrough uses the base `format_single_entry` unchanged. | SR-04 / FR-09 / C-7 | architecture/ADR-003-response-construction.md |
| Which entry's edge LIST on a resolved get | Rebuild edges on the RESOLVED-terminal id: `build_edges_view(&store, effective_id)`. Edge *targets* stay unresolved (NG-1). | OQ-A / SR-03 / FR-11 | architecture/ADR-003-response-construction.md |
| `format="json"` notice shape | Structured `resolution` object, present ONLY when non-clean (absent on clean passthrough to preserve json byte-identity). Typed discriminant, not a parsed string. | OQ-B / OQ-3 / FR-12 | architecture/ADR-003-response-construction.md |
| Orphaned/quarantined footer (`superseded_by IS NULL`) | Footer built from `Option<u64>`: `Some(Z) ⇒ "deprecated; superseded by #{Z} (omit follow_supersessions to follow)."`; `None ⇒ "deprecated; no recorded successor."`. No panic, no malformed `#{}`. | R-08 / AC-08 / FR-07 | architecture/ADR-003-response-construction.md |

## Files to Create / Modify

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-server/src/mcp/tools.rs` | MODIFY | Add `follow_supersessions: Option<bool>` to `GetParams` (`:246-274`); add resolution branch in `context_get` handler (`:950-1052`) selecting `effective_id` + `ResolutionNote`; route formatter; update tool-description strings (`:947-948`, C-5). |
| `crates/unimatrix-server/src/mcp/response/entries.rs` | MODIFY | Add `format_single_entry_with_note(entry, format, edges, note: &ResolutionNote)` + `ResolutionNote` enum. `format_single_entry` UNCHANGED. |
| `crates/unimatrix-server/src/mcp/graph_read_neighbors.rs` | MODIFY | Widen `follow_to_current` `pub(super)` → `pub(crate)` (canonical copy at `:36-55`). |
| `crates/unimatrix-server/src/mcp/graph_read.rs` | MODIFY | Re-export `follow_to_current` so the handler calls `crate::mcp::graph_read::follow_to_current` (Pattern #4436). ONLY visibility change. |

**Do NOT modify:** `graph_read_supersession.rs` (`handle_current` is the wrong primitive — errors on orphaned, violates AC-04; the duplicate `follow_to_current` at `:122` is NOT the one to call and is NOT consolidated here). No schema, no SQL, no changes to `context_search` / `context_lookup` / `context_graph`.

## Data Structures

```rust
// GetParams (tools.rs:246-274) — ADD one field, purely additive
/// Resolve a requested deprecated id to its active terminal (vnc-042).
/// - None (omitted) / Some(true) ⇒ DEFAULT-ON: follow superseded_by to the Active terminal.
/// - Some(false) ⇒ escape hatch: return the entry exactly as stored (any status).
#[serde(default)]
pub follow_supersessions: Option<bool>,

// ResolutionNote (response/entries.rs, NEW) — handler → formatter
enum ResolutionNote {
    Followed { from: u64, to: u64 },                       // AC-02 hop
    DeadEnd { requested: u64 },                            // AC-04 no active successor
    AsStoredDeprecated { requested: u64, superseded_by: Option<u64> }, // AC-03 / AC-08
    // clean passthrough carries NO note (uses format_single_entry)
}
```

Rendering (ADR-003):

| Variant | text (summary/markdown) | JSON `resolution` |
|---------|-------------------------|-------------------|
| `Followed{from,to}` | prepend `↻ Requested #{from} (deprecated) → returning current version #{to}.` | `{"status":"followed","requested_id":from,"returned_id":to}` |
| `DeadEnd{requested}` | prepend `⚠ Requested #{requested}: no active successor found (chain dead-ends on a non-active entry).` | `{"status":"no_active_successor","requested_id":requested}` |
| `AsStoredDeprecated` (Some Z) | append `deprecated; superseded by #{Z} (omit follow_supersessions to follow).` | `{"status":"as_stored_deprecated","requested_id":X,"superseded_by":Z}` |
| `AsStoredDeprecated` (None) | append `deprecated; no recorded successor.` | `{"status":"as_stored_deprecated","requested_id":X,"superseded_by":null}` |
| clean passthrough | — (no note) | *(no `resolution` key)* |

## Function Signatures

```rust
// REUSE (widen visibility only)
follow_to_current(store: &Store, id: u64) -> Option<u64>   // graph_read_neighbors.rs:36; pub(super)→pub(crate)
// REUSE (unchanged) — call with effective_id, not the requested id
build_edges_view(store: &Store, id: u64) -> Result<EdgesView, StoreError>  // get_edges.rs, tools.rs:991
// UNCHANGED (byte-identity invariant)
format_single_entry(&EntryRecord, ResponseFormat, Option<&EdgesView>) -> CallToolResult  // response/entries.rs:24
// NEW (mirrors format_store_success_with_note @ response/entries.rs:189)
format_single_entry_with_note(&EntryRecord, ResponseFormat, Option<&EdgesView>, note: &ResolutionNote) -> CallToolResult
```

Handler control flow (ARCHITECTURE data flow):

```
id = validated_id(params.id)?            // u64, tools.rs:977, no cast
follow_supersessions == Some(false)          → effective_id = id; AsStored
None | Some(true) (DEFAULT):
    follow_to_current(&store, id).await
        Some(t) && t == id → effective_id = id; CleanPassthrough (no note)
        Some(t) && t != id → effective_id = t;  Followed{from:id, to:t}
        None               → effective_id = id; DeadEnd{requested:id}   (ADR-002)
entry = entry_store.get(effective_id)    // single fetch, no double-read
  AsStored + deprecated → AsStoredDeprecated{requested:id, superseded_by: entry.superseded_by}
edges = include_edges != Some(false) ? build_edges_view(&store, effective_id) : None   // ADR-003
CleanPassthrough → format_single_entry(...)         // byte-identical
else             → format_single_entry_with_note(..., &note)
```

## Constraints

- **C-1** Reuse `follow_to_current` / `query_current_terminal`; NO reimplemented chain-walk. (AC-05, #4468)
- **C-2** `#[serde(default)]` on `Option<bool>`; handler owns default-on. Omitted field ⇒ follow path. (AC-06)
- **C-3** 50-hop cap + `status=0` orphaned-terminal guard are load-bearing; MUST NOT be weakened. (SR-05, #4538)
- **C-4** No `.unwrap()` in non-test code; errors via project error type + `.map_err`. Post-primary-read failures FAIL-LOUD (`tools.rs:984-987`).
- **C-5** Update `context_get` tool-description strings to document the new default + escape hatch. A lying description is a known hazard. (#4303)
- **C-6** ADR present (ADR-001/002/003). No further ADR gate for delivery.
- **C-7** Notice injection in the handler wrapper, never `format_single_entry`. (byte-identity)
- **Single fetch:** `effective_id` must thread to BOTH `entry_store.get` AND `build_edges_view`. A partial swap (terminal content, requested-id edges) is the highest-probability integration defect (R-03).
- **Canonical copy:** call `crate::mcp::graph_read::follow_to_current`, not the `graph_read_supersession.rs:122` duplicate (R-05).

## Dependencies

- **Present, no upstream blocker:** `follow_to_current` (`graph_read_neighbors.rs:36-55`), `query_current_terminal` (`graph_queries.rs:161-201`), `supersedes`/`superseded_by` columns (`schema.rs:67,69`; `db.rs:554-555`) written by `context_correct`. Notice precedent `format_store_success_with_note` (`response/entries.rs:189`).
- **No new crates or external services.**
- **Enables (out of scope):** deferred NG-1 neighbor-target resolution can build on this wiring.

## NOT in Scope

- **NG-1** Resolving stale neighbor/edge *targets* inside `include_edges` — deprecated targets still show old id+title. Only the requested entry resolves; the resolved get returns the terminal's edge *list* but leaves targets unresolved (SR-07, accepted asymmetry).
- **NG-2** Multi-entry / chain / evolution view on `context_get` — chain lookback stays in `context_graph` mode `chain`.
- **NG-3** No change to `context_search` / `context_lookup`.
- **NG-4** No change to `context_graph` or its `resolve_supersessions` param/default/semantics.
- **NG-5** No schema / storage change.
- Migration of store-layer read-back-after-deprecate tests — false positives; they call `store.get()` (stays as-stored) and do not break (#5383). Counting them over-scopes the acceptance map.
- Consolidating the duplicate `follow_to_current` copies — flagged for future cleanup, not this feature.

## Alignment Status

Vision alignment PASS (advances the "trustworthy, consistent" retrieval promise on the
most-used read tool; honors principles #4 typed graph and #5 fail-loud). One non-blocking
WARN carried forward:

- **WARN — Goal/Capability visibility (advisory, non-blocking):** vnc-042 is READ-side
  supersession resolution; the self-learning integrity capability SLN3 (#5230) has a purely
  WRITE-side `done_when`. The feature ships capability-invisible (advances no tracked
  `done_when`). **Recommendation: accept and proceed** — LOCKED (#843), correctness-motivated,
  vision-aligned. Separate advisory action, owner = uni-zero / goal steward (NOT a delivery
  task): add a read-side consistency clause to SLN3 or a sibling capability under #5219.

No blocking VARIANCE or FAIL. AC-08 is within-scope edge-case hardening of AC-03 (does not
alter the LOCKED set).
