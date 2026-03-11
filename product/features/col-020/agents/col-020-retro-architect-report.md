# col-020 Retrospective Architect Report

Agent: col-020-retro-architect
Mode: retrospective (post-shipment knowledge extraction)

## 1. Patterns

### New Entries

| ID | Title | Topic | Rationale |
|----|-------|-------|-----------|
| #882 | Best-Effort Optional Computation for Pipeline Extensions | unimatrix-server | col-020 established a clean pattern for adding optional computation steps to existing pipelines without risk of aborting the pipeline. Match/warn/None wrapper per step. Reusable for any future retrospective or pipeline extensions. |
| #883 | Chunked Batch Scan for Session-Scoped Queries | unimatrix-store | Chunked IN clause pattern (50-item chunks) for unbounded session ID lists. Reusable for any batch query against session-keyed tables. |
| #884 | Server-Side Cross-Table Computation as Scoped Exception to Trait Abstraction | unimatrix-server | Documents when and how to break the "all computation in consumer crate" rule. Scoped exception with clear rule of thumb. |

### Verified (Unchanged) Existing Patterns

| ID | Title | Status |
|----|-------|--------|
| #837 | Store CRUD Module Structure for New Tables | Verified accurate. col-020's set_topic_delivery_counters, scan_query_log_by_sessions, scan_injection_log_by_sessions, count_active_entries_by_category all follow this pattern exactly (file structure, imports, row helper, parameterized SQL, test module with TestDb). |
| #755 | Dependency Inversion via Trait-in-Consumer for Crate Independence | Verified accurate. col-020 deliberately did NOT extend ObservationSource (ADR-001), respecting this pattern's boundary. The decision to compute knowledge reuse server-side was the correct application of this pattern. |
| #646 | Backward-Compatible Config Extension via serde(default) | Verified accurate. col-020 used serde(default, skip_serializing_if) for all 5 new RetrospectiveReport fields. Backward-compat deserialization test confirmed. |
| #316 | ServiceLayer extraction pattern for unimatrix-server | Not directly applicable — col-020 extended context_retrospective handler inline rather than creating a new service. Consistent with the pattern: retrospective is a single tool handler, not a shared service. |

### Skipped

| Component | Reason |
|-----------|--------|
| C1 session_metrics module | New file but follows standard Rust module conventions. The computation logic (group-by-key, frequency count, top-N truncation) is domain-specific, not a reusable structural pattern. |
| C2 types extension | Follows existing serde(default) pattern (#646). No new structural pattern. |
| C5 report builder | No code change (post-build mutation). Already part of #882. |

## 2. Procedures

No new or changed procedures. col-020 followed the standard Store CRUD module structure (#837) and the standard delivery protocol. No novel techniques emerged.

## 3. ADR Validation

| ADR | Decision | Validation Status | Notes |
|-----|----------|-------------------|-------|
| ADR-001 (Knowledge Reuse Server-Side) | Compute in handler, not unimatrix-observe | **Validated** | Implementation confirmed: knowledge_reuse.rs lives in unimatrix-server/src/mcp/. ObservationSource trait unchanged. 26 tests pass. The "rule of thumb" (observation data -> observe, Store joins -> handler) held cleanly. |
| ADR-002 (Idempotent Counter Updates) | Absolute-set, not additive increment | **Validated** | set_topic_delivery_counters uses UPDATE SET (not += delta). Idempotency confirmed by test_set_topic_delivery_counters_idempotent. No re-run bugs reported. |
| ADR-003 (Attribution Metadata) | AttributionMetadata on report | **Validated** | Struct defined, populated in handler, serialized on report. Backward-compat tests pass. Consumers can assess coverage. |
| ADR-004 (File Path Extraction Mapping) | Explicit tool-to-field match | **Validated** | extract_file_path covers Read/Edit/Write/Glob/Grep with correct field names. 8 unit tests. Unknown tools return None safely. |

No ADRs flagged for supersession. All 4 decisions were sound and implementation confirmed them.

## 4. Lessons

### New Entries

| ID | Title | Source |
|----|-------|--------|
| #885 | Serde-heavy types need explicit test coverage in component test plans | Gate 3b failure: C2 types component had 0/8 serde tests. Reworked in 1 iteration. Generalizable: serde attributes are load-bearing contracts that need round-trip, backward-compat, skip-serializing, and partial-field tests. |
| #886 | Parallel worktree delivery inflates hotspot metrics — normalize by agent count | Hotspot analysis: 130 files, 110 compile cycles, 35 permission retries look alarming but divide by 6 agents to get ~22 files, ~18 compiles, ~6 retries per agent (all normal). Recommends auto-normalization in retrospective pipeline. |

## 5. Retrospective Findings

### Hotspot-Derived Analysis

| Hotspot | Raw Value | Normalized (6 agents) | Assessment |
|---------|-----------|----------------------|------------|
| file_breadth | 130 | ~22/agent | Normal for 6-component feature |
| reread_rate | 93 | ~15/agent | Normal — reading architecture + pseudocode + existing code |
| mutation_spread | 66 | ~11/agent | Normal — includes design artifacts (agent reports) |
| compile_cycles | 110 | ~18/agent | Normal — initial build + incremental + test cycles |
| permission_retries (Read) | 35 | ~6/agent | Borderline — settings.json allowlist would help |
| permission_retries (Bash) | 12 | ~2/agent | Normal |
| sleep_workarounds | 12 | NOT normalized | Genuine inefficiency — agents polling task output via sleep instead of run_in_background |
| output_parsing_struggle | 5 instances | NOT normalized | Genuine inefficiency — piping cargo output through multiple filters |
| source_file_count | 7 | N/A | Expected for 6-component feature (session_metrics.rs, knowledge_reuse.rs, + test/type files) |
| design_artifact_count | 27 | N/A | Expected — 6 agent reports + gate reports + design docs |

### Recommendation Actions

| Recommendation | Action Taken |
|----------------|-------------|
| Add common build/test commands to settings.json allowlist | Not actioned (infrastructure change, outside retro scope). Recorded in lesson #886. |
| Use run_in_background + TaskOutput instead of sleep polling | Not actioned (agent behavior change, requires protocol update). Noted as genuine inefficiency. |
| Consider incremental compilation or targeted cargo test | Already partially followed — agents used targeted `cargo test -p` per crate. Full workspace builds are inherent to final validation. |

### Outlier Notes

- No baseline outliers detected (col-020 was within expected ranges for all baselined metrics).
- Gate 3b failure was a single iteration rework for missing tests — low severity, quick fix.
- No gate 3a or 3c failures — design and final validation passed first attempt.
- The feature shipped cleanly across 2 sessions with 16 delivery agents in the second session.
