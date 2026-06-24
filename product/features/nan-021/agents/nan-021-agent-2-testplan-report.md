# Agent Report — nan-021-agent-2-testplan (Stage 3a, Test Plan Design)

## Deliverables (absolute paths)
- /workspaces/unimatrix/product/features/nan-021/test-plan/OVERVIEW.md
- /workspaces/unimatrix/product/features/nan-021/test-plan/c1-https-standup.md
- /workspaces/unimatrix/product/features/nan-021/test-plan/c2-bridge-cycle.md
- /workspaces/unimatrix/product/features/nan-021/test-plan/c3-uds-baseline.md
- /workspaces/unimatrix/product/features/nan-021/test-plan/c4-workload-comparator.md
- /workspaces/unimatrix/product/features/nan-021/test-plan/c5-gate-wiring.md

## Risk coverage mapping (R-01..R-14)
All 14 risks mapped to component test homes (OVERVIEW §2). Critical-6 (R-01..R-06) concentrate in C4
(comparator/barrier) + C2 (bridge). The first-live-run field-by-field validation procedure + NFR-8
disposition authority documented as a non-skippable DELIVERY GATE (OVERVIEW §4, C4 plan).

## Integration suite plan
- Mandatory regression: `pytest -m smoke`. Recommended: `lifecycle`, `tools`, `protocol` (triage failures).
- New tests ADDED: C4 `parity_workload.py` comparator/barrier/manifest (sole net-new module); pytest
  orchestrator `test_parity_https_vs_uds_metricvector` + pure-function comparator/barrier tests; C2
  `cloud_cycle_gates` smoke gate fn; C5 gate-spine stub-drive.
- Headline live parity assertion runs in the release-gate Docker lane (`workflow_dispatch`/tag), NOT per-PR.

## Self-check (Stage 3a) — all pass
- [x] OVERVIEW maps R-01..R-14 to scenarios
- [x] OVERVIEW integration harness plan (suites + new tests)
- [x] Per-component plans match C1..C5 architecture boundaries
- [x] Every high-priority risk has a specific test expectation
- [x] Integration tests at component boundaries (each plan has an Integration boundary section)
- [x] First-live-run field-by-field procedure (18 fields) + disposition authority documented
- [x] AC-03 no-seed reachability audit (C3 plan, primary)
- [x] AC-02 bridge-carried-traffic (Mcp-Session-Id replay + SSE parse + JSON-only negative control)
- [x] Symmetric durability barrier (FR-10) + Docker false-green discriminator (AC-05)
- [x] All files under product/features/nan-021/test-plan/

## Open questions (for pseudocode / Stage 3b)
1. **OQ2 manifest format** — pin the on-disk format (JSON/py) the shell C2 gate reads and the Python C3
   driver executes, so neither leg hand-writes a parallel script. Tests assume one manifest object.
2. **OQ1 HTTPS review read-back seam** — confirm C2 emits `MetricVector(HTTPS)` to a `$SANDBOX` file (vs.
   store inspection); the comparator's correlation-token reject test assumes the file path + token shape.
3. **OQ3 exit codes** — confirm `exit-4`=image-unacquirable vs the Docker-absent `3` numbering against
   `release-gate-lib.sh` as-shipped; C5 stub-drive tests assert these literals.
4. **OQ5 barrier predicate source** — confirm the expected observe count comes from the manifest's
   observe-firing-tool-call count and the durability read is DIR-granularity (incl `-wal`).
5. **First-run artifact** — the comparator must emit BOTH raw parsed vectors (not just pass/fail) for the
   first-run field-by-field record; confirm pseudocode surfaces this artifact path.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search + context_get(#5293, #5291) — strong hits
  #5293 (ADR-003 comparison contract + first-run gate + disposition authority), #5291 (ADR-006 symmetric
  barrier), #5290 (ADR-005 false-green), #5286 (ADR-001 hybrid), #5258/#5192 (nan-019 stub-drive spine),
  #5265 (WAL durability), #5280/#830 (idle-eviction self-heal), #5129 (rmcp SSE), #5208 (cache-miss false-fail).
- Stored: nothing novel — the candidate pattern ("live-vs-live parity gate needs symmetric durability
  barrier + closed exclusion set + first-run field-by-field human gate") is single-feature (nan-021 only);
  per the 2-feature threshold it is not yet stored. Generalizable patterns already exist as the entries above.
