# Agent Report — build-request-sentinel (vnc-027 / #680)

Agent: vnc-027-agent-3-build-request-sentinel
Component 6 / ADR-004 §1 / FR-27 / AC-08 / Risk R-11. Merge step 4.

## Summary

Retired standalone PreToolUse observation at the client level.
`buildCycleEventOrFallthrough` in `build-request-tools.js` now returns a `null`
no-send sentinel on every non-cycle path (non-cycle tool name, missing
`tool_input`, failed `validateCycleParams`) instead of `genericRecordEvent`.
Cycle frame construction (cycle_start / cycle_phase_end / cycle_stop), all stderr
diagnostics, and the F-02 exact-equality security gate are fully preserved. Only
PreToolUse changed; PostToolUse / PostToolUseFailure / generic fallthrough and
SubagentStart are untouched.

## 1. Files modified

- `/workspaces/unimatrix/packages/unimatrix/lib/hook-client/build-request-tools.js`
  (3 fallthrough returns: `genericRecordEvent(...)` → `null`)
- `/workspaces/unimatrix/packages/unimatrix/test/hook-client/build-request.test.js`
  (updated 3 existing fallthrough assertions to expect `null`; added a new
  `PreToolUse no-send sentinel (ADR-004 §1)` describe block — sentinel matrix,
  F-02 defense-in-depth, near-miss/suffixed, malformed tool_input no-throw,
  valid cycle frame parity, bare-tool promotion no-mutation, PostToolUse scope
  guard)

Committed as `b7c779e3` on `feature/vnc-027`. Only these two files were staged;
other agents' working-tree changes left untouched.

## 2. Tests

`node --test test/hook-client/build-request.test.js`: **90 pass / 0 fail**.
Size gate (`node test/check-hook-client-size.js`): **PASS** —
stripped 68399/100000, raw 110832/160000. `build-request-tools.js`: stripped
9445 B, raw 12946 B (added comments are comment-stripped; raw has wide headroom).

## 3. Issues / blockers

**Cross-component dependency (NOT a defect in this component) — owned by
component 10, parity-corpus-uds.** `parity-layer1.test.js` auto-discovers every
corpus subdir and asserts `buildRequest` output matches `expected-request.json`.
The 7 retired PreToolUse observation cases now diverge (they were RecordEvent
frames, now `null`), exactly as ADR-004 §4 mandates the corpus exclude:

- cycle-near-miss, cycle-near-miss-suffixed, cycle-invalid-type,
  cycle-invalid-topic, cycle-missing-tool-input, ptu-pre-non-cycle,
  alias-before-tool

Required corpus-side actions (parity-corpus-uds, same merge step 4): delete those
case dirs, update `MANIFEST.json` (`case_count` + `arms` map — audited by
`test_manifest_case_count_matches_disk`), and remove
`cycle-near-miss` / `cycle-near-miss-suffixed` / `cycle-invalid-type` from the
hard-coded R-01 `REQUIRED` inventory list in `parity-layer1.test.js`. Until then,
Layer 1 parity (AC-01) fails by design. No action available within this
component's scope (`build-request-tools.js` + `build-request.test.js`).

## 4. Confirmations

- Null sentinel on ALL non-cycle paths: yes (3 returns; verified by sentinel
  matrix + F-02 tests).
- Cycle interception preserved: yes (start/phase-end/stop frames byte-unchanged;
  `cycle-mcp-context-promotion` golden still passes).
- F-02 exact-equality gate intact: yes (`evil_context_cycle_bypass` →
  `null`/sends nothing; only exact `context_cycle` /
  `mcp__unimatrix__context_cycle` intercepted).
- Size gate passes: yes (stripped 68399/100000, raw 110832/160000;
  build-request-tools.js stripped 9445 / raw 12946).

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_search (pattern + decision) and context_get #4809
  -- found ADR-004 (#4811), the install-surface-registration pattern (#4809), and
  the transport-asymmetry notes (#4798/#4703). Applied ADR-004 §1 exactly.
- Stored: entry #4822 "Retiring a build-request arm via null sentinel is a
  two-file change: builder + parity corpus (auto-discovered)" via /uni-store-pattern
  (topic hook-client) — captures the Layer 1 auto-discovery trap and the
  MANIFEST/inventory follow-on, invisible from the builder source alone.
