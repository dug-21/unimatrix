# Agent Report: nan-011-agent-2-testplan

Phase: Stage 3a — Test Plan Design
Agent ID: nan-011-agent-2-testplan

## Output

Six test plan files written to `product/features/nan-011/test-plan/`:

| File | Scope | ACs | Risks |
|------|-------|-----|-------|
| OVERVIEW.md | Strategy, risk-to-test mapping, integration harness plan | all | all 15 |
| readme-vision.md | README + PRODUCT-VISION.md | AC-01 to AC-05 | R-10, R-11, R-13 |
| config-toml.md | config.toml full rewrite | AC-06 to AC-09 | R-01, R-02, R-03, R-06 |
| skills-audit.md | All 14 skill files + npm copy | AC-10 to AC-12, AC-16, AC-17 | R-05, R-07, R-11, R-12, R-14 |
| protocols-dir.md | protocols/ directory | AC-14, AC-15 | R-04, R-09, R-11 |
| npm-package.md | npm package update | AC-13 | R-08, R-12, R-15 |

## Risk Coverage

All 15 risks from RISK-TEST-STRATEGY.md are mapped to at least one test scenario in
the plan files. The three Critical risks (R-01, R-02, R-04) each have multiple
verification steps with explicit pass criteria.

## Integration Harness Plan

No integration suites apply. nan-011 introduces no Rust code, no MCP tools, and no
binary changes. The infra-001 harness and cargo/pytest are not relevant. Stage 3c
executes only the shell verification commands in the per-component test plan files.

## Design Decisions

- config-toml.md orders its steps so TOML parse validity runs first — a parse failure
  makes field-by-field value checks secondary and should be flagged as a Critical blocker.
- skills-audit.md documents the three specific confirmed violations from ADR-004 by
  file and approximate line number so the tester can spot-check fixed locations directly.
- protocols-dir.md verifies source files before copies (Step 2 before Step 3) to
  distinguish "copy made before source was fixed" from "source correction missed."
- npm-package.md leads with a toolchain pre-check to handle SR-02 (absent Node.js)
  without blocking the other 16 ACs.
- All grep commands use absolute paths (`/workspaces/unimatrix/...`) to be safe against
  working-directory resets between bash calls.

## Open Questions

1. The SPECIFICATION.md file exceeds token limit at full read (>10k tokens). The FR-1.1
   vision statement verbatim text was read from the first 150 lines. Tester must read
   SPECIFICATION.md FR-1.1 directly at execution time to get the complete approved text
   for the character-level diff in AC-01.

2. ARCHITECTURE.md Open Question 1 (skills/ directory at repo root) — whether this
   directory currently exists — is unresolved. The test plan (npm-package.md Step 3)
   verifies the path exists after delivery; it does not pre-check the pre-delivery state.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — found entries #1258 (workflow-scope
  delivery pattern), #1259 (workflow-only scope procedure), #555 (cross-file consistency
  procedure). All three are relevant; no novel pattern emerged from this feature that
  is not already captured.
- Queried: mcp__unimatrix__context_search "nan-011 architectural decisions" (category:
  decision, topic: nan-011) — found ADRs #4265, #4266, #4267; retrieved all three for
  test plan design. Directly informed config-toml.md and protocols-dir.md structure.
- Queried: mcp__unimatrix__context_search "documentation validation testing patterns
  shell verification" — found #555, #1259, #2928; no documentation-only feature testing
  procedure exists; the closest is #1259 which covers workflow-scope design, not test
  execution.
- Stored: nothing novel to store — shell verification test plan structure for
  documentation-only features is specific to this feature type; the closest stored
  pattern (#1259) already covers workflow-scope delivery; the two-pass grep pattern
  is ADR-004 (nan-011-specific, not a cross-feature pattern). No new cross-feature
  testing procedure emerged.
