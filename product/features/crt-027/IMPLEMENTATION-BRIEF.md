# crt-027 Implementation Brief — WA-4: Proactive Knowledge Delivery

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-027/SCOPE.md |
| Scope Risk Assessment | product/features/crt-027/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/crt-027/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-027/specification/SPECIFICATION.md |
| Risk Test Strategy | product/features/crt-027/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-027/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| hook.rs routing (SubagentStart + MIN_QUERY_WORDS + write_stdout_subagent_inject) | pseudocode/hook-routing.md | test-plan/hook-routing.md |
| wire.rs source field extension | pseudocode/wire-source-field.md | test-plan/wire-source-field.md |
| listener.rs dispatch_request + CompactPayload migration | pseudocode/listener-dispatch.md | test-plan/listener-dispatch.md |
| IndexBriefingService (replaces BriefingService) | pseudocode/index-briefing-service.md | test-plan/index-briefing-service.md |
| services/mod.rs ServiceLayer wiring update | pseudocode/service-layer-wiring.md | test-plan/service-layer-wiring.md |
| mcp/tools.rs context_briefing handler | pseudocode/context-briefing-handler.md | test-plan/context-briefing-handler.md |
| mcp/response/briefing.rs IndexEntry + format_index_table | pseudocode/index-entry-formatter.md | test-plan/index-entry-formatter.md |
| uni-delivery-protocol.md protocol update | pseudocode/protocol-update.md | test-plan/protocol-update.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Route SubagentStart hook events through the existing `ContextSearch` pipeline so subagents receive injected knowledge before their first token, writing results via a `hookSpecificOutput` JSON envelope (`write_stdout_subagent_inject`) to stdout per Claude Code documentation. Replace `BriefingService` entirely with a new `IndexBriefingService` that returns active-only entries in a flat indexed table format (k=20 default, no section headers), consumed by both the `context_briefing` MCP tool and the `handle_compact_payload` UDS path, providing WA-5 a typed compile-time-stable contract surface (`IndexEntry` + `format_index_table`). Update the SM delivery protocol to call `context_briefing(max_tokens: 1000)` at every phase boundary.

## Resolved Decisions Table

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| How to tag SubagentStart observations without duplicating ContextSearch dispatch logic | Add `#[serde(default)] source: Option<String>` to `HookRequest::ContextSearch`; server uses `source.as_deref().unwrap_or("UserPromptSubmit")` for the observation `hook` column | SCOPE.md OQ-1 | architecture/ADR-001-contextsearch-source-field.md |
| SubagentStart routing strategy and UserPromptSubmit word-count guard | Add `"SubagentStart"` arm before `_` fallthrough in `build_request`; add `MIN_QUERY_WORDS: usize = 5`; both guards use `.trim()` before evaluation | SCOPE.md OQ-1, Goals §5 | architecture/ADR-002-subagentstart-routing-and-word-guard.md |
| UNIMATRIX_BRIEFING_K fate and IndexBriefingService dependency wiring | Env var deprecated and not read; `k=20` hardcoded in `IndexBriefingService::new()`; `EffectivenessStateHandle` is a required non-optional constructor parameter (missing wiring = compile error) | SCOPE-RISK-ASSESSMENT SR-03, SR-05 | architecture/ADR-003-indexbriefingservice-replaces-briefingservice.md |
| CompactPayload migration format: section structure vs flat index | Flat indexed table for both `context_briefing` and `handle_compact_payload`; `CompactionCategories` deleted; `format_compaction_payload` rewritten to accept `Vec<IndexEntry>`; histogram block and session context header preserved; 10 test invariants rewritten as 11 named tests | SCOPE-RISK-ASSESSMENT SR-04, SCOPE.md OQ-2 | architecture/ADR-004-compaction-payload-flat-index-migration.md |
| WA-5 format contract surface: inline string vs typed struct | `IndexEntry` typed struct + `format_index_table` named function + `SNIPPET_CHARS: usize = 150` constant; compile-time stable; WA-5 depends on the type, not on column widths | SCOPE-RISK-ASSESSMENT SR-06 | architecture/ADR-005-indexentry-typed-wa5-contract.md |
| SubagentStart stdout format (SR-01 resolution) | SubagentStart writes `hookSpecificOutput` JSON envelope via `write_stdout_subagent_inject`; UserPromptSubmit retains plain text `write_stdout`; server unchanged; confirmed via Claude Code documentation | SCOPE-RISK-ASSESSMENT SR-01 | architecture/ADR-006-subagentstart-stdout-json-envelope.md |

