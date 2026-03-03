# Component 4: Provenance-Boost — Test Plan

## Unit Tests (confidence.rs)

### T-PB-01: PROVENANCE_BOOST constant value
- Assert PROVENANCE_BOOST == 0.02.

### T-PB-02: PROVENANCE_BOOST less than co-access max
- Assert PROVENANCE_BOOST < 0.03.

### T-PB-03: Score difference is exactly PROVENANCE_BOOST (AC-09)
- Compute rerank_score(sim=0.8, conf=0.6) for both entries.
- lesson-learned entry gets + PROVENANCE_BOOST.
- convention entry gets + 0.0.
- Assert difference is exactly 0.02.

### T-PB-04: Provenance boost is additive tiebreaker
- Two entries with identical similarity and confidence.
- Assert lesson-learned ranks higher after boost.

## Integration Tests

### T-PB-05: MCP context_search path (AC-09)
- Insert a lesson-learned entry and a convention entry with equal stored confidence.
- context_search query matching both.
- Assert lesson-learned ranks first.

### T-PB-06: UDS ContextSearch hook path (AC-09)
- Same entries as T-PB-05.
- ContextSearch via UDS path.
- Assert lesson-learned ranks first.

## Code Review Checks

### T-PB-07: Single constant source
- Verify `PROVENANCE_BOOST` is imported from `confidence.rs` at both application
  sites (tools.rs and uds_listener.rs). No magic number `0.02` literals.

### T-PB-08: Stored invariant preserved
- After applying provenance boost in search, verify:
  - No writes to EntryRecord.confidence
  - `W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST = 0.92` unchanged
