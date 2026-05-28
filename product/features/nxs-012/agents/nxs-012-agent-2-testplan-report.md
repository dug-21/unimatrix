# Agent Report: nxs-012-agent-2-testplan

## Task
Design per-component test plans for nxs-012 export/import complete persistent state coverage (Stage 3a).

## Output Files

| File | Lines | Content |
|------|-------|---------|
| test-plan/OVERVIEW.md | ~100 | Test strategy, risk-to-test mapping (24 risks), integration harness plan |
| test-plan/format-types.md | ~80 | 12 unit tests for 3 row structs + 3 ExportRow variants |
| test-plan/export-functions.md | ~80 | 18 unit tests + 3 integration tests for export functions |
| test-plan/import-inserters.md | ~75 | 14 unit tests for 3 inserter functions |
| test-plan/import-pipeline.md | ~75 | 6 unit + 14 integration tests for pipeline orchestration |
| test-plan/skip-quarantined.md | ~80 | 5 unit + 14 integration tests for --skip-quarantined feature |

## Risk Coverage Summary

All 24 risks mapped to specific tests:

| Priority | Risk Count | Scenarios | Status |
|----------|-----------|-----------|--------|
| High | 10 (R-01, R-02, R-14, R-15, R-16, R-17, R-18, R-19, R-20, R-23) | 33 | Full coverage planned |
| Medium | 7 (R-03, R-04, R-05, R-08, R-09, R-10, R-11) | 18 | Full coverage planned |
| Low | 7 (R-06, R-07, R-12, R-13, R-21, R-22, R-24) | 14 | Full coverage planned |

## Integration Harness Plan

- Smoke gate: mandatory `pytest -m smoke` -- confirms no regression from schema changes
- Suite selection: `lifecycle`, `edge_cases` -- relevant to storage/schema changes
- New infra-001 tests: none needed -- export/import are CLI subcommands, not MCP tools; Rust integration tests provide complete coverage
- Full suite: recommended pre-merge

## Open Questions

None. All architectural decisions resolved via ADRs. All acceptance criteria have defined verification methods.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- found ADR-001 (FK ordering), ADR-002 (format version), ADR-008 (skip-quarantined), testing convention #238, test-support pattern #747
- Stored: nothing novel to store -- test plan follows established patterns from prior export/import features (nan-001, nan-002) with no new testing infrastructure or techniques discovered