## Files to Create / Modify

### New File

| Path | Summary |
|------|---------|
| `crates/unimatrix-server/src/services/index_briefing.rs` | `IndexBriefingService`, `IndexBriefingParams`, `derive_briefing_query`; replaces deleted `briefing.rs` content |

### Modified Files

| Path | Summary |
|------|---------|
| `crates/unimatrix-engine/src/wire.rs` | Add `#[serde(default)] source: Option<String>` to `HookRequest::ContextSearch` variant |
| `crates/unimatrix-server/src/uds/hook.rs` | Add `"SubagentStart"` arm; add `MIN_QUERY_WORDS: usize = 5`; add `write_stdout_subagent_inject` helper; update UserPromptSubmit arm with `.trim().split_whitespace().count()` word-count guard |
| `crates/unimatrix-server/src/uds/listener.rs` | Replace hardcoded `"UserPromptSubmit"` with `source.as_deref().unwrap_or(...)` in `dispatch_request`; migrate `handle_compact_payload` from `BriefingService` to `IndexBriefingService`; rewrite `format_compaction_payload` accepting `Vec<IndexEntry>`; delete `CompactionCategories` struct |
| `crates/unimatrix-server/src/services/briefing.rs` | **Deleted**: entire file removed (struct, methods, tests, re-exports); replaced by `index_briefing.rs` |
| `crates/unimatrix-server/src/services/mod.rs` | Replace `briefing: BriefingService` with `briefing: IndexBriefingService`; update `ServiceLayer::with_rate_config()` construction; remove `parse_semantic_k()` call; add deprecation comment for `UNIMATRIX_BRIEFING_K` |
| `crates/unimatrix-server/src/mcp/tools.rs` | Update `context_briefing` handler (inside `#[cfg(feature = "mcp-briefing")]`) to call `IndexBriefingService::index()` with three-step query derivation via `derive_briefing_query`; return flat indexed table |
| `crates/unimatrix-server/src/mcp/response/briefing.rs` | Delete `Briefing` struct and `format_briefing`; add `IndexEntry`, `format_index_table`, `SNIPPET_CHARS: usize = 150`; retain `format_retrospective_report` |
| `.claude/protocols/uni/uni-delivery-protocol.md` | Add `context_briefing(topic="{feature-id}", session_id: "{session-id}", max_tokens: 1000)` after `context_cycle(type: "start", ...)` and after each of the five `context_cycle(type: "phase-end", ...)` call sites |

## Data Structures

### IndexEntry (WA-5 contract type)
```rust
// mcp/response/briefing.rs
// WA-5 contract: do not rename fields without updating WA-5 (PreCompact)
#[derive(Debug, Clone)]
pub struct IndexEntry {
    pub id: u64,
    pub topic: String,       // entry.topic — direct field, no join
    pub category: String,    // e.g., "decision", "pattern", "convention"
    pub confidence: f64,     // fused score: similarity + confidence + WA-2 boost
    pub snippet: String,     // first SNIPPET_CHARS chars of entry.content, UTF-8 safe
}

pub const SNIPPET_CHARS: usize = 150;
```

### IndexBriefingParams
```rust
pub struct IndexBriefingParams {
    pub query: String,
    pub k: usize,                    // default 20, not from UNIMATRIX_BRIEFING_K
    pub session_id: Option<String>,  // for WA-2 histogram boost
    pub max_tokens: Option<usize>,
}
```

### IndexBriefingService
```rust
pub(crate) struct IndexBriefingService {
    entry_store: Arc<Store>,
    search: SearchService,                            // carries its own EffectivenessStateHandle
    gateway: Arc<SecurityGateway>,
    default_k: usize,                                // hardcoded 20
    effectiveness_state: EffectivenessStateHandle,   // required, non-optional (ADR-003)
    cached_snapshot: Arc<Mutex<EffectivenessSnapshot>>,
}
```

