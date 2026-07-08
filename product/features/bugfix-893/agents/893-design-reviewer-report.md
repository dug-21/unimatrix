# Agent Report: 893-design-reviewer (design review of the proposed fix, GH #893)

Review posted: https://github.com/dug-21/unimatrix/issues/893#issuecomment-4916438081

## Assessment: APPROVED WITH NOTES
Option A is the ADR-sanctioned mechanism; Option B correctly rejected. The four-way waiver conjunction (documented_exception AND NOT blocks_c0_proof AND zero PARITY_FAIL AND per-row over all infra) is provably conservative — no vector silently GREENs a real PARITY_FAIL, undocumented infra, or a documented exception on a still-C0-blocking dim.

## Findings
| # | Finding | Severity |
|---|---------|----------|
| A1 | Flipping precompact `blocks_c0_proof=True→False` is a C0-proof-scope change (dual-purpose flag: release-gate waiver AND C0 #5304 flip scope). Needs explicit maintainer sign-off + a new ADR (supersedes 2026-06-25 all-five-True). Bugfix must not self-authorize. | **Blocking (process)** |
| B1 | Two decision sites diverge by design (rollup stamps verdict:ERROR; assert_rollup passes the job). Artifact can't distinguish waived-ERROR from failed-ERROR — the bug's own failure mode relocated to the artifact. Single-source the waiver in one pure helper in parity_outcome.py, record `gate_disposition`/`waived` in the table, have assert_rollup consume it. | Non-blocking (strongly recommended) |
| B2 | `documented_exceptions` (string sniff) and the waiver would be two recomputations of the same conjunction — collapse to one via B1. | Non-blocking |
| C1 | `suites/test_parity_dimensions.py:118` `test_blocks_c0_proof_all_six_true` BREAKS on the flip — missed in the investigator's blast radius. Update it to encode the signed exception (four block, precompact does not); the updated test becomes the in-repo record of the sign-off. | Non-blocking (must-fix-in-PR) |

## Confirmations (no change)
- rollup(): keep logic (verdict ERROR/exit 7 — ADR-006 honesty invariant); fix only its false docstring (line 350). The waiver belongs in assert_rollup + the single-source helper, NOT rollup.
- Key the waiver on the `blocks_c0_proof` flag, not the id `"precompact"` (hard-coding the id = SR-05 second-source drift ADR-001 forbids). Generalization is latent and safe.
- Pin (test) that `documented_exception` originates only from classify branch 1b; spoof surface is minimal (comparators emit PARITY_*, never INFRA_ERROR, so never enter the waived infra list).
- AC-10 respected: waiver widens no EXCLUDED set; gated on human-signed data.
- Structural flag replacing the string sniff fits ADR-001 single-source (pattern #5318 already flagged the detail-string as fragile).

## Required tests (off-Docker)
Investigator's five + (6) artifact self-description pin (waived table carries gate_disposition==PASS/waived:[precompact] while verdict=="ERROR"/exit 7) + extend the seam test through assert_rollup.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing (bugfix-893) + context_get #5313 (ADR-002 §4 blocks_c0_proof escape valve, rollup ERROR>RED>GREEN), #5305 (ADR-001 registry data-only single-source), #5314 (ADR-006 measurable=False documented limitation, never rounded up, human-signed exception), #5381 (ADR-008 standing-disposition), #5322 (rollup-ERROR-dominates D5 fixture trap — known hazard), #5318 (branch-1b ordering, fragile detail-string). Applied to confirm the mechanism, the sign-off requirement, and the honesty verdict.
- Declined to store: nothing novel now — bugs are GH issues not lessons (standing rule); disposition pends the A1 checkpoint. Generalizable lesson (designed escape valve shipped un-wired; two decision sites over one result set must be single-sourced + artifact self-describing) belongs to the bugfix-893 retro. Follow-up owned by the architect after sign-off: store the precompact documented-exception ADR via context_correct (supersedes the 2026-06-25 all-five-True confirmation).
