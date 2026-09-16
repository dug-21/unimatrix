# Gate 3c Report: vnc-049

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-09-16
> Result: PASS

OpenCode observation harness (C18) — behavioral-signal parity + local-model attribution.
Branch `feature/vnc-049` (#986), schema v32, `source_domain`/`model_id` persist opencode-only at ingest.

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Behavioral-outcome proof (scope lens) | PASS | Every scope-lens outcome maps to a passing test at the user path; AC-06 distinctness proven on the QUERIED stored row via the real assembled CLI→wire→INSERT→store→query path |
| 2. Risk mitigation proof | PASS | R-01..R-17 each map to passing coverage in RISK-COVERAGE-REPORT.md |
| 3. Test coverage completeness | PASS | Risk scenarios, integration (lifecycle/security/tools/protocol/edge_cases), and edge cases exercised |
| 4. Specification compliance | PASS | FR-01..12 implemented + tested; AC-01..07 discharged |
| 5. Architecture compliance | PASS | ADR-001 (opencode-only ingest stamp, fail-loud) + ADR-002 (model_id carrier) implemented as designed |
| 6. Knowledge stewardship | PASS | Tester report has `## Knowledge Stewardship` with Queried (context_briefing) + Stored (#5758) |
| INT-1: smoke gate | PASS | Independently re-ran `pytest -m smoke`: 37 passed, 0 failed (295s), incl. 2 new opencode smoke tests |
| INT-2: relevant suites run | PASS | lifecycle+security 136p/6xfail/1xpass; tools+protocol+edge_cases 264p/2xfail (all pre-existing markers) |
| INT-3: xfail hygiene | PASS | No new xfail/skip in `git diff main..HEAD` for infra-001 |
| INT-4: no test deletions | PASS | infra-001 diff is +366/-4; the 4 deletions are an import-line refactor, no test/assertion removed |
| INT-5: RISK-COVERAGE-REPORT integration counts | PASS | Report tabulates smoke 37, lifecycle+security 136, tools+protocol+edge_cases 264, 8 new tests |
| ADJ: AC-04/AC-07 harness-reach gap | PASS | Adequate non-proxy coverage; no fabricated harness proxy written |

## Detailed Findings

### 1. Behavioral-outcome proof (scope behavioral lens)
**Status**: PASS
Each row of the SCOPE behavioral lens maps to a passing test that drives the user path:
- **Local vs cloud distinct** (AC-06, gating): `test_local_vs_cloud_model_distinct_on_queried_row`
  (Rust listener) + `test_opencode_local_vs_cloud_model_distinct_on_queried_row` (infra-001) assert
  distinctness on the SELECTed stored row through the compiled binary — not in-memory ImplantEvent,
  not an injected dependency. Anti-tautology `test_opencode_model_id_survives_wire_to_insert` fails
  if the `?12` bind is dropped/homogenized/NULLed. I independently ran the Rust module: 7/7 pass.
- **source_domain=opencode + provider=opencode**: infra-001 + Rust
  `test_opencode_event_stored_source_domain_is_opencode` (positive + mandatory negative NOT
  `claude-code`); `test_opencode_provider_evidenced_by_stored_source_domain` (AC-02c).
- **All reachable events land / parity gap honest**: C1 plugin `entry`/`events`/`stop-guard`/
  `precompact` tests; SubagentStart injection recorded as measured gap.
- **Subagent aligned**: C1 `subagent.test.js` (parentID + validated agent on `extra.agent_type`,
  ordering, injection-not-attempted).
- **Installer + retrieval preserved**: C9 installer tests + infra-001 retrieval sentinel.

### 2. Risk mitigation proof
**Status**: PASS
RISK-COVERAGE-REPORT.md maps R-01..R-17 to named passing tests. No risk lacks coverage. R-04 cascade
gap (stale `test_schema_version_still_31` HEAD pin) was found and closed in-Stage-3c (test-code only);
`CURRENT_SCHEMA_VERSION == 32` and the pin now assert 32 (verified in source).

### 3. Test coverage completeness
**Status**: PASS
Rust workspace 6307 pass (report), JS 905 pass, infra-001 smoke 37 (re-verified) + lifecycle/security
136 + tools/protocol/edge_cases 264. Edge cases (no-model NULL, invalid charset, opencode-only stamp)
covered.

### 4. Specification compliance
**Status**: PASS
FR-01..12 traced. AC-01..07 verified per ACCEPTANCE-MAP; the two authoritative non-proxy ACs (AC-03,
AC-06) are asserted on the stored/queried row via the assembled path.

### 5. Architecture compliance
**Status**: PASS
`derive_source_domain` (listener.rs:3246) is opencode-only and fail-loud by construction — returns
`Some("opencode")` for provider opencode, `None` otherwise, NEVER `"claude-code"`. Matches ADR-001.
`resolve_model_id` validates+binds the carrier (ADR-002). No pipeline rewrite (NFR-02 honored).

### 6. Knowledge stewardship
**Status**: PASS
Tester report `## Knowledge Stewardship`: Queried `context_briefing`; Stored #5758
(topic testing, category pattern). Reason present.

### Integration validation
**Status**: PASS
Independently re-ran the mandatory smoke gate: `37 passed, 0 failed`. No integration tests deleted or
commented out (diff is additive + one import refactor). No new xfail markers. The 6 xfailed / 1 xpassed
in lifecycle+security and 2 xfailed in the regression suites are all pre-existing markers unrelated to
vnc-049; the single xpass is a signal to that marker's owner, out of scope here.

### Adjudication — AC-04 / AC-07 not driven end-to-end through infra-001
**Status**: PASS (adequate non-proxy coverage)
The plugin→CLI segment is a TS in-process artifact on OpenCode's Bun runtime and is not reachable from
infra-001 (a Python MCP+UDS harness that drives the CLI→store segment; documented in ARCHITECTURE
OVERVIEW §4.5). AC-04 subagent correlation and AC-07 observe-channel provenance are covered by the C1
node plugin tests (`subagent.test.js`: parentID + validated `session.agent` on `extra.agent_type`,
spoof rejection, conflicting-tool-args non-override, injection-not-attempted; `model.test.js` for the
model→`--model` mapping) plus the Rust crossing/structural tests. No fabricated harness proxy was
written — which is the correct anti-proxy discipline (a simulated plugin would be exactly the
seam/proxy false-confidence the risk strategy forbids, #907/#918/#930). AC-06 distinctness — the
gating outcome — IS proven on the stored queried row through the real assembled ingest path from the
wire onward, so this is a documented harness-reach boundary, not a coverage hole.

## Observations (non-blocking)

- **model_id charset breadth (WARN).** RISK-TEST-STRATEGY / SPECIFICATION state
  `^[a-z0-9_-]{1,64}$` for the attribution charset; the implemented carrier validates
  `^[a-z0-9._/-]{1,128}$` (`is_valid_model_id`) to admit real model ids like `ollama/qwen3-coder`
  (which require `/` and `.`). The security intent is preserved and tested — invalid values drop to
  NULL, never raw to SQL (`test_persist_rejects_model_id_violating_format`, infra-001
  `test_model_id_invalid_charset_not_persisted_raw`). Necessary and documented deviation; note for the
  spec/strategy to reconcile the stated regex with the shipped carrier charset in a future edit.

## Rework Required
None.
