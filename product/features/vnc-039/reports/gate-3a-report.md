# Gate 3a Report: vnc-039

> Gate: 3a (Component Design Review)
> Date: 2026-06-18
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Architecture alignment | PASS | C1–C5 + 5 bridge sub-modules map 1:1 to ARCHITECTURE §2/§3.1; ADR-001..005 honored; integration contracts (store schema, projectHash key, `node <bridge> <projectHash>` argv, cert-pin reuse) consistent. |
| 2. Specification coverage | PASS | All FR-01..FR-27 have corresponding pseudocode; NFRs addressed; no scope additions. |
| 3. Risk coverage | PASS | All R-01..R-17 mapped to named test scenarios incl. the critical gates by name (R-01/R-02, R-03, R-06, R-17); SSE treated as REQUIRED. |
| 4. Interface consistency | PASS | OVERVIEW shared types match per-component usage; canonical `remote.json` schema, token-free `.mcp.json`, single `projectHash` derivation coherent across files. |
| 5. Knowledge stewardship | PASS (1 WARN) | Architect: Stored. Spec/risk/test: Queried + reasoned "nothing novel". Pseudocode: stewardship block present; WARN — MCP tools unavailable in session, query degraded to in-repo ADRs. |

**Stage-3a server-source finding (SSE required):** Independently verified against pinned rmcp 1.7.0 source. CONFIRMED correct in pseudocode + test plans — see Check 1 / Check 3.

## Detailed Findings

### Check 1 — Architecture alignment
**Status**: PASS

**Component boundaries match decomposition.** pseudocode/OVERVIEW.md §Components and the per-file headers reproduce ARCHITECTURE §2's C1–C5 table exactly (file path, New/Mod, Scope, responsibility). The C2 bridge is decomposed into the five ARCHITECTURE §3.1 translation units (`stdio-frame`, `http-session`, `sse-parse`, `dispatch`, `lifecycle`) as separate files under `lib/hook-client/mcp-bridge/`, each independently testable (SR-01) and each under 500 lines.

**ADR decisions followed (ADRs read at `architecture/ADR-001..005`):**
- ADR-001 (fail-loud pinned-flush): mcp-bridge.md `http-session` places `req.end(body)` ONLY inside the `secureConnect` success branch after `verifyPeerFingerprint`; mismatch → `req.destroy(err)` before any body byte → stderr + `process.exit(1)`. Per-socket re-pin enforced via `agent:false` (no pool) so every socket re-pins. Matches ARCHITECTURE §3.2 verbatim.
- ADR-002 (`mcp-bridge` subcommand → JS early-return): bin-unimatrix.md inserts the `mcp-bridge` branch before the Rust `execFileSync` block, mirroring the `init` early branch; `.mcp.json` targets the resolved module path directly (lean spawn). Matches ARCHITECTURE §5.
- ADR-003 (key = `projectHash`, colocated path): credstore.md `pathFor` returns `~/.unimatrix/<projectHash>/remote.json`, mirroring `socketPathFor` null posture; one file per project, no global map. Matches ARCHITECTURE §4.1/§4.2.
- ADR-004 (single canonical schema, reconcile mismatch): credstore.md owns one schema with `STORE_SCHEMA_VERSION=1`; config-resolve.md repoints file-mode `resolve()` to read `observe_url`+`fingerprint` (not `url`) and populate `pinnedFp`. Matches ARCHITECTURE §4.3/§4.4.
- ADR-005 (bundle-only boundary, B-first): init-remote.md branches bundle vs legacy; legacy writes `fingerprint:null` + loud message + no bridge; OVERVIEW dependency graph sequences B first. Matches ARCHITECTURE §6/§7.

**Technology choices consistent.** Pure Node stdlib (`https`/`crypto`/`fs`/`path`/`os`), zero runtime deps, `cert-pin.js`/`transport-http.js` reused verbatim (NFR-01/02, ADR-001). No MCP SDK.

**Stage-3a server-source divergence handled correctly.** OVERVIEW "VERIFIED WIRE VALUES" §4 documents that Unimatrix builds the rmcp service with `StreamableHttpServerConfig::default()` overriding only `allowed_origins` → `stateful_mode:true`, `json_response:false` → all MCP POST responses are `text/event-stream`. I independently confirmed this against pinned rmcp 1.7.0 source (`router.rs:328-336`, `tower.rs:106-114` defaults, `tower.rs:1005/1059/1151/1165/1187` JSON-direct unreachable in stateful mode, `tower.rs:966-978` 406-on-non-dual-Accept, `http_header.rs:1` session constant) — every cited line matches. Consequently `ACCEPT_VALUE = "application/json, text/event-stream"` is sent on every POST (mcp-bridge.md wire constants) and `sse-parse` is treated as REQUIRED (built unless the live probe proves JSON-direct, which the source says it cannot in the current config). This is consistent with ARCHITECTURE designing `sse-parse` as a unit; the design does NOT assume SSE is droppable.

