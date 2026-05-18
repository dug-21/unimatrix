# Agent Report: vnc-016-agent-3-risk

## Output

Produced: `product/features/vnc-016/RISK-TEST-STRATEGY.md`

## Risk Summary

| Priority | Count |
|----------|-------|
| Critical | 4 |
| High | 3 |
| Medium | 2 |
| Low | 1 |
| **Total** | **10** |

## Top Concerns for Synthesizer

**R-01 (Critical)**: The integration test can pass vacuously in two independent ways before the SQL fix is applied: (a) `feature_cycle` omitted from entry A's `context_store` call, or (b) `agent_id` is not "human" (Restricted trust → `feature_entries` skipped silently per ADR-007). The test must fail against the pre-fix commit. The implementer must verify fail-first behavior manually or via a pre-fix CI run.

**R-04 (Critical)**: The Rust negative companion test (`empty_without_feature_entry`) is load-bearing — it is the only test that validates the `WHERE fe.feature_id = ?1` JOIN scoping. Without it, a "return all stale edges regardless of cycle" regression passes the positive test. The positive test alone is insufficient.

**R-03 (Critical)**: The Rust unit test positive assertion must check: `is_ok()`, `len() == 1`, and `[0] == (A_id, B_id)`. A weakened assertion (e.g., `!is_empty()` only, or `unwrap_or_default()`) replicates the silent-swallow bug inside the test itself.

**R-05 (High)**: The `unwrap_or_else` pattern in `tools.rs:2169-2177` is a permanent residual risk — the Rust unit test is the sole regression guard for future column renames or schema drift. The integration test is too far from the failure point to isolate SQL errors. A follow-up issue to promote the warn! to ERROR-with-metric is recommended but out of scope for vnc-016.

## Key Phase 2a Discoveries Incorporated

- Trust-level requirement for `context_store` (R-06): `agent_id="human"` is required for `feature_entries` to be written. Not in SCOPE.md or SCOPE-RISK-ASSESSMENT.md — surfaced in architecture phase.
- Confirmed single call site for `query_stale_prerequisite_edges_for_cycle` (tools.rs:2165) — bounds the blast radius of the SQL fix to one location.
- Negative companion requirement for Rust unit test is a new risk (R-04) beyond SR-01/SR-02 scope risks.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for `lesson-learned failures gate rejection` — found #4177 (tautological assertion, bugfix-505): informed R-03 assertion-structure risk.
- Queried: `/uni-knowledge-search` for `risk pattern silent failure false negative test` — found #4311 (gate-prerequisite test ordering for silent-fallthrough): informed R-01 fail-first coverage requirement.
- Queried: `/uni-knowledge-search` for `SQLite column name query integration test regression` — found #4445 (SQL column alias mismatch, vnc-016 pattern already stored): confirmed SR-01 pattern exists; no new store needed.
- Queried: `/uni-knowledge-search` for `context_cycle_review detection pipeline trust level feature_entries` — found #103 (ADR-007: FEATURE_ENTRIES Trust-Level Gating): directly informed R-06 (the trust-level silent-skip risk).
- Queried: `/uni-knowledge-get` on #4445, #4177, #103, #4311 for full content.
- Stored: nothing novel — #4445 already captures the SQL column alias / unwrap_or_else pattern for vnc-016. The trust-level + feature_entries silent-skip pattern (R-06) is ADR-007 specific to crt-001 and does not generalize beyond that ADR's documented behavior. No new cross-feature pattern identified.
