# Agent Report: col-024-agent-1-pseudocode

## Summary

Produced five pseudocode files for col-024 in `product/features/col-024/pseudocode/`.
All four affected components are covered. Architecture output was read before writing.
No interface names were invented.

## Files Produced

| File | Component |
|------|-----------|
| `pseudocode/OVERVIEW.md` | Component interaction, data flow, shared types, sequencing |
| `pseudocode/observation-source-trait.md` | `ObservationSource::load_cycle_observations` trait declaration |
| `pseudocode/load-cycle-observations.md` | `SqlObservationSource::load_cycle_observations` + `cycle_ts_to_obs_millis` |
| `pseudocode/enrich-topic-signal.md` | `enrich_topic_signal` helper + four write-site applications |
| `pseudocode/context-cycle-review.md` | `context_cycle_review` three-path observation loading |

## Components Covered

1. `ObservationSource` trait (`unimatrix-observe/src/source.rs`) — new method declaration
2. `SqlObservationSource::load_cycle_observations` + `cycle_ts_to_obs_millis`
   (`unimatrix-server/src/services/observation.rs`)
3. `enrich_topic_signal` + four write sites (`unimatrix-server/src/uds/listener.rs`)
4. `context_cycle_review` lookup order (`unimatrix-server/src/mcp/tools.rs`)

## Open Questions Found

### OQ-1: parameter binding index scheme in Step 3 SQL query

The Step 3 SQL uses both a timestamp range bound and a session_id IN-clause. SQLite's
positional `?N` syntax requires consistent index assignment. The pseudocode provides a
recommended scheme (`?1=min_ms`, `?2=max_ms`, `?3..?N=session_ids`) but flags this as
an implementation detail the agent must verify. The existing `load_feature_observations`
uses indices `?1..?N` for session IDs with no timestamp bound; the new method adds bounds.
Implementation agent must confirm the placeholder construction in the format! string
produces correct `?3`, `?4`, etc. indices when `min_ms` and `max_ms` consume `?1` and `?2`.

### OQ-2: record_topic_signal call in ContextSearch site

Before col-024, `record_topic_signal` at the ContextSearch site fires only when
`extract_topic_signal` returns `Some`. After enrichment, the signal may become `Some`
from the registry. The pseudocode proposes calling `record_topic_signal` on the enriched
signal. This is an improvement in signal fidelity but was not explicitly required by the
spec. The implementation agent should confirm this is intended or revert to the pre-col-024
guard (`if let Some(ref signal) = topic_signal` before enrichment).

### OQ-3: `impl ObservationSource for` search scope

I-01 (integration risk) requires finding all `impl ObservationSource for` blocks outside
`SqlObservationSource`. I found none during source reading but did not perform an
exhaustive workspace search. The implementation agent must run `grep -r "impl ObservationSource for"`
across the workspace before shipping the trait change.

## Source Files Read

- `product/features/col-024/IMPLEMENTATION-BRIEF.md`
- `product/features/col-024/architecture/ARCHITECTURE.md`
- `product/features/col-024/specification/SPECIFICATION.md`
- `product/features/col-024/RISK-TEST-STRATEGY.md`
- `product/features/col-024/architecture/ADR-001` through `ADR-005`
- `crates/unimatrix-observe/src/source.rs` (full)
- `crates/unimatrix-server/src/services/observation.rs` (lines 1-350 + 423-440)
- `crates/unimatrix-server/src/uds/listener.rs` (lines 1-50, 570-630, 660-720, 760-820, 820-880, 2257-2320)
- `crates/unimatrix-server/src/mcp/tools.rs` (lines 1184-1313)
- `crates/unimatrix-server/src/infra/session.rs` (lines 113-210)

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for "ObservationSource trait implementation patterns block_sync"
  -- found #763 (Server-Side Observation Intercept Pattern), #1758 (Extract spawn_blocking body into
  named sync helper), #3367 (topic_signal write-time enrichment pattern col-024)
- Queried: `/uni-context-lookup` for col-024 decisions -- found all 5 ADRs (#3371-#3375)
  confirming all architectural decisions are captured in Unimatrix
- Deviations from established patterns: none. The single-block_sync pattern (#763, #1758)
  is followed exactly. The topic_signal enrichment pattern (#3367) is implemented as
  documented. The named sync helper pattern (#1758) is used for `cycle_ts_to_obs_millis`.

## Knowledge Stewardship

- Stored: ADR-001 (single block_sync entry) → Unimatrix entry #3371, topic: col-024, category: decision
- Stored: ADR-002 (named timestamp conversion helper cycle_ts_to_obs_millis) → Unimatrix entry #3372, topic: col-024, category: decision
- Stored: ADR-003 (structured debug log on primary-path fallback) → Unimatrix entry #3373, topic: col-024, category: decision
- Stored: ADR-004 (shared enrich_topic_signal helper for all write sites) → Unimatrix entry #3374, topic: col-024, category: decision
- Stored: ADR-005 (open-ended window cap at unix_now_secs()) → Unimatrix entry #3375, topic: col-024, category: decision
- No new patterns discovered during pseudocode design — all implementation follows existing patterns in Unimatrix (#763, #1758, #3367)
