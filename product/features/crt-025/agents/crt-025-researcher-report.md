# crt-025 Researcher Report

## Agent ID
crt-025-researcher

## Output
SCOPE.md written to `product/features/crt-025/SCOPE.md`.

## Key Findings

### What `keywords` currently does
`keywords` is a `Vec<String>` field on `CycleParams` (max 5, each max 64 chars). The hook path serializes it to a JSON array string and writes it to `sessions.keywords` via a fire-and-forget targeted `UPDATE`. It is **never read** by any downstream consumer — not by `context_cycle_review`, not by search, briefing, or any other MCP tool. It is inert stored data. This makes its removal from the interface a zero-behavioral-impact change.

### Current schema version
`CURRENT_SCHEMA_VERSION = 14` (set by col-023 migration, `domain_metrics_json` on `observation_metrics`). This feature targets version 15.

### `FEATURE_ENTRIES` current schema
Two columns only: `feature_id TEXT, entry_id INTEGER`. No `phase` column. Written via two paths: `record_feature_entries` (direct write pool) and `AnalyticsWrite::FeatureEntry` (analytics drain). Phase tagging requires adding `phase TEXT NULL` and threading the value through both write paths from `SessionState.current_phase`.

### `SessionState` current shape
No `current_phase` field. Fields relevant to this feature: `feature: Option<String>` (set by cycle_start), `last_activity_at`. Adding `current_phase: Option<String>` follows the existing extension pattern.

### Hook path architecture
- `hook.rs` intercepts `context_cycle` PreToolUse, validates via `validate_cycle_params`, emits `cycle_start` or `cycle_stop` RecordEvent.
- `uds/listener.rs` routes `cycle_start` to `handle_cycle_start` (force-set attribution + keywords persist). `cycle_stop` falls through to generic observation path.
- New `phase-end` events need a third branch: INSERT to CYCLE_EVENTS + update `SessionState.current_phase`.

### col-023 dependency status
col-023 (W1-5, Observation Pipeline Generalization) is **complete and merged** (commit `bf3d53e`, PR #332). No blocking dependency.

### `context_cycle_review` uses no keywords data
Confirmed: the retrospective tool reads observations, metrics, sessions, entries — but never `sessions.keywords`. Phase narrative enrichment is purely additive.

### Outcome category retirement
The `outcome` category is currently in `CategoryAllowlist`. Retiring it means removing it from the allowlist. No existing entry deletion is needed; only new ingest is blocked.

## Scope Boundaries Rationale

The scope is tightly bounded to the five mechanical changes the product vision specifies: CycleParams/validation, CYCLE_EVENTS table, SessionState.current_phase, FEATURE_ENTRIES.phase tagging, context_cycle_review phase narrative. WA-2 (category histogram boosting) is explicitly excluded — it depends on this feature and is a separate delivery.

The `sessions.keywords` column is left in place (not dropped). Dropping would require a more invasive migration that recreates the sessions table (SQLite does not support DROP COLUMN in older versions). The column becomes dormant; no functional impact.

## Open Questions for Human

1. Does `start` with `next_phase: "scope"` immediately set `current_phase = "scope"`, or does phase only get set on the first `phase-end`? Vision implies yes — `next_phase` on start triggers immediate `current_phase` assignment.
2. Should existing `outcome`-category entries be left as-is (historical) or migrated to another category?
3. Is `seq` in CYCLE_EVENTS scoped per `cycle_id` globally (across all sessions for that feature), or per `(cycle_id, session_id)`?
4. When no CYCLE_EVENTS exist, should `context_cycle_review` explicitly note "phase signal not available (pre-WA-1)" or silently omit the section?
5. For `phase-end` with no prior `start` in CYCLE_EVENTS: insert the row anyway (audit completeness) or skip?

## Knowledge Stewardship
- Queried: `/uni-query-patterns` for context_cycle, schema migration, FEATURE_ENTRIES — found ADR-001 col-022, ADR-003 col-022 (JSON keywords column), established migration procedures (#374, #836), pattern entries (#370, #681).
- Stored: entry #2987 "context_cycle keywords field is inert — stored but never consumed" via `/uni-store-pattern` — this is a non-obvious finding that would cause future researchers to waste time tracing keywords through the codebase.
