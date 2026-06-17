# Agent Report: vnc-038-gate-3c (Validator, Gate 3c)

**Result**: PASS
**Report**: product/features/vnc-038/reports/gate-3c-report.md

## What I validated
Final risk-based validation for vnc-038. Independently verified (not just trusted the report):
- N=2 isolation proof GREEN for BOTH MCP and observe — read the recording-resolver test bodies; two distinct slugs/stores, sequence `[alpha,beta]`, both reach dispatch. Not vacuous.
- The three Default-arm tests are INVERTED to loud-404 assertions exercising the previously-broken path (#4452), not deleted.
- Local-binding guard (R-13/AC-10): path-hash is not a resolver key (`RouteError::UnknownProject`).
- Token redaction (R-14/AC-11): dual-stream absence + bundle round-trip.
- v:2 struct `{v,mcp_url,observe_url,token,fp}`, no base_url; JS decoder rejects v!==2.
- AC-12 router.rs=422; AC-13 public_url.rs clean.
- Integration smoke re-run independently: 24 passed, rc=0.
- #771 reconciled IN-DIFF (real-server.js +139 lines, registers slug + probes /v1/{slug}/observe); not xfail'd.
- Anti-stub/hygiene clean; the TODO/unsafe-word/unwrap grep hits are pre-existing or doc-comment text.

## Knowledge Stewardship
- Queried: read RISK-TEST-STRATEGY, SPECIFICATION, ARCHITECTURE+ADRs, ACCEPTANCE-MAP, RISK-COVERAGE-REPORT; cross-referenced #4452 (anti-vacuous-pass), #4974 (N=2 funnel), USAGE-PROTOCOL harness-scope rule.
- Stored: nothing novel to store — the validated patterns (N=2 funnel proof, invert-to-loud-error, harness-scope discharge at the reachable layer) already exist in Unimatrix; this was their application, not a new cross-feature pattern. No recurring gate-failure pattern surfaced (gate passed clean).
