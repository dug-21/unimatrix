# nan-024-agent-13-latency-info report

## Report
- D-096 applied: `benchmark-wiring.js` gains `--report-only` (or `LATENCY_REPORT_ONLY=1`): prints table + "REPORT-ONLY (D-096): breaches are informational", exits 0 on breach and on benchmark crash; default strict mode (exit 1 on breach) retained for local. D-094 budgets unchanged as reference.
- `nan-024-e2e.yml` latency-gate: job `continue-on-error: true`, step uses `--report-only`; artifact upload + job summary kept.
- test-plan/OVERVIEW.md, ci-and-docs.md: D-096 notes. RISK-COVERAGE-REPORT.md: AC-06/R-11/R-21/R-22 "evidenced, not enforced (D-096)"; full per-cell median+p95 tables for runs 45745887970 (A) and 45746445823 (B) added.
- Dispatch run 45809180517 (commit e37d94b4): first attempt parity-layer2 FAILED once (`test_transcript_delta_over_uds_merges_into_f2_buffer`, parity-layer2-uds.test.js:256, buffer content missing the streamed delta; passed in runs A/B and on rerun with no code change). Unmasked: apparent timing flake (async FNF delta vs compact read, no wait/poll); not caused by this change (no hook-client source touched). Recommend a follow-up to poll/await in that test. Rerun of failed job: success.
- Final: all jobs success, run concluded success.

## Knowledge Stewardship
- Queried: context_briefing -- only ADR-005 (#5898, superseded latency formula); no relevant hook-client patterns.
- Stored: nothing novel to store -- flake noted above is a test-owner follow-up.

Unimatrix entries consulted: #5898
