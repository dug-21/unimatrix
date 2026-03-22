# WA-1: Phase Signal + FEATURE_ENTRIES Tagging

## Problem Statement

The engine records which knowledge entries are produced during a feature cycle, but has no explicit, observable record of *where in the workflow* each entry was produced. The session knows which feature it is working on (via `context_cycle` start/stop), but not which phase — scope, design, implementation, testing — is currently active.

This has two downstream consequences:

1. **W3-1 training data is incomplete.** `FEATURE_ENTRIES` records `(feature_id, entry_id)` pairs, but has no `phase` column. The GNN cannot learn category→phase→usefulness correlations from historical data because that data was never collected.

2. **`context_cycle_review` phase narrative is absent.** The retrospective tool reconstructs behavior from observation telemetry but has no explicit sequence of phases, outcomes, or rework events — only implicit edit-pattern signals. There is no record of "scope ran once, design ran once, implementation ran twice (rework), testing ran once."

The current `context_cycle` interface (col-022) supports only `start` and `stop` event types and a `keywords` field. Keywords are stored as a JSON array string in `sessions.keywords` and are never read by any downstream consumer — they are inert data.

## Goals

1. Replace the `keywords` field on `context_cycle` with three structured event types: `start`, `phase-end`, `stop`.
2. Add `phase`, `outcome`, and `next_phase` parameters to `CycleParams`; update `validate_cycle_params` and the hook path.
3. Create a `CYCLE_EVENTS` append-only table that records one row per lifecycle event with `(cycle_id, seq, event_type, phase, outcome, next_phase, timestamp)`.
4. Add `current_phase: Option<String>` to `SessionState`; update it in the UDS listener on `phase-end` events.
5. Add a `phase TEXT NULL` column to `feature_entries`; populate it from `SessionState.current_phase` when `context_store` records a feature entry.
6. Enrich `context_cycle_review` to render an explicit phase narrative — phase sequence, rework detection, per-phase category distribution — from `CYCLE_EVENTS` and `FEATURE_ENTRIES.phase`; plus cross-cycle comparison: compare this feature's per-phase category distribution against the mean of all prior features that have phase-tagged data.
7. Retire the `outcome` category from ENTRIES: outcomes belong to `CYCLE_EVENTS`, not the knowledge base.
8. Advance schema version from 14 to 15.

## Non-Goals

- **No changes to `context_store` wire protocol.** Phase tagging is automatic from in-memory `SessionState.current_phase` — callers supply no new fields.
- **No changes to WA-2 (session context enrichment).** WA-2 adds category histogram boosting; that is a separate feature dependent on this one.
- **No semantic interpretation of phase strings.** The engine stores and surfaces them as opaque labels. Consistency is the protocol's responsibility, not the engine's.
- **No backfill of existing `feature_entries` rows.** Pre-existing rows get `phase = NULL`. This is correct: old data predates the signal.
- **No backfill of existing `feature_entries` rows.** Pre-existing rows get `phase = NULL`. This is correct: old data predates the signal.
- **No removal of `sessions.keywords` column.** The column exists and removing it would require a more invasive migration. Leave it in place; stop populating it on new events.
- **No changes to the hook binary's wire protocol** (`ImplantEvent` is already generic; new fields pass through the existing payload map).
- **No changes to `context_cycle_review` behavioral telemetry pipeline.** Observation metrics, hotspot detection, and baseline comparison are untouched. Behavioral corroboration (cross-referencing edit-pattern rework with explicit phase rework) is excluded — `context_cycle_review` already derives rework signals from observation metrics; the explicit phase rework signal visible in CYCLE_EVENTS is sufficient as an independent narrative. No corroboration layer is added.

## Background Research

### What `keywords` Currently Does

`keywords` is a `Vec<String>` on `CycleParams` (max 5, each max 64 chars). The hook path extracts it from the `context_cycle` `tool_input`, validates it via `validate_cycle_params`, serializes it to a JSON array string, and writes it to `sessions.keywords` as a fire-and-forget `UPDATE`. It is never read by any downstream consumer — not by `context_cycle_review`, not by search, not by briefing. It is inert stored data.

This confirms the feature can remove `keywords` from the interface without breaking any observable behavior.

### Current `CycleParams` (in `mcp/tools.rs`)

