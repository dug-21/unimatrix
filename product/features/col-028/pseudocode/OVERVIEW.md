# col-028: Unified Phase Signal Capture — Pseudocode Overview

## Problem

Two related gaps share the same root cause (phase not forwarded from SessionState to
read-side tool events):

- Gap 1 (in-memory): `UsageContext.current_phase` is always `None` for the four
  read-side tools. `context_get` uses `access_weight: 1` (undercounts deliberate reads).
  `context_briefing` uses `access_weight: 1` (overcounts unread offer events and burns the
  dedup slot, blocking subsequent `context_get` increments).
- Gap 2 (persistence): `query_log` has no `phase` column — phase is never written to
  disk even if captured in memory.

## Components Involved

| Component | File | Pseudocode |
|-----------|------|-----------|
| 1. SessionState + SessionRegistry | infra/session.rs | session-state.md |
| 2. Phase Helper + Four Read-Side Call Sites + query_log Write Site | mcp/tools.rs | tools-read-side.md |
| 3. D-01 Guard | services/usage.rs | usage-d01-guard.md |
| 4. Schema Migration v16→v17 + analytics.rs + query_log.rs | unimatrix-store | migration-v16-v17.md |

Components 1 and 3 have no direct inter-dependency at runtime. Both are prerequisites for
Component 2 to compile correctly. Component 4 (store) must land before or simultaneously
with Component 2's `QueryLogRecord::new` signature change.

## Data Flow

```
MCP Handler (tools.rs)
  │
  │  [C-01] FIRST statement, before any .await:
  ├─ let phase = current_phase_for_session(&self.session_registry, session_id_opt)
  │     └─ SessionRegistry.get_state(sid) → Option<SessionState>
  │          └─ .current_phase.clone()         [returns owned String; no lock held after]
  │
  │  [C-04] Same `phase` variable used for both consumers in context_search:
  ├─ UsageContext { current_phase: phase.clone(), access_weight: N, ... }
  │     └─ UsageService.record_access(...)      [fire-and-forget]
  │          ├─ AccessSource::McpTool → record_mcp_usage
  │          │     (no D-01 guard needed here — guard is only in Briefing path)
  │          └─ AccessSource::Briefing → record_briefing_usage
  │                [D-01 guard] if access_weight == 0 { return }  ← Component 3
  │
  ├─ (context_get only)
  │    SessionRegistry.record_confirmed_entry(session_id, entry_id)
  │
  ├─ (context_lookup with target_ids.len() == 1 only)
  │    SessionRegistry.record_confirmed_entry(session_id, entry_id)
  │
  └─ (context_search only) QueryLogRecord::new(..., phase)
        └─ store.insert_query_log(&record)     [fire-and-forget spawn_blocking]
             └─ enqueue_analytics(AnalyticsWrite::QueryLog { ..., phase })
                  └─ drain task: INSERT INTO query_log VALUES (?1..?9)
```

## Shared Types Introduced or Modified

### SessionState (infra/session.rs) — new field

```
pub confirmed_entries: HashSet<u64>
```

Exact doc comment required (AC-24):
- Populated by `context_get` (always) and `context_lookup` (single-ID, request-side
  cardinality only).
- Not populated by briefing, search, write, or mutation tools.
- In-memory only; reset on register_session; never persisted.
- First consumer: Thompson Sampling (future feature).

Initialised to `HashSet::new()` in `register_session`.

### UsageContext.current_phase doc comment (services/usage.rs) — updated

The field already exists. The doc comment currently says "None for all non-store
operations". It must be updated to list read-side tools as populating the field and
restrict "None" to mutation tools only (ADR-006).

### QueryLogRecord (unimatrix-store/src/query_log.rs) — new field + constructor param

```
pub phase: Option<String>   // col-028: workflow phase at query time; None for UDS rows
```

Constructor signature gains `phase: Option<String>` as final parameter.

### AnalyticsWrite::QueryLog (unimatrix-store/src/analytics.rs) — new variant field

```
phase: Option<String>   // NEW — col-028
```

INSERT adds `phase` as column 9 (`?9`). Both SELECT statements add `phase` as tenth
column. `row_to_query_log` reads index 9. All four sites are an atomic change unit (C-09).

## Sequencing Constraints

1. Component 4 (store: `QueryLogRecord::new` signature) must compile before Component 2
   calls it with the new `phase` argument. Deliver in the same commit or store first.
2. Component 1 (`confirmed_entries` field on `SessionState`) must be present in all
   `SessionState` struct literals before Component 2 calls `record_confirmed_entry`.
3. Component 3 (D-01 guard in `record_briefing_usage`) is self-contained. It can land
   in any order relative to 1, 2, 4.
4. Compile-fix sites (uds/listener.rs, eval/scenarios/tests.rs, mcp/knowledge_reuse.rs)
   must be updated atomically with Component 4's constructor change.

## Access Weight Summary (post-feature state)

| Tool | access_weight | Changed? |
|------|--------------|----------|
| context_search | 1 | No |
| context_lookup | 2 | No |
| context_get | 2 | YES (was 1) |
| context_briefing | 0 | YES (was 1) |

## Key Constraints Reference

| ID | One-line summary |
|----|-----------------|
| C-01 | Phase snapshot is first statement in handler, before any .await |
| C-04 | Single get_state call per handler; one variable shared by both consumers |
| C-05 | phase added as ?9 — no existing bind index changes |
| C-07 | No consumer of confirmed_entries in this feature |
| C-08 | uds/listener.rs:1324 compile-fix only — pass None |
| C-09 | analytics INSERT + both SELECTs + row_to_query_log = atomic change unit |
