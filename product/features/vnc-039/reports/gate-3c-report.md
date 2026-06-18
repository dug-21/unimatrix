# Gate 3c Report: vnc-039

> Gate: 3c (Final Risk-Based Validation) — RE-VALIDATION (rework iteration 1)
> Date: 2026-06-18
> Result: PASS
> Branch: main @ 5879d77f (fix commit on top of b8dec3f8)
> CI reality: JS-client-only (`node --test`); Rust validation = protocol gates + infra-001 harness (not GH CI).

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Risk mitigation proof (R-01..R-17) | PASS | Every risk maps to ≥1 named passing test; critical suites re-run green at HEAD; live tier honestly LIVE-PENDING (#779) |
| 2. Test coverage completeness | PASS | All Phase-2 risk→scenario mappings exercised; integration counts present in RISK-COVERAGE-REPORT |
| 3. Specification compliance | PASS | All FR/AC implemented; live-tier ACs honestly tiered, not falsely greened |
| 4. Architecture compliance | PASS | Component structure (C1–C5), ADR-001..005, integration surface honored |
| 5. Integration test validation | PASS | Smoke 24/24 (reported); JS harness is feature's integration coverage; no tests deleted/commented (removals are renames/relocations to out-of-tree store) |
| 6. Live-vs-stub honesty (R-03 / #4796) | PASS | Live-pending legitimate (no reachable endpoint); stub provably pinned; #779 OPEN with checklist |
| 7. Knowledge stewardship | PASS | Tester report has full block (Queried + nothing-novel with reason) |
| **8. Suite-green accuracy** | **PASS** | Prior FAIL RESOLVED at 5879d77f — size-gate 21/21; full suite 990·989·0·1 (exit 0), RISK-COVERAGE-REPORT reconciled |

## Prior Failure Resolution (Re-validation Focus)

The Gate 3c REWORKABLE FAIL was a single issue: `size-gate.test.js` header meta-test asserted the stale backstop string `"160,000"` while the human-approved cap raise (#775) corrected the source header to `"180,000"`, producing a deterministic 1-test failure at the prior HEAD (b8dec3f8) that the RISK-COVERAGE-REPORT's "0 fail / 21 PASS" claim contradicted.

**Resolved at HEAD (5879d77f):**
- `packages/unimatrix/test/hook-client/size-gate.test.js` — `test_header_documents_human_decision_rule` now asserts `src.includes("180,000")` (diff verified). This is in lockstep with `check-hook-client-size.js` line 9 (`BACKSTOP : raw … <= 180,000 bytes`) and line 35 (`BACKSTOP_LIMIT = 180000`). The sibling `test_limits_are_decimal` already asserted `=== 180000`. Both meta-assertions now track the same source value.
- `node --test test/hook-client/size-gate.test.js` → **tests 21 · pass 21 · fail 0**, reproduced across two runs.
- `RISK-COVERAGE-REPORT.md` reconciled: headline size-gate row now reads "PASS 21/21 (2 lockstep meta-assertions …)"; §Test Fixes #1 rewritten to cover both meta-assertions; the FLAGGED "stale doc comment" note marked RESOLVED (comment + meta-test now in lockstep, nothing deferred).
- Full suite re-run at HEAD: **990 tests · 989 pass · 0 fail · 1 skipped (Windows-gated), exit 0** — now matches the report headline exactly. No deterministic failure remains.

## Detailed Findings

### 1. Risk Mitigation Proof — PASS
Every R-01..R-17 maps to ≥1 named passing test (R-14 is a documented process checkpoint → NO FLIP). Critical suites re-run at HEAD:
- **R-01/R-02 (Critical, trust boundary):** `mcp-bridge-tls.test.js` **7/7** — live handshake against a real `https.createServer` self-signed leaf via production `HttpSession`+`Lifecycle`: good-pin connects + round-trips + token reaches server; wrong-pin fail-loud exit 1 with server seeing zero new request (token never on wire), loud expected-vs-presented error naming both fps, token absent from error; non-vacuous negative control (`test_bridge_negativeControl_wouldLeakIfPinNoOp`); per-socket re-pin (`secureConnects === socketsOpened`, `agent:false`), mid-session cert-swap → socket #2 rejected no-token-flushed. Live recipe (#4970), not shape assertions.
- **R-06/AC-08d (Critical):** `config.test.js` **67/67 (+1 Windows skip)** — file-mode observe over local pinned HTTPS: `pinnedFp` populated from `fingerprint`, good-pin lands on `observe_url`, wrong-pin fail-open exit 0 no-token, no UDS fallthrough. Behavioral, not field-presence.
- **R-17 (identity stability):** `mcp-bridge.test.js` **38/38** — byte-identical `Mcp-Session-Id` + `clientInfo.name` across requests; distinct projects → distinct identity.
- **R-04 (SSE):** `mcp-bridge-sse.test.js` **2/2**.

### 2. Test Coverage Completeness — PASS
RISK-COVERAGE-REPORT.md maps all Phase-2 scenarios; integration counts present (smoke 24/24 + the JS stub/LIVE-TLS/Layer-2 harness). No risk lacks coverage.

### 3. Specification Compliance — PASS
All FR-01..FR-27 / AC-01..AC-13 implemented. Live-tier ACs (AC-03/04/04b/05/06/12 live portions) honestly marked `[stub/local] PASS + LIVE-PENDING (#779)` with a tier table; none falsely reported validated-live. Hybrid flip-bar NO FLIP correct (SSE source-verified required; surface within human-approved budget — #775).

### 4. Architecture Compliance — PASS
C1 credstore, C2 mcp-bridge + sub-modules (all <500-line cap), C3 subcommand routing, C4 initRemote rewire, C5 config repoint. ADR-001 pinned-flush/per-socket re-pin, ADR-003 `projectHash` keying, ADR-004 canonical schema + behavioral validation, ADR-005 Scope B independence — all honored. Store keyed by `projectHash`.

### 5. Integration Test Validation — PASS
- Smoke gate (infra-001): 24 passed / 0 failed (per spawn + prior report) — mandatory minimum met. The Python integration harness is not present in this checkout; vnc-039 touches no server code, so no re-run was required to confirm the resolved JS-side failure.
- vnc-039 is JS-client-only; the feature's integration coverage is the JS harness (provenance-pinned stub, LIVE self-signed TLS, Layer-2 real-server parity). Correct per OVERVIEW §4.
- **No tests deleted or commented out as coverage loss.** Diff `edf200f7..HEAD` over `test/` is +2223/−219. The removed `it(...)` cases in `config.test.js` (`test_file_without_remote_key`, `test_malformed_settings_json`, `test_settings_local_remote_yields_http_mode`, `test_worktree_finds_main_root_settings`) are renamed/relocated to their out-of-tree-store equivalents (`test_malformed_store_json`, `test_store_remote_yields_http_mode`, `test_worktree_finds_main_root_store`, plus a large net AC-08c/08d/R-07/R-13/R-15 expansion). The removed `init-remote.test.js` in-tree `settings.local.json` 0600/verbatim/token-safety cases are replaced by out-of-tree equivalents (`test_initBundle_writesStoreOutOfTree0600`, `test_initBundle_repoTreeFreeOfTokenBearingPath`, etc.). These removals are the deliberate C4/C5 credential-relocation (ADR-003), not lost coverage. No xfail markers added.

### 6. Live-vs-Stub Honesty (R-03 / lesson #4796) — PASS
- Live `/v1/{slug}` determined NOT reachable (no `UNIMATRIX_PUBLIC_URL`/bundle; only DNS-unresolvable placeholder hosts; egress otherwise works). Legitimate environmental gap.
- Live-tier ACs marked LIVE-PENDING, NOT greened on stub. Tier table separates `[no-cloud]` / `[stub/local]` / LIVE columns.
- Stub provably pinned to `test/fixtures/mcp/rmcp-initialize-capture.json` with source-verified provenance block.
- **GH #779 verified OPEN** (`[vnc-039] LIVE cloud validation pending: rmcp 1.7.0 handshake + G1/G2/G3 against real /v1/{slug}`), with the ordered post-deploy checklist.
- Size caps human-approved on #775 (BACKSTOP 160k→180k); confirmed not a self-raise (recorded on issue, header documents the human-decision rule). Gate passes at raw 169317 ≤ 180000, stripped 98440 ≤ 100000.

### 7. Knowledge Stewardship — PASS
Tester report `vnc-039-agent-9-tester-report.md` has a complete `## Knowledge Stewardship` block: `Queried:` (#5124, #5125, #4970, #4781, #5098 via context_briefing/context_get) and `Stored: nothing novel` with a stated reason. Compliant.

### 8. Suite-Green Accuracy — PASS (prior FAIL RESOLVED)
Verified at HEAD 5879d77f (see Prior Failure Resolution above):
```
node --test test/hook-client/size-gate.test.js  →  tests 21 · pass 21 · fail 0   (×2 runs)
node --test (full)                              →  tests 990 · pass 989 · fail 0 · skipped 1 · exit 0
```
The committed suite now matches the RISK-COVERAGE-REPORT.md headline ("990 · 989 pass · 0 fail · 1 skipped", size-gate 21 PASS). The lockstep desync is corrected; the report's Test Fixes and FLAGGED sections are reconciled. The known pre-existing parallel-timing flakiness in UDS/replay suites did not manifest in the full-suite run (exit 0, 0 fail); it is environmental and not a vnc-039 regression.

## Rework Required
None.

## Notes
- All eight gate dimensions PASS. The single prior REWORKABLE issue (size-gate header meta-assertion lockstep + inaccurate suite-green claim) is fully resolved at 5879d77f with no production-code change.
- Live-tier LIVE-PENDING status (#779, OPEN) remains honestly reported and is explicitly NOT a fail — environmental (no reachable cloud endpoint), per R-03 / lesson #4796.
- No git commits made and no files edited outside `product/features/vnc-039/` during this re-validation.
