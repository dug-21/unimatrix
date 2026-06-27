# Gate 3a Report: infra-003

> Gate: 3a (Component Design Review)
> Date: 2026-06-27
> Result: PASS
> Validator: infra-003-gate-3a

Test-only feature. Artifacts validated: `pseudocode/` (OVERVIEW + C1–C7 +
r15-invariant-update) and `test-plan/` (OVERVIEW + C1–C7) against
ARCHITECTURE.md (+ ADR-001..004), SPECIFICATION.md, RISK-TEST-STRATEGY.md,
ACCEPTANCE-MAP.md.

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Architecture alignment | PASS | 7 components map 1:1 to ARCH C1–C7 + R-15 delivery action; ADR-001 shell host, ADR-002 read-as-barrier, ADR-003 per-session MCP, ADR-004 single-restart all faithfully encoded |
| 2. Specification coverage | PASS | All FR-01..FR-08 and AC-01..AC-15 have corresponding pseudocode; no scope additions (N5 unwired, UDS not re-run, as required) |
| 3. Risk coverage (test plans) | PASS | All 18 risks (R-01..R-18) mapped to ≥1 scenario; every Critical/High has a named teeth or INFRA-discrimination test |
| 4. Interface consistency | WARN | Shared types/markers/primitives coherent across files; one non-load-bearing query-string typo in C4 prose (see findings) |
| 5. Knowledge stewardship | PASS | pseudocode (Queried), testplan (Queried + Stored-reason), risk (Queried + Stored-reason) all present and well-formed |

Load-bearing invariants (spawn-prompt focus) — all faithfully encoded, no contradictions:

| Invariant | Encoded in | Verdict |
|-----------|-----------|---------|
| Bidirectional 2×2, 4 distinctly-marked writes, each store holds ONLY its own marker both directions | OVERVIEW markers; C3 (obs-a→A, obs-b→B); C4 (mcp-a→A, mcp-b→B); C5 4 cells; C6 4 negatives; C7 per-surface 2×2 | PASS |
| Own-store positive marker absent AT deadline = INFRA (never RED); RED reserved for cross-store wrong-store presence; positive-gates-negative; NO aggregate du barrier | C5 (timeout→INFRA), C6 (negative_cell gating + RED on foreign present), C7 (RED dominates INFRA dominates GREEN); store_size demoted to liveness-only | PASS |
| Markers mutually NON-SUBSTRING; MCP read is `LIKE '%marker%'`; charset [a-z0-9-] | OVERVIEW (literals + runtime non-substring self-check + charset constraint); C6 LIKE read | PASS |
| Per-route MCP session isolation; handshake/session failure=INFRA, wrong-store marker=RED | C4 (distinct SID_A/SID_B, structurally non-crossable, handshake failure→INFRA) | PASS |
| Demonstrable TEETH: planted wrong-store marker exits RED | test-plan C6 `test_c6_marker_in_wrong_store_is_red`, C7 `test_c7_planted_leak_is_red` via stub seam | PASS |
| R-15 #815 invariant update keeps teeth, ships in-PR | r15-invariant-update.md (one-line allowlist edit, closed-set preserved, same PR, #815 cross-link) | PASS |

## Detailed Findings

### Check 1 — Architecture alignment
**Status**: PASS
**Evidence**: pseudocode/OVERVIEW Components table maps each file to its ARCH
component and ADR (C1→ADR-001, C2→ADR-004, C3/C5/C6→ADR-002, C4→ADR-003,
C7→ADR-002/003). Component boundaries match the ARCH decomposition exactly; the
orchestration order (C1→C2→derive markers→per-cell write+barrier→negatives→verdict)
mirrors ARCH "Data Flow". Technology choice (standalone shell gate at
`product/test/infra-001/scripts/multi-tenant-isolation-smoke.sh`, sourcing only
define-on-source libs, self-contained) matches ADR-001. The Integration Surface
(`parse_project_key`, `resolve_store`, `adapter_for`, `observations.topic_signal`,
`entries.content`/`topic`, `vol()`, `cloud-bundle-lib` content-read idiom) is
referenced consistently and the seam is exercised, not modified.