### HookRequest::ContextSearch (extended)
```rust
// unimatrix-engine/src/wire.rs
ContextSearch {
    query: String,
    #[serde(default)] session_id: Option<String>,
    role: Option<String>,
    task: Option<String>,
    feature: Option<String>,
    k: Option<u32>,
    max_tokens: Option<u32>,
    #[serde(default)] source: Option<String>,  // NEW — None → "UserPromptSubmit"
}
```

### format_compaction_payload (updated signature)
```rust
// listener.rs
fn format_compaction_payload(
    entries: &[IndexEntry],
    role: Option<&str>,
    feature: Option<&str>,
    compaction_count: u32,
    max_bytes: usize,
    category_histogram: &HashMap<String, u32>,
) -> Option<String>
// Output: session context header block + flat indexed table + histogram block (if non-empty)
// Budget: hard ceiling truncation via truncate_utf8; lowest-ranked rows dropped first
```

## Function Signatures

```rust
// hook.rs — writes hookSpecificOutput JSON envelope for SubagentStart injection
fn write_stdout_subagent_inject(entries_text: &str) -> io::Result<()>
// { "hookSpecificOutput": { "hookEventName": "SubagentStart", "additionalContext": entries_text } }

// hook.rs — dispatch after HookResponse::Entries received
// if source == "SubagentStart": write_stdout_subagent_inject(text)
// else: write_stdout(text)  [unchanged plain-text path]

// services/index_briefing.rs — shared query derivation (both MCP and UDS call sites)
fn derive_briefing_query(
    task: Option<&str>,
    session_state: Option<&SessionState>,
    topic: &str,
) -> String
// Priority: (1) task if non-empty → (2) feature_cycle + top 3 topic_signals by vote count → (3) topic

// IndexBriefingService::new — required, non-optional EffectivenessStateHandle
pub(crate) fn new(
    entry_store: Arc<Store>,
    search: SearchService,
    gateway: Arc<SecurityGateway>,
    effectiveness_state: EffectivenessStateHandle,
) -> Self

// IndexBriefingService::index — primary method
pub(crate) fn index(
    params: IndexBriefingParams,
    audit_ctx: &AuditContext,
    caller_id: Option<&CallerId>,
) -> Result<Vec<IndexEntry>, ServiceError>
// Returns: status=Active entries only, sorted by fused score descending

// mcp/response/briefing.rs — canonical WA-5 contract formatter
pub fn format_index_table(entries: &[IndexEntry]) -> String
// Columns: row#, id, topic, category, confidence (2 decimal), snippet
// Empty slice → empty string
```

## Constraints

| Constraint | Detail |
|-----------|--------|
| Hook exit code | Always 0; SubagentStart path degrades gracefully on any error (FR-06, C-01) |
| Wire backward compat | `source: Option<String>` with `#[serde(default)]` — existing JSON without `source` deserializes to `None` unmodified (ADR-001) |
| BriefingService deletion | Complete — no dead code, no `#[allow(dead_code)]`; both callers migrated before deletion (C-03, AC-13) |
| HookRequest::Briefing | NOT removed — separate wire variant, not owned by this feature (C-04) |
| mcp-briefing feature flag | Guards MCP tool registration only; `IndexBriefingService`, `IndexEntry`, `format_index_table` compile unconditionally (NFR-05, C-07) |
| UNIMATRIX_BRIEFING_K | Deprecated and not read; k=20 hardcoded; deprecation comment at removal point in `services/mod.rs` (C-08, FR-13) |
| WA-5 surface contract | `IndexEntry` fields and `format_index_table` signature are the stable contract; column widths are implementation details (ADR-005) |
| HOOK_TIMEOUT | SubagentStart round-trip must complete within existing 40 ms budget (NFR-01) |
| MAX_COMPACTION_BYTES | Flat table budget enforced; rows truncated from end (lowest-ranked first) (NFR-03) |
| MIN_QUERY_WORDS guard scope | Applies to UserPromptSubmit only; SubagentStart uses `.trim().is_empty()` only (FR-05, ADR-002) |
| SM briefing token cap | `max_tokens: 1000` on every SM-initiated `context_briefing` call (NFR-07, AC-14) |
| Phase-conditioned ranking | Deferred to W3-1; `ServiceSearchParams` must remain extensible (C-05) |
| injection_history dedup | Not added to `context_briefing` index path (C-06) |