```rust
pub struct CycleParams {
    pub r#type: String,          // "start" or "stop" only
    pub topic: String,
    pub keywords: Option<Vec<String>>,
    pub agent_id: Option<String>,
    pub format: Option<String>,
}
```

### Current `validate_cycle_params` (in `infra/validation.rs`)

Accepts `type_str: &str`, `topic: &str`, `keywords: Option<&[String]>`. Returns `ValidatedCycleParams { cycle_type, topic, keywords }`. Recognizes only `"start"` and `"stop"` as valid type values. This function is shared between the MCP tool handler and the hook path (ADR-004).

### Current Hook Path for `context_cycle`

`hook.rs` intercepts `context_cycle` `PreToolUse` events, extracts `type`, `topic`, and `keywords` from `tool_input`, validates via `validate_cycle_params`, and emits a `HookRequest::RecordEvent` with `event_type` = either `"cycle_start"` or `"cycle_stop"`. The payload carries `feature_cycle` and (if non-empty) `keywords`.

In `uds/listener.rs`, `cycle_start` events are caught before the generic observation path and routed to `handle_cycle_start`, which force-sets `SessionState.feature` and persists keywords to `sessions.keywords`. `cycle_stop` events fall through to the generic observation path with no special handling.

### Current `SessionState` (in `infra/session.rs`)

Key fields: `session_id`, `role`, `feature: Option<String>`, `injection_history`, `coaccess_seen`, `compaction_count`, `topic_signals`. No `current_phase` field exists.

### Current `FEATURE_ENTRIES` Schema (in `store/db.rs`)

```sql
CREATE TABLE IF NOT EXISTS feature_entries (
    feature_id TEXT NOT NULL,
    entry_id   INTEGER NOT NULL,
    PRIMARY KEY (feature_id, entry_id)
)
```

No `phase` column. Writes go through two paths: `record_feature_entries` (direct write pool, used by usage recording) and `AnalyticsWrite::FeatureEntry` (analytics drain). Neither has any concept of phase.

### Current Schema Version

`CURRENT_SCHEMA_VERSION = 14` (set by col-023, `domain_metrics_json` column on `observation_metrics`). This feature will require version 15.

### Migration Pattern

The established pattern (observed across v7→v8, v9→v10, v11→v12, v13→v14) is:
- New tables: `CREATE TABLE IF NOT EXISTS` inside the main migration transaction.
- New columns: pre-check with `pragma_table_info` then `ALTER TABLE ADD COLUMN` (no `IF NOT EXISTS` in SQLite).
- Version bump: `INSERT OR REPLACE INTO counters ... 'schema_version'` at the end of `run_main_migrations`.
- Fresh DB path: `create_tables_if_needed` in `db.rs` must also be updated to include the new table/column from the start.

### col-023 Dependency

