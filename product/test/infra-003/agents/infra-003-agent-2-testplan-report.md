# Agent Report — infra-003-agent-2-testplan (Stage 3a, Test Plan Design)

## Deliverables
- `test-plan/OVERVIEW.md` — strategy, 18-risk mapping, integration harness plan, #815 invariant-update plan, #788 adoption note, AC-01…AC-15 coverage
- `test-plan/c1-preflight.md` … `c7-verdict.md` — per-component plans (1:1 with C1–C7)

## Strategy in one line
Two-tier test-of-a-test: (1) off-Docker **stub-driven gate-logic test** proves the
verdict truth table and the teeth (planted leak → RED, own-store timeout → INFRA),
(2) live Docker run proves point-in-time isolation. Tier 1 is mandatory — a correct
container never triggers RED, so teeth cannot be proven by the live run alone (#3624).

## Teeth / INFRA-vs-RED emphasis (per spawn directive)
- Fault injection: `test_c6_marker_in_wrong_store_is_red` + `test_c7_planted_leak_is_red`
  plant `B-*` in A's store (and symmetric) → RED at cross-store cell — proves non-vacuous.
- INFRA-vs-RED both directions both surfaces: own-store timeout = INFRA never RED
  (C5 `test_c5_own_timeout_is_infra_not_red`); cross-store presence = RED (C6/C7);
  handshake/session failure = INFRA per route (C4 R-01/R-17).
- #815: invariant update planned IN-PR (add to `KNOWN_SMOKE_SCRIPTS` in
  `release-gate-bundle-static-test.sh`), with a teeth-retention check (still fails on
  a synthetic unaccounted script). Verify-by-name marker `[*-smoke] ALL GATES PASSED`.
- #788: R-16 is a delivery-coordination action (durable #788 adoption comment); N3 stays partial.

## Open questions (carry to Stage 3b/3c)
1. **Stub seam shape (load-bearing).** The C5 read primitive and C6 two-store read
   must be routed through an injectable seam (env-overridable argv, like
   `SMOKE_STORE_SIZE_CMD`/`SMOKE_SHELL_CAPTURES`) so the C7 teeth tests run without
   Docker. Stage 3b must expose this seam or tier-1 testing is impossible.
2. **Terminal-marker tag.** Confirm the gate's `log()` prefix is a `*-smoke` token
   (e.g. `[isolation-smoke]`) so the anchored grep in `release-gate-lib.sh:59` matches.
3. **MCP write verb.** Default `context_store` (single-row marker); `context_correct`
   only if a prior entry exists (else silent no-op, R-02).
4. **Deadline value** for the read-as-barrier — mirror the ~10s store-grow wait in
   `docker-http-posture-smoke.sh` (arm64-CI headroom); deadline-poll, not fixed sleep.

## Self-check
All 18 risks mapped; every Critical/High has a teeth or INFRA test; all 15 ACs map to
a component plan; component plans match the C1–C7 architecture boundaries; integration
harness section present; #815 invariant-update plan with teeth-retention present.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` + `context_search` — surfaced infra-003
  ADRs (#5335/#5342/#5343/#5344) and the false-green/verify-by-name patterns
  (#5180 self-skip-must-fail, #5192 sourceable gate spine, #5258 stub-driven Docker
  gates), plus cumulative-test convention (#238). Applied directly to the two-tier
  stub-driven strategy and verify-by-name contract.
- Stored: nothing novel at plan stage — reused patterns already in Unimatrix
  (#5180/#5192/#5258). A planted-leak isolation fault-injection pattern may be worth
  storing at Stage-3c retro if it generalizes.
