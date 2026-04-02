# Agent Report: crt-038-agent-3-risk

## Output

- Produced: `/workspaces/unimatrix/product/features/crt-038/RISK-TEST-STRATEGY.md`

## Risk Summary

| Priority | Count |
|----------|-------|
| Critical | 3 |
| High     | 4 |
| Med      | 2 |
| Low      | 2 |
| **Total** | **11** |

## Top Risks for Human Attention

**R-01 / R-02 / R-03 form an interlocked critical chain.**

- R-01: The `effective()` short-circuit (AC-02) must be implemented before the eval gate. If it is absent or misplaced (after the nli_available branch rather than before it), every search query silently applies w_sim≈0.588 / w_conf≈0.412 instead of the specified conf-boost-c formula. No runtime error surfaces.

- R-02: If delivery runs the AC-12 eval before AC-02 is complete, the gate comparison is against the wrong scoring path. The PR must include the git commit hash in the eval output so reviewers can confirm ordering.

- R-03 (open question from spec writer): ADR-001 states the ASS-039 eval used `nli_enabled=true, w_nli=0.0` (effective(true) path, no re-normalization). If this is confirmed, the 0.2913 baseline is valid. If the eval instead used `nli_enabled=false` on a build without the short-circuit, the baseline was measured on the re-normalized formula (w_sim'≈0.588, w_conf'≈0.412) — a formula that crt-038 intentionally avoids. Delivery must verify the ASS-039 harness configuration or commit hash before treating 0.2913 as a valid AC-12 gate. This is the single most important open question for the feature.

- R-04: The three shared helpers (`write_nli_edge`, `format_nli_metadata`, `current_timestamp_secs`) imported by `nli_detection_tick.rs` at line 34 must survive removal. `write_edges_with_cap` (callerless after the removal) must be deleted to avoid a clippy warning (R-05) that would fail AC-11.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_search` for "lesson-learned failures gate rejection" — entry #2758 (gate-3c symbol retention failure) elevated R-06 to High.
- Queried: `mcp__unimatrix__context_search` for "risk pattern" (category=pattern) — entry #4003 confirmed the re-normalization risk chain (R-01/R-02/R-03).
- Queried: `mcp__unimatrix__context_search` for "FusionWeights effective scoring formula weight normalization" — entries #4003 and #4005 confirmed architecture alignment.
- Queried: `mcp__unimatrix__context_search` for "dead code removal surgical deletion compilation breakage shared symbols" — entry #3256 informed R-04 framing.
- Stored: nothing novel to store — the R-01/R-02/R-03 eval-gate-depends-on-correct-scoring-path pattern is already captured in entry #4003. If delivery confirms the resolution after shipping, a cross-feature pattern entry on "eval gate invalidated by wrong scoring path" may be warranted at retro time.