col-023 (W1-5, Observation Pipeline Generalization) introduced `domain_metrics_json` on `observation_metrics` and generalized the observation event type system. It is **already merged** (commit `bf3d53e`, PR #332). This feature has no blocking dependency on col-023.

### `context_cycle_review` Data Sources

The retrospective tool loads observations via `SqlObservationSource`, runs detection rules, and produces `RetrospectiveReport`. It does not read `sessions.keywords` at any point. Phase narrative enrichment will require a new query against `CYCLE_EVENTS` keyed by `feature_cycle` topic, and a new aggregate query against `FEATURE_ENTRIES` grouped by phase.

### Outcome Category Retirement

The product vision specifies that the `outcome` category is retired: "Outcomes belong to CYCLE_EVENTS (workflow layer), not the knowledge base." In practice, `context_cycle_review` auto-persists lesson-learned entries (not outcome entries), so this primarily means removing `outcome` from the `CategoryAllowlist` and adding a migration comment. No existing data deletion is required — existing outcome-category entries can remain; only new ingest of `outcome` category is blocked.

## Proposed Approach

**Phase 1 — Wire protocol and validation layer:**
- Add `phase`, `outcome`, `next_phase` fields to `CycleParams` (all optional `String`).
- Remove `keywords` from `CycleParams` (breaking change to the MCP schema, but no caller currently uses it).
- Extend `validate_cycle_params` to accept and validate `phase-end` as a third valid `type`, validate `phase` (contiguous block, no spaces, lowercase — see constraints), and return a `ValidatedCycleParams` with the new fields.
- Update `CYCLE_START_EVENT` / `CYCLE_STOP_EVENT` constants; add `CYCLE_PHASE_END_EVENT = "cycle_phase_end"`.

**Phase 2 — Schema migration (v14 → v15):**
- New table: `CYCLE_EVENTS (id INTEGER PRIMARY KEY AUTOINCREMENT, cycle_id TEXT NOT NULL, seq INTEGER NOT NULL, event_type TEXT NOT NULL, phase TEXT, outcome TEXT, next_phase TEXT, timestamp INTEGER NOT NULL)` with index on `cycle_id`.
- Alter `feature_entries`: add `phase TEXT` nullable column.
- Remove `outcome` from `CategoryAllowlist`.
- Update `create_tables_if_needed` in `db.rs` and `run_main_migrations` in `migration.rs`.

**Phase 3 — SessionState and UDS listener:**
- Add `current_phase: Option<String>` to `SessionState`.
- Update `register_session` to initialize `current_phase = None`.
- Handle `cycle_phase_end` in UDS listener: INSERT to `CYCLE_EVENTS`, update `SessionState.current_phase` to `next_phase` if present.
- Handle `cycle_start`: INSERT to `CYCLE_EVENTS` (seq=0), `current_phase = next_phase` from start event if provided.
- Handle `cycle_stop`: INSERT to `CYCLE_EVENTS`, clear `current_phase = None`.

**Phase 4 — `context_store` phase tagging:**
- When `record_feature_entries` is called, pass the active `current_phase` from `SessionState` alongside the entry IDs.
- Write `phase` to the `feature_entries` row (new column). Phase is nullable; entries written without an active phase get `NULL`.

**Phase 5 — `context_cycle_review` enrichment:**
- New query: load `CYCLE_EVENTS` ordered by `seq` for the given `cycle_id`.
- New query: aggregate `FEATURE_ENTRIES` by `phase` and entry category.
- Render phase narrative in the existing retrospective report: phase sequence, rework (phase name appearing more than once), per-phase category counts.

## Acceptance Criteria

- AC-01: `CycleParams` has `phase`, `outcome`, `next_phase` fields and no `keywords` field. Deserialization of old calls that include `keywords` silently ignores the field (backward-compatible via `#[serde(deny_unknown_fields)]` NOT set).
- AC-02: `validate_cycle_params` accepts `"phase-end"` as a valid `type`. `"start"` and `"stop"` remain valid. All other values are rejected with a descriptive error.
- AC-03: `phase` on any event type is rejected if it contains a space character or is longer than 64 characters. Normalization to lowercase is applied at ingest.
- AC-04: `phase` on `"start"` and `"phase-end"` events: if provided, stored in `CYCLE_EVENTS.phase`. If absent on `"start"`, stored as NULL.
- AC-05: `next_phase` on `"phase-end"` events updates `SessionState.current_phase` in-memory. If `next_phase` is absent, `current_phase` is left unchanged.
- AC-06: `"stop"` event clears `SessionState.current_phase` to `None`.
- AC-07: Each `context_cycle` call produces exactly one INSERT to `CYCLE_EVENTS` with a monotonically increasing `seq` value scoped to the `cycle_id`.
- AC-08: `context_store` writes the current `SessionState.current_phase` to the new `feature_entries.phase` column at insert time. Entries stored before any phase transition receive `phase = NULL`.
- AC-09: Schema version advances from 14 to 15. Migration is idempotent (pre-check before `ALTER TABLE ADD COLUMN`, `CREATE TABLE IF NOT EXISTS`).
- AC-10: `create_tables_if_needed` in `db.rs` includes `CYCLE_EVENTS` table and `feature_entries.phase` column for fresh databases.
- AC-11: `context_cycle_review` response includes a phase narrative section when `CYCLE_EVENTS` data exists for the queried feature cycle. Narrative includes: ordered phase list, rework flag (phase seen more than once), per-phase category counts from `FEATURE_ENTRIES`.
- AC-12: `context_cycle_review` response is unchanged (no phase section) when no `CYCLE_EVENTS` data exists (backward-compatible for pre-WA-1 features).
- AC-13: `outcome` category is removed from `CategoryAllowlist`. Attempting to store an entry with category `"outcome"` via `context_store` returns a category-rejected error.
- AC-14: Hook path handles `phase-end` events: emits `cycle_phase_end` event type, validates params, logs warnings and falls through to generic observation path on validation failure (FR-03.7 — hook never hard-fails).
- AC-15: All new database operations are tested: CYCLE_EVENTS insert, feature_entries phase column write, schema migration idempotency.

## Constraints

### Phase String Format (GNN Label Requirement)

The `phase` field must be a single contiguous block — no spaces, no embedded whitespace. This is a hard requirement from the W3-1 GNN training pipeline: phase strings are used as discrete class labels, and spaces would fragment a label or require post-processing that introduces ambiguity. Valid examples: `scope`, `design`, `implementation`, `testing`, `gate-review`. Invalid: `"scope review"`, `"gate review"`.

Enforcement: `validate_cycle_params` rejects any `phase` value containing a space character. Normalization (lowercase, trim) is applied before the space check so that `"Scope"` becomes `"scope"`.

### Backward Compatibility

- MCP schema: `keywords` removal is a wire-breaking change on the JSON schema (`CycleParams`). However, because `keywords` was never documented as required and callers that pass unknown fields have them silently ignored (no `#[serde(deny_unknown_fields)]`), existing callers passing `keywords` will simply have it ignored. Callers not using `keywords` (the majority) are unaffected.
- Schema: `feature_entries.phase` is `TEXT NULL` — existing rows read as `NULL` without any migration of data.
- `sessions.keywords` column: left in place, no longer populated. Existing data remains queryable.

### No Wire Protocol Changes

`ImplantEvent` in `unimatrix-engine` is already generic (`payload: serde_json::Value`). New fields (`phase`, `outcome`, `next_phase`) travel as payload keys. No struct changes in the engine crate.

### Shared Validation Function (ADR-004)

`validate_cycle_params` must remain a shared pure function callable from both the MCP tool handler and the hook path. The hook path cannot use `ServerError`; the function signature must return `Result<ValidatedCycleParams, String>`.

### Schema Migration Idempotency

SQLite does not support `ALTER TABLE ADD COLUMN IF NOT EXISTS`. The established pattern is a `pragma_table_info` pre-check. This feature must follow that pattern for the `feature_entries.phase` column and any new columns.

### Sequence Numbering for CYCLE_EVENTS

`seq` must be monotonically increasing per `cycle_id`. The simplest implementation: `SELECT COALESCE(MAX(seq), -1) + 1 FROM cycle_events WHERE cycle_id = ?`. This is safe inside the UDS listener's fire-and-forget spawn because the UDS listener serializes events per session.

### Hook Latency Budget

The hook path has a 40ms transport timeout. CYCLE_EVENTS INSERT is a fire-and-forget write that must not block the hook response path beyond the transport send.

## Decisions

1. **`phase` on `start` event**: `next_phase` on `start` immediately sets `current_phase`. `context_cycle(type: "start", topic: "crt-025", next_phase: "scope")` results in `SessionState.current_phase = Some("scope")` before any `phase-end` arrives.

2. **`seq` numbering scope**: Per `cycle_id` (feature/topic scope). All sessions for the same feature share one monotonic sequence counter.

3. **`phase-end` with no prior `start`**: Insert anyway. The append log is an audit trail; the session may have started before the daemon was running.

4. **`context_cycle_review` — no CYCLE_EVENTS**: Silently omit the phase narrative section. Present the data available; no "not available" placeholder.

5. **`outcome` category retirement**: Blocked at ingest (remove from `CategoryAllowlist`) in this feature. Existing entries and configuration cleanup tracked in follow-up GH issue (see Tracking below).

## Open Questions

None — all questions resolved.

## Tracking

GH #330. Follow-up (outcome retirement cleanup): GH #338.

## Dependencies

- **WA-0 (`crt-024`)**: Complete. The six-term ranking fusion is in place. This feature does not touch the ranking pipeline.
- **col-023 (W1-5)**: Complete (merged, PR #332). This feature touches `feature_entries` and `CYCLE_EVENTS` — both are independent of col-023's changes to `observation_metrics` and domain pack registry.
- **WA-2**: Depends on this feature. `WA-2` consumes `SessionState.current_phase` for phase-conditioned category affinity boosting. WA-1 must ship first.
- **W3-1**: Depends on this feature. The GNN training pipeline consumes `FEATURE_ENTRIES.phase` as supervised labels. WA-1 must accumulate data before W3-1 training begins.
