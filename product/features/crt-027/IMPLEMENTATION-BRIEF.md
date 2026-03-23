# crt-027 Implementation Brief — WA-4: Proactive Knowledge Delivery

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-027/SCOPE.md |
| Architecture | product/features/crt-027/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-027/specification/SPECIFICATION.md |
| Risk Test Strategy | product/features/crt-027/RISK-TEST-STRATEGY.md |
| Alignment Report | N/A — no vision variance raised during Session 1 |

---

## Goal

Route `SubagentStart` hook events through the existing `ContextSearch` pipeline so subagents
receive injected Unimatrix knowledge before their first token. Simultaneously replace
`BriefingService` with a new `IndexBriefingService` that returns a flat indexed table of
active-only entries (k=20 default) used by both the `context_briefing` MCP tool and the
`handle_compact_payload` UDS path, providing WA-5 a clean typed contract surface for
transcript prepend.

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| wire.rs (source field) | pseudocode/wire-source-field.md | test-plan/wire-source-field.md |
| hook.rs (SubagentStart arm + word guard) | pseudocode/hook-routing.md | test-plan/hook-routing.md |
| listener.rs (dispatch + compact payload) | pseudocode/listener-dispatch-compact.md | test-plan/listener-dispatch-compact.md |
| services/briefing.rs → index_briefing.rs | pseudocode/index-briefing-service.md | test-plan/index-briefing-service.md |
| services/mod.rs (ServiceLayer wiring) | pseudocode/service-layer-wiring.md | test-plan/service-layer-wiring.md |
| mcp/tools.rs (context_briefing handler) | pseudocode/mcp-briefing-handler.md | test-plan/mcp-briefing-handler.md |
| mcp/response/briefing.rs (formatter) | pseudocode/index-entry-formatter.md | test-plan/index-entry-formatter.md |
| uni-delivery-protocol.md (SM update) | pseudocode/sm-protocol-update.md | test-plan/sm-protocol-update.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Resolved Decisions Table

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| How to tag observations from SubagentStart-sourced ContextSearch | Add `#[serde(default)] source: Option<String>` to `HookRequest::ContextSearch`; `dispatch_request` uses `source.as_deref().unwrap_or("UserPromptSubmit")` for observation `hook` column | ADR-001 | architecture/ADR-001-contextsearch-source-field.md |
| SubagentStart routing strategy and UserPromptSubmit word guard | Route SubagentStart to ContextSearch when `prompt_snippet` is non-empty; add `MIN_QUERY_WORDS: usize = 5` constant guarding UserPromptSubmit (not SubagentStart) | ADR-002 | architecture/ADR-002-subagentstart-routing-and-word-guard.md |
| `UNIMATRIX_BRIEFING_K` env var fate and `IndexBriefingService` k default | Env var deprecated and not read; k=20 hardcoded in `IndexBriefingService::new()`; `parse_semantic_k()` deleted | ADR-003 | architecture/ADR-003-indexbriefingservice-replaces-briefingservice.md |
| `EffectivenessStateHandle` wiring on `IndexBriefingService` | Required non-optional constructor parameter; missing wiring is a compile error (ADR-004 crt-018b pattern) | ADR-003 | architecture/ADR-003-indexbriefingservice-replaces-briefingservice.md |
| CompactPayload format after BriefingService removal | Flat indexed table (not preserved section structure); `CompactionCategories` deleted; `format_compaction_payload` rewritten to accept `Vec<IndexEntry>`; 11 tests rewritten (8 invariants survive, 2 replaced) | ADR-004 | architecture/ADR-004-compaction-payload-flat-index-migration.md |
| WA-5 format contract surface | Typed `IndexEntry` struct + `format_index_table()` function + `SNIPPET_CHARS = 150` constant in `mcp/response/briefing.rs`; WA-5 depends by name, not by parsing rendered string | ADR-005 | architecture/ADR-005-indexentry-typed-wa5-contract.md |

---

## Files to Create/Modify

### New File

| Path | Summary |
|------|---------|
| `crates/unimatrix-server/src/services/index_briefing.rs` | New `IndexBriefingService`, `IndexBriefingParams`, and `derive_briefing_query` shared helper |

### Modified Files

