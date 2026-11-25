# Gate 3c Report: vnc-050

> Gate: 3c (Risk Validation) — iteration 2 (re-run after rework 1)
> Date: 2026-11-25
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Behavioral-outcome proof (scope lens) | PASS | "`UNIMATRIX_BUNDLE` alone → observe events land" now proven by spawned hook (`observe-bundle.test.js`) |
| Risk mitigation proof | PASS | R-07.2 / R-07.3 exercised at hook entry point; all other risks unchanged from iteration 1 (cited, passing) |
| Test coverage completeness | PASS | Prior WARN (observe-latch env pair only) closed by bundle-driven cases |
| Specification compliance | PASS | AC-01..29 evidenced; AC-16 observe leg now covered; AC-13/AC-14 live checks post-delivery per HD 9 |
| Architecture compliance | PASS | ADR-004 #5826, ADR-006 #5829, HD 9-12; hook precedence matches resolveRemote/bridge |
| Integration tests | PASS | smoke 37/0; 12 suites 527/0; xfails pre-existing with GH issues; infra diff additions only; counts in RISK-COVERAGE-REPORT |
| Knowledge stewardship | WARN | Carried: tester `Queried:` records no query made |

## Detailed Findings

### Prior FAIL: behavioral outcome / R-07.3
**Status**: PASS
**Evidence**: Validator ran `node --test test/hook-client/observe-bundle.test.js`: 10 pass / 0 fail, rc=0. Clean HOME, all remote env vars scrubbed (lines 31-32). Cases: `bundle_only_pinned_fp_event_lands_at_bundle_observe_url`, `bundle_only_fp_null_ca_trust_event_lands` (NODE_EXTRA_CA_CERTS), negatives `wrong_fp_sends_nothing`, `fp_null_without_trusted_ca_sends_nothing`. Bearer token asserted (line 81). File not matched by `DEFAULT_EXCLUDE`, so it is selected by `npm test` (reported 1406 / 1404 pass / 0 fail / 2 skip).

### Prior FAIL: R-07.2 hook-level precedence
**Status**: PASS
**Evidence**: Suite "hook credential precedence = resolveRemote/bridge": store-only, env>store, bundle>store, bundle+URL and bundle+TOKEN conflict (nothing sent, exit 0, var named), empty-string vars no conflict (AC-28). Same winners as the bridge matrix.

### RISK-COVERAGE-REPORT
**Status**: PASS
**Evidence**: R-07 row (line 15) rewritten, names new tests, and notes the iteration-1 overstatement.

### Other checks
Unchanged from iteration 1 evidence. Rework touched only test files (new test + `stub-server.js` TLS option), with no production code change. No re-validation needed.

## Knowledge Stewardship
- Stored: nothing novel to store -- seam-test-as-entry-point-proof pattern already covered by vnc-047/#944 lesson.
