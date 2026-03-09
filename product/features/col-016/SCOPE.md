# Intelligence Ground Truth Validation

## Problem Statement

Unimatrix has a confidence formula that scores entries and a search pipeline that ranks results. col-015 validates that the formula behaves as designed (drift detection). But neither col-015 nor anything else validates that the intelligence is actually *working* -- that queries return relevant results and that irrelevant results get demoted over time. There is no ground truth against which to measure retrieval quality.

Without ground truth, we cannot answer: "Is Unimatrix returning the right knowledge?" We can only answer: "Is the formula producing the expected numbers?" These are fundamentally different questions.

The human who operates this system has stated a critical constraint: **"It is easier to tell you a wrong answer than a right one."** This means ground truth must accumulate from negative signals (rejection, correction, deprecation) rather than positive curation. This aligns with how the existing system already works -- unhelpful votes, corrections, deprecations, and quarantines are all negative signals. What is missing is the infrastructure to aggregate these signals into measurable retrieval quality metrics.

## Goals

1. Establish a ground truth dataset that grows organically from negative labeling signals already flowing through the system.
2. Compute retrieval quality metrics (NDCG, precision@k) against the ground truth dataset.
3. Correlate retrieval quality with feature delivery outcomes (success/failure from outcome tracking).
4. Provide a programmatic API (not just tests) for running retrieval quality evaluations, so they can be triggered during retrospectives or on-demand.
5. Surface retrieval quality trends over time, enabling detection of quality regression distinct from formula drift.

## Non-Goals

- **Positive labeling UI or workflow.** No curation sessions, no "mark as relevant" tools. Ground truth comes from existing negative signals only.
- **Automated re-tuning of confidence weights.** This feature measures quality; it does not change the formula. Weight adjustment is a future feature that depends on having ground truth first.
- **Real-time query interception.** This feature does not add latency to the search pipeline. Evaluation runs offline against historical data.
- **New MCP tools for the human.** This is internal infrastructure. The human interacts with it indirectly through retrospective reports and status output.
- **Replacing col-015 drift detection.** Drift detection (formula behaves as designed) and quality validation (results are actually good) are complementary, not competing.

## Background Research

### Existing Negative Signals

Six negative signal sources already exist in the codebase:

| Signal | Where | What it captures | Gap |
|--------|-------|------------------|-----|
| `unhelpful_count` | `entries.unhelpful_count` column, incremented via `UsageService::record_mcp_usage` | Per-entry count of "not helpful" votes from agents | Not linked to the query that surfaced the entry. We know entry X was unhelpful, but not "unhelpful for what query." |
| Correction chains | `context_correct` tool, `StoreService::correct()` | Original entry deprecated, new entry created with `supersedes` link | Captures "this was wrong, here is the right answer" -- strong ground truth signal. Not linked to original retrieval query. |
| Deprecation | `context_deprecate` tool | Entry marked as no longer relevant | Weak signal (entry may have been good but is now outdated). |
| Quarantine | `context_quarantine` tool | Entry removed from retrieval entirely | Strong signal: entry was harmful enough to remove. |
| Knowledge gaps | `KnowledgeGapRule` extraction rule | Zero-result `context_search` calls across 2+ sessions | Captures queries with no results -- useful for recall measurement. Not stored as structured ground truth. |
| Outcome tracking | `OUTCOME_INDEX` table, `sessions.outcome` column | Feature delivery success/failure tagged by feature_cycle | Indirect signal: correlates overall retrieval quality with delivery outcomes. |

### Query-Result Relationship Tracking

**Critical finding:** The system does NOT currently track query-to-result mappings for MCP tool calls. Here is what exists:

- **Audit log** (`audit_log` table): Records `operation: "search_service"` with `target_ids` (the entry IDs returned) and `detail` ("returned N results"). But it does NOT store the query text or embedding.
- **Injection log** (`injection_log` table): Records `session_id`, `entry_id`, `confidence`, `timestamp` for hook-injected entries. No query text (injection is not query-driven; it is briefing-driven).
- **Observations** (`observations` table): Records tool calls including `context_search` with `input` (contains query) and `response_snippet`. PostToolUse observations capture the response. This is the closest to a query-result mapping but is stored as raw JSONL-style records, not structured query-result pairs.
- **Usage dedup** (`UsageDedup` in-memory): Tracks which agent+entry pairs have been counted. Ephemeral; lost on server restart.

