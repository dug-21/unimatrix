# Agent Report: crt-038-agent-2-testplan

**Phase**: Stage 3a — Test Plan Design
**Feature**: crt-038 — conf-boost-c Formula and NLI Dead-Code Removal

---

## Deliverables Produced

| File | Description |
|------|-------------|
| `product/features/crt-038/test-plan/OVERVIEW.md` | Overall test strategy, risk-to-test mapping, integration harness plan, AC-12 eval gate procedure |
| `product/features/crt-038/test-plan/effective-short-circuit.md` | Component 1: FusionWeights::effective() — three new unit tests, one test update, edge cases |
| `product/features/crt-038/test-plan/config-defaults.md` | Component 2: InferenceConfig defaults — two test updates, optional new test, failure modes |
| `product/features/crt-038/test-plan/dead-code-removal.md` | Components 3+4+5: structural absence verification, retained symbol checks, build sequence |

---

## Risk Coverage Mapping

| Risk | Priority | Test Plan Coverage |
|------|----------|--------------------|
| R-01 — effective() short-circuit omitted/misplaced | Critical | 3 unit tests in effective-short-circuit.md |
| R-02 — eval run before AC-02 | Critical | OVERVIEW.md ordering enforcement section |
| R-03 — baseline measured on wrong path | Critical | OVERVIEW.md AC-12 eval gate section (baseline validity analysis) |
| R-04 — shared helpers deleted | High | dead-code-removal.md Step 2 (retained symbol verification) |
| R-05 — write_edges_with_cap retained | High | dead-code-removal.md Step 1 absence grep + clippy gate |
| R-06 — residual symbol references | High | dead-code-removal.md full symbol checklist |
| R-07 — NliStoreConfig partial deletion | High | dead-code-removal.md Step 1 NliStoreConfig/nli_store_cfg greps |
| R-08 — process_auto_quarantine call site | Med | dead-code-removal.md incremental build + call site inspection |
| R-09 — formula test message not updated | Med | effective-short-circuit.md test update section |
| R-10 — operator overrides silently lost | Low | config-defaults.md optional new test |
| R-11 — stale sequencing comment | Low | dead-code-removal.md Step 1 R-11 grep |

All 11 risks have documented test or verification coverage.

---

## Integration Harness Plan Summary

**Minimum gate**: `pytest -m smoke` (mandatory)
**Required suites**: `tools`, `lifecycle`, `edge_cases`
**Not required**: `security`, `contradiction`, `confidence`, `volume`
**New integration tests needed**: None — formula correctness is tested by unit tests + AC-12 eval gate; dead-code removal has no MCP-visible effect

---

## AC-12 Eval Gate Key Findings

1. **Baseline validity confirmed**: The ASS-037/039 harness runs offline against a snapshot DB, bypassing `FusionWeights::effective()` entirely. Baseline MRR=0.2911 (FINDINGS.md) was measured on direct conf-boost-c profile weights. After AC-02 is implemented, `effective(false)` with `w_nli=0.0` returns weights unchanged (short-circuit) — matching the profile exactly. The baseline IS valid provided AC-02 is in place.

2. **MRR value discrepancy**: SPECIFICATION.md and RISK-TEST-STRATEGY.md cite the gate as MRR ≥ 0.2913, but FINDINGS.md reports conf-boost-c MRR=0.2911 on 1,585 scenarios. Stage 3c tester must resolve this before declaring AC-12 pass or fail. Open question flagged in OVERVIEW.md.

3. **Eval runner location**: No standalone eval runner script is visible in `product/research/ass-037/` — only profile TOMLs, snapshot DB, and scenario files. Stage 3c must locate or reconstruct the MRR computation script. The harness infrastructure exists; the orchestration script may be missing or not versioned.

---

## Open Questions for Stage 3c

1. **MRR gate value**: Is the correct gate 0.2913 (SPECIFICATION.md) or 0.2911 (FINDINGS.md)? Check against the original AC-12 authoring source.
2. **Eval runner script**: Locate the script that computes MRR against `scenarios.jsonl` + `snapshot.db` + profile TOML. Check `product/research/ass-037/harness/` more thoroughly — the glob only showed TOML and DB files, not Python scripts.
3. **13th and 14th nli_detection test names**: SPECIFICATION.md notes "the remaining 2 of the 13 declared test functions" beyond the named 11. Stage 3b must identify these from the live source before deleting. Stage 3c must grep-verify all 13 are absent.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 17 entries; top matches: #3949 (composite guard predicates independent negative tests), #2970 (ADR-002 crt-024 apply_nli_sort removal), #3985 (NLI infrastructure audit verdict). Entry #3949 directly informed the approach to R-01's three separate test scenarios.
- Queried: `mcp__unimatrix__context_search` for "crt-038 architectural decisions" — returned ADRs #4005, #4006, #4007 covering effective() short-circuit, NliStoreConfig deletion, and implementation ordering.
- Queried: `mcp__unimatrix__context_search` for "scoring formula testing patterns" — returned #2972 (ADR-004 formula as extractable pure function), #724 (behavior-based ranking tests), #749 (calibration scenario builder).
- Stored: entry #4009 "ASS-037/039 eval harness bypasses FusionWeights::effective() — baseline MRR is on direct profile weights" via context_store (pattern). Novel finding from reading the harness profile TOMLs and FINDINGS.md — this distinction is not captured in existing entries.
