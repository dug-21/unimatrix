# Agent Report: vnc-017-agent-3-risk

## Output

- Produced: `product/features/vnc-017/RISK-TEST-STRATEGY.md`

## Risk Summary

| Priority | Count |
|----------|-------|
| Critical | 3 |
| High | 7 |
| Medium | 3 |
| Low | 1 |
| **Total** | **14** |

## Key Findings for Design Leader Attention

**R-01 (Critical)**: SPECIFICATION FR-07 states `redirect_graph_edge` returns `Ok(true)/Ok(false)` — this contradicts the architecture's ADR-003 contract table which defines `Result<(), EdgeRedirectError>`. These are irreconcilable: `Ok(false)` cannot occur in a `Result<(), _>` type. Lesson #4042 is directly applicable. SPEC FR-07 must be corrected to match ADR-003 before delivery begins, or the implementer will face either a compile error or silent mis-handling of AC-09.

**R-02 (Critical)**: SPECIFICATION FR-04 specifies loop-level Supersedes exclusion; ARCHITECTURE ADR-002 specifies SQL-level exclusion. Both are documented as resolved (ADR-002 wins per architecture), but the SPECIFICATION's OQ-01 hedges by calling it "a design choice" — leaving the implementer without a clear single source of truth. ADR-002 must be stated as the implementation reference; SPEC FR-04 should be annotated as superseded.

**R-06 (Critical)**: The Contradicts bidirectional redirect with a mixed-status fan-in (some sources Active, some Quarantined) is the highest-complexity code path. The 4-row transaction inside `redirect_graph_edge` writes edges from the source to the new target. If the source is quarantined, the skip-with-warn correctly prevents the call — but the test scenarios in AC-08 only cover the fully-quarantined case. A mixed-batch test (one valid Contradicts, one quarantined Contradicts) is required and not currently specified.

## Open Questions Surfaced

1. **AC-09 counter semantics**: Does `redirected++` increment for a conflict-case `Ok(())`? ADR-003's table says `Ok(()) = redirected++` inclusive of conflicts, but the spec AC-09 says "or 0" — these must be reconciled by the Design Leader.

2. **All-skipped response text**: When all N incoming edges are skipped (all sources Quarantined), `total_found = N > 0` and `redirected = 0`. FR-10 appends "Redirected 0 incoming edges (0 failed, see logs)". This is technically correct but semantically misleading. No AC covers this case. Low priority but should be documented.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for lesson-learned graph edges redirect — found #4077 (direction semantics), #4042 (return contract table), #4076 (test omission gate failure)
- Queried: `/uni-knowledge-search` for risk patterns graph edge write transaction — found #4041 (write_graph_edge bool return divergence), #4417, #4435
- Queried: `/uni-knowledge-search` for source validation quarantined — found #4459 (pre-staged pattern)
- Queried: `/uni-knowledge-search` for SQLite INSERT OR IGNORE unique constraint — found #4396 (TOCTOU WAL race, informs R-14)
- Stored: nothing novel to store — R-01 divergence is feature-specific; no cross-feature pattern established yet
