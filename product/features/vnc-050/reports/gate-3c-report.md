# Gate 3c Report: vnc-050

> Gate: 3c (Risk Validation)
> Date: 2026-11-25
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Behavioral-outcome proof (scope lens) | FAIL | "`UNIMATRIX_BUNDLE` set alone → observe events land" has no entry-point test |
| Risk mitigation proof | FAIL | R-07 (Critical) scenario 3 unexercised; report claims "Full" |
| Test coverage completeness | WARN | All other RTS scenarios mapped; observe-latch drives env pair, not a bundle |
| Specification compliance | PASS | AC-01..29 evidenced; AC-13 live / AC-14 post-delivery per HD 9 |
| Architecture compliance | PASS | ADR-004 (#5826), ADR-006 (#5829), HD 9-12 reflected; gate 3b found no drift |
| Integration tests | PASS | smoke 37/0; 12 suites 527/0; no xfail added; no tests deleted (infra diff +378/-0) |
| npm test script change | PASS | `run-hook-client.js` sets + orphan-suite audit; only benchmark-spawn excluded (CI-excluded, own job) |
| Knowledge stewardship | WARN | Tester: `Queried:` says context_briefing not called; `Stored:` declined with reason |

## Detailed Findings

### Behavioral-outcome proof
**Status**: FAIL
**Evidence**: SCOPE-RISK-ASSESSMENT row "`UNIMATRIX_BUNDLE` set alone, bridge launched (CI / routine) | MCP initialize + tools/call succeed; observe events land". SR-15: "Both paths consume the bundle; test each from the env var."
- MCP half is proven: `headless-smoke.test.js` (spawned `mcp-bridge` with `UNIMATRIX_BUNDLE`), `compat-matrix.test.js` AC-24 cell.
- Observe half is not. Under `packages/unimatrix/test`, `UNIMATRIX_BUNDLE` appears only in `headless-smoke.test.js`, `compat-matrix.test.js` (bridge only) and `ca-trust-tls.test.js:167` (a conflict-exit negative test). No spawned hook (`index.js` / `config.js`) test sets `UNIMATRIX_BUNDLE` and asserts that an event reaches the bundle's `observe_url`. `config.test.js` has no bundle-env case. `observe-latch.test.js` (`read_bundle_n_events_one_warning`) sets `UNIMATRIX_REMOTE_URL`/`UNIMATRIX_REMOTE_TOKEN`, not a bundle.
- `remote-resolve.test.js` "bundle only -> both consumers from bundle" is a seam test one layer below the entry point. Its own header says "Spawned-process re-proofs live in the bridge suites", and those cover the bridge only.
- The code is wired (`lib/hook-client/config.js:276` calls `resolveRemote(env, …)`), so this is a missing test, not a known defect. It is the exact silent-telemetry-drop divergence that SR-15/R-07 exist to catch.

### Risk mitigation proof
**Status**: FAIL
**Evidence**: RTS R-07 (Critical) scenario 2 requires that every precedence case also runs end-to-end through `mcp-bridge.js` and through `config.js`. Scenario 3: "With `UNIMATRIX_BUNDLE` only, an observe event arrives at the bundle's `observe_url` using the bundle's trust." RISK-COVERAGE-REPORT marks R-07 "Full" and cites "spawned bridge/config.js", but no spawned config.js/hook bundle test exists. All other risks (R-01..R-06, R-08..R-22) have cited passing tests that match the RTS scenarios. R-19 is a WARN carried from gate 3b.

### Test coverage completeness
**Status**: WARN
**Evidence**: Integration covers the cross-component risks (R-02/03/04/05/13/15/17) at a real daemon in `test_headless_access.py` (9 tests, 3 green reruns). The AC-20 write and FR-39 baseline diff gaps were closed in follow-up commits. Rust: 7240 passed, 0 failed. JS: 1393 passed, 2 skipped; both skips are justified (a Windows-only test, and an env-gated test that was run separately with the baseline binary).

### Specification compliance
**Status**: PASS
**Evidence**: The ACCEPTANCE-MAP rows are all verified in the RISK-COVERAGE-REPORT AC table. AC-16's MCP leg is proven. Its "UNIMATRIX_BUNDLE alone attaches" wording is satisfied for the bridge. The observe leg is the gap above.

### Architecture compliance
**Status**: PASS
**Evidence**: Single resolver (`remote-resolve.js`) is shared by the bridge and config.js (SR-05/13). There is a separate CA TLS branch (`tls-guard.js`, ADR-006 #5829). Read-token learning suppression is tested at the sinks (ADR-004 #5826). Observe 403 marker latch follows HD 10. Pin tests are unchanged vs main.

### Integration test validation
**Status**: PASS
**Evidence**: The RISK-COVERAGE-REPORT lists counts per suite. smoke 37/0. All 10 xfails are pre-existing, with GH#405/#291/#406/#111. `git diff main...HEAD -- product/test/infra-001` shows 378 insertions and 0 deletions: one new suite, one new security test, and harness extensions. Not re-run by the validator; tester evidence accepted.

### npm test script change
**Status**: PASS
**Evidence**: `"test"` now runs the default hook-client set, the parity-layer2 set, and the package, opencode-plugin and audit sets. `DEFAULT_EXCLUDE = [/^parity-layer2/, /^benchmark-spawn\.test\.js$/]`. parity-layer2 runs as its own set, and the audit set fails if any suite is neither selected nor excluded. The only suite not run is benchmark-spawn, which is CI-excluded.

### Knowledge stewardship
**Status**: WARN
**Evidence**: `vnc-050-agent-4-tester-report.md` has the block. Its `Queried:` entry records that no query was made. `Stored:` is declined with a reason.

## Rework Required

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| No entry-point proof that `UNIMATRIX_BUNDLE` alone drives observe (R-07.3, SR-15, scope lens) | uni-tester (or uni-js-dev) | Add a spawned-hook test (pattern: `observe-latch.test.js` `run()`, clean HOME, no remote.json, no URL/TOKEN env). Set `UNIMATRIX_BUNDLE` with `observe_url` pointing at a stub. Assert that the event arrives at the bundle's `observe_url` with the bundle token. Add a pinned (fp) case and a `fp:null` CA case (local CA via `NODE_EXTRA_CA_CERTS`). Ensure it is selected by `npm test`. |
| R-07.2 config.js end-to-end precedence | uni-tester | Through the spawned hook, cover bundle vs env pair vs store, asserting the same winner as the bridge. |
| RISK-COVERAGE-REPORT R-07 overstated | uni-tester | Update the R-07 row with the new tests, and re-run `npm test`. |

## Knowledge Stewardship
- Stored: nothing novel to store -- the "seam test cited as entry-point proof for second consumer" pattern is already covered by the vnc-047/#944 lesson referenced in the gate definition.
