# vnc-039 Implementation Brief — Remote/Cloud `init` Wiring: Pure-JS stdio→HTTPS MCP Bridge + Out-of-Tree Credential Store

> **Status:** Design complete; ratified at the human design gate. **#774 MERGED** (PR #778, commit `913b78cb`, 2026-06-18) — Scope A live cloud validation is **UNBLOCKED and EXPECTED in delivery**. Stub/local is the fast first tier; live is a delivery gate. #774 is **not a blocker anywhere** in this feature.
> **Sequencing:** **Scope B lands FIRST** by risk/independence (fixes a current live commit-leak + a current observe-over-HTTPS break; no #774 dependency, ever). **Scope A follows**, now live-validatable.

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/vnc-039/SCOPE.md |
| Scope Risk Assessment | product/features/vnc-039/SCOPE-RISK-ASSESSMENT.md |
| Specification | product/features/vnc-039/specification/SPECIFICATION.md |
| Architecture | product/features/vnc-039/architecture/ARCHITECTURE.md |
| ADR-001 (bridge translation + pinned-flush) | product/features/vnc-039/architecture/ADR-001-bridge-translation-and-pinned-flush.md |
| ADR-002 (bridge entrypoint subcommand) | product/features/vnc-039/architecture/ADR-002-bridge-entrypoint-subcommand.md |
| ADR-003 (credstore keying + path) | product/features/vnc-039/architecture/ADR-003-credential-store-keying-and-path.md |
| ADR-004 (canonical schema, reconcile mismatch) | product/features/vnc-039/architecture/ADR-004-canonical-store-schema-reconcile-mismatch.md |
| ADR-005 (bundle-only boundary + scope sequencing) | product/features/vnc-039/architecture/ADR-005-bundle-only-boundary-and-scope-sequencing.md |
| Risk-Based Test Strategy | product/features/vnc-039/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-039/ALIGNMENT-REPORT.md |
| Acceptance Map | product/features/vnc-039/ACCEPTANCE-MAP.md |

## Component Map

Components from the architecture (C1–C5). **Stage 3a COMPLETE** — pseudocode + test-plan paths below are real and verified to exist.

| Component | Scope | Pseudocode | Test Plan |
|-----------|-------|-----------|-----------|
| C1 `credstore.js` (out-of-tree store: path/read/write/0600) | B | pseudocode/credstore.md | test-plan/credstore.md |
| C2 `mcp-bridge.js` (stdio↔Streamable-HTTP bridge) | A | pseudocode/mcp-bridge.md | test-plan/mcp-bridge.md |
| C3 `bin/unimatrix.js` (`mcp-bridge` subcommand → JS) | A | pseudocode/bin-unimatrix.md | test-plan/bin-unimatrix.md |
| C4 `init.js` `initRemote()` + `.mcp.json`/store write + legacy message | A+B | pseudocode/init-remote.md | test-plan/init-remote.md |
| C5 `config.js` hook-client `resolve()` repoint + `okHttp` `pinnedFp` | B | pseudocode/config-resolve.md | test-plan/config-resolve.md |

**Stage 3a findings (drive Stage 3b):**
- **C2 decomposed into 5 sub-modules** (`stdio-frame` / `http-session` / `sse-parse` / `dispatch` / `lifecycle`) so no file exceeds 500 lines and each ADR-001 unit is independently testable. C2 remains ONE component / ONE dev agent.
- **SSE is REQUIRED, not contingent (server-source verified).** Unimatrix builds rmcp with `StreamableHttpServerConfig::default()` (`json_response:false`, `router.rs:326-336`), so all MCP responses are `text/event-stream`; the bridge MUST send `Accept: application/json, text/event-stream` (JSON-only → 406). The SSE-skip probe is expected to FAIL → `sse-parse` is built. R-04 stands; the live probe (Stage 3c) remains the definitive gate but the expectation is flipped to SSE-required.
- **Live re-confirm (DELIVERY CHECKPOINT):** all rmcp-owned wire values (`Mcp-Session-Id` server-minted+replayed, `MCP-Protocol-Version` echo G1, SSE priming/keep-alive G2, `clientInfo.name` passthrough-vs-override G3) confirmed live in Stage 3c, session-id handshake first.

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Restore the on-demand `context_*` MCP surface over HTTPS for cloud/remote-attached edge clients — the half of the personal-cloud contract (goal #4946) that remote attach silently drops today (`initRemote()` skips `.mcp.json`). **Scope A** adds a pure-Node-stdlib stdio→HTTPS MCP bridge that proxies stdio JSON-RPC to the cloud's per-slug Streamable-HTTP MCP endpoint over a fingerprint-pinned TLS connection, wired as a `stdio` server in `.mcp.json`. **Scope B** (independent, lands first) relocates the bearer credential out of the repo working tree into a unimatrix-owned, per-`projectHash`, out-of-tree store that both consumers resolve from — eliminating a current commit-leak vector and fixing a current file-mode-observe-over-HTTPS break.

## Resolved Decisions

All scope-shaping open questions are closed. The residual non-blocking cleanup detail (legacy in-tree creds migration) is resolved in ADR-004 §migration (delete stale subtree on next bundle `init`).

| Decision | Resolution | Source | ADR File |
|----------|------------|--------|----------|
| OQ-1 — bridge depth / build-vs-adopt | **BUILD/DIY**, pure-JS stdlib, no MCP SDK. JSON-first response handling; SSE parser contingent on a live probe. Hybrid flip-bar pre-thresholded, runs AFTER the SSE probe resolves. | ass-080 (#777); SCOPE Resolved Decisions | architecture/ADR-001-bridge-translation-and-pinned-flush.md |
| OQ-2 — legacy `--remote`/env-HTTPS in or out (#773) | **Bundle-only.** Legacy gets no bridge; emits a loud, deterministic unsupported message. Resolves #773 by deprecating env-HTTPS for cloud-attach MCP. | SCOPE OQ-2 | architecture/ADR-005-bundle-only-boundary-and-scope-sequencing.md |
| OQ-3 — bridge config source | Bridge **reads the store at spawn time**; token never on the command line, never in `.mcp.json`. | SCOPE OQ-3 | architecture/ADR-004-canonical-store-schema-reconcile-mismatch.md |
| OQ-4 — bridge entrypoint shape | **`unimatrix mcp-bridge` subcommand routed to JS** in `bin/unimatrix.js` (REQUIRED — non-`init` subcommands `execFileSync` the absent Rust binary on remote-only clients). | SCOPE OQ-4 | architecture/ADR-002-bridge-entrypoint-subcommand.md |
| OQ-5 — gitignore append vs ignored-path | **Collapsed** — no in-repo creds file exists to gitignore; `gitignoreWarning` removed. | SCOPE OQ-5 | architecture/ADR-004-canonical-store-schema-reconcile-mismatch.md |
| OQ-6 — creds-store key | Key by **`projectHash`** (`sha256(projectRoot)[:16]`); store at `~/.unimatrix/<projectHash>/remote.json`, mode 0600, colocated with existing per-project state. | SCOPE OQ-6 | architecture/ADR-003-credential-store-keying-and-path.md |
| OQ-7 — SSE-skip probe gating | **Resolved (#774 merged).** Probe runs **LIVE early in delivery** for a definitive answer (fast in-repo pre-check: rmcp source + vnc-024/PR #686 `/observe` content-negotiation). `sse-parse` stays contingent on it. | SCOPE Tracking / ARCHITECTURE §11 OQ-7 | architecture/ADR-001-bridge-translation-and-pinned-flush.md |

## #774 MERGED Posture (load-bearing across delivery)

- #774 (rmcp `allowed_hosts` wired from `UNIMATRIX_PUBLIC_URL`) **merged** — PR #778, commit `913b78cb`, 2026-06-18. Remote MCP requests no longer 403 at the host gate.
- **Scope A live cloud validation is UNBLOCKED and EXPECTED.** Stub/local-endpoint validation is the fast first tier (no reachable cloud needed); **live validation against the real `/v1/{slug}` endpoint is a delivery gate** — green-on-stub must NOT stand in for live.
- The stub's wire contract (status codes, `Mcp-Session-Id` semantics, `application/json` vs `text/event-stream` framing) MUST be pinned to a **captured real rmcp `initialize` response** so the fast tier stays honest ahead of the live run.
- **The active post-#774 live-validation checklist is in delivery.** Its **FIRST item** is validating the **rmcp 1.7.0 session-id handshake** live — whether the session id is **server-minted + returned** (in the `initialize` response `Mcp-Session-Id` header) vs **client-minted**, and that the bridge replays the server's value verbatim. ARCHITECTURE §3.3 labels this handshake validation a **DELIVERY CHECKPOINT** (cross-refs this checklist item #1 + AC-12) — the `Mcp-Session-Id` capture/replay contract is a live-validation checkpoint, not a build-only behavior. This is the bridge's hardest seam and is unpinnable from `unimatrix-server` alone; confirm it live before AC-03/AC-04/AC-12 are trusted as validated-live.
- #774 is **NOT a blocker anywhere** in this feature. The earlier "not-validated-live (#774)" caveat is retired.

## Files to Create / Modify

| File | New/Modified | Scope | Summary |
|------|--------------|-------|---------|
| `lib/hook-client/credstore.js` | NEW | B | Sole owner of the out-of-tree store: `pathFor`/`read`/`write` (idempotent merge, mode 0600), one schema, one key (`projectHash`). |
| `lib/hook-client/mcp-bridge.js` | NEW | A | Pure-stdlib stdio↔Streamable-HTTP bridge: stdio framing, pinned TLS, `Mcp-Session-Id` capture/replay, dual response framing, MCP lifecycle, fail-loud-on-mismatch. |
| `bin/unimatrix.js` | MODIFIED | A | Add `mcp-bridge` subcommand, early-return to the JS bridge (never `execFileSync` the Rust binary), mirroring the `init` branch. |
| `lib/init.js` | MODIFIED | A+B | `initRemote()`: write credential via `credstore` (B) instead of `writeRemoteSettingsLocal`; write the stdio `.mcp.json` bridge entry (A) instead of "Skipped .mcp.json"; legacy path emits a loud unsupported message; remove `gitignoreWarning`; delete stale in-tree `unimatrix.remote` subtree on bundle `init`. |
| `lib/hook-client/config.js` | MODIFIED | B | Repoint file-mode `resolve()` from `.claude/settings.local.json` to the out-of-tree store; read `observe_url`+`fingerprint` (not `url`); add `pinnedFp` to `okHttp` so file-mode remote observe runs over pinned HTTPS. |
| `test/` (existing harness) | MODIFIED | A+B | Extend the existing hook-client / Layer-2 real-server harness and cert-pin/transport fixtures (cumulative — no isolated scaffolding). |

**Unchanged (load-bearing, do not touch):** `cert-pin.js`, `transport-http.js` (the `post`/`pingForInit` observe POST mechanics are reused as-is), `bundle.js` (no schema change), the local (non-remote) `init()` flow.

## Data Structures

**Canonical `remote.json` store schema** (`~/.unimatrix/<projectHash>/remote.json`, mode 0600; both consumers read this single schema — ADR-004):

```json
{
  "schema_version": 1,
  "mcp_url": "https://host/v1/<slug>",
  "observe_url": "https://host/v1/<slug>/observe",
  "token": "<64 hex>",
  "fingerprint": "sha256:<64 hex>",
  "timeouts": { "connect_ms": 750, "sync_ms": 2000, "fnf_ms": 3000 }
}
```

- `schema_version` — unknown version is a **terminal, diagnosable** read failure (never a silent skip).
- `observe_url` **replaces** the old broken `url` key. Hook client reads it as its post target.
- `fingerprint` — read by **both** consumers. Hook client threads it into `config.pinnedFp` (the current-break fix). `null` on the legacy path → hook client stays unpinned (preserves today's legacy behavior).
- `timeouts` optional; absent → `DEFAULT_TIMEOUTS`.
- Field ownership: `mcp_url` → bridge only; `observe_url`+`timeouts` → hook client only; `token`+`fingerprint` → both.

**`.mcp.json` entry written by `init`** (token-free — AC-09):

```json
{
  "mcpServers": {
    "unimatrix": {
      "command": "node",
      "args": ["<abs path to mcp-bridge module>", "<projectHash>"],
      "env": {}
    }
  }
}
```

**`Mcp-Session-Id`** — transport-level session UUID (server-managed), captured from the `initialize` response headers, replayed byte-for-byte on every subsequent request; **stable** for the bridge process lifetime (distinct from any tool-param session_id — entry #4708).

## Function Signatures

| Symbol | Signature | Source | Consumers |
|--------|-----------|--------|-----------|
| `credstore.write` | `(projectHash, cred, {dryRun}) → string[]` (actions; mode 0600) where `cred = {mcp_url, observe_url, token, fingerprint, timeouts?}` | NEW C1 | C4 |
| `credstore.read` | `(projectHash) → {schema_version, mcp_url, observe_url, token, fingerprint, timeouts?} \| null` (null on ENOENT; throws on malformed/unknown version) | NEW C1 | C2, C5 |
| `credstore.pathFor` | `(projectHash) → string \| null` (null on no-homedir) | NEW C1 | C1, tests |
| `computeProjectHash` | `(projectRoot: string) → string` (16 hex; the store key) | `config.js:123` (unchanged) | C1, C4, C5 |
| `detectProjectRoot` | `(startDir) → string` | `init.js:25` | C4 (write-side root) |
| `walkToProjectRoot` | `(startDir) → string` | `config.js:44` | C5 (read-side root) |
| `applyCertPin` | `(options, isTls, pinnedFp) → options` | `cert-pin.js:131` (unchanged) | C2 |
| `verifyPeerFingerprint` | `(socket, pinnedFp) → Error \| null` | `cert-pin.js:67` (unchanged) | C2 |
| `computeFingerprint` | `(derBuffer: Buffer) → "sha256:"+hex` | `cert-pin.js:26` | C2 (parity) |
| `okHttp(...)` | resolved config — **gains a `pinnedFp` field** sourced from `fingerprint` | `config.js:203` (MODIFIED) | C5 → `transport-http.post` |
| bridge argv | `node <bridge> <projectHash>` | C4 `.mcp.json` | C2 |

## Constraints

- **Single edge language — pure JS.** Bridge MUST be pure JS; the Rust binary is the Linux-only server, not the client bridge. No Python, no third edge language.
- **Zero runtime dependencies (by decision, ass-080 #777).** Node stdlib only (`http`, `https`, `net`, `crypto`, `fs`, `path`, `os`); no `@modelcontextprotocol/sdk`, no `mcp-remote`.
- **Dumb-client invariant (vnc-038 spine).** Bridge POSTs `mcp_url` **verbatim** — composes no path, derives no slug; slug is server-authoritative payload inside `mcp_url`.
- **Cert-pin trust model + pinned-flush (vnc-038 ADR-002 / F1).** Complete the self-signed handshake (`rejectUnauthorized:false`), verify the leaf fingerprint on `secureConnect`, flush the token-bearing body **only after** the pin matches; reuse `cert-pin.js`, never re-implement TLS trust.
- **Per-socket re-pin (ADR-001 §3.2).** The bridge is a **persistent, fail-loud** connection (vs the single-shot, fail-open observe path). Every new TLS socket re-runs `verifyPeerFingerprint` on `secureConnect` before its first body byte; keep-alive reuse only on an already-pinned socket; no connection-pool agent that could flush on an unverified socket.
- **Stable session/attribution identity (ADR-001, vnc-014/#4708).** The bridge presents a **stable** `Mcp-Session-Id` (server-minted value replayed verbatim, never rotated mid-session) and a **stable** `clientInfo.name` (fixed bridge identifier, not per-spawn random/timestamped). Unstable identity bleeds audit attribution across sessions, undercutting the 1-client:1-project integrity basis.
- **NFR-06 — no token to logs.** Token absent from `printSummary()`, stdout, stderr, `.mcp.json`, the pin-mismatch error, and any thrown message on the remote path.
- **Cloud MCP is bundle-only (OQ-2, resolves #773).** Legacy env-HTTPS path is not bridged (no `fp` pin) and emits a loud, deterministic unsupported message.
- **#774 merged (PR #778) — bridge validated LIVE in delivery.** Stub/local is the fast first tier; live end-to-end validation against the real cloud is a delivery gate. #774 is not a blocker.
- **Credential out-of-tree, unimatrix-owned, keyed by `projectHash`.** Store at `~/.unimatrix/<projectHash>/remote.json` (mode 0600), never in the repo tree, never in `.claude/settings.local.json`. Cleartext-at-rest accepted; the hardened risk is cleartext-in-repo + namespace-squatting.
- **Pre-existing schema mismatch must be RECONCILED, not ported (ADR-004).** Writer emits `{mcp_url, observe_url, token, fingerprint}`; hook client reads `{url, token, timeouts}` and never `fingerprint` — so file-mode remote observe falls through to UDS today and would run unpinned. Land ONE coherent schema; hook client reads `observe_url`+`fingerprint`; prove file-mode remote observe runs over pinned HTTPS post-fix (AC-08d).
- **Test infrastructure is cumulative.** Extend the existing hook-client / Layer-2 harness and cert-pin/transport fixtures; no isolated scaffolding. Rust validation = protocol gates + release workflows, not GH CI (JS-client-only).

## Dependencies

- **`v:2` bundle + per-slug `.../v1/{slug}` Streamable-HTTP MCP route** — server-ready and frozen (vnc-038 #770; bundle entry #5081). This feature is client-only.
- **#774** (rmcp `allowed_hosts` from `UNIMATRIX_PUBLIC_URL`) — **MERGED** (PR #778, commit `913b78cb`, 2026-06-18). Not a blocker; was a sequencing dependency for Scope A **live** validation only. Scope B never had a #774 dependency.
- **ass-080 (#777)** — research spike grounding the BUILD/DIY (zero-dep) decision, the ~450-LoC budget, and the hybrid flip-bar.
- **Reused JS modules:** `cert-pin.js`, `transport-http.js:150-176` (flush-after-pin reference), `init.js` (`init()`/`initRemote()`/`writeMcpJson` idempotency reference/`printSummary()`), `config.js:276-306` (file-mode `resolve()`), `bundle.js:67-156` (`decodeBundle`).
- **Node stdlib:** `http`, `https`, `net`, `crypto`, `fs`, `path`, `os`.
- **External service:** the cloud's per-slug Streamable-HTTP MCP endpoint (`mcp_url`) — now reachable for live validation (#774 merged).

## NOT in Scope

- At-rest token encryption / OS keychain — cleartext-at-rest accepted; keychain enterprise-deferred.
- A new MCP SDK / heavyweight dependency (`@modelcontextprotocol/sdk`, bundling `mcp-remote`) — rejected by decision.
- Server-side changes — `v:2` bundle and `.../v1/{slug}` route are frozen; #774 was a separate (now merged) server fix.
- MCP over the legacy `--remote`/`--token` env-HTTPS path — unsupported (bundle-only), loud unsupported message only. **The legacy `--remote` user's path forward is to migrate to a `v:2` bundle — legacy is NOT "fixed" here.** Legacy credential relocation writes `fingerprint: null`, legacy observe stays **unpinned**, and legacy gets **no** MCP bridge; both pinned observe and cloud MCP are **bundle-only** capabilities. This is the intended message (human-confirmed), consistent with deprecating env-HTTPS per #773 — the direction is bundle-only, not retrofitting legacy.
- **Auto-wiring for non-Claude-Code MCP clients (Codex/Gemini CLI) — DEFERRED (conscious deferral, not an omission).** The bridge is **client-agnostic / reusable across MCP clients by construction** (a real multi-LLM personal-cloud win, goal #4946 — any MCP client can spawn it as a stdio server). But `init` auto-wires **only** `.mcp.json` (Claude Code). Generating config for other MCP clients is deferred; the multi-LLM-connect criterion is advanced by the reusable bridge even though only Claude Code is auto-wired in this feature.
- Reworking the observe/hook POST mechanics — `transport-http.js`'s request/SSE/pinned-flush plumbing is reused as-is; only the credential **source** moves and the file-mode resolve schema is reconciled.
- Changing the bundle schema — `v:2` already carries `mcp_url`/`observe_url`/`token`/`fp`.
- `#768` stale remote-mode docs — a separate pre-committed fast-follow, not in this diff.

## Alignment Status

Vision-guardian verdict (ALIGNMENT-REPORT.md, 2026-06-18): **PASS** on Vision Alignment, Milestone Fit, Scope Gaps, Architecture Consistency, Risk Completeness. **WARN** on Scope Additions.

- **WARN-1 (accepted) — universal legacy creds relocation.** The out-of-tree store write is **universal**: the legacy `--remote`/`--token` credential is also relocated out of the tree, written with `fingerprint: null`, with the hook client staying **unpinned** for legacy (preserving today's behavior). This touches the legacy-path credential despite SCOPE's "keep legacy as-is" Non-Goal. **Accepted** — the relocation is coherent only if universal (leaving legacy creds in-tree re-opens the exact commit-leak the feature closes); no legacy MCP is wired, so legacy is **not functionally extended**. Posture: universal relocation, `fingerprint: null`, legacy unpinned, no legacy MCP wired. Validated by R-15.
- **Approved-in-scope addition (traceability) — file-mode observe-over-HTTPS break fix.** Reconciling the schema mismatch fixes a **current break** (file-mode remote observe falls through to UDS today; would run unpinned). SCOPE explicitly folds this into Scope B's blast radius; validated by AC-08d / R-06.
- **Posture correction since the report was written:** the ALIGNMENT-REPORT lists "Scope A live validation deferred (#774)" as a Simplification with a `not-validated-live` caveat. **That posture is superseded — #774 has MERGED (PR #778); Scope A live validation is in scope and is a delivery gate.** The simplification and the `not-validated-live` caveat no longer apply.

## Critical-Risk Delivery Gates (from RISK-TEST-STRATEGY)

Non-negotiable — the validation gate verifies these **by name**, not by a green suite (lesson #4970):

- **R-01 / R-02 — trust boundary.** A **live** wrong-pin handshake proving the bearer token **never reaches the server on mismatch** (capturing server asserts zero `Authorization` received), plus **per-socket re-pin** across the persistent session (every TLS socket re-pins before its first body byte; no pooling on an unverified socket), plus **fresh-context security review even on green gates**.
- **R-03 — live, not stub.** Live cloud validation against the real `/v1/{slug}` endpoint is a **delivery gate**; green-on-stub must NOT stand in for live. Stub framing must be provably derived from a captured rmcp response. The **active post-#774 live-validation checklist** (session-id handshake FIRST) is in delivery.
- **R-04 — SSE-skip probe (FIRST delivery experiment, runs LIVE).** Probe `Accept: application/json`-only across the full lifecycle live; if JSON-only, **drop the ~90-LoC `sse-parse` unit and R-04 evaporates**; only if the live probe forces `text/event-stream` is `sse-parse` built.
- **R-06 — prove pinned HTTPS.** File-mode remote observe **actually runs over pinned HTTPS** post-fix (good-pin connects, wrong-pin fail-closed, token never on the wire) — `config.pinnedFp` populated is necessary but **not sufficient**. The file-mode remote-observe error boundary (ARCHITECTURE §9) cross-refs **AC-08d** as its formal wire test — AC-08d is the formal test for that boundary, not a restatement of the wire assertions.
- **R-17 (High) — session/attribution stability.** Within one bridge session the `Mcp-Session-Id` and `clientInfo.name` are proven **identical across all requests**; across distinct project sessions they are proven **distinct**. Live-validated first via the session-id handshake checklist item.
- **Hybrid flip-bar.** Pre-thresholded; runs **AFTER** the SSE probe resolves (a passing probe removes the SSE component from the ~260-LoC surface). Flip only if ALL hold: LoC overrun > ~520, correctness instability persists, and a 30-min SDK re-check is positive. On flip, AC-02 (zero-dep) is knowingly waived and re-approved by the human.