| Path | Summary |
|------|---------|
| `crates/unimatrix-engine/src/wire.rs` | Add `#[serde(default)] source: Option<String>` to `HookRequest::ContextSearch` variant |
| `crates/unimatrix-server/src/uds/hook.rs` | Add `"SubagentStart"` arm in `build_request`; add `MIN_QUERY_WORDS: usize = 5` constant; apply word-count guard to `UserPromptSubmit` arm |
| `crates/unimatrix-server/src/uds/listener.rs` | Replace hardcoded `"UserPromptSubmit"` literal in `dispatch_request` with `source` field value; migrate `handle_compact_payload` from `BriefingService::assemble()` to `IndexBriefingService::index()`; rewrite `format_compaction_payload` to accept `Vec<IndexEntry>` |
| `crates/unimatrix-server/src/services/briefing.rs` | **Deleted** — all contents replaced; file renamed/rewritten as `index_briefing.rs` |
| `crates/unimatrix-server/src/services/mod.rs` | Replace `BriefingService` field and re-export with `IndexBriefingService`; remove `parse_semantic_k()` call; update `ServiceLayer::with_rate_config()` construction |
| `crates/unimatrix-server/src/mcp/tools.rs` | Update `context_briefing` handler (inside `#[cfg(feature = "mcp-briefing")]`) to call `IndexBriefingService::index()` with three-step query derivation |
| `crates/unimatrix-server/src/mcp/response/briefing.rs` | Replace `Briefing` struct and `format_briefing()` with `IndexEntry`, `format_index_table()`, and `SNIPPET_CHARS = 150`; retain `format_retrospective_report` |
| `.claude/protocols/uni/uni-delivery-protocol.md` | Add `context_briefing(topic: "{feature-id}", session_id: "{session-id}", max_tokens: 1000)` after `context_cycle(type: "start")` and after each of the five `context_cycle(type: "phase-end")` calls |

---

## Data Structures

### `IndexEntry` (new — in `mcp/response/briefing.rs`)

```rust
#[derive(Debug, Clone)]
pub struct IndexEntry {
    pub id: u64,
    pub topic: String,       // entry.topic (direct field, no join)
    pub category: String,    // e.g., "decision", "pattern", "convention"
    pub confidence: f64,     // fused score (similarity + confidence + WA-2 boost)
    pub snippet: String,     // first SNIPPET_CHARS (150) chars of entry.content, UTF-8 safe
}

pub const SNIPPET_CHARS: usize = 150;
```

### `IndexBriefingParams` (new — in `services/index_briefing.rs`)

```rust
pub struct IndexBriefingParams {
    pub query: String,                  // derived by derive_briefing_query()
    pub k: usize,                       // default 20
    pub session_id: Option<String>,     // for WA-2 histogram boost
    pub max_tokens: Option<usize>,
}
```

### `HookRequest::ContextSearch` (modified — in `unimatrix-engine/src/wire.rs`)

Gains one new field:
```rust
#[serde(default)]
source: Option<String>,   // None => "UserPromptSubmit"; Some("SubagentStart") for SubagentStart events
```

All existing callers that omit `source` continue to work (`#[serde(default)]`).

### Removed Structs

- `BriefingService`, `BriefingParams`, `BriefingResult`, `InjectionSections`, `InjectionEntry`
  — all deleted from `services/briefing.rs`
- `CompactionCategories` — deleted from `uds/listener.rs`
- `Briefing` (response type) and `format_briefing()` — deleted from `mcp/response/briefing.rs`

---

## Function Signatures

### New Interfaces

```rust
// services/index_briefing.rs
pub(crate) struct IndexBriefingService { ... }

impl IndexBriefingService {
    pub(crate) fn new(
        entry_store: Arc<Store>,
        search: SearchService,
        gateway: Arc<SecurityGateway>,
        effectiveness_state: EffectivenessStateHandle,  // required, non-optional
    ) -> Self;

    pub(crate) async fn index(
        &self,
        params: IndexBriefingParams,
        audit_ctx: &AuditContext,
        caller_id: Option<&CallerId>,
    ) -> Result<Vec<IndexEntry>, ServiceError>;
}

// shared query derivation helper (location: services/index_briefing.rs or services/query_derive.rs)
pub(crate) fn derive_briefing_query(
    task: Option<&str>,
    session_state: Option<&SessionState>,
    topic: &str,
) -> String;
// Priority: (1) task if non-empty, (2) feature_cycle + top 3 topic_signals by vote count,
// (3) topic param fallback
```

```rust
// mcp/response/briefing.rs
pub fn format_index_table(entries: &[IndexEntry]) -> String;
// Flat indexed table: row#, id, topic, category, confidence (2dp), snippet
// Empty slice → empty string
// WA-5 contract: call this function; do not parse its output
```

### Modified Signatures

```rust
// uds/listener.rs — format_compaction_payload
fn format_compaction_payload(
    entries: &[IndexEntry],
    role: Option<&str>,
    feature: Option<&str>,
    compaction_count: u32,
    max_bytes: usize,
    category_histogram: &HashMap<String, u32>,
) -> Option<String>;
// Previous signature: fn format_compaction_payload(categories: &CompactionCategories, ...) -> Option<String>
// Returns: None when entries empty AND histogram empty; Some(flat table + histogram block) otherwise
```

### Deleted Functions

- `BriefingService::assemble()`
- `briefing::parse_semantic_k()`
- `format_briefing()`

---

## Constraints

