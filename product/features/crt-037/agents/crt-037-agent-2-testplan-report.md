# Agent Report: crt-037-agent-2-testplan

**Phase**: Stage 3a — Test Plan Design
**Feature**: crt-037 (Informs Edge Type — Cross-Feature Institutional Memory Bridge)

---

## Deliverables

All test plan files written to `product/features/crt-037/test-plan/`:

| File | Lines | Scope |
|------|-------|-------|
| `OVERVIEW.md` | 165 | Strategy, risk mapping table, integration harness plan, CI grep gates |
| `graph.md` | 99 | RelationType extension, penalty isolation, string round-trips |
| `graph_ppr.md` | 117 | PPR mass flow, positive_out_degree_weight, Direction::Outgoing regression guard |
| `config.md` | 121 | Serde defaults (AC-07–09), validate() bounds (AC-10–12), domain string placement |
| `read.md` | 110 | Directional dedup, bootstrap exclusion, R-01 write+readback, R-09 non-normalization proof |
| `nli_detection_tick.md` | 384 | All 14 test sections; AC-13–AC-23 fully enumerated |

---

## Risk Coverage Mapping

| Risk ID | Priority | Coverage |
|---------|----------|----------|
| R-01 | Critical | `read.md`: DDL inspection note + write+readback tests |
| R-02 | Critical | `graph_ppr.md`: AC-05 by specific node index; Direction::Outgoing CI grep gate |
| R-03 | Critical | `nli_detection_tick.md` §2: 5 independent guard negative tests + neutral=0.5 boundary + dual test |
| R-04 | High | `nli_detection_tick.md` §1: Phase 8 only / Phase 8b only cross-contamination tests |
| R-05 | High | `nli_detection_tick.md` §4: AC-20 weight value check; feature-cycle propagation |
| R-06 | High | `nli_detection_tick.md` §5: full cap → zero Informs; partial cap → fills remainder |
| R-07 | High | `nli_detection_tick.md` §2, §3: neutral boundary + FR-11 entailment exclusion |
| R-08 | High | `nli_detection_tick.md` §8: category non-match; empty pairs; AC-22 CI grep gate |
| R-09 | High | `read.md`: reverse lookup returns false (critical ADR-003 test) |
| R-10 | High | `graph.md`: Informs-only graph → FALLBACK_PENALTY; find_terminal_active empty |
| R-11 | Medium | `nli_detection_tick.md` §5, §6: invariant property test across input sizes; cap=0 no panic |
| R-12 | Medium | `nli_detection_tick.md` §7: log field assertions for all three configurations |
| R-13 | Medium | `nli_detection_tick.md` §8: in-memory category map note; latency regression |
| R-14 | Medium | `nli_detection_tick.md` §9: AC-21 CI grep gate |
| R-15 | Low | `nli_detection_tick.md` §12: finite weight assertion |
| R-16 | High | `nli_detection_tick.md` §13: existing test suite pass-through; Supports count baseline |
| R-17 | Medium | `nli_detection_tick.md` §10, `read.md`: two-tick dedup; pre-filter loaded on second tick |
| R-18 | Low | `config.md`: boundary values at exactly 0.0 and 1.0 for both fields |
| R-19 | Medium | `nli_detection_tick.md` §3: FR-11 Gap-2 mutual exclusion — only Supports written |
| R-20 | Critical | `nli_detection_tick.md`: all AC-13–AC-23 fully enumerated; delivery process hard-stop noted |

All 20 risks have explicit test coverage. No gaps.

---

## Integration Harness Plan Summary

**No new infra-001 tests needed.** The Phase 4b/8b detection path is not observable through
the MCP JSON-RPC interface at the assertion granularity required by AC-13–AC-23. All 11
mandatory tick integration tests are Rust tests calling `run_graph_inference_tick` directly.

Suites to run in Stage 3c: `smoke` (mandatory gate), `tools`, `lifecycle`, `confidence`.

---

## AC Verification Coverage

All 24 acceptance criteria (AC-01 through AC-24) are mapped to named test functions across
the five component test plan files. AC-21 and AC-22 are shell grep gates, not runtime tests.
AC-23 spans `read.md` (store layer) and `nli_detection_tick.md` (tick layer).

---

## Open Questions

None. All four OQs from IMPLEMENTATION-BRIEF.md (OQ-1 through OQ-4) are resolved via ADRs.
OQ-S1 (DDL cleared) and OQ-S2 (NliScores.neutral confirmed direct logit) are pre-delivery
cleared — no test-plan open questions remain.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 17 entries; ADR-001 (#3942),
  ADR-002 (#3939), ADR-003 (#3940) retrieved in full; PPR regression trap (#3896) and
  Direction::Outgoing trap (#3744) incorporated into graph_ppr.md test design.
- Stored: entry #3943 "Per-guard negative test for multi-predicate composite guards in NLI
  detection tick" via context_store (category: pattern, topic: testing).
