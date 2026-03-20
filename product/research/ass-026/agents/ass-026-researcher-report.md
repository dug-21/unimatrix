# Researcher Report: ass-026-researcher

## Summary

Completed research spike for ASS-026: Power of Ten Protocol Governance Audit.

## Artifacts Produced

- `product/features/ass-026/SCOPE.md` — Full research output

## Sources Read

**Paper:**
- Gerard Holzmann, "The Power of Ten: Rules for Developing Safety-Critical Code" (JPL/NASA, 2006) — fetched from spinroot.com/gerard/pdf/P10.pdf. Summary content extracted; full verbatim text was not reproduced (copyright). Secondary source at cs.otago.ac.nz provided complete rule descriptions.

**Protocols read (all 4 files):**
- `.claude/protocols/uni/uni-design-protocol.md` (360 lines)
- `.claude/protocols/uni/uni-delivery-protocol.md` (573 lines)
- `.claude/protocols/uni/uni-bugfix-protocol.md` (431 lines)
- `.claude/protocols/uni/uni-agent-routing.md` (188 lines)

**Agent definitions read (all 16 files):**
- uni-scrum-master, uni-rust-dev, uni-validator, uni-tester, uni-architect
- uni-security-reviewer, uni-researcher, uni-risk-strategist, uni-pseudocode
- uni-specification, uni-synthesizer, uni-vision-guardian, uni-bug-investigator
- uni-docs, AGENT-CREATION-GUIDE.md, README.md

## Key Findings

1. Rules 1, 2, 5, 6, 9, 10 are well-covered by existing protocols. These are the structural rules about control flow, loop bounds, validation gates, output scope, authority chains, and zero-warning enforcement.

2. Rules 3, 4, 7, 8 have gaps or partial coverage. The most significant: no cognitive size limits on protocol files (Rule 4), no formal input validation requirement before agents begin work (Rule 7), and cross-reference depth is unbounded (Rule 8).

3. Top three high-priority recommendations: establish cognitive size limits for protocols and agents (REC-01), add formal input validation to agent definitions (REC-02), formalize the cross-reference budget per protocol phase (REC-03).

4. Three areas where current protocols are too strict: stewardship as a gate-blocking hard FAIL for read-tier agents (RELAX-01), 500-line file size limit as an absolute FAIL (RELAX-02), and the 2-iteration rework cap escalating directly to SCOPE FAIL without coordinator triage (RELAX-03).

5. The dual-mode uni-risk-strategist (288 lines, two modes in one file) is the strongest candidate for structural simplification.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for protocol governance -- no directly matching results; adjacent results found (token-budget constraints, stewardship patterns)
- Stored: entry #2674 "Power of Ten Analogical Mapping to Agent Protocol Governance" via `/uni-store-pattern`
- Stored: entry #2675 "Protocol and Agent Definition Size Metrics (as of 2026-03, no enforced limit)" via `/uni-store-pattern`
