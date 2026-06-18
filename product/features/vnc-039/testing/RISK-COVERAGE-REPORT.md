# Risk Coverage Report: vnc-039

> Pure-JS stdio→HTTPS MCP bridge (Scope A) + out-of-tree credential relocation (Scope B).
> Stage 3c execution, 2026-06-18. CI reality: **JS-client-only** (`node --test`); Rust validation = protocol gates + the infra-001 server harness, not GH CI.
> **LIVE cloud reachability: NOT REACHABLE** in this delivery environment (see §Live-Reachability Determination). Live-tier ACs are reported **[stub/local]-validated + LIVE-PENDING**, never validated-live on stub evidence (lesson #4796 / R-03). Live checklist filed as **GH #779**.

---

## Test Results

### Unit + `[no-cloud]` + `[stub/local]` (Node `--test`)

Full package suite (binary built, `ORT_DYLIB_PATH` set): **990 tests · 989 pass · 0 fail · 1 skipped** (exit 0).
The 1 skip is `test_root_walk_windows_separators` — a Windows-platform-gated test (pre-existing, not a vnc-039 concern).

| vnc-039 suite | File | Tests | Result |
|---------------|------|-------|--------|
| credstore (C1) | `test/hook-client/credstore.test.js` | 33 | PASS |
| config resolve (C5) | `test/hook-client/config.test.js` | 67 (+1 platform-skip) | PASS |
| mcp-bridge lifecycle/session/identity/framing/no-leak (C2) | `test/hook-client/mcp-bridge.test.js` | 38 | PASS |
| mcp-bridge LIVE trust boundary (C2, R-01/R-02) | `test/hook-client/mcp-bridge-tls.test.js` | 7 | PASS |
| mcp-bridge SSE wire path (C2 `sse-parse`, R-04) | `test/hook-client/mcp-bridge-sse.test.js` | 2 | PASS |
| bin mcp-bridge routing (C3) | `test/bin-mcp-bridge.test.js` | 7 | PASS |
| init-remote (C4) | `test/init-remote.test.js` | 54 | PASS |
| size gate (AC-02 budget) | `test/hook-client/size-gate.test.js` | 21 | PASS (1 stale assertion fixed — see §Test Fixes) |
| zero-dep gate (AC-02) | `test/check-zero-deps.js` | n/a | PASS — no runtime deps, no MCP SDK |

Broader hook-client regression (representative): `index` 54, `index-decoration` 31 (1 fixture migrated — see §Test Fixes), `build-request` 90, `parity-layer1` 91, `transport-http` 34, `transport-uds` 33, `merge-settings` 73, `remote-client` 26, all Layer-2 parity suites (`parity-layer2`/`-uds`/`-concurrency`/`-precompact`) green with the cargo binary present.

- Total (full suite): **990**
- Passed: **989**
- Failed: **0**
- Skipped: **1** (Windows-platform-gated)

### Integration — infra-001 server harness (regression baseline)

Smoke gate (mandatory minimum): `pytest -m smoke --timeout=60` → **24 passed, 0 failed** (382 deselected). Exercises the Rust server over MCP stdio.

| | Total | Passed | Failed |
|---|------|--------|--------|
| Integration smoke | 24 | 24 | 0 |

**Suite-selection rationale:** vnc-039 is a **JS-only edge-client** feature; it adds no server tool logic, no schema/storage change, and no server-visible behavior. The infra-001 harness exercises the compiled `unimatrix` **server** — none of which vnc-039 touches — so its role here is a **regression baseline** confirming the feature did not break the server contract. The feature's *own* integration coverage lives in the JS harness: the provenance-pinned Streamable-HTTP MCP stub (`test/helpers/mcp-stub-server.js`), the LIVE self-signed `https.createServer` trust-boundary tests, and the Layer-2 real-server parity suites — all green above. **No new infra-001 suites were needed** (the OVERVIEW §4 integration plan targets the JS harness, which is fully implemented and passing).

### LIVE cloud tier (Scope-A delivery gate)

**NOT RUN — endpoint unreachable.** Tracked as **GH #779** with the ordered post-deploy checklist (handshake first). See §Live-Reachability Determination and §Validation-Tier Status.

---

## Live-Reachability Determination

**Determination: a real cloud `/v1/{slug}` Streamable-HTTP MCP endpoint is NOT reachable from this delivery environment.**

Evidence (2026-06-18):
- No `UNIMATRIX_PUBLIC_URL` / `UNIMATRIX_REMOTE_*` env configured; no v:2 bundle artifact pointing at a deployed cloud.
- The `~/.unimatrix/*/remote.json` stores present on the host point only at placeholder hosts (`cloud.example`, `unimatrix.example.com`), both **DNS-unresolvable (ENOTFOUND)** — leftover local-test artifacts, not a live target.
- Outbound HTTPS egress works generally (GitHub reachable, status 403 = reached) — the gap is the **absence of a real target**, not a network egress block.

This matches the Stage 3a plan flag that live reachability was unconfirmed. Per R-03 / lesson #4796, the live-tier ACs are **NOT greened**; they are marked `[stub/local]-validated + LIVE-PENDING`. The stub is **provably pinned** to the captured rmcp `initialize` response (`test/fixtures/mcp/rmcp-initialize-capture.json`, with a source-verified provenance block citing rmcp tower.rs line ranges for the server-minted `Mcp-Session-Id`, the `Accept: …text/event-stream` requirement → 406, and the `text/event-stream` response framing). #774 (allowed_hosts, PR #778) is merged — the live block is removed; only a deployed endpoint is missing.

---

## Coverage Summary (R-01..R-17)

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Silent token leak: bearer flushed before pin matches | `mcp-bridge-tls`: `test_bridge_goodPin_connectsAndRoundTrips`, `test_bridge_wrongPin_destroysSocket_zeroAuthorization`, `test_bridge_wrongPin_hammer_neverLeaksToken`, `test_bridge_negativeControl_wouldLeakIfPinNoOp` | PASS | Full (stub/local; LIVE good-pin against real leaf PENDING — #779) |
| R-02 | Per-socket re-pin gap on persistent connection | `mcp-bridge-tls`: `test_bridge_everySocket_repinsBeforeFirstBodyByte`, `test_bridge_noConnectionPoolAgent`, `test_bridge_midSessionCertSwap_socket2Rejected_noTokenFlushed` | PASS | Full |
| R-03 | Green-on-stub must not stand in for live | Stub provenance pinned to `rmcp-initialize-capture.json` (fixture provenance block present); live checklist filed GH #779 | PARTIAL (by design) | Fast tier full; **LIVE-PENDING #779** |
| R-04 | SSE parse correctness (probe → **SSE required**) | `mcp-bridge-sse`: `test_bridge_fullLifecycle_overSse_jsonResults`, `test_bridge_acceptHeaderIncludesEventStream` (avoids rmcp 406); `sse-parse.js` built | PASS | Full (stub; live SSE probe PENDING #779) |
| R-05 | `Mcp-Session-Id` capture/replay correctness | `mcp-bridge`: http-session capture/replay/absent-header/teardown tests | PASS | Full (stub; live handshake item #1 PENDING #779) |
| R-06 | Schema mismatch — current break (pinned-HTTPS observe) | `config.test.js`: `test_resolve_fileMode_populatesPinnedFpFromFingerprint`, AC-08d `test_observe_fileMode_goodPin_postsToObserveUrlOverPinnedHttps`, `..._wrongPin_failOpenExit0_noTokenOnWire`, `..._noUdsFallthrough_withValidRemoteCred` | PASS | Full (`[no-cloud]`, LIVE local https) |
| R-07 | Two-key store resolution failure | `credstore`: one-key round-trip + per-project separation; `config`: `test_resolve_keyedByProjectHash_roundTrip` | PASS | Full |
| R-08 | Scope B coupled to Scope A / cloud | `credstore` + `config` Scope-B independence assertions; full Scope-B set runs `[no-cloud]`, bridge un-spawned | PASS | Full |
| R-09 | Token to a loggable surface | `mcp-bridge` no-leak (happy/mismatch/thrown), `init-remote` AC-09 surfaces, `credstore` action-string token-free | PASS | Full |
| R-10 | `.mcp.json` clobber/dup/dry-run | `init-remote` AC-07 idempotency/co-resident/dry-run/malformed | PASS | Full |
| R-11 | Legacy path silent skip | `init-remote` AC-10 exact unsupported-message + exit, no bridge wired | PASS | Full |
| R-12 | Credential left in repo tree | `init-remote` AC-08 out-of-tree 0600, repo-tree-free, stale-subtree migration, partial-write | PASS | Full |
| R-13 | Store read-error posture (per-consumer) | `credstore` read-error matrix; `config` hook-posture matrix; `mcp-bridge` bridge fail-loud matrix | PASS | Full |
| R-14 | Hybrid flip-bar undefined | Process checkpoint — see §Hybrid Flip-Bar | N/A (no flip) | Documented |
| R-15 | Legacy `fingerprint:null` pin posture | `config`: `test_resolve_nullFingerprint_resolvesUnpinned`, `test_resolve_presentFingerprint_pinned` | PASS | Full |
| R-16 | stdio framing split | `mcp-bridge` stdio-frame byte-split-invariant tests | PASS | Full |
| R-17 | Session/attribution identity stability | `mcp-bridge`: sessionId byte-identical across requests, stable `clientInfo.name`, two-spawns-same-name, distinct-projects-distinct-identity | PASS | Full (stub; live identity item #1 PENDING #779) |

**Critical (5):** R-01, R-02, R-04, R-05, R-06 — all FULL on the stub/local + LIVE-local tier. R-01/R-04/R-05's *cloud-live* portions are PENDING per #779.

---

## Hybrid Flip-Bar Checkpoint (R-14)

**Outcome: NO FLIP.** The SSE-skip probe resolved to **SSE-required** (rmcp `StreamableHttpServerConfig::default` → `json_response:false`; `Accept` must include `text/event-stream` or 406 — source-verified, fixture-pinned), so the full `sse-parse` + `http-session` + dispatch surface was built. Despite that, the DIY path stayed coherent: zero-dep gate green, size gate within the human-approved budget (raw 169317 ≤ 180000, stripped 98440 ≤ 100000), and the correctness suites (chunk-split-invariant SSE, capture/replay) are green with no non-stdlib machinery. None of the three FLIP conditions (LoC > ~520; persistent correctness instability; positive 30-min SDK re-check) hold. AC-02 (zero-dep) stands, no human waiver needed.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS `[no-cloud]` | `init-remote`: `test_initBundle_writesStdioUnimatrixEntry`, `..._bridgeCommandNotRustBinary`, `..._noSkippedMcpJsonLine` |
| AC-02 | PASS `[no-cloud]` | `check-zero-deps.js` (no runtime deps, no MCP SDK across 25 modules); size-gate budget green |
| AC-03 | **[stub/local] PASS + LIVE-PENDING (#779)** | Stub full lifecycle initialize→tools/list→tools/call + session replay green; live re-run pending |
| AC-04 | **[stub/local] PASS + LIVE-PENDING (#779)** | LIVE self-signed good/wrong-pin zero-`Authorization` + negative-control green; **fresh-context security review recommended even on green** (#4970); live real-leaf handshake pending |
| AC-04b | **[stub/local] PASS + LIVE-PENDING (#779)** | Per-socket re-pin: every-socket-repins, no-pool, mid-session cert-swap rejected no-token-flushed |
| AC-05 | **[stub/local] PASS + LIVE-PENDING (#779)** | `test_bridge_postsToMcpUrlVerbatim` — logged URL equals `mcp_url` exactly |
| AC-06 | **[stub/local] PASS + LIVE probe PENDING (#779)** | SSE-required (source-verified); SSE wire-path + `Accept` 406-avoidance green; live `Accept: application/json`-only → 406 confirmation pending |
| AC-07 | PASS `[no-cloud]` | `init-remote` idempotency/co-resident/dry-run/malformed |
| AC-08 | PASS `[no-cloud]` | `init-remote` out-of-tree 0600, repo-tree token-free, settings.local migration |
| AC-08b | PASS `[no-cloud]` | `credstore`/`init-remote` two-projects-two-stores, re-init-A-untouches-B |
| AC-08c | PASS `[no-cloud]` | `config` `observe_url` (not `url`) + **populated `pinnedFp`**; `credstore` both-consumers-one-schema |
| AC-08d | PASS `[no-cloud]` | `config` AC-08d: observe POST transits pinned local HTTPS (good-pin lands, wrong-pin fail-open exit0 no-token, no UDS fallthrough) — wire behavior proven, not just `pinnedFp` presence |
| AC-09 | PASS `[stub/local]+[no-cloud]` | `init-remote` + `mcp-bridge` no-token-in-surfaces (printSummary/stdout/stderr/.mcp.json/mismatch error); `.mcp.json` token/mcp_url/fp-free |
| AC-10 | PASS `[no-cloud]` | `init-remote` AC-10 exact unsupported message + exit, no bridge, observe unchanged, `fingerprint:null` written |
| AC-11 | PASS `[no-cloud]` | Scope B set green with no cloud + bridge un-spawned; **Scope B is mergeable independently of Scope A** |
| AC-12 | **[stub/local] PASS + LIVE-PENDING (#779)** | Stub identity stability (byte-identical sessionId + clientInfo.name; distinct projects distinct); live session-id mint-direction handshake (DELIVERY CHECKPOINT) pending |
| AC-13 | PASS `[no-cloud]` | `bin-mcp-bridge`: routes to JS, early-returns before Rust exec, forwards projectHash, non-regression of other subcommands |

**Live-PENDING ACs (NOT greened on stub evidence):** AC-03, AC-04, AC-04b, AC-05, AC-06, AC-12 — live portions only; their `[stub/local]` tiers PASS. Plus G1 (`MCP-Protocol-Version` echo), G2 (SSE priming/teardown + 406 reality), G3 (`clientInfo.name` passthrough), and the session-id mint-direction handshake — all tracked in **GH #779**.

---

## Validation-Tier Status Table

| AC-ID | `[no-cloud]` | `[stub/local]` | LIVE |
|-------|:---:|:---:|:---:|
| AC-01 | ✅ validated | — | — |
| AC-02 | ✅ validated | — | — |
| AC-03 | — | ✅ validated | ⏳ PENDING (#779) |
| AC-04 | — | ✅ validated | ⏳ PENDING (#779) |
| AC-04b | — | ✅ validated | ⏳ PENDING (#779) |
| AC-05 | — | ✅ validated | ⏳ PENDING (#779) |
| AC-06 | — | ✅ validated (SSE-required, source-verified) | ⏳ PENDING (live probe — #779) |
| AC-07 | ✅ validated | — | — |
| AC-08 | ✅ validated | — | — |
| AC-08b | ✅ validated | — | — |
| AC-08c | ✅ validated | — | — |
| AC-08d | ✅ validated (LIVE local https) | — | — |
| AC-09 | ✅ validated | ✅ validated | — |
| AC-10 | ✅ validated | — | — |
| AC-11 | ✅ validated | — | — |
| AC-12 | — | ✅ validated | ⏳ PENDING (handshake item #1 — #779) |
| AC-13 | ✅ validated | — | — |
| G1 `MCP-Protocol-Version` echo | — | source-verified | ⏳ PENDING (#779) |
| G2 SSE priming/teardown + 406 | — | source-verified | ⏳ PENDING (#779) |
| G3 `clientInfo.name` passthrough | — | ✅ stub-validated | ⏳ PENDING (#779) |
| Session-id mint-direction handshake | — | source-pinned (server-minted per rmcp source) | ⏳ PENDING (DELIVERY CHECKPOINT — #779) |

Legend: ✅ validated · ⏳ LIVE-PENDING (tracked GH #779) · — not applicable to this tier.

---

## Gaps

- **No risk lacks test coverage.** R-01..R-17 each map to ≥1 named passing test (R-14 is a documented process checkpoint with a no-flip outcome).
- **Live-tier gap (by design, not a coverage hole):** the cloud-live portions of R-01/R-03/R-04/R-05/R-17 and AC-03/04/04b/05/06/12 + G1/G2/G3 + session-id mint-direction are **LIVE-PENDING** because no real `/v1/{slug}` endpoint is reachable here. The fast tier is green and provenance-pinned; live is deferred to **GH #779** with an ordered post-deploy checklist (handshake first). This is honest pending-status per R-03 / lesson #4796 — NOT a greened unrun AC.

---

## Test Fixes Applied (in-scope test files only)

Both are stale test artifacts left by this feature's own changes — the "fix the test" triage branch (USAGE-PROTOCOL). No production code edited.

1. **`test/hook-client/size-gate.test.js`** — `test_limits_are_decimal` asserted `BACKSTOP_LIMIT === 160000`, but vnc-039 deliberately raised the gate's backstop 160000→180000 (`check-hook-client-size.js:35`, human-approved on #775) for the ~24KB bridge. Updated the meta-assertion to `180000` to track the source. The real gate already passes (raw 169317 ≤ 180000).
2. **`test/hook-client/index-decoration.test.js`** — `writeRemoteConfig` still seeded the **in-tree** `{root}/.claude/settings.local.json` `{unimatrix:{remote:{url,token}}}` shape, which vnc-039 C5 stopped reading (credential moved to the out-of-tree HOME-keyed `~/.unimatrix/<projectHash>/remote.json`). The spawned child therefore resolved no remote config → 0 FNF POSTs → deterministic failure. Migrated the helper to seed the canonical store under the child's HOME via the real lib walk (mirrors the already-migrated `index.test.js` `storePathFor`/`writeRemoteConfig`; entry #5125). Now 31/31.

---

## FLAGGED for Delivery Leader (NOT fixed — out of test scope)

- **Stale doc comment, `lib/hook-client/check-hook-client-size.js:9`** — the header still documents `BACKSTOP : raw … <= 160,000 bytes` while line 35 sets the constant to `180000`. A `size-gate header` meta-test (`test_header_documents_human_decision_rule`) asserts the source documents `"160,000"`, so it currently passes against the stale comment. When the comment is corrected to `180,000`, that header test must be updated in lockstep. This is a production-file comment edit — left for the Delivery Leader's rework routing, not patched here.
- **Inert stale fixture, `test/hook-client/benchmark-spawn.js:51,98`** — `makeProject`/`measureInProcessWork` still write the old in-tree `settings.local.json` remote shape. The benchmark only asserts timing budget + breadcrumb (not remote POSTs), so the stale credential is **inert** and the suite passes. Low-priority cleanup; not failing, not blocking. (Left unchanged to keep this PR's test diff minimal.)
- **Fixture-internal note, `test/fixtures/mcp/rmcp-initialize-capture.json`** — `response_headers.content-type` is `application/json` while the provenance prose (and the stub the tests run against) use `text/event-stream` SSE framing. The stub/tests follow the SSE-required prose (correct per rmcp source); the live run (#779) should reconcile the captured header value. Non-blocking.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` (vnc-039 Stage 3c execution) — surfaced #5124 (fail-loud pinned-flush bridge test traps: settle-on-mismatch, requester-seam spy, setSecureContext cert-swap), #5125 (credstore HOME-keyed fixture migration — directly diagnosed the `index-decoration` failure), #4970 (trust-boundary live-not-shape lesson), #4781 (Stage-3c pre-existing-failure triage procedure), #5098 (Layer-2 pinnedFp harness). `context_get` on #5124/#5125 for the full recipes.
- Stored: nothing novel to store — the bridge-test traps (#5124), the credstore fixture-migration trap (#5125), the live-boundary-not-shape lesson (#4970), and the Layer-2 pinnedFp harness (#5098) already exist as active entries and fully covered every technique used in this execution. No 2nd-instance pattern beyond them surfaced that would warrant a new entry. (The size-gate-backstop-raise-vs-meta-test staleness is a one-off feature-specific artifact, not a reusable cross-feature pattern.)
