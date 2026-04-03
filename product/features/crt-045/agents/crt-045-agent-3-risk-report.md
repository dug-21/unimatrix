# Agent Report: crt-045-agent-3-risk

## Output

- `/workspaces/unimatrix/product/features/crt-045/RISK-TEST-STRATEGY.md` — written

## Risk Summary

| Priority | Count |
|----------|-------|
| High     | 3 (R-02, R-03, R-04) |
| Medium   | 4 (R-01, R-05, R-06, R-09) |
| Low      | 3 (R-07, R-08, R-10) |
| Total    | 10 |

## Critical Risks for Human Attention

**R-02 (Wired-but-unused, High):** The post-construction write-back may pass structural tests while the live search path remains broken. The AC-06 test MUST include a live `search()` call as the third assertion layer — handle inspection alone is insufficient. Entry #3935 confirms this failure mode caused a gate-3b rejection in crt-036.

**R-03 (Quarantined-entry fixture, High):** If the test seeds Quarantined entries or entries with no edges, `use_fallback=false` will be asserted on a structurally empty graph. The AC-06 `non-empty typed_graph` assertion becomes vacuously false. Fixture MUST use Active entries with at least one S1/S2/S8 edge (C-09, SPECIFICATION.md).

**R-04 (Rebuild error aborts, High):** `from_profile()` MUST return `Ok(layer)` with `use_fallback=true` on cycle-detected rebuild failure. A test covering the cycle degraded path is a non-negotiable gate check per entry #2758.

**R-09 (mrr_floor drift, Medium):** `mrr_floor=0.2651` is a point estimate from crt-042. Delivery agent must run `baseline.toml` eval before merging and confirm current MRR is at or above this threshold. If drifted, requires scope variance flag before changing the value.

## Scope Risk Traceability — All SR-XX Risks Traced

All six scope risks (SR-01 through SR-06) have rows in the Scope Risk Traceability table. SR-01 and SR-03 are resolved by architecture decisions (ADR-001, ADR-005). SR-02, SR-04, SR-05, SR-06 are mitigated by test requirements in the strategy.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for `lesson-learned failures gate rejection eval harness` — entry #3935 (structural coverage without production path causes gate-3b failure; directly elevated R-02 to non-negotiable). Entry #2758 (always confirm non-negotiable test presence before PASS). Entry #2661 (snapshot artifact completeness, IR-03).
- Queried: `/uni-knowledge-search` for `wired-but-unused anti-pattern` — entry #4100 (ADR-003 already in architecture); entry #3691 (cold-start RwLock guard pattern).
- Queried: `/uni-knowledge-search` for `risk pattern TypedGraphState rebuild eval layer` — entry #4096 (confirmed cold-start anti-pattern already stored).
- Stored: nothing novel to store — crt-045 risks are feature-specific. The broader pattern (eval layers must call the same init hooks as the background tick) is already captured in entry #4096.
