# col-022: Explicit Feature Cycle Lifecycle -- Implementation Brief

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/col-022/SCOPE.md |
| Scope Risk Assessment | product/features/col-022/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/col-022/architecture/ARCHITECTURE.md |
| Specification | product/features/col-022/specification/SPECIFICATION.md |
| Risk-Test Strategy | product/features/col-022/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/col-022/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| shared-validation | product/features/col-022/pseudocode/shared-validation.md | product/features/col-022/test-plan/shared-validation.md |
| schema-migration | product/features/col-022/pseudocode/schema-migration.md | product/features/col-022/test-plan/schema-migration.md |
| mcp-tool | product/features/col-022/pseudocode/mcp-tool.md | product/features/col-022/test-plan/mcp-tool.md |
| hook-handler | product/features/col-022/pseudocode/hook-handler.md | product/features/col-022/test-plan/hook-handler.md |
| uds-listener | product/features/col-022/pseudocode/uds-listener.md | product/features/col-022/test-plan/uds-listener.md |

### Cross-Cutting Artifacts

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | product/features/col-022/pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | product/features/col-022/test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Provide an explicit, authoritative `context_cycle` MCP tool that SM/coordinator agents call to declare which feature cycle a session belongs to, replacing heuristic-only attribution that fails for worktree-isolated subagents, single-spawn workflows, and mixed-signal sessions. The explicit declaration uses force-set semantics (ADR-002) to override heuristic attribution, while preserving backward compatibility when no explicit declaration is made. Keywords are stored alongside the session for future context injection use.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Wire protocol approach | Reuse `RecordEvent` with `event_type: "cycle_start"` / `"cycle_stop"` -- no new HookRequest variants | SCOPE Resolved Decision 1, SR-03 | architecture/ADR-001-reuse-record-event-wire-protocol.md |
| Attribution semantic for explicit cycle_start | Force-set (`set_feature_force`) overrides heuristic attribution; explicit > eager > majority priority ordering | ADR-002, Vision variance approval | architecture/ADR-002-force-set-for-explicit-attribution.md |
| Keywords storage | JSON TEXT column on sessions table; nullable, max 5 strings of 64 chars each | SCOPE Open Question 2, SR-05/SR-09 | architecture/ADR-003-json-column-for-keywords.md |
| Shared validation | Single `validate_cycle_params()` function in `infra/validation.rs` used by both MCP tool and hook handler | SR-07, NFR-05 | architecture/ADR-004-shared-validation-function.md |
| Schema migration | v11 -> v12: `ALTER TABLE sessions ADD COLUMN keywords TEXT`; no data backfill | SR-09 | architecture/ADR-005-schema-v12-keywords-migration.md |

## Files to Create/Modify

### New Files

| Path | Summary |
|------|---------|
| (none) | All changes are modifications to existing files in existing crates |

### Modified Files

| Path | Summary |
|------|---------|
| `crates/unimatrix-server/src/mcp/tools.rs` | Add `context_cycle` MCP tool handler with `CycleParams` struct, validation, and acknowledgment response |
| `crates/unimatrix-server/src/infra/validation.rs` | Add `validate_cycle_params()`, `ValidatedCycleParams`, `CycleType` enum |
| `crates/unimatrix-server/src/infra/session.rs` | Add `SessionRegistry::set_feature_force()` returning `SetFeatureResult` enum |
| `crates/unimatrix-server/src/uds/hook.rs` | Extend `build_request()` to detect `context_cycle` in PreToolUse, extract params, build cycle RecordEvent |
| `crates/unimatrix-server/src/uds/listener.rs` | Extend `dispatch_request()` with `cycle_start` match arm (force-set + keywords persistence); add `update_session_keywords()` |
| `crates/unimatrix-store/src/sessions.rs` | Add `keywords: Option<String>` to `SessionRecord`; update `SESSION_COLUMNS` and `session_from_row` |
| `crates/unimatrix-store/src/migration.rs` | Add v11->v12 migration; bump `CURRENT_SCHEMA_VERSION` to 12 |

## Data Structures

### New Types

```rust
// unimatrix-server/src/infra/validation.rs
pub enum CycleType {
    Start,
    Stop,
}

pub struct ValidatedCycleParams {
    pub cycle_type: CycleType,
    pub topic: String,
    pub keywords: Vec<String>,
}

// unimatrix-server/src/infra/session.rs
pub enum SetFeatureResult {
    Set,                              // feature was NULL, now set
    AlreadyMatches,                   // feature already set to same value
    Overridden { previous: String },  // feature was different, overwritten
}

// unimatrix-server/src/mcp/tools.rs
#[derive(Deserialize, JsonSchema)]
pub struct CycleParams {
    pub r#type: String,              // "start" or "stop"
    pub topic: String,               // feature cycle identifier
    pub keywords: Option<Vec<String>>, // up to 5 semantic keywords
}
```

### Modified Types

```rust
// unimatrix-store/src/sessions.rs
pub struct SessionRecord {
    // ... existing fields ...
    pub keywords: Option<String>,  // NEW: JSON array string, e.g. '["keyword1","keyword2"]'
}
```