### Check 2 — Specification coverage
**Status**: PASS

Every functional requirement traces to pseudocode:
- FR-01..FR-06 (lifecycle/session/verbatim) → mcp-bridge.md `lifecycle` + `http-session` (initialize round-trip, `Mcp-Session-Id` capture/replay, verbatim `mcp_url` post via `url.pathname+url.search` with no append).
- FR-07 (stdio framing) → `stdio-frame.js`. FR-08 (JSON path, primary) + FR-09 (SSE, contingent-but-required-here) → `dispatch.js` + `sse-parse.js`.
- FR-10..FR-13 (owned pinned TLS, pinned-flush, fail-loud, bearer forwarding) → `http-session` request path.
- FR-14..FR-17 (`.mcp.json` write, idempotent/merge/dry-run, no-token) → init-remote.md `writeMcpBridgeEntry`.
- FR-18..FR-19 (legacy unsupported, loud deterministic message) → init-remote.md legacy branch + `LEGACY_MCP_UNSUPPORTED_MESSAGE`.
- FR-20..FR-26 (store write/read, both consumers, single schema, key) → credstore.md + config-resolve.md + init-remote.md. Note: the store keying resolves to `projectHash` (ADR-003), reconciling FR-20/FR-24's earlier "per-slug" framing — slug is payload inside `mcp_url`, not the key. The pseudocode correctly implements the architecture's resolution; this is an architecture-phase decision the spec explicitly deferred (FR-26/OQ-6), not a coverage gap.
- FR-27 (legacy in-tree creds migration) → init-remote.md `cleanStaleRemoteSubtree`.

NFRs addressed: NFR-01/02 (pure JS, zero deps) ride the existing zero-dep gate; NFR-03/NFR-06 (no token to logs) covered by no-leak scenarios; NFR-04 (0600) by credstore mode + chmod re-assert; NFR-07 (LoC budget / flip-bar) by the Hybrid Flip-Bar checkpoint; NFR-08 (cumulative tests) by the harness-extension plan.

**No scope additions.** Pseudocode implements no unrequested features. The `MCP-Protocol-Version` echo (mcp-bridge.md G1) is a wire-correctness requirement surfaced from the rmcp source (validated on non-init requests, `tower.rs:1033-1034`), not a new feature — it is flagged as a live-confirm gap, not an invented surface.

### Check 3 — Risk coverage
**Status**: PASS

Every R-01..R-17 maps to named test scenarios in the test-plan; test-plan/OVERVIEW §2 carries the full risk→test→AC matrix. Critical-risk gates verified BY NAME per the spawn prompt:

