# Scope Risk Assessment: col-020

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | query_log `result_entry_ids` is a JSON string, not a native array. Parsing fragility (malformed JSON, empty strings, nulls) could silently produce incorrect reuse counts | Med | High | Architect should define a robust parser with explicit error handling; never unwrap JSON blindly |
| SR-02 | Knowledge reuse computation requires cross-table joins (query_log + injection_log + entries + sessions) in SQLite without a query planner — manual Rust-side joins risk combinatorial blowup on large topics | Med | Med | Architect should design batch-load-then-join-in-memory with bounded session counts; consider early-exit if session count exceeds threshold |
| SR-03 | Rework outcome detection via substring matching ("rework", "failed") is brittle — outcome text is free-form and may evolve, producing false positives or negatives | Low | High | Spec should enumerate the exact outcome patterns matched and document the false-positive trade-off |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | File path extraction from heterogeneous tool `input` JSON (Read vs Edit vs Glob vs Write — different field names) is under-specified. Missing or unexpected tool schemas produce silent data loss in file zone and reload metrics | Med | Med | Architect should design an explicit tool-to-path-field mapping with an "unknown tool" fallback that logs rather than silently drops |
| SR-05 | "Tier 1 only" boundary for knowledge reuse is clear but the scope implicitly assumes injection_log records are reliably present. If injection_log has gaps (hook failures, races), reuse counts undercount silently | Med | Med | Spec should state the degradation behavior: missing injection_log data produces conservative (lower) reuse counts, not errors |
| SR-06 | context_reload_pct definition ("files read in N+1 also read in prior session") requires ordering sessions chronologically by earliest observation timestamp. Concurrent sessions (same topic, overlapping time) break the N/N+1 model | Low | Low | Architect should define behavior for overlapping sessions — either exclude or treat as single logical session |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | col-017 (topic attribution) is a hard dependency. If session attribution is incomplete or incorrect, all col-020 metrics inherit that error. Attribution quality directly bounds col-020 output quality | High | Med | Architect should design col-020 to report attribution coverage (sessions with vs without topic) so consumers can assess metric trustworthiness |
| SR-08 | Option B (compute knowledge reuse in unimatrix-server, not unimatrix-observe) breaks the existing architectural split where all retrospective computation lives in unimatrix-observe. This creates a precedent for server-side computation that may fragment the pipeline | Med | High | Architect should explicitly decide and document whether this is a one-off exception or a pattern shift. Consider a thin data-loading trait rather than direct Store coupling |
| SR-09 | Updating topic_deliveries counters (AC-12) during retrospective computation introduces a write side-effect into what has been a read-only analysis pipeline. Repeated retrospective runs on the same topic will double-count | Med | Med | Architect should ensure idempotent counter updates — store last-computed values and delta from those, or use absolute replacement rather than additive increment |

## Assumptions

- **Sessions table has reliable outcome data** (SCOPE "Data Available for Rework Sessions"): If outcome is rarely populated, rework_session_count will be trivially zero, making the metric useless rather than wrong.
- **query_log and injection_log are populated for sessions post-nxs-010** (SCOPE "Constraints"): Pre-nxs-010 sessions have no query_log data. The scope acknowledges graceful degradation but does not quantify what fraction of a topic's sessions might lack data.
- **ObservationRecord timestamps are monotonically increasing within a session** (SCOPE "Constraints"): Session ordering by earliest ts assumes clocks are stable. Not a real risk in practice but worth noting.

## Design Recommendations

- **SR-07 is the highest-impact risk.** The architect should design col-020 output to include attribution metadata (e.g., `attributed_session_count` / `total_session_count`) so consumers can gauge metric reliability.
- **SR-09 (idempotent counters) must be resolved in architecture.** Additive counter increments on a repeatable operation is a correctness bug waiting to happen. The spec should mandate idempotency.
- **SR-08 (server-side computation precedent) deserves an ADR.** This is an architectural pattern decision, not just a col-020 implementation detail.
