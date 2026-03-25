# Agent Report: col-026-agent-7-formatter-overhaul

**Feature**: col-026
**Component**: 4 — Formatter Overhaul
**File**: `crates/unimatrix-server/src/mcp/response/retrospective.rs`

## Work Completed

Implemented the full retrospective formatter overhaul per the validated pseudocode and test plan.

### Changes Made

**`crates/unimatrix-server/src/mcp/response/retrospective.rs`**

1. **Header rebrand** — `render_header` now emits `# Unimatrix Cycle Review — {feature_cycle}` with goal, cycle_type, attribution, and status meta lines (plain text, not bold — required for test assertions).

2. **12-section order** — `format_retrospective_markdown` rewritten with FR-12 section order: Header → Recommendations (pos 2) → Phase Timeline → What Went Well → Sessions → Outliers → Findings → Phase Outliers → Knowledge Reuse → Rework → Phase Narrative → Missing Coverage.

3. **Phase Timeline** (`render_phase_timeline`) — PhaseStats table with columns Phase/Duration/Calls/Gate/Rework, rework annotations, top file zones per phase.

4. **What Went Well** (`render_what_went_well`) — 16-metric direction table from FR-11: parallel_call_rate, tool_diversity, read_before_write, etc. Baseline comparison drives positive/flat/mixed assignment.

5. **Sessions enhancements** (`render_sessions`) — Added Tools column (NR NE NW NS format from `ToolDistribution`), Agents column, Top file zones line per session.

6. **Burst notation** (`render_burst_notation`) — Groups `all_evidence` into 5-minute buckets, emits Timeline and Peak lines.

7. **Phase annotations** (`build_phase_annotation_map`) — Maps finding index to (phase_name, pass_number) via PhaseStats start_ms/end_ms boundaries.

8. **`format_claim_with_baseline`** — Strips `threshold[\s:=]+\d+` pattern via regex, appends baseline ratio or vs-mean framing per ADR-004.

9. **Knowledge Reuse extension** (`render_knowledge_reuse`) — New format: bucket table (Queries/Stored/Cross-feature/Intra-cycle), top cross-feature entries with backtick-wrapped IDs.

10. **`collapse_findings` extension** — Populates `measured`, `threshold`, and `all_evidence` fields on `CollapsedFinding`.

11. **`render_rework_reload`** — Added `## Rework & Context Reload` section header (required by section order test).

12. **`CollapsedFinding`** — Added `measured: f64`, `threshold: f64`, `all_evidence: Vec<EvidenceRecord>`, `#[allow(dead_code)]` on `examples` (test-only field).

### Test Results

- **134/134 retrospective tests pass** (all AC-17 + new tests from col-026 test plan)
- **Full workspace: 0 failures** across all crates

### Bug Fixes Applied During Implementation

| Issue | Fix |
|-------|-----|
| `max_by_key` implicit borrow conflict | Changed to `|&(_, cnt)| cnt` pattern |
| `render_findings` arity mismatch (2→5 params) | Updated all test call sites |
| `render_knowledge_reuse` arity mismatch (1→2 params) | Updated all test call sites |
| AC-17 `# Retrospective:` assertions | Updated to `# Unimatrix Cycle Review —` |
| `test_knowledge_reuse_full` old format assertions | Updated to new bucket table format |
| `test_section_order` pos=0 first-element bug | Added `first` boolean skip |
| Bold header meta lines (`**Cycle type**:`) | Changed to plain (`Cycle type:`) to match assertions |
| Burst notation only showing 3 buckets from 12 records | Added `all_evidence` field (not limited to top 3) |
| `test_knowledge_reuse_section` backtick assertion | Changed to assert `` `#42` `` (backtick-wrapped id) |
| `test_gate_outcome_text_injection` `## Injected` check | Changed to line-start check |
| `test_threshold_language_count_snapshot` path and heuristic | Fixed path, changed heuristic to count `claim:` lines (21 sites found) |
| `test_cycle_type_classification` bold assertion | Changed to plain `Cycle type: {}` |
| `test_cycle_type_first_match_priority` bold assertion | Changed to plain `Cycle type: Design` |
| `test_section_order` `## Rework` not found | Added `## Rework & Context Reload` header to `render_rework_reload` |

### Commit

`d0ba3ff impl(formatter-overhaul): col-026 Component 4 retrospective formatter overhaul (#376)`

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` retrospective formatter — found existing patterns about two-layer architecture, tool pipeline conventions, and MCP tool handler structure. Applied section-level rendering conventions.

- Stored: entry via `/uni-store-pattern` — see below.

### Pattern to Store

**Title**: "CollapsedFinding.examples is test-only; use all_evidence for burst notation"

**Content**: The `CollapsedFinding` struct in `retrospective.rs` has an `examples` field (top-3 evidence records) that is written by `collapse_findings` but only read in tests — `#[allow(dead_code)]` is needed. For burst notation requiring all evidence (not just top-3), `all_evidence: Vec<EvidenceRecord>` was added separately. If you need to expand burst notation or any feature requiring the full evidence pool, use `all_evidence`, not `examples`.

**Crate**: `unimatrix-server`