**Gap:** To build ground truth, we need to reconstruct "query Q returned entries [A, B, C]" and then overlay negative signals ("entry B was unhelpful for that kind of query"). The observations table has the raw data but no structured extraction for this purpose.

### Infrastructure from col-015

col-015 is scoped but not yet built. Its roadmap description mentions:
- Cross-cutting test infrastructure for the full pipeline
- Confidence calibration testing (are weights producing correct rankings?)
- Extraction quality validation

col-015 does not mention Kendall tau, NDCG, or a TestHarness -- those are not yet in the codebase. col-016 should complement col-015 by adding retrieval quality metrics (NDCG, precision@k) that col-015 does not cover. The two features can share test infrastructure patterns but serve different purposes.

### Crate Placement

The retrieval quality evaluation logic should live in `unimatrix-engine` alongside the existing confidence computation (`crates/unimatrix-engine/src/confidence.rs`). The engine crate already owns scoring logic and is the natural home for quality metrics. The ground truth dataset storage (negative judgments) belongs in `unimatrix-store` as new tables. The evaluation runner that wires store + engine + search belongs in `unimatrix-server` (service layer).

## Proposed Approach

### 1. Ground Truth Accumulation (Passive)

Introduce a `relevance_judgments` table that captures negative query-entry associations:

- When an agent votes unhelpful on search results, record (query_text_hash, entry_id, judgment=irrelevant, source="unhelpful_vote").
- When `context_correct` is called, record (query_text_hash, original_entry_id, judgment=irrelevant) and (query_text_hash, corrected_entry_id, judgment=relevant) -- corrections are the one case where we get both signals.
- When an entry is quarantined, retroactively mark all recent query associations for that entry as irrelevant.
- Mine the observations table for `context_search` PostToolUse records to reconstruct query-result pairs and link them to subsequent unhelpful votes within the same session.

The key insight: we do NOT need the human to label queries. We reconstruct ground truth from behavioral signals that already flow through the system. The query text hash (not the full text) is sufficient for grouping.

### 2. Retrieval Quality Metrics

Implement NDCG@k and precision@k evaluation functions in `unimatrix-engine`:

- Given a query and a set of relevance judgments, re-run the search pipeline and compute metrics.
- Binary relevance by default (relevant/irrelevant), derived from the negative labeling: entries NOT marked irrelevant are assumed relevant (open-world assumption; absence of negative signal = neutral, not positive).
- Graded relevance where correction chains provide it: corrected entry = 0, correction = 2, other results = 1.

### 3. Evaluation Runner

A service-layer component that:

- Selects queries from the ground truth dataset that have enough judgments (minimum threshold, e.g., 3+ judgments per query).
- Runs each query through the current search pipeline.
- Computes NDCG@k and precision@k for each query.
- Aggregates into a quality report with mean, median, and trend over time.
- Can be triggered from `context_retrospective` or `context_status`.

### 4. Outcome Correlation

Cross-reference retrieval quality metrics with feature delivery outcomes from `OUTCOME_INDEX`:

- For each feature cycle with outcomes, compute the average retrieval quality of queries issued during that feature's sessions.
- Report whether low retrieval quality correlates with delivery failures.

## Acceptance Criteria

