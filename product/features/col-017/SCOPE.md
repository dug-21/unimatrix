# col-017: Hook-Side Topic Attribution

## Problem Statement

The observation pipeline captures 3,200+ events/day across all sessions, but sessions have **no topic attribution**. The `sessions.feature_cycle` column is always NULL because:

1. Claude Code doesn't send `feature_cycle` in hook input (SessionRegister's `feature` field is never populated)
2. Content-based attribution in `attribution.rs` works but only runs at retrospective time — and never persists its results
3. The retrospective pipeline queries `sessions WHERE feature_cycle IS NOT NULL`, which returns zero rows

**Impact**: `context_retrospective` returns empty for 100% of features. The entire feedback loop — hotspot detection, metrics, baseline comparison, knowledge extraction — is broken.

## Solution

Push topic extraction to the edge (hook process) and accumulate signals server-side. Resolve dominant topic on SessionClose and persist it.

Three layers:
1. **Hook-side extraction**: Extract topic signals from tool inputs per-event (file paths, prompt text, git branch names) using existing `extract_from_path()` and `extract_feature_id_pattern()` functions from `attribution.rs`
2. **Server-side accumulation**: Tally topic signals per session in-memory (`SessionState` or dedicated accumulator)
3. **SessionClose resolution**: Majority-vote topic resolution → UPDATE `sessions.feature_cycle` → retrospective fast path works

## Scope

### In Scope

- **New column**: `observations.topic_signal TEXT` (nullable) — stores per-event topic signal extracted by hook
- **Hook-side extraction**: In `build_request()`, extract topic signal from tool inputs before constructing HookRequest. Attach as field on `RecordEvent` / `RecordEvents`
- **Wire protocol change**: Add `topic_signal: Option<String>` to `ImplantEvent` (serde(default) for backward compat)
- **Server-side accumulation**: On RecordEvent dispatch, if `topic_signal` is present, tally in SessionState
- **SessionClose resolution**: On SessionClose, resolve dominant topic from accumulated signals (majority vote). UPDATE `sessions.feature_cycle`. Fall back to full content-based attribution via `attribute_sessions()` if no signals
- **Persist content-based attribution results**: When retrospective runs content-based attribution, persist results to `sessions.feature_cycle` so subsequent calls hit the fast path
- **Schema migration**: v9 → v10 (shared with col-018/col-019 if they land in same migration)
- **Backfill**: Run attribution on existing unattributed sessions during migration

### Out of Scope

- `topic_deliveries` table (nxs-010, Wave 2)
- `query_log` table (nxs-010, Wave 2)
- UserPromptSubmit dual-route storage (col-018)
- PostToolUse response_size/snippet fix (col-019)
- Multi-session retrospective (col-020, Wave 2)
- Explicit topic registration tool (future — coordinator agents calling a registration MCP tool)

## Architecture

### Data Flow

```
Claude Code Hook Event
        │
        ▼
  build_request()                    ◄── NEW: extract topic signal
  ┌─────┴─────┐
  │           │
  ▼           ▼
RecordEvent  RecordEvents
(+topic_signal)
  │           │
  ▼           ▼
dispatch_request()
  ├── insert observation (+ topic_signal column)
  └── tally topic_signal in SessionState    ◄── NEW
        │
        ▼
  SessionClose
  ├── majority_vote(accumulated_signals) → topic
  ├── UPDATE sessions SET feature_cycle = topic
  └── fallback: load observations → attribute_sessions() → persist
```

### Key Code Paths

| File | Change |
|------|--------|
| `crates/unimatrix-engine/src/wire.rs` | Add `topic_signal: Option<String>` to `ImplantEvent` |
| `crates/unimatrix-server/src/uds/hook.rs` | Extract topic signal in `build_request()` and `generic_record_event()` |
| `crates/unimatrix-server/src/infra/session.rs` | Add `topic_signals: Vec<String>` to `SessionState` |
| `crates/unimatrix-server/src/uds/listener.rs` | Tally signals on RecordEvent; resolve + persist on SessionClose |
| `crates/unimatrix-server/src/uds/listener.rs` | Add `topic_signal` to `ObservationRow`, `insert_observation`, `extract_observation_fields` |
| `crates/unimatrix-store/src/migration.rs` | `ALTER TABLE observations ADD COLUMN topic_signal TEXT` |
| `crates/unimatrix-observe/src/attribution.rs` | Make `extract_from_path`, `extract_feature_id_pattern`, `extract_from_git_checkout` public |
| `crates/unimatrix-store/src/sessions.rs` | Backfill unattributed sessions during migration |

### Extraction Logic

Reuses existing functions from `attribution.rs` (already tested with 20+ unit tests):

1. **File paths**: `extract_from_path()` — scans for `product/features/{id}/` pattern
2. **Feature ID pattern**: `extract_feature_id_pattern()` — word-boundary match for `alpha-digits` (e.g., "col-002")
3. **Git branch**: `extract_from_git_checkout()` — parses `feature/{id}` branch names

Priority order (same as retrospective attribution): file path > feature ID pattern > git branch.

