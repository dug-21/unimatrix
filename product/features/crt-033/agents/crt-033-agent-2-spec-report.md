# Agent Report: crt-033-agent-2-spec

## Task

Write the SPECIFICATION.md for crt-033: CYCLE_REVIEW_INDEX memoization for context_cycle_review.

## Output

`product/features/crt-033/specification/SPECIFICATION.md`

## Key Decisions Made

### 1. pending_cycle_reviews source: cycle_events not query_log (OQ-02)

SCOPE.md references `query_log.feature_cycle` for the pending_cycle_reviews query. Code inspection shows the current `query_log` table has no `feature_cycle` column (confirmed via `query_log.rs` and `migration_v16_to_v17.rs` which shows the v17-shape columns). The specification substitutes `cycle_events.cycle_id` with `event_type = 'cycle_start'` as the authoritative source. This also happens to achieve the pre-cycle_events exclusion requirement more cleanly. Flagged as OQ-02 for architect confirmation.

### 2. Explicit SQL for pending_cycle_reviews

The SCOPE's proposed SQL joined on `query_log.feature_cycle`. The specification provides revised SQL joining on `cycle_events` with `event_type = 'cycle_start'` and `timestamp >= k_window_cutoff_secs`. This is a testable, bounded query consistent with the pre-cycle_events exclusion constraint.

### 3. SR-07 contract made explicit

The SCOPE described the `force=true` + purged-signals path but left the discrimination logic to the architect. The specification makes the contract precise: use the stored record's `raw_signals_available` field to distinguish "purged" from "never had signals," and accept `ERROR_NO_OBSERVATION_DATA` for the no-stored-record case regardless of whether signals were purged or never existed (OQ-01 captures the residual ambiguity).

### 4. K-window default pinned to 90 days

SR-04 flagged that the K-window depends on an unmerged GH #409 constant. Per the risk assessment recommendation, the specification pins a fallback default of 90 days as a named constant `DEFAULT_PENDING_REVIEW_WINDOW_DAYS`, not a magic number, with explicit reconciliation obligation at #409 merge time.

### 5. Five schema cascade touchpoints enumerated explicitly

AC-02b enumerates all five touchpoints from entry #3539: `CURRENT_SCHEMA_VERSION` constant, migration block, `create_tables_if_needed()` DDL, sqlite_parity test update, and column-count structural test update. The SCOPE's AC-13 only covered the migration test. The specification expands this to a named AC covering all five.

### 6. NFR-03: 4MB summary_json ceiling added

SR-02 (unbounded blob size) is addressed by a hard 4MB ceiling with a store-layer error return — not a panic. The SCOPE accepted the size risk as "well under 1MB"; the specification makes this a testable constraint.

### 7. AC-16 verification method is compile + round-trip test

SR-01 requires compile-time verification for serde completeness. The specification specifies both: (a) `cargo build` as the compile-time gate and (b) a JSON round-trip unit test on a fully-populated `RetrospectiveReport` to catch `#[serde(default)]` deserialization gaps not detected by compilation.

## Conflicts with SCOPE.md Worth Flagging

1. **query_log.feature_cycle does not exist** (OQ-02): The SCOPE's pending_cycle_reviews SQL references a column that is absent from the current schema. The specification works around this, but if the intent was to add `query_log.feature_cycle` as part of crt-033 or a concurrent feature, the architect must widen the scope or open a separate issue.

2. **CURRENT_DETECTION_RULES_VERSION in unimatrix-observe**: SCOPE mentions this as the source for `SUMMARY_SCHEMA_VERSION` but then resolves it as "no cross-crate coupling" using a plain integer in `cycle_review_index.rs`. The specification adopts the resolved position. No conflict, but the intermediate mention in SCOPE.md could confuse downstream agents.

3. **raw_signals_available flag update semantics (FR-05)**: The SCOPE states "return stored record with `raw_signals_available=false`" on the force=true+purged path. The specification clarifies that the response reports this flag as false but the stored row is NOT updated — the flag in the stored record is an attribute of the original computation, not of the retrieval. If the intent was to update the stored row, this is a divergence that the architect should resolve.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — entry #3619 (lesson: write_pool_server vs analytics queue from col-029) directly confirmed C-02. Entry #723 (spec/arch inconsistency lesson from crt-013) informed the level of precision applied to FR-05 and the SQL query for pending_cycle_reviews.