- AC-01: A `relevance_judgments` table exists in the schema with columns: `id`, `query_hash`, `entry_id`, `judgment` (enum: irrelevant/relevant), `source` (enum: unhelpful_vote/correction/quarantine/correction_positive), `session_id`, `timestamp`.
- AC-02: When `UsageService::record_mcp_usage` processes an unhelpful vote, it writes a relevance judgment linking the query (from the most recent `context_search` audit log entry in the same session) to the entry.
- AC-03: When `StoreService::correct()` executes, it writes two relevance judgments: irrelevant for the original entry, relevant for the correction entry, linked to the original entry's most recent query context.
- AC-04: When an entry is quarantined, all `injection_log` records for that entry are used to generate irrelevant judgments retroactively.
- AC-05: `ndcg_at_k(results: &[(u64, f64)], judgments: &HashMap<u64, RelevanceGrade>, k: usize) -> f64` is implemented in `unimatrix-engine` and returns values in [0.0, 1.0].
- AC-06: `precision_at_k(results: &[u64], relevant: &HashSet<u64>, k: usize) -> f64` is implemented in `unimatrix-engine` and returns values in [0.0, 1.0].
- AC-07: An evaluation runner in the server service layer can select queries with 3+ judgments, re-execute searches, and compute aggregate NDCG@5 and precision@5.
- AC-08: The evaluation report includes per-query scores, aggregate mean/median, and the number of evaluated queries.
- AC-09: Retrieval quality metrics are exposed through `context_status` when `maintain=true` is set (piggybacking on the existing maintenance path).
- AC-10: Feature-level outcome correlation is computed: for each feature cycle with 5+ evaluated queries, report average NDCG@5 alongside the feature outcome.
- AC-11: The ground truth dataset grows without human intervention -- all accumulation is from existing signal flows.
- AC-12: No new MCP tools are added. Ground truth is internal infrastructure.
- AC-13: Evaluation does not affect search latency. All quality computation runs offline in background tasks.

## Constraints

- **No query text storage in audit log.** The audit log records `target_ids` and `detail` but not the query string. Linking unhelpful votes back to queries requires either (a) adding query text/hash to audit log entries, or (b) mining the observations table for the preceding `context_search` PreToolUse record in the same session. Option (b) avoids schema changes to audit_log but depends on observation data being available.
- **Observation data has a 60-day retention window.** The `observation_stats` source tracks aging observations. Ground truth derived from observations must be extracted and persisted separately before the observations age out.
- **Wilson score minimum sample guard.** The existing helpfulness score requires 5+ votes before deviating from neutral. Similarly, ground truth evaluation should require a minimum number of judgments per query (proposed: 3) to be meaningful.
- **col-015 is not yet built.** col-016 cannot depend on col-015 test infrastructure since it does not exist yet. The two features should be designed to complement each other but not block each other.
- **Schema migration.** Adding `relevance_judgments` requires a schema migration (v6 to v7, or wherever the version currently stands). This must follow the existing migration pattern in `crates/unimatrix-store/src/migration.rs`.
- **Embedding model boundary.** NDCG computation needs to re-run searches, which requires the embedding model. This means the evaluation runner must have access to `EmbedServiceHandle` and runs in the server crate, not in store or engine alone.

## Open Questions

1. **Query hash vs. query text:** Should `relevance_judgments` store a hash of the query text (privacy-preserving, smaller) or the full query text (enables re-running the exact query)? Storing the full text enables re-evaluation as the index changes; storing only the hash requires reconstructing the query from observations.

2. **Session-scoped vs. global judgments:** If agent A votes entry X unhelpful for query Q, does that judgment apply globally (entry X is irrelevant for query Q regardless of who asks) or only within that agent's context? The human's "wrong answer" framing suggests global judgments.

3. **Judgment decay:** Should old judgments lose influence over time? An entry corrected 6 months ago may have been corrected again since then. The correction chain already handles this (latest correction supersedes), but standalone unhelpful votes may become stale.

4. **Minimum judgments threshold:** 3 judgments per query is proposed as the evaluation threshold. Is this too low (noisy) or too high (most queries will never reach it)?

5. **When to trigger evaluation:** On every `context_status maintain=true` call? On retrospective? On a schedule? The human's workflow pattern determines the right trigger.

6. **Graded vs. binary relevance:** Binary (relevant/irrelevant) is simpler and sufficient for most cases. Graded relevance (0/1/2) from correction chains adds precision but complexity. Should we start with binary only?

7. **Interaction with col-015:** If col-015 builds test scenario infrastructure, should col-016 reuse it or build independently? The answer depends on which ships first.

## Tracking

Will be updated with GH Issue link after Session 1.
