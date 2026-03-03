# Specification: col-010b Retrospective Evidence Synthesis & Lesson-Learned Persistence

Feature: col-010b
Status: Draft
Author: col-010b-agent-2-spec
Date: 2026-03-02
Prerequisite: col-010 P0 merged (PR #77)

---

## Objective

col-010b extends the col-002 retrospective pipeline with evidence-limited output, narrative synthesis, actionable recommendations, lesson-learned auto-persistence, and a provenance boost for lesson-learned entries in search results. All changes are application-logic only — no schema migration, no new tables.

---

## Functional Requirements

### FR-01: Evidence-Limited Retrospective Output

**FR-01.1**: `RetrospectiveParams` MUST gain an `evidence_limit: Option<usize>` field. When absent, default to `3`.

**FR-01.2**: When `evidence_limit > 0`, the `context_retrospective` handler MUST truncate each `HotspotFinding.evidence` array to at most `evidence_limit` items before serialization. Truncation MUST operate on a cloned report — the in-memory report used for narrative synthesis and lesson-learned content MUST retain full evidence arrays (ADR-001).

**FR-01.3**: When `evidence_limit = 0`, no truncation is applied. The response is structurally identical to pre-col-010b output (backward compatible).

**FR-01.4**: The `hotspots: Vec<HotspotFinding>` type on `RetrospectiveReport` is unchanged. No new wrapper type. Truncation is server-side serialization-time only.

**FR-01.5**: Default `evidence_limit = 3` MUST produce a total JSON payload of 10KB or less for a report with 13 hotspots (AC-01).

**Addresses**: AC-01, AC-02, AC-10, SR-04

---

### FR-02: R-09 Blocking Gate (Test Audit)

**FR-02.1**: Before implementing any code for FR-01, the developer MUST audit all existing integration tests for `context_retrospective` that assert on `hotspots[].evidence` array lengths or contents.

**FR-02.2**: Any test asserting exact evidence array lengths MUST be updated to either:
- Pass `evidence_limit = 0` (restore full arrays), or
- Update expected count to <= 3

**FR-02.3**: The audit MUST be documented as a comment in the implementation PR, confirming either "no tests assert on evidence array lengths" or listing the specific tests updated.

**Addresses**: AC-10, SR-04

---

### FR-03: Narrative Synthesis Types

**FR-03.1**: `RetrospectiveReport` MUST gain two additive fields:
- `narratives: Option<Vec<HotspotNarrative>>` with `#[serde(default, skip_serializing_if = "Option::is_none")]`
- `recommendations: Vec<Recommendation>` with `#[serde(default, skip_serializing_if = "Vec::is_empty")]`

**FR-03.2**: `HotspotNarrative` MUST contain:
- `hotspot_type: String` — matches `HotspotFinding.rule_name`
- `summary: String` — non-empty, human-readable synthesized description
- `clusters: Vec<EvidenceCluster>` — timestamp-clustered event groups
- `top_files: Vec<(String, u32)>` — top 5 files by occurrence count
- `sequence_pattern: Option<String>` — monotone sequence for sleep_workarounds

**FR-03.3**: `EvidenceCluster` MUST contain:
- `window_start: u64` — unix epoch millis of first event in cluster
- `event_count: u32` — number of events in the cluster
- `description: String` — human-readable description

**FR-03.4**: `Recommendation` MUST contain:
- `hotspot_type: String`
- `action: String` — non-empty actionable text
- `rationale: String`

**Addresses**: AC-03, AC-04, AC-05

---

### FR-04: Narrative Synthesis Logic

**FR-04.1**: `synthesize_narratives(hotspots: &[HotspotFinding]) -> Vec<HotspotNarrative>` MUST produce one `HotspotNarrative` per `HotspotFinding`.

**FR-04.2**: **Timestamp clustering**: Group evidence events by proximity. Events within `CLUSTER_WINDOW_SECS = 30` seconds (30,000 ms) of each other form a cluster. Each cluster reports `window_start` (earliest event ts), `event_count`, and a description string.

**FR-04.3**: **Sequence extraction**: For hotspots with `rule_name == "sleep_workarounds"`, scan evidence descriptions for numeric duration values. If extracted numbers form a strictly monotonically increasing sequence of >= 2 values, format as `"Ns->Ns->..."` (e.g., `"30s->60s->90s->120s"`). If non-monotone or < 2 values, return `None`.

**FR-04.4**: **Top-N files**: Parse evidence descriptions for file paths or filenames. Count occurrences per file. Sort descending by count. Return top 5 as `Vec<(String, u32)>`. If more than 5 distinct files exist, the `summary` field includes `"... and N more"`.

**FR-04.5**: **Summary generation**: Build a human-readable summary string from the hotspot claim, cluster count, top files, and sequence pattern. The summary MUST be non-empty for every hotspot.

**FR-04.6**: `narratives` is `Some(...)` when the structured-events path is used (SESSIONS data exists). `narratives` is `None` when the JSONL fallback path is used.

**Addresses**: AC-03, AC-04

---

### FR-05: Recommendation Templates

**FR-05.1**: `recommendations_for_hotspots(hotspots: &[HotspotFinding]) -> Vec<Recommendation>` MUST produce recommendations for recognized hotspot types.

**FR-05.2**: Templates MUST cover exactly these four hotspot types:
- `"permission_retries"` -> action: "Add common build/test commands to settings.json allowlist"
- `"coordinator_respawns"` -> action: "Review coordinator agent lifespan and handoff patterns"
- `"sleep_workarounds"` -> action: "Use run_in_background + TaskOutput instead of sleep polling"
- `"compile_cycles"` (only when `measured > 10.0`) -> action: "Consider incremental compilation or targeted cargo test invocations"

**FR-05.3**: Unrecognized hotspot types produce no recommendation. A report with no recognized hotspot types MUST produce an empty `recommendations` vector.

**FR-05.4**: `recommendations` is populated on BOTH the structured-events path and the JSONL fallback path (recommendations only need hotspot data, not SESSIONS/INJECTION_LOG).

**Addresses**: AC-05

---

### FR-06: Lesson-Learned Auto-Persistence

**FR-06.1**: After `context_retrospective` builds a report with `hotspots.len() > 0 OR recommendations.len() > 0`, automatically write a `category: "lesson-learned"` entry to Unimatrix.

**FR-06.2**: The lesson-learned entry MUST be written with full ONNX embedding (via existing embed pipeline). The write is fire-and-forget via `tokio::spawn` — `context_retrospective` returns its report before embedding completes (col-010 ADR-004).

**FR-06.3**: Entry fields:
- `title`: `"Retrospective findings: {feature_cycle}"`
- `content`: narrative summaries + recommendations (structured path); hotspot claims + recommendations (JSONL path)
- `topic`: `"retrospective/{feature_cycle}"`
- `category`: `"lesson-learned"`
- `tags`: `["feature_cycle:{feature_cycle}", "hotspot_count:{N}", "source:retrospective"]`
- `created_by`: `"cortical-implant"`
- `trust_source`: `"system"` (0.7 trust score)
- `feature_cycle`: the feature cycle string

**FR-06.4**: **CategoryAllowlist check**: Before writing, verify `"lesson-learned"` is in the active CategoryAllowlist. If absent, log an error (`tracing::error!`) and skip the write. Do not fail the retrospective response.

**FR-06.5**: **De-duplication by supersede**: Before writing, query for an existing active `lesson-learned` entry with `topic == "retrospective/{feature_cycle}"`. If found:
1. Deprecate the existing entry (set `superseded_by` on old entry)
2. Write new entry with `supersedes = old_entry_id`

**FR-06.6**: The supersede check-and-write is NOT atomic. Concurrent `context_retrospective` calls for the same feature_cycle may briefly produce two active entries. This is a known tolerated limitation (col-010 SR-09).

**FR-06.7**: If ONNX embedding fails, the entry MUST still be written with `embedding_dim = 0`. Log the failure at `tracing::warn!`. The entry is queryable via `context_lookup` by topic/category. On the next retrospective call, the supersede path replaces it with a properly embedded entry.

**FR-06.8**: A report with zero hotspots AND zero recommendations MUST NOT trigger a lesson-learned write.

**Addresses**: AC-06, AC-07, AC-08

---

### FR-07: Provenance Boost

**FR-07.1**: Add `pub const PROVENANCE_BOOST: f64 = 0.02` as a named constant in `crates/unimatrix-engine/src/confidence.rs`.

**FR-07.2**: At search result re-ranking time, apply `PROVENANCE_BOOST` additively to the rerank score for any entry where `entry.category == "lesson-learned"`:
```
final_score = rerank_score(similarity, confidence) + co_access_boost + provenance_boost
```
where `provenance_boost = PROVENANCE_BOOST` if `category == "lesson-learned"`, else `0.0`.

**FR-07.3**: Apply at BOTH:
- `tools.rs`: MCP `context_search` handler re-ranking (both initial sort and co-access re-sort)
- `uds_listener.rs`: ContextSearch hook handler re-ranking (both initial sort and co-access re-sort)

**FR-07.4**: `PROVENANCE_BOOST` MUST NOT modify stored `confidence` values. It is query-time only. The stored weight invariant (`W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST = 0.92`) is unchanged.

**FR-07.5**: Two entries with identical `similarity` and `confidence` scores where one has `category == "lesson-learned"` and the other does not: the lesson-learned entry MUST rank higher by exactly `PROVENANCE_BOOST = 0.02`.

**Addresses**: AC-09

---

## Non-Functional Requirements

### NFR-01: Performance

**NFR-01.1**: `context_retrospective` response latency MUST NOT include ONNX embedding time (fire-and-forget).

**NFR-01.2**: Default `evidence_limit = 3` MUST produce <= 10KB JSON payload for a 13-hotspot report.

**NFR-01.3**: Narrative synthesis is deterministic heuristics only — no ML, no external API calls.

### NFR-02: Backward Compatibility

**NFR-02.1**: `evidence_limit = 0` MUST produce output structurally identical to pre-col-010b output.

**NFR-02.2**: The `build_report()` JSONL path in `unimatrix-observe` MUST remain unchanged and functional.

**NFR-02.3**: All existing tests MUST pass without modification (AC-10). `RetrospectiveReport` additions use `#[serde(default)]` and `skip_serializing_if`.

**NFR-02.4**: No breaking changes to any existing MCP tool signatures.

### NFR-03: Reliability

**NFR-03.1**: Fire-and-forget lesson-learned write failures MUST be logged at `tracing::warn!` level. They MUST NOT fail the `context_retrospective` response.

**NFR-03.2**: CategoryAllowlist absence for `"lesson-learned"` MUST be logged at `tracing::error!` and the write skipped. The retrospective response MUST still succeed.

---

## Acceptance Criteria Verification Map

| AC | Statement | Implementing FR | Verification |
|----|-----------|-----------------|-------------|
| AC-01 | Default `evidence_limit` returns <= 3 evidence items per hotspot; payload <= 10KB for 13-hotspot report | FR-01.2, FR-01.5 | Integration test |
| AC-02 | `evidence_limit = 0` returns complete evidence arrays — identical to pre-col-010b | FR-01.3 | Integration test (snapshot) |
| AC-03 | Structured-events path returns `narratives: Some(...)` alongside capped evidence; JSONL path returns `narratives: None` | FR-03.1, FR-04.6 | Integration test |
| AC-04 | sleep_workarounds with monotone intervals -> sequence_pattern; non-monotone -> None | FR-04.3 | Unit test |
| AC-05 | permission_retries -> Recommendation with non-empty action; no recognized types -> empty list. All 4 templates verified. | FR-05.1-05.3 | Unit test |
| AC-06 | After retrospective with >= 1 finding, lesson-learned entry exists with correct fields | FR-06.1-06.3 | Integration test |
| AC-07 | Second retrospective supersedes first lesson-learned entry | FR-06.5 | Integration test |
| AC-08 | context_search finds lesson-learned entry for related query | FR-06.2, FR-07.2 | Integration test |
| AC-09 | lesson-learned ranks higher than equal entry by exactly PROVENANCE_BOOST | FR-07.2, FR-07.5 | Unit + integration test |
| AC-10 | All existing tests pass without modification | FR-02.1-02.3, NFR-02.3 | cargo test --workspace |

---

## Open Questions

None. All design decisions are resolved — inherited from col-010 ADR-004, ADR-005, and SCOPE.md resolved decisions.