- **R-01 / R-02 (live wrong-pin handshake, AC-04/04b):** mcp-bridge.md test plan §R-01 specifies `test_bridge_wrongPin_destroysSocket_zeroAuthorization` (capturing server asserts NO `Authorization` and NO body — token never on the wire), `test_bridge_negativeControl_wouldLeakIfPinNoOp` (non-vacuous), and an explicit gate rule rejecting shape-only assertions. §R-02 covers per-socket re-pin (`test_bridge_everySocket_repinsBeforeFirstBodyByte`, `test_bridge_noConnectionPoolAgent`, `test_bridge_midSessionCertSwap_socket2Rejected_noTokenFlushed`). LIVE `https.createServer` recipe (lesson #4970) + fresh-context security review even on green. Maps AC-04 / AC-04b.
- **R-03 (live-not-stub; stub provenance-pinned):** test-plan/OVERVIEW §4.3 mandates the stub contract be provably derived from a captured rmcp `initialize` response (`test/fixtures/mcp/rmcp-initialize-capture.json` + provenance comment); §4.4 makes live cloud validation a delivery gate (session-id handshake FIRST); test-design rule 6 forbids reporting any Scope-A AC `validated-live` on stub evidence alone.
- **R-06 (file-mode observe over pinned HTTPS, AC-08d):** config-resolve.md §AC-08d specifies a LOCAL pinned `https.createServer` wire test proving the observe POST transits pinned HTTPS (good-pin delivers; wrong-pin fail-open exit 0 with no token on wire; no UDS fall-through), with an explicit "field-presence ≠ wire behavior" gate rule citing the vnc-034 dead-pin lesson.
- **R-17 (session/attribution stability, AC-12):** mcp-bridge.md §R-17 covers byte-identical `Mcp-Session-Id` + stable `clientInfo.name` across requests in one process, distinct identity across project sessions, never per-request minting.

Test plans include integration (both-consumers-one-schema keystone, `.mcp.json` idempotency) and edge cases (chunk-split fuzz, CRLF/LF, 1 MiB guard, absent session header, `fingerprint:null` legacy, malformed/unknown-version store reads). Risk priorities reflected: the 5 Critical risks get full LIVE-boundary/wire-behavior coverage; R-15 (Low) gets basic coverage. R-04 correctly retained as REQUIRED given the server-source finding (not pre-dropped).

### Check 4 — Interface consistency
**Status**: PASS

- **Canonical `remote.json` schema** in OVERVIEW (§Shared type) matches credstore.md (owner), config-resolve.md (reads `observe_url`+`fingerprint`+`timeouts`+`token`), and mcp-bridge.md (reads `mcp_url`+`token`+`fingerprint`). Field ownership table ("mcp_url → bridge only; observe_url+timeouts → hook only; token+fingerprint → both") is consistent across all three. No per-consumer dialect.
- **`projectHash` keying.** All of C1 write, C4 write, C5 read, C2 read derive the key from the single `computeProjectHash` export (`config.js:123`). init-remote.md Step 1b and config-resolve.md `resolve()` both call it; the bridge takes `argv[2]`. One derivation → cannot disagree (R-07). Consistent.
- **`node <bridge> <projectHash>` argv.** OVERVIEW "Bridge argv contract", bin-unimatrix.md (calls `main(["node","mcp-bridge",projectHash])` so `argv[2]===projectHash`, identical to direct spawn), init-remote.md `.mcp.json` `args:[bridgePath, projectHash]`, and mcp-bridge.md `argv[2]` are mutually consistent.
- **cert-pin pinned-flush reuse.** mcp-bridge.md `http-session` reuses `applyCertPin`/`verifyPeerFingerprint` (cert-pin.js) and the `transport-http.js:150-176` flush-after-pin pattern; config-resolve.md threads `pinnedFp` through `okHttp` → `config.pinnedFp` → `transport-http.post` (the existing consumer). Both reuse, neither re-implements TLS trust.
- **Data flow coherent.** OVERVIEW §Data flow (init-write → store; runtime hook → resolve → pinned observe POST; Claude Code spawn → bridge read → pinned MCP) matches every per-component file. No contradictions found between files.

### Check 5 — Knowledge stewardship compliance
**Status**: PASS (1 WARN)

Design-phase agent reports checked under `agents/`:
- **Architect** (`vnc-039-agent-1-architect-report.md`): `## Knowledge Stewardship` present. Active-storage agent — has `Stored:` (ADRs #5108–#5112 via context_store, one typed edge #5108 Supports #4970) plus `Queried:`. Compliant.
- **Specification** (`vnc-039-agent-2-spec-report.md`): block present; `Queried:` + `Stored: nothing — read-only tier; spec decisions are feature-specific.` Reason given. Compliant.
- **Risk-strategist** (RISK-TEST-STRATEGY.md §Knowledge Stewardship, inline): `Queried:` (#4970, #4153/#4373, #4796 + context_get) + `Stored: nothing novel to store -- the cross-feature risk pattern ... already exists as lesson #4970`. Reason given. Compliant.
- **Tester / test-plan** (test-plan/OVERVIEW.md §Knowledge Stewardship, inline): `Queried:` (context_briefing #5115/#5119/#5105/#5108, #4970, #4965, #5098) + `Stored: nothing novel at plan stage` with reason and a flagged Stage-3c candidate. Compliant.
- **Pseudocode** (`vnc-039-agent-1-pseudocode-report.md`): block present. Read-only tier — `Queried:` entry states Unimatrix MCP tools were NOT available this session (ToolSearch returned no matches), so the agent proceeded from the five in-repo ADR files + source docs. **WARN**: the query obligation degraded to in-repo artifacts rather than a live Unimatrix query. This is non-blocking — the block is present with a stated reason, the in-repo ADRs fully covered the design surface, and the pseudocode demonstrably reuses established patterns (cert-pin pinned-flush, writeMcpJson idempotency, single-derivation key) verbatim. Flag for delivery: rust/js-dev agents should run `/uni-query-patterns` live before implementing.

No missing stewardship block on any agent → no REWORKABLE FAIL.

## Rework Required

None. (One WARN on pseudocode stewardship — degraded query due to MCP tools unavailable, non-blocking; surfaced for delivery awareness.)

## Scope Concerns

None.
