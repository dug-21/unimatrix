# col-020 Researcher Report

## Files Explored

- `crates/unimatrix-observe/src/metrics.rs` -- current metric computation (22 universal metrics)
- `crates/unimatrix-observe/src/report.rs` -- report assembly and recommendations
- `crates/unimatrix-observe/src/types.rs` -- all retrospective types (RetrospectiveReport, MetricVector, etc.)
- `crates/unimatrix-observe/src/source.rs` -- ObservationSource trait
- `crates/unimatrix-observe/src/attribution.rs` -- feature attribution logic
- `crates/unimatrix-observe/src/lib.rs` -- public API surface
- `crates/unimatrix-server/src/mcp/tools.rs` -- context_retrospective handler (lines 1033-1257)
- `crates/unimatrix-server/src/services/observation.rs` -- SqlObservationSource implementation
- `crates/unimatrix-store/src/topic_deliveries.rs` -- TopicDeliveryRecord CRUD
- `crates/unimatrix-store/src/query_log.rs` -- QueryLogRecord insert/scan
- `crates/unimatrix-store/src/db.rs` -- schema (sessions, entries, injection_log tables)
- `product/features/nxs-010/SCOPE.md` -- dependency feature scope
- `product/research/ass-018/MILESTONE-PROPOSAL.md` -- Activity Intelligence milestone
- `product/PRODUCT-VISION.md` -- roadmap context

## Key Findings

1. **Data is ready.** The sessions table has `outcome` and `feature_cycle` columns. query_log and topic_deliveries tables exist (nxs-010). injection_log has session_id + entry_id. All the raw data needed for cross-session metrics exists.

2. **ObservationRecord already carries everything needed for session summaries.** Tool names, input JSON with file paths, SubagentStart events with agent names, session_id for grouping. No schema changes needed for session decomposition.

3. **Knowledge reuse crosses the observe/server boundary.** unimatrix-observe has no access to Store (by design, ADR-002). Reuse computation requires query_log + injection_log + entries table joins. This must happen in unimatrix-server, not unimatrix-observe. The assembled result can be passed to the report builder.

4. **RetrospectiveReport is designed for additive extension.** Existing pattern uses `#[serde(default, skip_serializing_if)]` for optional fields (entries_analysis, narratives, recommendations). New fields follow the same pattern.

5. **context_retrospective handler is already large (~225 lines).** Adding session summary computation, knowledge reuse, rework counting, reload rate, and counter updates will add significant logic. Consider extracting helper functions or a dedicated service.

6. **query_log stores result_entry_ids as JSON strings.** Parsing `"[1,2,3]"` to `Vec<u64>` is needed for reuse computation. The `QueryLogRecord` struct already has this as a String field.

7. **No existing batch scan for injection_log.** `scan_query_log_by_session` exists for single sessions but there is no batch variant. injection_log has no scan API at all -- only insert via fire-and-forget in listener.rs. New Store methods needed.

## Scope Boundaries

**In scope:**
- Per-session activity profiles (tool distribution, file zones, agents, knowledge flow)
- Cross-session knowledge reuse (Tier 1 only: search/lookup sequences + helpful signals)
- Rework session count per topic
- Context reload percentage
- topic_deliveries counter updates

**Explicitly out of scope:**
- Session-type classification (design/delivery/bugfix)
- Session efficiency trend (dropped by design decision)
- Tier 2/3 reuse measurement (briefing effectiveness)
- Changes to existing detection rules or hotspot logic
- Retrospective output formatting (vnc-011's responsibility)

## Risks

1. **Rework outcome detection is undefined.** The `sessions.outcome` column is free-form TEXT. Need to agree on what constitutes a "rework" signal before implementation.
2. **Historical data gap.** query_log only exists after nxs-010 lands. Topics with sessions predating nxs-010 will show zero knowledge reuse even if reuse actually happened. The feature should note this clearly.
3. **Handler complexity.** The context_retrospective handler is already substantial. Adding 5 new computation steps without refactoring risks an unmaintainable function.

## Deliverable

SCOPE.md written to: `product/features/col-020/SCOPE.md`