| ID | Constraint |
|----|-----------|
| C-01 | Hook exit code is always 0. SubagentStart must degrade gracefully on empty prompt, server unavailable, or search error. |
| C-02 | `HookRequest::ContextSearch` `source` field is backward-compatible (`#[serde(default)]`). All existing callers unaffected. |
| C-03 | `BriefingService` deleted in this feature — no dead code, no deferred cleanup. |
| C-04 | `HookRequest::Briefing` wire variant is NOT removed in this feature. |
| C-05 | Phase-conditioned ranking deferred to W3-1. `IndexBriefingService` uses extensible `ServiceSearchParams`. |
| C-06 | `injection_history` dedup filter NOT added to `context_briefing`. |
| C-07 | `mcp-briefing` feature flag guards MCP tool registration only. `IndexBriefingService` compiles unconditionally. |
| C-08 | `UNIMATRIX_BRIEFING_K` env var deprecated and not read by `IndexBriefingService`. A code comment at removal documents deprecation. |
| NFR-01 | SubagentStart hook round-trip must complete within the existing 40 ms `HOOK_TIMEOUT`. |
| NFR-03 | Flat indexed table respects `MAX_COMPACTION_BYTES` on CompactPayload path; rows truncated tail-first. |
| NFR-04 | `IndexEntry.snippet` truncated at valid UTF-8 character boundary (`chars().take(150)`). |
| NFR-05 | `IndexBriefingService` compiles regardless of `mcp-briefing` flag. |
| NFR-07 | SM briefing calls in protocol specify `max_tokens: 1000` per call. |

---

## Dependencies

| Dependency | Location | Notes |
|-----------|----------|-------|
| `unimatrix-store` (rusqlite/SQLite) | workspace crate | `EntryRecord`, `Store`, `Status` |
| `unimatrix-engine` wire types | workspace crate | `HookRequest`, `HookResponse`, `HookInput` |
| `SearchService` | `services/search.rs` | Fused search with histogram boost; consumed unchanged |
| `ServiceSearchParams` | `services/search.rs` | Carries `session_id` for WA-2 boost |
| `SecurityGateway` | `services/gateway.rs` | Auth wrapper on search calls; required in `IndexBriefingService::new()` |
| `EffectivenessStateHandle` | `services/effectiveness.rs` | Required constructor dep — compile error if missing (ADR-003) |
| `SessionRegistry` | `infra/session.rs` | MCP path uses `get_session_state(session_id)` for query derivation step 2 |
| `#[cfg(feature = "mcp-briefing")]` | `Cargo.toml` | Guards MCP tool registration; does NOT gate `IndexBriefingService` |

---

## NOT in Scope

- WA-4a candidate cache (phase-transition set rebuilt on PreToolUse) — deferred
- Phase-to-category config mapping / phase-conditioned ranking — deferred to W3-1
- `feature_cycle` ranking boost formula — W3-1
- `injection_history` dedup filter on `context_briefing`
- Successor pointer display for deprecated entries
- WA-5 PreCompact transcript extraction (crt-028 or similar) — this feature delivers format surface only
- Changes to `context_briefing` MCP tool signature (`role` and `task` remain declared)
- New `UNIMATRIX_BRIEFING_K` replacement env var
- `HookRequest::Briefing` wire variant removal
- Changes to `context_enroll`, `context_cycle`, or any other MCP tool

---

## Alignment Status

No ALIGNMENT-REPORT.md was produced during Session 1 (no vision guardian variance reported).
All open questions from scoping (OQ-1 through OQ-4) were resolved inline before architecture:

- **OQ-1** (SubagentStart observation tagging): resolved by ADR-001 (`source` field).
- **OQ-2** (CompactPayload format): resolved by ADR-004 (flat indexed table, section headers removed).
- **OQ-3** (Briefing query when `task` absent): resolved by three-step priority in FR-11.
- **OQ-4** (`BriefingService` removal timing): resolved — deleted in this feature.

**SR-01 (OPEN):** SubagentStart stdout injection behavior in Claude Code is unconfirmed at
architecture time. The architecture degrades gracefully (observation + topic_signal still
recorded if stdout is ignored). AC-SR01 must be marked OPEN or CONFIRMED before Gate 3c.
This is the only unresolved risk entering Session 2.

All other scope risks (SR-02 through SR-09) are fully resolved:
- SR-02: `IndexBriefingService` unconditionally compiled; CompactPayload tests flag-independent.
- SR-03: `EffectivenessStateHandle` is required constructor parameter — compile error if missed.
- SR-04: 11 `format_compaction_payload` test invariants enumerated in ADR-004 and AC-16–AC-21.
- SR-05: `UNIMATRIX_BRIEFING_K` deprecated; `parse_semantic_k()` deleted.
- SR-06: `IndexEntry` typed struct + `format_index_table()` + `SNIPPET_CHARS` constant — WA-5 compile-time contract.
- SR-07: `#[serde(default)]` on `source` — backward compat; struct-literal breakages are compile errors.
- SR-08: Step-3 fallback to `topic` param always available; empty result is `Ok(vec![])`.
- SR-09: `max_tokens: 1000` cap on all six SM protocol briefing call sites.
