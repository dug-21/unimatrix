# vnc-039 Architecture — Remote `init` Wiring: Pure-JS stdio→HTTPS MCP Bridge + Out-of-Tree Credential Store

**Feature:** vnc-039 · **Scope:** A (MCP bridge, live-validatable — #774 merged) + B (credential relocation, independent, lands first by risk)
**Status:** Design · **Builds on:** vnc-038 (`v:2` bundle #5081, per-slug route), ass-080 (#777 BUILD/DIY), goal #4946 (personal-cloud)

---

## 1. System Overview

Unimatrix's personal-cloud contract (goal #4946) promises two surfaces over HTTPS: **proactive delivery** (hook events → `observe_url`) and **on-demand curation** (`context_*` tools). Remote attach today delivers only the first; `initRemote()` explicitly skips `.mcp.json`, so `mcp_url` is persisted-but-dead config.

This feature wires the second surface **without asking Claude Code to trust an unpinnable self-signed cert.** Claude Code's native `http`/`streamable-http` MCP transports run CA-chain verification first and reject a self-signed leaf (`DEPTH_ZERO_SELF_SIGNED_CERT`) before any fingerprint pin can apply (entry #5105, the F1 wall). The house pattern for daemon-backed MCP (entry #1897: local UDS daemon) is a **thin stdio bridge** — `.mcp.json` spawns a lightweight stdio process that owns the backend connection, so Claude Code sees an ordinary stdio server and never does TLS itself. The remote analogue is a JS stdio→HTTPS bridge that owns a pinned TLS connection to `mcp_url` and translates stdio JSON-RPC ⇄ the cloud's Streamable-HTTP MCP endpoint.

Two scopes, deliberately decoupled:

- **Scope A — MCP bridge.** A new pure-Node-stdlib bridge process restoring the `context_*` surface over the cloud. ~450 LoC across five areas (ass-080); TLS/pinning reused from `cert-pin.js` at ~0 net LoC. **Live-validatable now** — #774 merged (PR #778, commit 913b78cb), wiring rmcp `allowed_hosts` from `UNIMATRIX_PUBLIC_URL`, so remote MCP requests no longer 403.
- **Scope B — credential relocation.** Move the bearer credential out of the repo working tree (`.claude/settings.local.json`) to a unimatrix-owned, out-of-tree, per-key store. Independent (no reachable-cloud dependency) and lower-risk; **lands first by risk/independence** (SR-05). Reconciles a pre-existing write/read schema mismatch (SR-07) and resolves the keying ambiguity (SR-08) as a fixed constraint.

The boundary between them: **Scope B owns the credential store (write + both reads); Scope A owns the bridge process and consumes the store.** The bridge reads its config from the store at spawn time — the token is never on the command line and never in `.mcp.json` (OQ-3).

```
                    ┌─────────────────────────────────────────────────────────┐
                    │  init --bundle <v:2>   (initRemote, bin → lib/init.js)   │
                    └───────────────┬──────────────────────┬──────────────────┘
                       Scope B write │       Scope A write   │
                                     ▼                       ▼
        ┌───────────────────────────────────────┐   ┌──────────────────────────┐
        │  OUT-OF-TREE CREDENTIAL STORE (0600)   │   │  <project>/.mcp.json      │
        │  ~/.unimatrix/<projectHash>/           │   │  mcpServers.unimatrix =   │
        │      remote.json  (colocated)          │   │   { stdio: node bridge,   │
        │  { schema_version, mcp_url,            │   │     args:[<projectHash>], │
        │    observe_url, token, fingerprint,    │   │     NO token }            │
        │    timeouts? }                         │   └────────────┬─────────────┘
        └──────────────┬──────────────┬──────────┘                │ spawn
              read      │              │ read                      ▼
       (hook/observe)   │              │ (bridge)      ┌──────────────────────────────┐
                        ▼              └──────────────▶│  lib/hook-client/mcp-bridge.js│
        ┌───────────────────────────┐                 │  stdio JSON-RPC ⇄ Streamable- │
        │ hook-client transport-http │                 │  HTTP, pinned TLS to mcp_url  │
        │  POST observe_url (pinned) │                 │  (reuses cert-pin.js +        │
        └───────────────────────────┘                 │   pinned-flush pattern)       │
                        │                              └───────────────┬──────────────┘
                        ▼                                              ▼
                 ☁  CLOUD  /v1/{slug}/observe              ☁  CLOUD  /v1/{slug}  (MCP)
```

---

## 2. Component Breakdown

| # | Component | New/Modified | Scope | Responsibility |
|---|-----------|--------------|-------|----------------|
| C1 | **`credstore.js`** (`lib/hook-client/credstore.js`) | NEW | B | Sole owner of the out-of-tree credential store (`~/.unimatrix/<projectHash>/remote.json`): canonical path derivation, read, idempotent merge-write, mode 0600. One schema, one key. |
| C2 | **`mcp-bridge.js`** (`lib/hook-client/mcp-bridge.js`) | NEW | A | The stdio↔Streamable-HTTP translation process. Owns the pinned TLS connection, MCP lifecycle, session capture/replay, dual response framing, fail-loud-on-mismatch. |
| C3 | **bridge entrypoint** (`bin/unimatrix.js`) | MODIFIED | A | Routes `mcp-bridge` subcommand to C2 in JS (never the Rust binary). OQ-4 → subcommand. |
| C4 | **`initRemote()` + `.mcp.json` writer** (`lib/init.js`) | MODIFIED | A+B | On bundle path: writes the credential via C1 (B) instead of `writeRemoteSettingsLocal`; writes the stdio `.mcp.json` bridge entry (A) instead of the "Skipped .mcp.json" line. On legacy path: loud unsupported message. |
| C5 | **hook-client `resolve()`** (`lib/hook-client/config.js`) | MODIFIED | B | File-mode branch repointed from in-tree `.claude/settings.local.json` to C1's store; reads canonical schema (`observe_url`+`fingerprint`, not `url`); preserves env-pair override and UDS fall-through. Fixes the current unpinned/UDS-fallback break (file-mode remote observe does not run today); validated behaviorally — observe must run over pinned HTTPS. |

**Unchanged (load-bearing, do not touch):** `cert-pin.js`, `transport-http.js` (the `post`/`pingForInit` observe path is untouched — A3), `bundle.js` (no schema change), the local (non-remote) `init()` flow.

---

## 3. Scope A — The Bridge Translation Architecture

### 3.1 Process shape

`mcp-bridge.js` is a **thin, single-session stdio process** spawned by Claude Code per session (the entry #1897 house shape). It is **not** a byte forwarder (the local UDS bridge #2582 is, because UDS speaks newline-delimited JSON identically to stdio). The remote bridge **terminates and re-frames**: it reads newline-delimited JSON-RPC on stdin, POSTs each message to `mcp_url` over pinned HTTPS, parses `application/json` or `text/event-stream` responses, and writes JSON-RPC responses to stdout.

The five translation areas (ass-080 LoC budget; TLS is reused, **not** in the budget):

| Area | LoC | Module region | Risk (SR-01) |
|------|-----|---------------|--------------|
| stdio framing — newline-delimited JSON-RPC on stdin/stdout | ~80 | `stdio-frame` | Low |
| HTTP request + `Mcp-Session-Id` capture/replay (stable identity contract, §3.3) | ~120 | `http-session` | **High** |
| SSE (`text/event-stream`) parser — **CONTINGENT** on the SSE-skip probe (§3.4); DROPPED if the probe passes | ~90 | `sse-parse` | **High** |
| json / SSE response dispatch + id-correlation | ~80 | `dispatch` | Med |
| MCP lifecycle (`initialize`→session→`tools/list`,`tools/call`; stable `clientInfo.name`, §3.3) | ~80 | `lifecycle` | Low |

**SSE-skip delivery probe (FIRST delivery task — per OQ-1 JSON-first).** `sse-parse` is the hardest correctness surface and dominates SR-01. Before building it, delivery runs a probe: request with `Accept: application/json` **only** and exercise the full lifecycle (`initialize → tools/list → tools/call`) against the rmcp endpoint. **If every step returns `application/json`** (the endpoint never forces `text/event-stream`), the `sse-parse` unit is **DROPPED** (~90 LoC of the hardest hand-rolled code removed; `dispatch` keeps a JSON-only path). **If the probe fails** (SSE forced on any step under JSON-only `Accept`), `sse-parse` is built as designed — it remains a **designed contingency, not removed**. With #774 merged the probe **runs LIVE directly against the real endpoint as the first delivery task** (a fast pre-check reads the in-repo rmcp server source — `crates/unimatrix-server` + the `/observe` content-negotiation wired in vnc-024/PR #686 — but the live probe is the definitive answer). So the JSON-vs-SSE question is settled early in delivery, not deferred: `sse-parse` is built only if the live probe forces SSE.

### 3.2 TLS reuse — the trust boundary (SR-02, ADR-001)

The bridge **reuses `cert-pin.js` and the `transport-http.js:150-176` pinned-flush pattern verbatim** — it never re-implements TLS trust. The invariant (ADR-001 of this feature) is: **the bearer token is written to the wire only after the leaf fingerprint matches `fp`.** Concretely, the bridge's request path:

1. `applyCertPin(options, true, pinnedFp)` → `rejectUnauthorized:false`, `ca:undefined` (complete the self-signed handshake; the pin is the trust model).
2. On `req.on('socket')` → `s.once('secureConnect')` → `verifyPeerFingerprint(s, pinnedFp)`.
3. On mismatch: `req.destroy(err)` and fail **loud** (the bridge surfaces the diagnosable expected-vs-presented error to stderr and exits non-zero) — the body is never flushed.
4. On match only: `req.end(body)` — the `Authorization: Bearer` header and JSON-RPC body reach the wire.

**Critical divergence from the observe path (A3):** the observe client is single-shot, fail-**open** (exit 0, fall to UDS). The bridge is a **persistent connection, fail-loud**. This changes two things and both are ADR-bound:

- **Connection lifecycle.** A persistent Streamable-HTTP session means the pin must be verified on **every** new TLS socket the bridge opens, not just one. ADR-001 mandates: any new socket re-runs `verifyPeerFingerprint` on `secureConnect` before its first body byte; keep-alive reuse is allowed only on an already-pinned socket. The bridge MUST NOT open a connection-pool agent that could flush a request on an un-verified socket.
- **Fail-loud, not fail-open.** A broken pin on the bridge surfaces to the user (stderr + non-zero exit) — a dead `context_*` surface with a diagnosable cause, never a silent degrade (contrast the observe path's UDS fall-through). This is testable: a wrong-pin handshake test asserts (a) connection refused, (b) token never written to the wire.

### 3.3 Streamable-HTTP session contract (entry #4708/#4706)

The transport-level **`Mcp-Session-Id` HTTP header** (server-managed UUID, distinct from any agent-declared tool-param session_id — entry #4708) is the session handle. The bridge:

- Sends `initialize` (no session header) → captures `Mcp-Session-Id` from the response headers.
- Replays that header on **every** subsequent request (`tools/list`, `tools/call`, …).
- On session teardown (stdin EOF / process exit), sends `DELETE` with the session header (best-effort).

**Stability is an attribution contract (ADR-001).** The server keys audit attribution on the `Mcp-Session-Id` header **and** the `initialize` `clientInfo.name` (vnc-014 / #4708). The bridge MUST present a **stable** identity: the captured session id is replayed byte-for-byte for the whole process lifetime (never rotated/regenerated mid-session), and `clientInfo.name` is a fixed bridge identifier (not per-spawn random/timestamped). Unstable identity causes **cross-session attribution bleed** in the server's audit trail, undercutting the 1-client:1-project integrity basis (vnc-034 A1). Validated by a byte-stability assertion across `initialize → tools/list → tools/call` within one process.

> **DELIVERY CHECKPOINT — rmcp 1.7.0 session-id handshake validation.** The `Mcp-Session-Id` capture/replay handshake above is a live-validation checkpoint, not a build-only behavior. It is sequenced as the post-#774 live-validation checklist item #1 (see SPECIFICATION.md / RISK-TEST-STRATEGY) and is the wire-behavior under AC-12. This is a pointer to that existing sequencing — no substance is restated here.

### 3.4 Dual response framing (AC-06)

Dispatch on response `Content-Type`:
- `application/json` → single JSON-RPC response object → write to stdout (newline-framed).
- `text/event-stream` → SSE line parser (`event:`/`data:`/`id:`, blank-line record boundary, `Last-Event-ID` for resumable streams) → each `data:` JSON-RPC payload → stdout. The 1 MiB body guard and timeout constants are reused from `transport-http.js`. **This branch is contingent (§3.1 probe):** if the SSE-skip probe shows the rmcp endpoint answers the full lifecycle in `application/json` under JSON-only `Accept`, the `text/event-stream` branch and the `sse-parse` unit are dropped and `dispatch` is JSON-only.

### 3.5 Dumb-client invariant (AC-05, ADR-001 vnc-038)

The bridge POSTs to `mcp_url` **verbatim** — it composes no path, derives no slug, appends nothing. `mcp_url` is the server-composed `.../v1/{slug}` endpoint from the validated `v:2` bundle, persisted into the store by C4. This invariant is the vnc-038 spine carried onto the new surface.

---

## 4. Scope B — The Out-of-Tree Credential Store

### 4.1 The keying decision (OQ-6 / SR-08) — RESOLVED: `projectHash`

**Both consumers index the store by `projectHash`** = `computeProjectHash(projectRoot)` (first 16 hex of SHA-256 over the realpath'd project root; `config.js:123`). This is a **fixed constraint, not an open question.**

Rationale (the SR-08 trap is "two consumers, two keys → one silently fails to resolve"):
- The hook/observe client (C5) has **no slug** at runtime. It never decodes a bundle; it only walks to the project root and hashes it. Keying on slug would force the hook client to learn/derive slug — new machinery, new failure mode.
- `projectHash` is already the codebase's per-project key: `~/.unimatrix/<projectHash>/` holds the UDS socket and state dir (`config.js:170-200`). The store slots into the **same directory**, reusing the existing derivation both consumers already share.
- The slug (server-authoritative, in `mcp_url`) is **not discarded** — it is encoded *inside* `mcp_url`, which the store carries. The bridge gets the slug for free by posting `mcp_url` verbatim. The slug is **payload, not key.**
- `init` writes the store using the **same `projectHash` derivation** (it already calls `detectProjectRoot`; it computes the hash via the shared `computeProjectHash`). Write-key and read-key are computed by one function → they cannot disagree.

**Constraint:** store key = `projectHash`. The slug lives in `mcp_url` (payload). Neither consumer keys by slug.

### 4.2 Store path/layout (OQ-6) — RATIFIED: `~/.unimatrix/<projectHash>/remote.json` (colocated)

The store is **colocated** in the existing per-project `~/.unimatrix/<projectHash>/` directory as `remote.json` (mode 0600), alongside the existing per-project state — **not** a separate `credentials.json`, **not** a new XDG path:

```
~/.unimatrix/<projectHash>/
    unimatrix.sock          (existing — local UDS)
    hook-client/            (existing — state dir)
    remote.json             (NEW — mode 0600, this feature; the remote-attach credential + endpoints)
```

- **Colocated, per-project file**, not a global `slug→entry` map. Each project's hash directory holds exactly one `remote.json`, sitting next to the UDS socket and state dir for the same project. This matches the existing per-project state layout, keeps mode-0600 scoping per file, and makes idempotent re-`init` a single-file rewrite (no map-merge race across projects — AC-08b is satisfied by directory separation, not by in-file keying).
- **Out-of-tree** under `os.homedir()`; mode 0600 on write and re-asserted (the `writeRemoteSettingsLocal` chmod pattern, `init.js:249`). No-homedir → the same null/terminal posture the existing `socketPathFor` uses.
- Not under XDG `~/.config/` and not a parallel `credentials.json` file: the precedent root is `~/.unimatrix/<projectHash>/`; splitting the credential into a separate file or a `~/.config/` tree while state stays in `~/.unimatrix/` would fork the per-project root and break the "single derivation" invariant the hook client relies on. One root, one hash, one place — `remote.json` colocated with the rest of the project's state.

### 4.3 The canonical schema (SR-07) — RESOLVED: reconcile, don't port

**This is a current break in shipped code, not a latent risk.** Today the writer emits `{mcp_url, observe_url, token, fingerprint}` but the hook client's file-mode remote observe path reads `unimatrix.remote.url` (a key **never written**) and **never reads `fingerprint`**. Right now, in consequence: (a) the file-mode guard fails on the absent `url` and the path **silently falls through to UDS** — file-mode remote observe does not actually run today; (b) `config.pinnedFp` is **never populated**, so if it did resolve it would POST the bearer **unpinned**. Both are active in the current code. The relocation rewrites exactly this load/store pair, so the schema is reconciled here, not carried forward.

**Canonical `remote.json` schema (both consumers read this):**

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

- `schema_version` future-proofs the store (an unknown version is a terminal, diagnosable read failure — not a silent skip).
- `observe_url` replaces `url`. The hook client now reads `observe_url` as its post target (this is the schema fix — the old `url` key is gone).
- `fingerprint` is **read** by both consumers (the hook client newly populates `config.pinnedFp` from it — this fixes the current break; the observe POST resolves to the http file path instead of falling to UDS, and becomes actually pinned). `timeouts` is optional; absent → `DEFAULT_TIMEOUTS`.
- `mcp_url` is read only by the bridge; `observe_url`+`timeouts` only by the hook client; `token`+`fingerprint` by both. One schema, no per-consumer dialects.

### 4.4 hook-client `resolve()` repointing (C5, AC-08c)

The file-mode branch of `resolve()` (`config.js:275-306`) is repointed. Precedence order is **preserved exactly**:

1. **Env pair** (`UNIMATRIX_REMOTE_URL`/`_TOKEN`) → http, wins outright (unchanged — `config.js:263-273`). Env-mode remains unpinned by design (legacy override).
2. **Store file** `~/.unimatrix/<projectHash>/remote.json` (was: `<root>/.claude/settings.local.json`). Read canonical schema → `okHttp(observe_url, token, timeouts, "file", …)` **plus `pinnedFp: fingerprint`** threaded into the resolved config so `transport-http.post` pins. Parse failure (non-ENOENT) → terminal `malformed`; ENOENT or incomplete → UDS fall-through (unchanged semantics).
3. **UDS fall-through** (unchanged).

`okHttp` gains a `pinnedFp` field (it is already plumbed through `post` via `config.pinnedFp`; today it is simply never set on the file path — that is the bug). This is the minimal change that makes the observe path actually pinned.

**Required post-fix validation (behavioral, not field-presence).** Asserting that `config.pinnedFp` is populated is necessary but **not sufficient**. The architecture REQUIRES validating that file-mode remote observe **actually runs over pinned HTTPS** end to end: with a `remote.json` carrying a `fingerprint`, the observe POST must (a) resolve to the http file path and **not** fall through to UDS, (b) target `observe_url`, and (c) flush the bearer only after the leaf fingerprint matches — good-pin round-trips, wrong-pin is rejected with the token never on the wire. A `pinnedFp`-is-set shape check would have passed vnc-034's dead-pin false-green; the test must exercise wire behavior.

### 4.5 `.mcp.json` write contract (AC-07, AC-09, SR-09)

A remote analogue of `writeMcpJson` (`init.js:59-104`). Idempotent, merge-preserving, `--dry-run`-aware, malformed-JSON-throws. The entry:

```json
{
  "mcpServers": {
    "unimatrix": {
      "command": "node",
      "args": ["<abs path to mcp-bridge entry>", "<projectHash>"],
      "env": {}
    }
  }
}
```

**No token, no `mcp_url`, no `fp` in `.mcp.json`** (AC-09). The only argument is `projectHash` — the store key. The bridge reads the credential from the store at spawn time. `.mcp.json` is committable with zero secret exposure.

### 4.6 Migration & legacy cleanup (OQ-5 residual)

On bundle-path `init`, if a prior `.claude/settings.local.json` carries `unimatrix.remote`, C4 **deletes that subtree** (merge-preserving — other `unimatrix.*` and Claude Code keys survive) after writing the new store, so no stale in-tree credential lingers. `gitignoreWarning` is **removed** (no in-tree creds file to warn about — AC-08). Best-effort; failure to clean does not abort init.

---

## 5. OQ-4 — Bridge Entrypoint Shape — RESOLVED: `unimatrix mcp-bridge` subcommand

**Decision:** a new `mcp-bridge` subcommand in `bin/unimatrix.js`, routed to JS (C2), **not** a direct `node <bridge-path>` command in `.mcp.json`.

**JS routing is REQUIRED, not preferred.** Every non-`init` subcommand `execFileSync`s the Rust binary (`bin/unimatrix.js:36-54`), which a non-Linux / remote-only pure-JS client does not ship. A `mcp-bridge` left to fall through to that exec block would **throw on exactly the clients the bridge exists to serve** — the self-signed remote-cloud population that cannot use Claude Code's native TLS. The early-return JS branch is the only shape that runs on that population; it is a correctness requirement, not a stylistic preference.

Rationale:
- **Stable command across installs.** `.mcp.json` records `command:"node", args:["<abs bridge path>", …]` either way — but with a subcommand the resolved path is computed by `init` from `require.resolve("./hook-client/mcp-bridge.js")` (the same contract `initRemote` already uses for the hook client path, `init.js:409`), so it is correct per-install. Both shapes ultimately spawn `node <path>`; the subcommand additionally gives a **discoverable, documented CLI surface** (`unimatrix mcp-bridge <projectHash>`) for debugging the bridge by hand.
- **Mirrors the local pattern.** The local `init` writes `command:<rust binary>` for MCP. The remote analogue writing `command:node, args:[bridge]` is the parallel, and routing `mcp-bridge` through `bin/unimatrix.js` keeps a single CLI entrypoint (consistency with how `init` is the one JS-handled subcommand today).
- **The Rust exec path is the hazard, not the JS branch.** `bin/unimatrix.js` already special-cases `init` to JS before the Rust `execFileSync` fallthrough (`unimatrix.js:10`). Adding `mcp-bridge` is the same early-JS-branch pattern. The OQ-4 worry is inverted by the client reality: a subcommand that *reaches* the Rust binary is the failure mode (no binary on the bridge's non-Linux/remote-only clients → throw), so routing to JS via early return is mandatory.
- The `.mcp.json` entry still writes the **resolved absolute bridge path** in `args` (not the literal subcommand string) so Claude Code spawns `node <path> <projectHash>` directly without re-entering `bin/unimatrix.js` — the subcommand is the human/debug surface; `.mcp.json` targets the module directly for a lean spawn. (Either the subcommand or the direct path resolves to the same module; `init` writes whichever is leaner to spawn — the direct module path — while the subcommand exists for discoverability.)

---

## 6. Bundle-Only Cloud-MCP Boundary (AC-10, OQ-2, SR-06)

Cloud MCP is **bundle-only** (`v:2`). The legacy `--remote`/`--token` path has no `fp` pin (#773); bridging it would write the bearer over an unverified TLS connection — a security regression — and adding MCP to legacy violates vnc-038's "keep legacy working, don't extend it."

- On the **bundle path**, C4 writes the store (B) and the `.mcp.json` bridge entry (A).
- On the **legacy path**, C4 writes **no** bridge entry and emits a **loud, deterministic** message: cloud MCP requires a `v:2` bundle. Not a silent skip (SR-06). The legacy observe path continues to work unchanged (it has always been unpinned; this feature does not extend it). This resolves #773 by deprecating env-HTTPS for cloud-attach MCP.

Note: Scope B's store write also applies on the legacy path's credential (the relocation is universal — no in-tree creds for any remote path). But the legacy credential carries `fingerprint: null`; the hook client's pin stays unset for legacy (preserving today's unpinned-legacy behavior — the schema fix pins **bundle** credentials, which now correctly carry `fingerprint`).

---

## 7. Scope A/B Independence & Sequencing (SR-05, AC-11)

The component boundary enforces independence:

- **Scope B (C1, C5, the store-write half of C4, legacy message half of C6)** ships **without a reachable cloud.** Validatable by: write the store on `init --bundle`, assert nothing token-bearing lands in the repo tree (`git status` clean of secrets), assert both consumers resolve from the store (hook client pins + posts; bridge config-loads). No #774 dependency. **Lands first.**
- **Scope A (C2, C3, the `.mcp.json`-write half of C4)** depends on B for its config source. Its **live** validation is now available — #774 merged (PR #778, commit 913b78cb), so remote MCP requests reach the rmcp endpoint instead of 403ing. The SSE-skip probe and the bridge round-trip can be validated live in delivery.

Sequencing: **B → A, by risk and independence.** B is the independent, lower-risk fix (it removes a current live commit-leak vector and needs no reachable cloud), so it lands first; A follows and is now live-validatable in the same delivery. B-first is a risk/independence ordering, no longer a #774 block (SR-05).

---

## 8. Integration Surface

| Integration Point | Type / Signature | Source | Consumer(s) |
|-------------------|------------------|--------|-------------|
| `decodeBundle(raw)` | `→ {v:2, mcp_url, observe_url, token, fp}` | `lib/hook-client/bundle.js:67` (unchanged) | C4 (`initRemote` resolve) |
| `computeProjectHash(projectRoot)` | `(string) → string` (16 hex) | `lib/hook-client/config.js:123` (export, unchanged) | C1 write, C4, C5 — **the store key** |
| `detectProjectRoot(startDir)` | `(string) → string` | `lib/init.js:25` (export) | C4 (write-side project root) |
| `walkToProjectRoot(startDir)` | `(string) → string` | `lib/hook-client/config.js:44` | C5 (read-side project root) |
| `applyCertPin(options, isTls, pinnedFp)` | mutates+returns https options | `lib/hook-client/cert-pin.js:131` (unchanged) | C2 bridge |
| `verifyPeerFingerprint(socket, pinnedFp)` | `→ Error\|null` | `lib/hook-client/cert-pin.js:67` (unchanged) | C2 bridge |
| `computeFingerprint(derBuffer)` | `(Buffer) → "sha256:"+hex` | `lib/hook-client/cert-pin.js:26` | C2 (parity) |
| **`credstore.write(projectHash, cred, {dryRun})`** | `cred = {mcp_url, observe_url, token, fingerprint, timeouts?}` → `string[]` actions, mode 0600 | **NEW C1** | C4 |
| **`credstore.read(projectHash)`** | `→ {schema_version, mcp_url, observe_url, token, fingerprint, timeouts?} \| null` (null on ENOENT; throws on malformed/unknown version) | **NEW C1** | C2 bridge, C5 hook client |
| **`credstore.pathFor(projectHash)`** | `(string) → string\|null` (null on no-homedir) | **NEW C1** | C1, tests |
| **bridge stdin** | newline-delimited JSON-RPC (MCP) | Claude Code → C2 | C2 |
| **bridge stdout** | newline-delimited JSON-RPC responses | C2 → Claude Code | C2 |
| **bridge argv** | `node <bridge> <projectHash>` | C4 `.mcp.json` | C2 |
| `Mcp-Session-Id` (HTTP header) | server UUID, capture on `initialize` resp, replay on all subsequent | cloud `/v1/{slug}` (entry #4708) | C2 |
| `okHttp(...)` resolved config | gains `pinnedFp` field from `fingerprint` | `lib/hook-client/config.js:203` (MODIFIED) | C5 → `transport-http.post` |
| `config.pinnedFp` | consumed by pinned-flush | `transport-http.js:117` (unchanged) | observe path |
| `.mcp.json` entry | `{command:"node", args:[<bridge path>, <projectHash>], env:{}}` — **no token** | C4 write | Claude Code |

### Canonical store schema (the contract both reads obey)

```
remote.json (mode 0600, ~/.unimatrix/<projectHash>/, colocated with unimatrix.sock + hook-client/)
  schema_version : 1            (unknown → terminal read error)
  mcp_url        : https URL    (bridge reads; posted verbatim)
  observe_url    : https URL    (hook client reads; post target)
  token          : 64 hex       (both read; Authorization: Bearer)
  fingerprint    : sha256:64hex | null  (both read; pin. null = legacy/unpinned)
  timeouts?      : {connect_ms, sync_ms, fnf_ms}  (hook client; absent → defaults)
```

---

## 9. Error Boundaries

| Origin | Behavior | Posture |
|--------|----------|---------|
| Bundle decode (C4, trust boundary) | `BundleError`, token-free message, throws → init exits 1 | loud (init) |
| Store write fail (C1) | throw → init exits 1 (creds must persist) | loud (init) |
| Store read: ENOENT (C1) | `null` → UDS fall-through (hook) / bridge exits with diagnosable "no credential for project" | hook fail-open / bridge loud |
| Store read: malformed/unknown schema_version (C1) | throw → hook terminal `malformed`; bridge exits non-zero | loud |
| **Cert-pin mismatch (C2 bridge)** | `verifyPeerFingerprint` Error → socket destroyed **before body** → stderr diagnosable (expected-vs-presented) → exit non-zero | **loud, fail-closed (SR-02)** |
| Cert-pin mismatch (observe, C5→transport) | connect-class failure → breadcrumb → exit 0 | fail-open (unchanged) |
| Streamable-HTTP 4xx/5xx (C2) | surfaced as JSON-RPC error on stdout | per-request |
| Legacy path cloud-MCP request (C4) | loud deterministic unsupported message, no bridge wired | loud (AC-10) |

The file-mode remote-observe path's error-boundary behavior (the cert-pin mismatch / pinned-flush row above, and the §4.4 behavioral validation requirement) is formally tested by the **file-mode remote-observe-over-pinned-HTTPS wire test mandated by AC-08d** in SPECIFICATION.md. That acceptance criterion is the formal test for this boundary — this section points at it rather than restating the wire-behavior assertions.

---

## 10. Decision Summary (ADR Index)

| ADR | Decision |
|-----|----------|
| ADR-001 | Bridge translation architecture + fail-loud pinned-flush trust contract (SR-01, SR-02) |
| ADR-002 | OQ-4 — `unimatrix mcp-bridge` subcommand entrypoint |
| ADR-003 | OQ-6/SR-08 — store keyed by `projectHash`; colocated path `~/.unimatrix/<projectHash>/remote.json` |
| ADR-004 | SR-07 — single canonical store schema; reconcile the current unpinned/UDS-fallback break; require behavioral pinned-HTTPS validation |
| ADR-005 | Bundle-only cloud-MCP boundary; Scope A/B independence & B-first sequencing (SR-05, SR-06) |

---

## 11. Open Questions for the Human

1. **`mcp-bridge` invocation in `.mcp.json` — direct module path vs. subcommand string.** §5 resolves OQ-4 to a subcommand for discoverability, but specifies `.mcp.json` writes the **resolved module path** (leaner spawn, no re-entry into `bin/unimatrix.js`). If you'd rather `.mcp.json` carry the literal `unimatrix mcp-bridge` form for legibility (at the cost of an extra process hop), that's a one-line flip — flag if preferred. Not a blocker.
2. **Legacy-credential migration aggressiveness (OQ-5 residual).** §4.6 deletes a stale in-tree `unimatrix.remote` subtree on next bundle `init`. If you'd prefer init to be non-destructive (leave the stale file, only stop writing it), say so — the SCOPE leans toward removing the leak at root, which favors deletion. Minor.
3. **`#774` live-validation gate — RESOLVED (merged).** #774 merged (PR #778, commit 913b78cb, 2026-06-18) wiring rmcp `allowed_hosts` from `UNIMATRIX_PUBLIC_URL`; remote MCP requests no longer 403. Scope A is now live-validatable in delivery — the "not-validated-live" caveat (SR-04) no longer applies. Sequencing stays B→A by risk/independence (§7), but A's ACs can be validated live, not stub-only.
4. **OQ-7 — SSE-skip probe gating — RESOLVED (#774 merged).** The probe asked whether the `sse-parse` drop could be settled before #774, since the JSON-only-`Accept` probe was only conclusive against the live (then-403ing) endpoint. With #774 merged the probe **runs LIVE directly as the first delivery task** (§3.1/§3.4) — a fast pre-check reads the in-repo rmcp server source (`crates/unimatrix-server` + the `/observe` content-negotiation wired in vnc-024/PR #686), and the live handshake gives the definitive answer. `sse-parse` stays **contingent** on that now-live-runnable probe: built only if the live lifecycle forces `text/event-stream` under JSON-only `Accept`. The stub phase no longer has to carry `sse-parse` defensively purely because the probe was ungated — the probe resolves the JSON-vs-SSE question early in delivery.