## Dependencies

| Dependency | Location | Role |
|-----------|---------|------|
| `unimatrix-store` (rusqlite/SQLite) | workspace | `EntryRecord`, `Store`, `Status` |
| `unimatrix-engine` wire types | workspace | `HookRequest`, `HookResponse`, `HookInput` |
| `SearchService` | `services/search.rs` | Fused search with WA-2 histogram boost |
| `ServiceSearchParams` | `services/search.rs` | Carries `session_id` for histogram boost |
| `SecurityGateway` | `services/gateway.rs` | Auth wrapper on all search calls; must wrap all three query derivation paths |
| `EffectivenessStateHandle` + `EffectivenessSnapshot` | `services/effectiveness.rs` | Required non-optional constructor dep; generation-cached ranking snapshot |
| `SessionRegistry` | `infra/session.rs` | `get_category_histogram`, `get_session_state` — used on MCP path for query derivation step 2 |
| `SessionState` | `infra/session.rs` | Held directly on UDS path; no registry lookup needed for step 2 |
| `serde_json` | workspace | `write_stdout_subagent_inject` envelope construction |
| `dirs` crate | workspace | Home dir resolution in `hook.rs` |
| `#[cfg(feature = "mcp-briefing")]` | `Cargo.toml` | Guards `context_briefing` MCP tool registration |

## NOT In Scope

- WA-4a phase-transition candidate cache (rebuilt on phase transition, drawn on PreToolUse) — deferred to W3-1
- Phase-to-category config mapping (phase-conditioned ranking) — deferred to W3-1
- `feature_cycle` ranking boost formula — W3-1 owns scoring changes
- `injection_history` dedup filter on `context_briefing` — no dedup on briefing index
- Successor pointer display for deprecated entries — post-WA-4 refinement
- WA-5 PreCompact transcript extraction — separate feature; crt-027 delivers format surface only
- Changes to `context_briefing` MCP tool signature — `role` and `task` remain present as declared fields
- New `UNIMATRIX_BRIEFING_K` replacement env var — deprecated and ignored; no replacement introduced
- `HookRequest::Briefing` wire variant removal — not owned by this feature
- Changes to `context_enroll`, `context_cycle`, or any other MCP tool

## Alignment Status

**Overall: PASS — 0 variances requiring human approval.**

Alignment review completed 2026-03-23 (v2, post-resolution re-review against updated artifacts including ADR-006). All three v1 WARN items resolved:

- **WARN-1 resolved**: `MIN_QUERY_WORDS` is now SCOPE.md Goal #5 with full spec coverage (FR-05, AC-02b, ARCHITECTURE.md §2b).
- **WARN-2 resolved**: SubagentStart stdout injection confirmed via Claude Code documentation. ADR-006 specifies the `hookSpecificOutput` JSON envelope and `write_stdout_subagent_inject` helper. AC-SR01 is CONFIRMED in SPECIFICATION.md; AC-SR02 and AC-SR03 track the divergent-format requirement. No spike required.
- **WARN-3 resolved**: `.trim().is_empty()` guards fully specified in ARCHITECTURE.md §2, SPECIFICATION.md FR-02 (SubagentStart) and FR-05 (UserPromptSubmit). EC-01 whitespace-only edge case is resolved.

Two minor observations (no approval required):
1. `derive_briefing_query` file location is TBD (`services/briefing.rs` or `services/query_derive.rs`) — implementation-time decision with no design risk.
2. `SNIPPET_CHARS` constant referenced in RISK-TEST-STRATEGY R-05 but not named in SPECIFICATION.md — R-05 scenario 3 will catch any drift at Gate 3c.

Two documented simplifications vs. product vision (both explicitly deferred in SCOPE.md §Non-Goals):
- WA-4a: routes SubagentStart to ContextSearch only; phase-transition candidate cache deferred to W3-1.
- WA-4b: uses existing fused score only; phase-to-category affinity ranking deferred to W3-1.

Tracking: https://github.com/dug-21/unimatrix/issues/349
