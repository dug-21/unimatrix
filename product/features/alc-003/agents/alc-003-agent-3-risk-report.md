# Agent Report: alc-003-agent-3-risk

**Role**: Architecture-Risk Strategist
**Mode**: architecture-risk
**Output**: `product/features/alc-003/RISK-TEST-STRATEGY.md`

## Completion Status

DONE. RISK-TEST-STRATEGY.md written to feature root.

## Risk Summary

| Priority | Count | Risk IDs |
|----------|-------|----------|
| Critical | 2 | R-01, R-02 |
| High | 3 | R-03, R-04, R-05 |
| Medium | 6 | R-06, R-07, R-08, R-09, R-10, R-11, R-13 |
| Low | 2 | R-12, R-14 |
| **Total** | **14** | |

(Note: R-13 counted in Medium; total unique risks = 14.)

## Top Risks by Severity

**R-01 (Critical/High)** — Pre-flight blast radius skipped. If Phase 0 measurement is not the first implementation act, behavioral and fixture failures become indistinguishable. This is the sequencing risk that underpins everything else. ADR-005 mandates it; the test strategy requires a three-phase gate to verify it was honoured.

**R-02 (Critical/High)** — `require_cap()` call site update not atomic. All 12 tool handlers must be updated simultaneously with the signature change. Any handler that compiles but was not updated will either fail the build (best case) or silently use stale logic (worst case). The entry #317 (ToolContext pattern) documents this exact failure class from prior features.

**R-03 (High/Med)** — Startup refusal missing from one transport path. `tokio_main_daemon` and `tokio_main_stdio` are maintained separately. If the env-var read is added to one but not the other, a deployment mode bypasses the identity requirement. Subprocess-level tests for both paths independently are required.

**R-05 (High/Low)** — `enroll_session_agent()` bypasses protected-agent guard. The function calls `store.agent_enroll()` directly. Protected-name validation must be in `SessionIdentitySource::resolve()` — not assumed from a higher-level caller. Case-insensitive check for `"system"` and `"human"` is the specific gap to verify.

**R-10 (Med/High)** — Breaking change: operator upgrades without env var. Likelihood is High because this is a hard breaking change with no fallback. The error message quality is the only mitigation. NFR-02 requires the message to name the variable and the required action without producing a Rust panic trace.

## Self-Check

- [x] Every risk has a Risk ID (R-01 through R-14)
- [x] Every risk has at least one test scenario
- [x] Severity and likelihood assessed for each risk
- [x] Integration Risks section present and non-empty
- [x] Edge Cases section present and non-empty
- [x] Failure Modes section describes expected behavior under failure
- [x] RISK-TEST-STRATEGY.md written to feature root (not in test-plan/)
- [x] No placeholder risks — all risks specific to alc-003 architecture
- [x] Security Risks section present — untrusted inputs and blast radius assessed
- [x] Scope Risk Traceability table present — all SR-01 through SR-08 have rows
- [x] Knowledge Stewardship report block included

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection" — entry #1609 (ServiceError variant names, crt-014), entry #1928 (daemon-mode fixture pattern). Both relevant.
- Queried: `/uni-knowledge-search` for "risk pattern capability identity authentication" — entry #261 (AuditSource behaviour differentiation), entry #317 (ToolContext pre-validated context). Entry #317 directly informs R-02.
- Queried: `/uni-knowledge-search` for "outcome rework registry enrollment capability" — no directly applicable rework patterns found.
- Queried: `/uni-knowledge-search` for "startup failure daemon stdio process identity env var" — entry #1839 (ADR: UNIMATRIX_CLIENT_TOKEN), entry #1928 (daemon fixture). Both referenced.
- Queried: `/uni-knowledge-search` for "breaking change migration deployment upgrade" — entry #376 (DDL-before-migration ordering failure). Informs R-10.
- Stored: nothing novel to store — no new cross-feature pattern visible from alc-003 alone. Pattern #317 already covers the stale-caller-at-refactoring risk. If delivery confirms an underestimated blast radius, a lesson-learned entry should be filed post-delivery.