### Schema Change

```sql
-- v11 -> v12
ALTER TABLE sessions ADD COLUMN keywords TEXT;
```

## Function Signatures

### New Functions

```rust
// Shared validation (C5)
pub fn validate_cycle_params(
    type_str: &str,
    topic: &str,
    keywords: Option<&[String]>,
) -> Result<ValidatedCycleParams, String>;

// Force-set attribution (C3/ADR-002)
impl SessionRegistry {
    pub fn set_feature_force(&self, session_id: &str, feature: &str) -> SetFeatureResult;
}

// Keywords persistence (C4)
pub fn update_session_keywords(
    store: &Store,
    session_id: &str,
    keywords_json: &str,
) -> Result<()>;

// MCP tool handler (C1)
impl UnimatrixServer {
    #[tool]
    async fn context_cycle(&self, params: CycleParams) -> Result<CallToolResult>;
}
```

### Modified Functions

```rust
// Hook handler -- new match arm in build_request()
fn build_request(event_type: &str, input: &HookInput) -> Option<HookRequest>;

// UDS listener -- new match arm in dispatch_request()
async fn dispatch_request(&self, request: HookRequest) -> HookResponse;

// Session read -- updated column indexing
fn session_from_row(row: &Row) -> SessionRecord;
```

## Constraints

1. **Hook latency budget (50ms)**: PreToolUse interception must add <5ms marginal cost. Fire-and-forget UDS write, no response wait.
2. **First-writer-wins preserved for heuristics**: `set_feature_if_absent` unchanged for SessionStart, eager voting, majority vote. Only `cycle_start` uses `set_feature_force`.
3. **No MCP server session state**: MCP server has no session_id. Attribution happens on hook/UDS path. MCP tool is validation + acknowledgment only.
4. **RecordEvent reuse**: No new `HookRequest` variants. Cycle events use existing `RecordEvent` with special `event_type` values.
5. **Shared validation**: Both MCP tool and hook handler call `validate_cycle_params()`. No independent validation.
6. **Fire-and-forget persistence**: `update_session_feature_cycle` and `update_session_keywords` use `spawn_blocking` fire-and-forget.
7. **Hook never fails**: Validation failure in hook falls through to generic RecordEvent path. No panics, exit code 0.
8. **`SessionWrite` capability required**: context_cycle requires SessionWrite (already granted to hook connections).

## Dependencies

### Crates (existing, no new dependencies)

| Crate | Role |
|-------|------|
| `unimatrix-server` | MCP tool, hook handler, UDS listener extensions |
| `unimatrix-engine` | `ImplantEvent` struct, `SessionRegistry` |
| `unimatrix-store` | `SessionRecord`, schema migration |
| `unimatrix-observe` | `is_valid_feature_id()` (needs pub export or duplication) |
| `rmcp` 0.16.0 | MCP tool registration (existing) |
| `serde_json` | ImplantEvent payload serialization (existing) |

### Cross-Crate Decision

`is_valid_feature_id()` is currently `pub(crate)` in `unimatrix-observe`. Either re-export as `pub` or duplicate in `unimatrix-server/src/infra/validation.rs`. Implementer decides based on existing dependency graph (`unimatrix-server` may already depend on `unimatrix-observe`).

## NOT in Scope

1. **Protocol/agent file updates** -- follow-up GH issue for SM agent definitions and protocol files to call `context_cycle(start)`.
2. **Keyword-driven context injection** -- follow-up GH issue. Keywords stored only; injection behavior deferred.
3. **Multi-feature sessions** -- one session = one feature constraint (#1067) maintained.
4. **SubagentStart signal weighting** -- separate enhancement to heuristic pipeline.
5. **Cross-session feature lifecycle** -- `cycle_stop` marks in-session boundary, not feature completion.
6. **MCP server session state** -- server remains session-unaware.
7. **Retrospective pipeline changes** -- existing pipeline handles sessions with explicit attribution.

## Alignment Status

**Overall: PASS with two resolved variances.**

### Variance 1: Force-Set Semantics (RESOLVED -- accepted by human)

The architecture (ADR-002) introduces `set_feature_force()` which overrides heuristic attribution. This contradicted SCOPE AC-03's original first-writer-wins requirement. **Human approved force-set**: explicit `context_cycle(start)` is authoritative and overrides heuristic attribution. SCOPE AC-03 updated to reflect this. Priority ordering: explicit > eager > majority.

### Variance 2: `was_set` Response Field (RESOLVED -- accepted by human)

Specification FR-19 defined a `was_set` boolean, but the MCP server has no session identity to determine attribution outcome. **Human approved replacing `was_set` with simple "noted" acknowledgment.** SCOPE AC-05 updated. The tool response is acknowledgment-only; attribution confirmation requires `context_retrospective`.

### Vision Alignment

Three vision pillars supported:
- Hook-driven delivery: attribution routed through PreToolUse hook
- Invisible delivery: reliable attribution feeds the self-learning pipeline
- Auditable knowledge lifecycle: feature cycle attribution strengthens audit trail

No vision concerns. No milestone concerns.

## Tracking

GitHub Issue: #214
