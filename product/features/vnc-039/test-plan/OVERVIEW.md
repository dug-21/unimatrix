# vnc-039 Test Plan — OVERVIEW

> Pure-JS stdio→HTTPS MCP bridge (Scope A) + out-of-tree credential relocation (Scope B).
> Roots: RISK-TEST-STRATEGY.md (R-01..R-17), ACCEPTANCE-MAP.md (AC-01..AC-13, AC-04b/08b/08c/08d), SPECIFICATION.md (FR-01..27 / NFR-01..08), ARCHITECTURE.md (C1..C5).
> CI reality: **JS-client-only** (Node `--test`). There is no Rust CI gate; Rust validation = protocol gates + release workflows (memory: cargo-test-in-protocol-not-ci). This feature is **client-only** — no Rust unit tests are added.
> **Test infrastructure is CUMULATIVE** (NFR-08). Every plan extends `packages/unimatrix/test/` — the cert-pin/transport fixtures, the Layer-2 real-server harness, `helpers/stub-server.js`, and the existing `init-remote`/`config` suites. **No isolated scaffolding.**

---

## 1. Test Strategy

Three tiers, mapped to the validation tiers in ACCEPTANCE-MAP:

| Tier | What it proves | Where it runs | Reachable cloud? |
|------|----------------|---------------|------------------|
| **Unit** | Per-unit correctness of the five bridge translation areas (`stdio-frame`, `http-session`, `sse-parse`*, `dispatch`, `lifecycle`) + credstore + config resolve. | Node `--test`, no network. | No |
| **Integration `[no-cloud]` (Scope B)** | Store write/read round-trip, mode 0600, no-secret-in-tree, both-consumers-one-schema, **file-mode observe over a LOCAL pinned `https.createServer`**, `.mcp.json` idempotency, legacy loud-message. | Node `--test` against local `https.createServer` / stub. | No |
| **Integration `[stub/local]` (Scope A)** | Full MCP lifecycle through the bridge against a **local Streamable-HTTP stub pinned to a captured rmcp `initialize` response** + a **LIVE self-signed `https.createServer`** for the trust-boundary (good/wrong-pin) tests. | Node `--test` against local TLS stub. | No |
| **Integration `+ live` (Scope A delivery gate)** | Every Scope-A AC (AC-03/04/05/06/12) re-validated against the **real `/v1/{slug}` cloud endpoint**. Session-id handshake FIRST. | Manual/scripted delivery run against the real cloud (#774 merged). | **Yes** |

\* `sse-parse` unit and its SSE integration scenarios are **CONTINGENT** on the SSE-skip probe (R-04). If the live probe shows JSON-only across the full lifecycle, the `sse-parse` unit is **dropped** and its tests are **not written** (assert `sse-parse` absent/unused instead).

### Test-design rules (binding, from lesson #4970 / pattern #4965)

1. **Trust-boundary code is proven by exercising the real boundary** — a real `https.createServer` handshake, never a shape assertion. A test that only asserts `rejectUnauthorized===false` or `typeof verifyPeerFingerprint === "function"` **does not satisfy R-01/R-02 and must be rejected at gate.** This is the vnc-034 F1 lesson; cert-pin pinning is verified on `secureConnect` (#4965), and the token-write is gated behind the pin.
2. **Capturing server asserts zero leak** — the wrong-pin tests assert, server-side, that the capturing server received **NO `Authorization` header and NO request body** (extend the `observedAuth[]` pattern in `cert-pin-tls.test.js`).
3. **Negative-control** — at least one R-01 test proves it would FAIL if the pin were a no-op (so the assertion is not vacuous).
4. **Stub framing is provably derived, not invented** — the Streamable-HTTP stub's wire contract (status codes, `Mcp-Session-Id` header semantics, `application/json` vs `text/event-stream` framing) is pinned to a **captured real rmcp `initialize` response**, with a fixture-provenance comment citing the capture (R-03).
5. **Field-presence ≠ wire behavior** — `config.pinnedFp` populated is necessary but **not sufficient** for R-06; AC-08d must prove the observe POST transits pinned HTTPS to a local `https.createServer`.
6. **Live is a delivery gate** — no Scope-A AC is reported `validated-live` on stub evidence alone. Un-run live ACs are tagged `pending-live-run`, never `not-validated-live (#774)` (the #774 block is retired).

---

## 2. Risk → Test Mapping

| Risk | Priority | Component(s) | Test plan section | Primary AC |
|------|----------|--------------|-------------------|------------|
| **R-01** Silent token leak (bearer before pin) | **Critical** | C2 | mcp-bridge §Trust-boundary (LIVE good/wrong-pin, zero-`Authorization` capture, negative-control) | AC-04 |
| **R-02** Per-socket re-pin gap | **Critical** | C2 | mcp-bridge §Per-socket re-pin (2nd-request re-pin, no-pool, mid-session cert swap) | AC-04b |
| **R-03** Green-on-stub ≠ live | High | C2 | OVERVIEW §4 (stub-provenance + live checklist) + mcp-bridge §Live | AC-03/04/05/06/12 |
| **R-04** SSE parse correctness* | **Critical*** | C2 `sse-parse` | mcp-bridge §SSE (probe-gated; golden corpus + chunk-split fuzz) | AC-06 |
| **R-05** `Mcp-Session-Id` capture/replay | **Critical** | C2 `http-session` | mcp-bridge §Session capture/replay | AC-03 |
| **R-06** Schema mismatch — current break | **Critical** | C5, C1 | config-resolve §Pinned-HTTPS wire test (LOCAL pinned https) | AC-08c/08d |
| **R-07** Two-key resolution failure | High | C1, C4, C5 | credstore §One-key round-trip | AC-08b |
| **R-08** Scope B coupled to A/cloud | High | C1, C5 | OVERVIEW §5 + credstore/config (`[no-cloud]` suite) | AC-11 |
| **R-09** Token to a loggable surface | High | C2, C4 | mcp-bridge §No-leak + init-remote §No-leak (grep-all-surfaces) | AC-09 |
| **R-10** `.mcp.json` clobber/dup | Med | C4 | init-remote §Idempotency (extends `writeMcpJson` fixtures) | AC-07 |
| **R-11** Legacy silent skip | Med | C4 | init-remote §Legacy loud-message | AC-10 |
| **R-12** Credential left in tree | High | C4, C1 | init-remote §Out-of-tree + migration | AC-08 |
| **R-13** Store read-error posture | Med | C1, C2, C5 | credstore §Read-error matrix (per-consumer posture) | AC-08c |
| **R-14** Hybrid flip-bar undefined | Med | (process) | OVERVIEW §6 (delivery checkpoint, not a runtime test) | NFR-07 |
| **R-15** Legacy `fingerprint:null` pin posture | Low | C5 | config-resolve §Null-fingerprint legacy | AC-08c |
| **R-16** stdio framing split | Med | C2 `stdio-frame` | mcp-bridge §stdio framing (byte-split-invariant) | AC-03 |
| **R-17** Session/attribution stability | High | C2 `http-session`/`lifecycle` | mcp-bridge §Identity stability | AC-12 |

\* R-04 (and the Critical count) **retires entirely if the FIRST-delivery SSE-skip probe passes.**

---

## 3. Cross-Component Test Dependencies

- **Store schema is the highest-leverage integration seam** (R-06/R-07/R-13). One file, one `projectHash` key, one schema, read by **both** C2 (bridge) and C5 (hook client) and written by C4. The `both-consumers-one-schema` resolution test (credstore plan) is the keystone: it seeds ONE `remote.json` and asserts C2 reads `mcp_url`/token/`fingerprint` and C5 reads `observe_url`/token/`fingerprint`/`timeouts` from the **same file** — no per-consumer dialect.
- **`computeProjectHash` is the single key derivation** (C1 write, C4 write, C5 read, C2 read). The R-07 one-derivation test asserts write-key and both read-keys come from the one shared export so they cannot disagree.
- **C4 → C1**: `initRemote()` writes the store via `credstore.write`; the init-remote plan exercises the integration (store written out-of-tree at 0600 on `init --bundle`), and credstore unit tests own the write semantics.
- **C2/C5 reuse `cert-pin.js` verbatim** — the pin trust model is NOT re-implemented per component; both trust-boundary plans assert the production `applyCertPin`/`verifyPeerFingerprint`/`computeFingerprint` are the seam under test.
- **C3 (`bin/unimatrix.js`)** has no cross-component data dependency; it is a routing test (AC-13) that asserts `mcp-bridge` early-returns to JS and never `execFileSync`es the Rust binary.

---

## 4. Integration Harness Plan (MANDATORY)

### 4.1 Existing harness this feature extends (cumulative — NFR-08)

| Existing asset | Path | How vnc-039 extends it |
|----------------|------|------------------------|
| **Live TLS cert-pin recipe** | `test/cert-pin-tls.test.js` | The template for R-01/R-02/R-06 LIVE tests: generate a self-signed leaf via the `openssl` CLI into a per-run temp dir (skip if unavailable — never fail), compute `REAL_FP` via the production `computeFingerprint`, capture `observedAuth[]` server-side, assert good-pin connects and wrong-pin leaks NO token. The bridge tests stand up the same kind of `https.createServer` but make it speak **Streamable-HTTP MCP** (initialize→session→tools) instead of Ping/Pong. |
| **HTTP stub** | `test/helpers/stub-server.js` (`startStubServer`, request log, scriptable responder, `startSilentTcpServer`, `refusedPort`) | Extend into an **MCP Streamable-HTTP stub** helper (`startMcpStubServer` over `https.createServer`): mints/returns `Mcp-Session-Id` on `initialize`, requires its replay on follow-ups (4xx if absent), serves `application/json` (and `text/event-stream` only if the probe forces it), logs every request's URL/headers/body for verbatim-URL + session-replay + identity-stability assertions. Provenance-pinned to a captured rmcp response (§4.3). |
| **Layer-2 real-server harness** | `test/helpers/real-server.js`, `test/helpers/layer2-fixtures.js` | Pattern for the **live cloud** validation tier and for the `pinnedFp`/`server.pinnedFp = "sha256:"+hex` convention (#5098). The observe-over-pinned-HTTPS AC-08d test reuses the `server.pinnedFp` leaf-fingerprint convention from this harness. |
| **`init-remote` suite** | `test/init-remote.test.js` | Extend for AC-01/07/08/08b/09/10: `.mcp.json` stdio entry, idempotency/merge/dry-run (reusing the `writeMcpJson` fixtures), out-of-tree store at 0600, legacy loud message, no-token-in-surfaces, stale-subtree migration. |
| **`config` suite** | `test/hook-client/config.test.js` (`makeProject`, `project-hash-goldens.json`, `writeLocalSettings`) | Extend for C5: repoint file-mode `resolve()` to the store, assert `observe_url` (not `url`) + populated `pinnedFp`; add a `writeRemoteStore(projectHash, cred)` helper mirroring `writeLocalSettings`. |
| **Zero-dep / size gates** | `test/check-zero-deps.js`, `test/check-hook-client-size.js`, `test/hook-client/size-gate.test.js` | AC-02 (no runtime deps, no MCP SDK) rides the existing zero-dep gate; the ~450-LoC budget rides the size gate. |

### 4.2 New test files (within `packages/unimatrix/test/`, cumulative)

| New file | Component | Risks/ACs |
|----------|-----------|-----------|
| `test/hook-client/credstore.test.js` | C1 | R-07, R-12, R-13, AC-08b/08c |
| `test/hook-client/mcp-bridge.test.js` | C2 | R-01, R-02, R-03, R-05, R-09, R-16, R-17, AC-03/04/04b/05/09/12 |
| `test/hook-client/mcp-bridge-sse.test.js` | C2 `sse-parse` | R-04, AC-06 — **only if the probe forces SSE** |
| `test/hook-client/mcp-bridge-tls.test.js` | C2 | R-01/R-02 LIVE handshake (clone of `cert-pin-tls.test.js` for the bridge) |
| `test/bin-mcp-bridge.test.js` | C3 | AC-13 |
| `test/helpers/mcp-stub-server.js` | helper | the provenance-pinned Streamable-HTTP MCP stub (extends `stub-server.js`) |
| `test/fixtures/mcp/rmcp-initialize-capture.json` | fixture | captured rmcp `initialize` response (headers + body) — stub golden (R-03) |
Extensions to existing `init-remote.test.js` and `config.test.js` (no new file) cover C4/C5.

### 4.3 Stub provenance (R-03) — required before the stub is trusted

The MCP stub's wire contract MUST be **provably derived from a captured rmcp `initialize` response**, not invented:
1. Capture a real rmcp `initialize` POST response (the SCOPE-noted raw POST → 200) — status, the `Mcp-Session-Id` response header, and the `Content-Type` (`application/json` vs `text/event-stream`) — into `test/fixtures/mcp/rmcp-initialize-capture.json`.
2. The stub helper reads that fixture and mirrors its headers/framing, with a fixture-provenance comment citing the capture date/source.
3. After the live run (§4.4 item 1), reconcile any stub/real divergence back into the fixture and the stub.

### 4.4 Live cloud validation + SSE probe (Scope-A delivery gate, #774 merged)

These run **LIVE** against the real `/v1/{slug}` cloud endpoint, in order. Run as a scripted delivery checklist (not Node CI — CI is local-only); the coverage report records each as `validated-live` or `pending-live-run`.

1. **FIRST — rmcp 1.7.0 session-id handshake (DELIVERY CHECKPOINT, ARCHITECTURE §3.3).** Confirm whether `Mcp-Session-Id` is **server-minted and returned** in the `initialize` response header vs client-minted, and that the bridge replays the server's value verbatim. This seam is **unpinnable from `unimatrix-server` alone**; it gates AC-03/AC-04/AC-12 being trusted as validated-live. Reconcile the observed behavior into the stub contract (§4.3) and the R-05/R-17 assertions.
2. **SSE-skip probe (FIRST delivery experiment — can RETIRE R-04).** Send every request with `Accept: application/json` **only** across the full lifecycle (`initialize`→`tools/list`→`tools/call`, including a large/streaming-prone result) against the **live** endpoint. **Two outcomes, both planned:**
   - **JSON-only** → the `sse-parse` unit is **DROPPED**; `mcp-bridge-sse.test.js` is **not written**; `dispatch` keeps a JSON-only path; assert `sse-parse` absent/unused; R-04 evaporates and drops from the Critical count and the flip-bar surface.
   - **SSE forced on any step** → R-04 stands in full: build `sse-parse` and write `mcp-bridge-sse.test.js` (golden corpus: single/multi-line `data:`, multi-event, `event:`/`id:` ignored-or-honored, CRLF + bare-LF, **chunk-split-invariant fuzz**, 1 MiB guard).
3. **Re-run AC-03/04/05/06/12 live**: initialize→session→tools/list→tools/call round-trip, verbatim-URL, both response framings (per probe outcome), the live good-pin handshake against the real leaf, and identity stability across the live session.
4. **Reconcile** any stub/real divergence into the stub contract.

### 4.5 Mandatory minimum gate

- Full `node --test` suite green (all unit + `[no-cloud]` + `[stub/local]` tiers).
- The **named** critical gates verified by name (lesson #4970), NOT just a green suite: AC-04 LIVE good/wrong-pin zero-leak, AC-04b per-socket re-pin, AC-08c/08d pinned-HTTPS wire, AC-12 identity stability, stub-provenance comment present.
- **Fresh-context security review** of the bridge trust boundary even when gates are green (the #4970 second-order lesson).
- AC-02 zero-dep gate green.

---

## 5. Scope B Independence (R-08, AC-11)

The entire Scope B test set (`credstore.test.js`, the C5 `config.test.js` extensions, the C4 store-write/legacy halves of `init-remote.test.js`) MUST run green **with no cloud reachable and the `mcp-bridge` module never spawned**. The coverage report states explicitly that Scope B is mergeable independently of Scope A. The local `https.createServer` used by AC-08d is a *local* pinned server, not the cloud — it preserves `[no-cloud]`. No Scope B test may `require`/spawn `mcp-bridge.js` or hit `mcp_url`.

---

## 6. Hybrid Flip-Bar Checkpoint (R-14 — process gate, not a runtime test)

Pre-thresholded, evaluated **once, AFTER the SSE-skip probe resolves** (a passing probe removes the SSE component). FLIP to the SDK-hybrid only if **ALL** hold: (1) combined `sse-parse`+`http-session`+dispatch surface > ~520 LoC at first-green; (2) chunk-split-invariance (R-04) or capture/replay (R-05) tests stay red after a focused fix OR need non-stdlib parsing; (3) a 30-min SDK re-check shows the client-only `@modelcontextprotocol/sdk` now tree-shakes the server/OAuth deps. On flip, AC-02 (zero-dep) is knowingly waived and re-approved by the human. The cert-pin pinned-flush contract (R-01/R-02) is unchanged either way. Documented in the coverage report regardless of outcome.

---

## 7. Coverage Targets

| Priority | Risks | Target |
|----------|-------|--------|
| Critical | R-01, R-02, R-04*, R-05, R-06 | Full — LIVE-boundary or wire-behavior tests, each named in the coverage report. |
| High | R-03, R-07, R-08, R-09, R-12, R-17 | Full. |
| Medium | R-10, R-11, R-13, R-14, R-16 | Full (R-14 is a documented checkpoint). |
| Low | R-15 | Basic (null-fingerprint legacy path: unpinned, no crash). |

Every AC-ID in ACCEPTANCE-MAP (AC-01..13, AC-04b/08b/08c/08d) maps to at least one named test in a component plan below.

## Knowledge Stewardship
- Queried: `context_briefing` (vnc-039 ADRs #5115/#5119/#5105/#5108; lesson #4970 trust-boundary false-green; pattern #4965 secureConnect pinning; pattern #5098 Layer-2 pinnedFp harness) — all directly shaped the trust-boundary and integration-harness plans.
- Stored: nothing novel at plan stage — the load-bearing patterns (#4965 secureConnect+gated-token-write, #4970 live-boundary-not-shape, #5098 Layer-2 pinnedFp harness, #4153/#4373 schema-bump cascade) already exist. A bridge-specific persistent-connection per-socket re-pin pattern is a candidate to store from Stage 3c **if** the implementation surfaces a reusable technique beyond #4965 (flagged to the executing tester).
