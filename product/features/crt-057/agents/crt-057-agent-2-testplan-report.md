# Agent Report — crt-057-agent-2-testplan (Stage 3a: Test Plan Design)

## Deliverables
`product/features/crt-057/test-plan/` — OVERVIEW.md + 13 per-component files (1:1 with the ARCH §3 / brief
Component Map). Every AC-01..AC-19 and every risk R-01..R-18 is mapped to a component test-plan file.

## MUST-COVER disposition (from spawn prompt)
1. **R-01 silent false negative** → `distill-before-purge.md`: per-loss-condition `search_complete==false`
   matrix (each of `elided_bytes>0`/`has_holes`/`Reconstructed`/`dropped_candidates>0` + OR), clean-Primary
   trustworthy negative, no-bare-boolean shape assertion, loss-on-match-hit.
2. **AC-19 ownership boundary** → `consumer-reconciliation.md` §AC-19: dedicated NEGATIVE schema-shape +
   code-path tests (no attribution/join/ledger field; no cross-GH-block synthesis). Not leaned on R-18.
3. **Deleted-helper migration (C-5)** → `orphan-deletion.md` + `backstop-reclaim.md`: delete
   purge_cycle_transcripts/clear_transcripts_for_feature/purge_held_for_feature (dead-code), re-home exhaustive
   `TranscriptRetention` onto backstops (no `_` arm), prove TTL/cap/session-close reclaim with content-free audits.
4. **RENAME of distill_before_purge** → `distill-before-purge.md` + OVERVIEW §4: `{NEW_NAME}` counted ×4;
   purge ×4 + attach-before-purge assertions REMOVED with rationale; fold-read ×4 PRESERVED. **Concrete name
   NOT yet available** — pseudocode/OVERVIEW.md not authored at plan time (OQ-A).
5. **`"summary"` drop consumer sweep** → `render-dispatch.md`: ERROR_INVALID_PARAMS ×4 loci, exact message,
   grep guard on reconciled consumers.
6. **Clock/window (R-05)** → `window.md` + `distill-before-purge.md`: fixed-offset boundary triples, ±120000ms
   / ±3-block default, ts:None byte_offset fallback, windowed-never-exact, never now_ts().
7. **Scoped filters (R-09)** → `transcript-scope.md`: AND-intersection, `{}`≡`match:".*"`, phase-ignores-window,
   omit=summary-only, invalid regex → error.
8. **AC-10 token reduction** → `cycle-review-handler.md`: populated fixture + ratio + vacuity guard (#3548).
9. **Residency/no-persistence (R-03)** → `attach-to-response-assembly.md` + `backstop-reclaim.md`: content-scan
   every sink incl. reclamation-without-review; loss carrier response-transient.
10. **Two-protocol lifecycle (R-04)** → `retro-lifecycle.md`: per-protocol e2e (delivery AND bugfix),
    merge→close→retro order, cycle(stop) buffer-inert, protocol-parity grep.

## Construction constraints encoded (whole matrix)
Synchronous-state negative assertions (#4879); path-proven four-site rows incl. memo-hit (#4452); fixed-offset
clock boundaries (#4195/#4236); populated-fixture ratio for AC-10 (#3548); per-protocol reachability (#5383);
extend existing `distill_handler.rs` fixtures — no isolated scaffolding (C-7).

## Integration suite plan (infra-001)
Run: smoke (mandatory) + tools, protocol, lifecycle, edge_cases, security (per suite-selection table). New
tests specified in OVERVIEW §6c across test_tools.py / test_lifecycle.py / test_security.py. Called out
(OVERVIEW §6d): R-01 matrix, R-05 skew, R-07 memo-hit, R-06 re-home, source-assertion migration are UNIT-only
— integration proves the MCP contract, not those matrices. R-04 is doc-grep + per-protocol simulated cycle.

## Open questions
- **OQ-A** `{NEW_NAME}` — resolve from pseudocode/OVERVIEW.md at Stage 3b/3c; substitute in source-assertion strings.
- **OQ-B** anchor/phase id representation — pseudocode detail; fixtures bind to whatever pseudocode fixes.
- **OQ-C** confirm infra-001 harness can read server SQLite/logs for the R-03 content-scan; else R-03 leans on
  the Rust `#[traced_test]` unit guard.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + 2× context_search — surfaced ADR-001..006 (#5433-5438),
  crt-052 seams (#4847/#4851/#4856/#5425), #4750/#4585/#4879/#4452/#3548/#5383 test-construction lessons.
  Applied to the risk→test mapping and construction constraints.
- Stored: nothing novel — Stage 3a test-plan design is a read/plan tier; all applicable patterns already exist
  in Unimatrix (the risk strategist already flagged the candidate cross-feature pattern as
  2nd-feature-confirmation-pending). No new fixture/harness technique was discovered here.