### Check 2 — Specification coverage
**Status**: PASS
**Evidence**: AC traceability fully closed — AC-01(C2), AC-02(C3), AC-03(C5),
AC-04(C6), AC-05(C7/C6), AC-06(C4), AC-07(C5), AC-08(C6), AC-09(C7), AC-10(C5/C7),
AC-11(C1/C5/C6), AC-12(C6), AC-13(C2 + r15 + git-diff), AC-14(C7 inspection),
AC-15(C4). NFRs honored: NFR-01 test-only/cumulative (no `crates/` touched —
pseudocode is shell-only), NFR-02 distroless via `vol` (no `docker exec`), NFR-03
sqlite3 host-provisioned hard-INFRA, NFR-04 N3 stays `partial`, NFR-05 marker
determinism + mutual non-substring. No scope additions: N5/#788 explicitly not
wired, no UDS behavioral probe, no parity-matrix shape — matching the NOT-in-Scope
list.

### Check 3 — Risk coverage (test plans)
**Status**: PASS
**Evidence**: test-plan/OVERVIEW "Risk-to-test mapping (all 18 risks)" gives each
of R-01..R-18 a covering component and a named key-teeth test. Critical risks
covered with fault-injection/discrimination teeth: R-01 `test_c4_handshake_failure_is_infra`,
R-02/R-03 `test_c7_planted_leak_is_red` / `test_c7_positive_gates_negative_per_direction`,
R-04 `test_c6_other_store_wal_copied`. The two-tier strategy (off-Docker stub-driven
gate-logic teeth + live point-in-time run) directly addresses the dominant
false-GREEN/vacuous-pass risk class and mirrors the #3624 lesson. R-15/R-16 correctly
classified as delivery-coordination obligations with concrete #815/#788 linkage.
Integration and edge-case scenarios present (WAL-on-other-store, missing-B-db INFRA,
crossed-session, substring cross-match, planted-leak teeth).

### Check 4 — Interface consistency
**Status**: WARN
**Evidence**: Shared script-globals (markers, slugs, POS_*/NEG_* result tokens),
the `read_marker` primitive (defined C6, wrapped by C5), `query_for` (defined C5,
reused by C6), and the four marker literals are coherent across all files. Verdict
state set NEG ∈ {ABSENT, RED, SKIPPED} is consistent OVERVIEW↔C6↔C7; C7's GREEN
assertion (`all NEG == ABSENT`) is sound because step 2 excludes any INFRA positive
before SKIPPED could survive.
**Issue (non-blocking)**: `pseudocode/c4-mcp-probe.md:138` (Data Flow prose) states
the read predicate as `content LIKE '%marker%' OR topic = '%marker%'` — the
`topic = '%marker%'` form has stray `%` wildcards inside an `=` exact match. The
**authoritative** query construction lives in `c5-read-as-barrier.md:79` and is
used by C6, both correctly `... OR topic = '<marker>'` (matching AC-07 canonical).
C4 issues no reads (C5/C6 own predicate construction), so this is a documentation
typo with no behavioral effect. Recommend a one-line correction in C4 prose to
`topic = '<marker>'` for fidelity; not a rework blocker.

### Check 5 — Knowledge stewardship compliance
**Status**: PASS
**Evidence**:
- agent-1 (pseudocode, read-only): `## Knowledge Stewardship` with `Queried:`
  (context_search → ADR #5335/#5342/#5343/#5344, pattern #5193) + deviation note.
- agent-2 (testplan/tester): `Queried:` (context_briefing/search → #5180/#5192/#5258/#238)
  + `Stored:` "nothing novel at plan stage -- reused patterns already in Unimatrix" (reason given).
- agent-3 (risk-strategist, active-storage): `Queried:` (#3624/#5180/#5177/#5296/#4708/#5193)
  + `Stored:` "nothing novel -- load-bearing patterns already exist; candidates below 2+-feature bar" (reason given).
All three blocks present with explicit reasons. (Synthesizer report carries no
stewardship block, but it is a synthesis-phase agent and not a 3a artifact producer;
ADRs are stored in Unimatrix per the cited IDs — not a 3a blocker.)

## Non-blocking notes (carried for delivery, not gate failures)

- **Resolved drift (verified)**: the synthesizer report flagged a SPEC contradiction
  (FR-06.2 / ubiquitous-language / AC-03 / AC-07 saying at-deadline = RED). The
  **current** SPECIFICATION.md reads INFRA in all four locations, consistent with
  ADR-002, ARCHITECTURE C5/C7, RISK R-05, ACCEPTANCE-MAP, and all pseudocode. The
  most load-bearing invariant (own-store timeout = INFRA, never RED) is therefore
  consistent across every source and artifact. No contradiction remains.
- **ALIGNMENT-REPORT.md is stale** (old single-direction / `eval-baseline` framing)
  per the synthesizer note — a vision-alignment artifact outside the 3a check set;
  flagged for tidy-up, not blocking.

## Rework Required

None. (Optional polish: correct the C4 Data Flow query-prose typo noted above.)
