# Test Plan: C2′ — `scripts/bridge-cycle-driver.js` (extended)

Covers the HTTPS-leg-emit half of **R-03 (Critical)**, the bridge-in-path integration risk
(D-2), and the fork-smell guard **R-16 (Low)**. The JS bridge driver is EXTENDED to also issue
retrieval (`context_search`/`context_lookup`/`context_get`) + `context_briefing` `tools/call`
envelopes THROUGH the existing `mcp-bridge.js` and emit them into `dimension_bundle`. NO net-new
transport/cert/spawn code (C-2/AC-11) — that is a fork smell to FLAG, not to add.

Surface under test (extended):
- `bridge-cycle-driver.js <projectHash> <manifestPath> --bridge <p> --witness <p>` — additional
  `tools/call` envelopes for retrieval + briefing; stdout JSON now carries the dimension bundle

Tier: **C (live, `@pytest.mark.parity`)** — exercised through the Docker HTTPS smoke; the
bridge-carriage assertion is part of the matrix orchestrator (`test_https_uds_parity.md`). No
standalone JS unit framework — the JS leg is proven live + by the cross-language bundle contract
(`parity_bundle_contract.md`).

## Test Expectations

### New tools/call envelopes ride the bridge (R-03 emit half, AC-02/AC-05)
- The driver issues `context_search`/`context_lookup`/`context_get` (retrieval, D1) and
  `context_briefing` (proactive, D4) as `tools/call` envelopes and emits `result_ids`+`scores`
  (retrieval) and `briefing_ids`+`briefing_scores`+`injection_set` (briefing) into the
  `dimension_bundle` under the registry capture_keys. Verified live in the matrix orchestrator
  (assert the emitted bundle carries non-empty retrieval/proactive captures with the documented shape).
- Double-capture: the driver captures retrieval + briefing TWICE (`capture_2`) in the same drive
  so the intra-stability check has both captures (R-07 source on the HTTPS leg).

### Bridge-in-path carriage (D-2 Integration Risk — load-bearing)
- The new retrieval/briefing calls go THROUGH the shipped `mcp-bridge.js` over pinned HTTPS
  (SSE + `Mcp-Session-Id` replay), NEVER a direct `mcp_url` POST. The matrix orchestrator's
  bridge-carriage assertion (via `bridge-witness.js`) confirms the bridge actually carried the
  new calls (`test_https_uds_parity.md`). A direct POST would be a fork-path that drifts from
  production (R-16).

### #5298 frames for observe-driven dimensions (R-03 scenario 2 — HTTPS half)
- Observe-driven dimensions (behavioral, precompact, analytics-cycle, isolation-write) emit the
  byte-identical #5298 11-frame sequence on the `/observe` route; NO rework/legacy frame variant.
  Cross-leg byte-identity vs the UDS leg is asserted in the matrix orchestrator.

### Fork-smell guard (R-16 scenario 2 — review-flag)
- Review assertion (Stage 3c): NO net-new transport/cert/spawn/framing code added to the driver —
  it reuses `mcp-bridge.js`/`cert-pin.js`/`credstore.js`/`bundle.js`/`init.js` as-is. Any net-new
  such code is a fork smell to FLAG (not a silent add). Covered by the `git diff` confinement
  check (`cloud-cycle-lib.md` / AC-11).

## Coverage Requirement
The new retrieval/briefing `tools/call`s are carried by the shipped bridge (in-path, not a direct
POST — proven by the witness), emit the documented retrieval/proactive captures (with `capture_2`
for intra) into the dimension bundle, and the observe-driven dimensions emit the #5298 11-frame
sequence with no rework/legacy variant. No net-new transport/cert/spawn code (R-16). These are
proven LIVE via the matrix orchestrator + the cross-language bundle contract test.