**Hook-side extraction sources per event type**:

| Event | Signal source | Field |
|-------|--------------|-------|
| PreToolUse | `tool_input` (file paths, command text) | `input.extra["tool_input"]` |
| PostToolUse (rework) | `file_path` in rework payload | `payload["file_path"]` |
| PostToolUse (non-rework) | `tool_input` (same as PreToolUse) | `input.extra["tool_input"]` |
| SubagentStart | `prompt_snippet` | `input.extra["prompt_snippet"]` |
| UserPromptSubmit | Prompt text | `input.prompt` |

### Majority Vote Resolution

On SessionClose:
1. Count occurrences of each topic signal accumulated during the session
2. If clear winner (plurality): use it
3. If tie: pick the one seen most recently (last-write-wins among tied)
4. If no signals at all: fall back to content-based attribution (load observations from DB, run `attribute_sessions()`)
5. Persist result: `UPDATE sessions SET feature_cycle = ?`

### Backward Compatibility

- `ImplantEvent.topic_signal` uses `#[serde(default)]` — old hook binaries send no field, server reads `None`
- `observations.topic_signal` is nullable — existing rows unaffected
- `sessions.feature_cycle` is already nullable TEXT — no schema change, just start populating
- Content-based fallback path remains — handles sessions where hook extraction finds no signals
- `load_unattributed_sessions()` fallback remains for legacy retrospective path

## Constraints

1. **Hook process is short-lived**: Each hook invocation is a separate process. Extraction must be cheap (string scanning only, no I/O, no network). The existing attribution functions are suitable.
2. **No platform changes**: Claude Code hook schema is fixed. We can only add logic on our side.
3. **Session identity**: `session_id` is shared across parent + subagents. Topic signals from all subagents contribute to the parent session's topic.
4. **`topic` = `feature_cycle`**: Semantically the same concept. New code uses `topic` naming where possible; `feature_cycle` remains in existing schemas for backward compat.
5. **Migration coordination**: If col-018 and col-019 land in the same release, the schema migration (v9 → v10) should be shared. Each feature adds its own changes to the same migration.
6. **Fire-and-forget writes**: Session update on SessionClose must be fire-and-forget (non-blocking) to avoid adding latency to the hook response path.

## Risks

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| False-positive topic signals from `extract_feature_id_pattern` matching non-feature text | Medium | Low | Majority vote resolves ambiguity; file path extraction has higher priority; `is_valid_feature_id` rejects most noise |
| Multi-feature sessions attributed to wrong topic | Low | Medium | Partition logic already handles multi-feature sessions in retrospective; majority vote picks dominant topic for the session record |
| High cardinality of topic signals in long sessions | Low | Low | Vec<String> in memory; sessions are bounded by duration (4h stale threshold); majority vote is O(n) |
| Wire protocol change breaks old server with new hook (or vice versa) | Low | Medium | `serde(default)` ensures backward compat in both directions |
| Migration backfill takes too long on large databases | Low | Low | Backfill runs once; ~100 sessions typical; content scan is cheap |

## Open Questions (Resolved)

1. **Naming** (RESOLVED): Add `topic_signal: Option<String>` directly to `ImplantEvent`. The purity argument for wrapping in RecordEvent is premature with only one metadata field. Refactor if a second metadata field appears.

2. **Auto-outcome topic** (CLOSED — GH #430): `write_auto_outcome_entry()` has been deleted. It wrote to ENTRIES instead of OUTCOME_INDEX and was dead code with broken intent. SESSIONS holds all session telemetry. This open question is moot.

3. **Backfill strategy** (RESOLVED): Backfill in migration (blocking). Small data volume (~100 sessions), content scan is cheap, runs once. No background task complexity.

4. **col-018 interaction** (RESOLVED): col-018 uses a server-side intercept pattern. The observation write happens in the ContextSearch dispatch arm with the prompt text already in-hand. So for UserPromptSubmit events, the **server** extracts the topic signal from the query text (calling `extract_feature_id_pattern(&query)`) when writing the observation. This means:
   - col-017's hook-side extraction covers RecordEvent paths (tool use, subagent)
   - Server-side extraction covers ContextSearch paths (UserPromptSubmit)
   - No wire protocol coordination needed between col-017 and col-018

## Dependencies

- **Upstream**: None. All extraction functions exist in `attribution.rs`.
- **Parallel**: col-018 (UserPromptSubmit Dual-Route), col-019 (PostToolUse Response Capture) — same wave, schema migration coordination
- **Downstream**: nxs-010 (topic_deliveries table), col-020 (multi-session retrospective) — Wave 2, depends on sessions having feature_cycle populated

## Success Criteria

1. After a session completes, `sessions.feature_cycle` is populated for sessions where tool inputs contain topic-identifying content
2. `context_retrospective` for a known feature returns non-empty results (retrospective fast path works)
3. New `observations.topic_signal` column stores per-event signals
4. Backward compatible: old hook binaries still work with new server; new hook binaries still work with old server
5. Existing attribution tests pass unchanged; new tests cover hook extraction, accumulation, and majority-vote resolution
