# Agent Report: 893-investigator (bug diagnosis, GH #893)

Diagnosis posted: https://github.com/dug-21/unimatrix/issues/893#issuecomment-4916387617

## Outcome
Root cause identified (HIGH confidence). Two-part:
1. The human-signed documented-exception escape valve designed by ADR-002 (#5313) / ADR-006 (#5314) — `blocks_c0_proof` — is consulted NOWHERE in the gate decision. `rollup()` (`harness/parity_outcome.py:346-366`) has docstring-only flag awareness (any INFRA_ERROR → ERROR/exit 7 over all dimensions); `assert_rollup()` (`harness/parity_matrix_support.py:111-117`) raises on ANY infra row, ignoring `documented_exceptions` (already computed by `evidence_table`, via a fragile string sniff) and `blocks_c0_proof` (already in every table row).
2. The seam test `test_matrix_orchestrator_seam_with_fixture_bundle` models the expected documented-gap live shape but stops before `assert_rollup` — the job disposition for the honest expected outcome was never tested.

The "matched exit 7=7" in the failure message is `exit_code` vs the `EXIT_INFRA` constant (a tautology), not a per-leg comparison; the per-leg analogue is the `measurable` flags, semantics fixed by ADR-006 (either-leg unmeasurable = documented limitation).

## Proposed fix (both options detailed on the issue; recommendation: Option A)
- **A (recommended)**: waive at `assert_rollup` only when EVERY infra row is (classifier-set `documented_exception=True` via D5 branch 1b) AND (`blocks_c0_proof=False` — requires the human-signed data flip of precompact in `parity_dimensions.py`) AND zero PARITY_FAIL. `rollup` untouched → artifact retains `verdict: ERROR`/exit 7 + `documented_exceptions` (honesty invariant preserved verbatim). Structural flag replaces the string sniff.
- **B**: tristate shell degrade in release.yml (jq over the emitted table); rejected as primary — exit-code swallowing false-green hazard, logic split out of the off-Docker seam. Job-level `continue-on-error` rejected outright (masks real REDs).

## Missing tests (off-Docker, NFR-1/NFR-2/AC-11)
Extend the seam test through `assert_rollup`; guard tests: undocumented infra raises, documented-on-blocking-dim raises, documented+PARITY_FAIL raises RED, flag set only by branch 1b, honesty pin (verdict ERROR retained when waived).

## Secondary
22 deprecated Node-20 action pins (ci.yml 3, release.yml 19) — recommend separate chore commit post-merge; out of scope here.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced ADR-002 #5313, ADR-006 #5314, patterns #5318/#5322 (the D5 always-INFRA fixture trap was a known hazard), lesson #5332; context_get #5313, #5314; context_search (ADR-006/OQ-2 precompact measurability).
- Stored: nothing novel to store — defect diagnosis lives on GH #893 (standing rule: bugs are GH issues, not lessons); disposition pends the human Option A/B checkpoint; generalizable lessons belong to the bugfix retro after the fix direction is chosen.
