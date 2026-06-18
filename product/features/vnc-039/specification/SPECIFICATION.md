# SPECIFICATION — vnc-039: Remote `init` Wiring (pure-JS stdio→HTTPS MCP bridge + out-of-tree creds store)

> Source: `product/features/vnc-039/SCOPE.md` (Goals, Non-Goals, Resolved Decisions, Acceptance Criteria AC-01..AC-11, Constraints — all binding). Risk inputs: `SCOPE-RISK-ASSESSMENT.md` (SR-01..SR-09). Architecture (`ARCHITECTURE.md` + ADRs) authored in parallel by vnc-039-agent-1-architect; this spec is written from SCOPE and does not block on it.

## Objective

Restore the on-demand `context_*` MCP surface over HTTPS for cloud/remote-attached edge clients, which remote attach silently drops today (`initRemote()` skips `.mcp.json`). **Scope A** adds a new pure-JS, Node-stdlib stdio→HTTPS MCP bridge that proxies stdio JSON-RPC to the cloud's per-slug Streamable-HTTP MCP endpoint over a fingerprint-pinned TLS connection, and wires it as a `stdio` server in `.mcp.json`. **Scope B** (independent, foldable-first) relocates the bearer credential out of the repo working tree into a unimatrix-owned, per-slug, out-of-tree credential store that both the bridge and the existing hook/observe client resolve from, eliminating the commit-leak vector at its root.

Scope A and Scope B are independent: **Scope B can land first** and carries no `#774` dependency.

---

## Functional Requirements

Each requirement is numbered (FR-NN), testable, and traces to one or more AC-IDs.

### Bridge — MCP lifecycle proxying (Scope A)

- **FR-01 — initialize round-trip.** Given an MCP `initialize` JSON-RPC request arriving on stdin, the bridge POSTs it to `mcp_url` and surfaces a valid MCP `initialize` result on stdout. *(AC-03)*
- **FR-02 — `Mcp-Session-Id` capture.** On the `initialize` response, the bridge captures the `Mcp-Session-Id` HTTP response header and retains it for the lifetime of the bridge process. *(AC-03; Constraint: Streamable-HTTP session contract; entry #4708)*
- **FR-03 — `Mcp-Session-Id` replay.** On every subsequent request to `mcp_url`, the bridge sets the captured `Mcp-Session-Id` request header. *(AC-03)*
- **FR-03a — stable session identity (attribution integrity).** Within one bridge process, the `Mcp-Session-Id` and the `clientInfo.name` the bridge sends are **stable** — they do not vary request-to-request. The session id is the value captured at `initialize` (FR-02), replayed unchanged; `clientInfo.name` is a constant for the session. The server keys audit attribution on these (vnc-014 / #4708), so unstable identity causes cross-session attribution bleed that undercuts 1-client:1-project integrity. *(AC-12; Constraint: Streamable-HTTP session contract; entries #4708/#4355)*
- **FR-04 — `tools/list` proxy.** A `tools/list` request, after a completed `initialize`, returns the full `context_*` tool set (parity with the local Rust-binary stdio server: `context_search`, `context_get`, `context_store`, and the rest of the surface the cloud exposes). *(AC-03; Goal 1)*
- **FR-05 — `tools/call` round-trip.** A `tools/call` for `context_search`, `context_get`, and `context_store` round-trips: request on stdin → POST to `mcp_url` → JSON-RPC result on stdout. *(AC-03)*
- **FR-06 — verbatim posting (dumb-client invariant).** The bridge POSTs to `mcp_url` **verbatim** after the request is read — it composes no URL path, derives no slug, and appends nothing. `mcp_url` is the server-composed `.../v1/{slug}` Streamable-HTTP endpoint read from the validated `v:2` bundle. *(AC-05; Constraint: dumb-client invariant, vnc-038 spine)*

### Bridge — stdio framing and response handling (Scope A)

- **FR-07 — stdio JSON-RPC framing.** The bridge reads and writes **newline-delimited JSON-RPC** on stdin/stdout (one JSON-RPC message per line, identical framing to the local UDS daemon's newline-delimited JSON, entry #2582). Partial lines are buffered until a newline completes a message. *(AC-03, AC-06)*
- **FR-08 — `application/json` response handling (JSON-first primary path).** The bridge requests JSON responses (`Accept: application/json`) and, when the Streamable-HTTP response carries `Content-Type: application/json`, parses the JSON body and writes the JSON-RPC result to stdout. This is the **primary, always-required** response path. *(AC-06; OQ-1 JSON-first)*
- **FR-09 — `text/event-stream` (SSE) response handling — CONTINGENT on the first-delivery probe (probe now runs live).** SSE handling is **contingent**, not unconditionally required (per OQ-1 JSON-first lifecycle). A first-delivery probe — an `Accept: application/json`-only content-negotiation probe — MUST determine whether `Accept: application/json` yields JSON across the **full** MCP lifecycle (`initialize`, `tools/list`, `tools/call`). **With #774 merged (PR #778), this probe is no longer #774-gated: it runs live early in delivery for a definitive answer** (a fast in-repo pre-check is the rmcp `/observe` content-negotiation observed in vnc-024 / PR #686). **If the live probe confirms JSON-only across the full lifecycle, the SSE parser is dropped** (FR-09 is not implemented). **Only if the probe fails** — i.e., the server returns `text/event-stream` for any lifecycle step despite `Accept: application/json` — does FR-09 apply: the bridge then parses SSE framing (`event:`/`data:` lines, blank-line event boundaries), reassembles the `data` payload(s) into JSON-RPC message(s), and writes each result to stdout. The SSE FRs are retained as required-only-if-the-probe-fails; they are not deleted. *(AC-06; OQ-1 JSON-first; Constraint: Streamable-HTTP session contract; SR-01)*

### Bridge — TLS / cert-pin (Scope A)

- **FR-10 — owned pinned TLS connection.** The bridge owns its TLS connection to `mcp_url`: it completes the self-signed handshake with `rejectUnauthorized:false` and manually verifies the leaf fingerprint against the bundle's `fp` on `secureConnect`, reusing `cert-pin.js` (`computeFingerprint`, `verifyPeerFingerprint`, `applyCertPin`) — it does not re-implement TLS trust. *(AC-04; Constraint: cert-pin trust model, vnc-038 ADR-002 / F1)*
- **FR-11 — pinned-flush contract (token after pin only).** The bearer token is written to the wire **only after** the leaf fingerprint matches `fp`. The token-bearing request body/headers MUST NOT be flushed before the pin is verified. This mirrors the `transport-http.js:150-176` flush-after-pin pattern. *(AC-04, AC-09; Constraint: cert-pin trust model; SR-02)*
- **FR-12 — fail-loud on pin mismatch.** On fingerprint mismatch, the bridge destroys the socket **before any token-bearing byte is sent**, and fails loud with a diagnosable message reporting expected-vs-presented fingerprint. The bridge does not silently degrade or retry unpinned. The mismatch message MUST NOT contain the bearer token. *(AC-04, AC-09; Constraint: NFR-06; SR-02)*
- **FR-13 — bearer forwarding.** On every request (after pin match per FR-11), the bridge presents `Authorization: Bearer <token>`, where `<token>` is read from the out-of-tree credential store at spawn time (FR-21). *(AC-04, AC-09)*

### `init` — `.mcp.json` remote-write contract (Scope A)

- **FR-14 — wire the bridge as a stdio server.** On the remote **bundle** path, `initRemote()` writes a `unimatrix` **stdio** MCP server entry into `.mcp.json` whose command invokes the JS bridge (not the Rust binary), configured to target the persisted `mcp_url`/`fp` and resolve the token from the out-of-tree store. The prior "Skipped .mcp.json: remote mode does not register a local MCP server" line is removed for the remote bundle path. *(AC-01; Goal 3)*
- **FR-15 — idempotent + merge-preserving write.** Re-running `init` does not duplicate the `unimatrix` entry and does not clobber other MCP servers already present in `.mcp.json`. The write mirrors the local `writeMcpJson` idempotency contract (`init.js:59-104`). *(AC-07; SR-09)*
- **FR-16 — dry-run aware.** When `--dry-run` is set, `initRemote()` reports the intended `.mcp.json` change without writing it. *(AC-07)*
- **FR-17 — no token in `.mcp.json`.** `.mcp.json` references only the bridge command and the project slug/path (and the bridge's config-resolution inputs) — **never** the bearer token, neither on the command line nor in the file. *(AC-09; Resolved OQ-3)*

### `init` — legacy path (Scope A)

- **FR-18 — legacy `--remote`/`--token` is unsupported for cloud MCP.** On the legacy env-HTTPS path (no `fp` pin, #773), `init` does **not** wire the bridge and does **not** write a `unimatrix` MCP entry. *(AC-10; Resolved OQ-2 → bundle-only; Constraint)*
- **FR-19 — loud, deterministic unsupported message.** On the legacy path, `init` emits a loud, deterministic message stating that cloud MCP requires a `v:2` bundle (it is not a silent skip). The message text and the command's exit behavior are deterministic and assertable. *(AC-10; SR-06)*

### Credential store — relocation (Scope B)

- **FR-20 — write credential to out-of-tree per-slug store.** On remote `init`, the bearer token (plus `mcp_url`, `observe_url`, `fp`) is written **only** to a unimatrix-owned, out-of-tree credential store under the user's home (XDG `~/.config/unimatrix/` or `~/.unimatrix/`), mode `0600`, keyed by project **slug**. It is written to **no** path inside the repo working tree and **not** to `.claude/settings.local.json`. *(AC-08; Constraint: credential out-of-tree)*
- **FR-21 — bridge resolves from the store.** The bridge reads `mcp_url`/token/`fp` from the out-of-tree store at spawn time, keyed by the project slug. *(AC-08c; Resolved OQ-3)*
- **FR-22 — hook/observe client resolves from the store.** The hook/observe client's file-mode `resolve()` (`config.js:276-306`) is repointed at the out-of-tree store: it loads token + `observe_url` (+ `fp`/timeouts) for the project's slug and POSTs to `observe_url` unchanged. Its `UNIMATRIX_REMOTE_URL`/`_TOKEN` env-var-pair override and UDS fall-through are preserved. *(AC-08c; Constraint: both consumers one store)*
- **FR-23 — single coherent store schema (reconcile the mismatch; fixes a CURRENT break).** The store has **one** coherent schema that both consumers read. The pre-existing write/read mismatch MUST be reconciled, not ported forward: today the writer emits `{mcp_url, observe_url, token, fingerprint}` (`init.js:230-240`) while the hook client's file-mode `resolve()` reads `unimatrix.remote.url` and **never reads `fingerprint`** (`config.js:298-306`). The writer never populates `url`, so file-mode `resolve()` does not match and the path **falls through to UDS today** — and even when it would resolve, `config.pinnedFp` is unpopulated, so the file-mode remote observe path **does not run over pinned HTTPS today and would be unpinned**. This is a **current break, not a latent one**: reframe the prior "Scope B doesn't touch the observe path" framing — Scope B repairs the file-mode remote observe path. The new schema MUST be one in which the hook client reads `observe_url` and `fingerprint` (not `url`), so file-mode remote observe both resolves and pins correctly. The fix MUST be validated end-to-end (AC-08d), not only asserted at the schema/`pinnedFp` level. *(Constraint: pre-existing schema mismatch; SR-07; current-break fix in Scope B blast radius)*
- **FR-24 — per-slug, single global store.** The store is a single global store across all attached projects, keyed by slug. Attaching two different projects yields two keyed entries (or two per-slug dirs) in one store, not two in-repo files. Re-running `init` for a slug updates that slug's entry idempotently without clobbering other slugs' entries. *(AC-08b)*
- **FR-25 — `gitignoreWarning` removed.** With no in-repo creds file, the `gitignoreWarning` logic (`init.js:273-297`) is removed — there is nothing in the tree to warn about. *(Scope B; Goal 5)*
- **FR-26 — single agreed store key.** Both consumers index the store by **one** agreed key. Slug (server-authoritative, from the bundle) and `projectHash` (client-derived, the hook client's existing `~/.unimatrix/<projectHash>/` key) are distinct keys; the two consumers MUST agree on which one indexes the store so neither silently fails to resolve. The exact key and path/layout is deferred to the architecture (OQ-6); the requirement is that exactly one key is chosen and both consumers use it. *(SR-08; OQ-6)*

### Migration (Scope B, residual)

- **FR-27 — legacy in-tree creds handling.** If a prior `init` left a `.claude/settings.local.json` carrying `unimatrix.remote`, the design decides (architecture-phase) whether to migrate it into the new store and/or clean the in-tree copy on the next `init`. The requirement is that the behavior is defined and deterministic; the exact policy is deferred to the architecture (Scope B step 7 / OQ-5 residual).

---

## Non-Functional Requirements

- **NFR-01 — pure Node stdlib, zero runtime dependencies (by decision).** The bridge is pure Node stdlib (`http`, `https`, `net`, `crypto`); it adds **no** runtime `dependencies` to `package.json` and pulls in no MCP SDK (`@modelcontextprotocol/sdk`, `mcp-remote`). This is the ass-080 (#777) BUILD/DIY decision, not merely the inherited posture. *Verification: `package.json` `dependencies` is absent/empty after the change.* *(AC-02; Constraint: single edge language, zero deps)*
- **NFR-02 — single edge language (pure JS).** The bridge is pure JS — no Python, no third edge language. The Rust binary (Linux-only server) is not the client bridge. *(Constraint)*
- **NFR-03 (NFR-06 parity) — no token to logs.** The bearer token never appears in `printSummary()` output, on stdout, on stderr, in `.mcp.json`, in any thrown/error message (including the pin-mismatch message), or in any other written-and-loggable surface on the remote path. *Verification: grep all output surfaces and thrown messages for the token in tests; assert absence.* *(AC-09; Constraint NFR-06)*
- **NFR-04 — store file permissions `0600`.** The out-of-tree credential store file(s) are created with mode `0600` (owner read/write only). *Verification: stat the store file after `init`.* *(AC-08; Constraint)*
- **NFR-05 — cleartext-at-rest accepted.** The token is stored in cleartext at rest (recoverable at spawn time for `Authorization: Bearer`). At-rest encryption / OS keychain is a **Non-Goal** — the hardened risk is cleartext-in-the-repo, removed by relocation, not encryption. *(Non-Goal; Constraint A4)*
- **NFR-06 — fail-loud over silent-degrade on the trust boundary.** The bridge surfaces cert-pin failures loudly (FR-12); it never silently runs unpinned. *(SR-02; Constraint)*
- **NFR-07 — bounded build surface.** The bridge build surface is budgeted at ~450 LoC net-new (ass-080 #777); a material (~2x) overrun is the documented trigger to revisit the BUILD verdict via the hybrid flip-bar (OQ-1). *(Resolved OQ-1; SR-03; Assumption A2)*
- **NFR-08 — cumulative test infrastructure.** Tests extend the existing hook-client harness (the Layer-2 real-server harness, `test/`; JS-only CI per entry #4835) and reuse the cert-pin/transport fixtures; no isolated test scaffolding. *(Constraint)*

---

## Acceptance Criteria (with verification methods)

Validation tier legend:
- **[stub/local]** — fast first tier: validatable against a stub or local Streamable-HTTP endpoint. With **#774 merged** (PR #778, commit 913b78cb, closed COMPLETED 2026-06-18 — rmcp `allowed_hosts` now wired from `UNIMATRIX_PUBLIC_URL`; remote MCP requests no longer 403), Scope-A live-cloud validation is **no longer blocked or deferred**: stub/local stays as the fast first tier, but **live cloud validation is now expected in delivery** for the Scope-A ACs (AC-03/AC-04/AC-12, etc.). The stub's wire contract MUST still be pinned to observed real-server (rmcp) behavior so the fast tier stays honest ahead of the live run.
- **[no-cloud]** — Scope B; validatable with no reachable cloud.

| AC | Criterion | Verification method | Tier |
|----|-----------|---------------------|------|
| **AC-01** | After `init --bundle <bundle>`, `.mcp.json` contains a `unimatrix` **stdio** entry invoking the JS bridge (not the Rust binary) configured against the bundle's `mcp_url`/`fp`; the "Skipped .mcp.json" remote behavior is gone. | Run `init --bundle` against a fixture bundle; assert `.mcp.json` has a `stdio` `unimatrix` entry with the bridge command; assert no "Skipped .mcp.json" line. | [no-cloud] |
| **AC-02** | The bridge is pure Node stdlib; **no** runtime `dependencies` added, no MCP SDK pulled in (by decision). | Assert `package.json` `dependencies` absent/empty; assert no `@modelcontextprotocol/sdk` / `mcp-remote` import in the bridge module. | [no-cloud] |
| **AC-03** | The bridge proxies a full MCP lifecycle: `initialize` → valid result; session id captured + replayed; `tools/list` returns `context_*`; `context_search`/`context_get` `tools/call` round-trips. | Drive the bridge stdin with `initialize`, `tools/list`, `tools/call` against a stub/local Streamable-HTTP endpoint whose responses mirror observed rmcp behavior; assert stdout results and that `Mcp-Session-Id` is replayed on requests after `initialize`; **then validate live against the cloud (#774 merged, PR #778)**. | [stub/local] + live |
| **AC-04** | The bridge owns TLS and pins `fp`: token flushed **only after** leaf fingerprint matches; on mismatch the socket is destroyed before the token is sent and the bridge fails loud with an expected-vs-presented message. | Live self-signed handshake test: (a) good-pin → connects and round-trips; (b) wrong-pin → socket destroyed, token never reaches the wire (assert via a capturing test server that no `Authorization` header was received), loud diagnosable error raised. Route to fresh-context security review even if gates are green (SR-02). | [stub/local] |
| **AC-05** | The bridge posts to `mcp_url` **verbatim** — composes no path, derives no slug, appends nothing. | Assert the request URL equals `mcp_url` exactly for every proxied request. | [stub/local] |
| **AC-06** | The bridge handles `application/json` (always required) and, **only if the first-delivery probe fails**, `text/event-stream` (SSE) framing — surfacing the JSON-RPC result on stdout for each handled type (OQ-1 JSON-first; FR-08/FR-09). | Run the first-delivery probe **live** across the full lifecycle with `Accept: application/json` (no longer #774-gated; #774 merged, PR #778) — an in-repo rmcp `/observe` content-negotiation pre-check (vnc-024 / PR #686) is the fast pre-check: if JSON-only, assert the SSE parser is absent/unused and JSON results surface on stdout. If the probe fails (server emits `text/event-stream`), stub both `application/json` and `text/event-stream` (multi-line SSE `data:` framing) and assert correct JSON-RPC result on stdout for both; carve SSE-parse and session-replay into separately-testable units with their own fixtures (SR-01). | [stub/local] + live probe |
| **AC-07** | The remote `.mcp.json` write is idempotent + merge-preserving and honors `--dry-run`. | Pre-seed `.mcp.json` with a co-resident server; run `init` twice; assert the `unimatrix` entry is not duplicated, the co-resident server is preserved; run with `--dry-run` and assert no write. | [no-cloud] |
| **AC-08** | The bearer token is written **only** to the out-of-tree store (`~/.config/unimatrix/` or `~/.unimatrix/`, mode 0600, keyed by slug) and **not** to any path in the repo tree; `git status`/`git add -A` surfaces no token-bearing file; no `unimatrix.remote` credential in `.claude/settings.local.json`. (`.claude/settings.json` hooks unaffected.) | Run remote `init` in a fixture repo; assert the store file exists out-of-tree at mode 0600 with the token; assert `git status --porcelain` / a `git add -A` dry-run lists no token-bearing path; grep `.claude/settings.local.json` for the token → absent. | [no-cloud] |
| **AC-08b** | Single global store holds **per-slug** entries: two projects → two keyed entries in one store; re-`init` for a slug updates that slug idempotently without clobbering others. | Run `init` for slug A then slug B; assert one store with two keyed entries; re-run `init` for slug A; assert A updated, B untouched. | [no-cloud] |
| **AC-08c** | Both consumers resolve from the new store: hook/observe `resolve()` loads token + `observe_url` (+ `fp`/timeouts) for the slug and POSTs to `observe_url` unchanged; the bridge loads `mcp_url`/token/`fp` from the same store. Relocation breaks neither. | Seed the store; assert the hook client's file-mode `resolve()` returns token + `observe_url` + populated `pinnedFp`; assert the bridge resolves `mcp_url`/token/`fp`. Include a regression assertion that `pinnedFp` is now populated (reconciled mismatch, FR-23). | [no-cloud] |
| **AC-08d** | File-mode remote observe **actually runs over pinned HTTPS** post-fix (fixes the current break, FR-23). With the store populated (incl. `fingerprint`), the hook client's file-mode path resolves, sets `config.pinnedFp`, and POSTs to `observe_url` over a pinned HTTPS connection — it does **not** fall through to UDS and does **not** run unpinned. | Stand up a **local pinned HTTPS** observe server (self-signed, known leaf fp) and seed the store with its `observe_url`/`fingerprint`/token; drive a hook event through file-mode `resolve()`; assert the POST lands on the HTTPS server (not UDS), that the connection was pinned (good-pin → delivered; wrong-pin → fail-loud, no token on the wire), and `config.pinnedFp` was populated. | [no-cloud] |
| **AC-12** | The bridge sends a **stable** `Mcp-Session-Id` and `clientInfo.name` across a session: identity does not vary request-to-request within one bridge process, because the server keys audit attribution on them (vnc-014 / #4708). Unstable identity → cross-session attribution bleed, undercutting 1-client:1-project integrity. | Drive multiple requests through one bridge session against a capturing stub; assert the `Mcp-Session-Id` header and the `clientInfo.name` are byte-identical across all post-`initialize` requests in that session (captured `Mcp-Session-Id` from `initialize` is replayed unchanged; `clientInfo.name` is constant). | [stub/local] |
| **AC-09** | The token never appears in `printSummary()`, stdout, stderr, `.mcp.json`, or any other remote-path log; it is read by the bridge from the store at spawn time, never on the command line or in `.mcp.json`. | Capture all output surfaces during remote `init` and a bridge run (incl. pin-mismatch path); assert the token string appears in none. | [stub/local] + [no-cloud] |
| **AC-10** | The legacy `--remote`/`--token` path is **explicitly unsupported** for cloud MCP: no bridge wired, and a **loud, deterministic** message states cloud MCP requires a `v:2` bundle (not a silent skip). | Run `init --remote/--token`; assert `.mcp.json` has no `unimatrix` MCP entry; assert the exact unsupported message text and the command exit behavior (make wording + exit a testable AC per SR-06). | [no-cloud] |
| **AC-11** | Scope B lands independently of Scope A with no #774 dependency — validatable without a reachable cloud (store written on `init`; nothing token-bearing in the repo tree; both consumers resolve from the store). | Run the full Scope B test set with no cloud reachable; all pass. Confirm Scope B can merge independently of Scope A's live validation — Scope B never had a #774 dependency (SR-05). | [no-cloud] |

**Cross-cutting validation tiering (SR-04):** with **#774 merged** (PR #778), Scope-A ACs are no longer *not-validated-live*; they are live-validatable and live validation is **expected in delivery**. The stub/local tier remains the fast first pass; the stub's wire behavior (status codes, `Mcp-Session-Id` semantics, `application/json` vs `text/event-stream` framing) MUST still be pinned to observed real-server (rmcp) behavior — a stub that diverges produces false-green confidence (the vnc-034 false-green class) ahead of the live run.

**Live-validation sequencing (now that #774 has merged, PR #778):** live cloud validation of Scope-A is now a concrete delivery activity. The **first** live validation step is to validate the **rmcp 1.7.0 session-id handshake** against the live cloud — specifically whether the session id is **server-minted and returned** on the `initialize` response vs. **client-minted** (and how `Mcp-Session-Id` capture/replay, FR-02/FR-03, behave against the real server). This is the bridge's **hardest seam** and is **unpinnable from `unimatrix-server` alone** — the stub cannot ground it, so it must be confirmed live before the remaining Scope-A live ACs (AC-03/AC-04/AC-12) are trusted as validated-live.

---

## Domain Models / Ubiquitous Language

- **Bridge** — the new pure-JS, Node-stdlib stdio process spawned by Claude Code via `.mcp.json`. Reads newline-delimited JSON-RPC on stdin, holds a pinned HTTPS connection to `mcp_url`, forwards the bearer, proxies to the cloud's Streamable-HTTP MCP endpoint, and writes JSON-RPC responses to stdout. Per-session, thin (the "house pattern", entries #1897/#2582).
- **stdio JSON-RPC** — newline-delimited JSON-RPC framing on stdin/stdout (one message per line). Identical framing to the local UDS daemon's MCP socket.
- **Streamable-HTTP** — the MCP transport the cloud exposes at `mcp_url`. JSON-RPC over HTTP POST, with session continuity via the `Mcp-Session-Id` header and responses framed as either `application/json` or `text/event-stream` (SSE).
- **`Mcp-Session-Id`** — the HTTP header carrying the MCP session id. Established on the `initialize` response (rmcp keys the session on `clientInfo.name`, entries #4708/#4706/#4355); captured by the bridge and replayed on every subsequent request.
- **`mcp_url`** — the server-composed `.../v1/{slug}` Streamable-HTTP MCP endpoint, carried in the validated `v:2` bundle (entry #5081). The bridge posts to it verbatim. Distinct from `observe_url`.
- **`observe_url`** — the server-composed `.../v1/{slug}/observe` hook-telemetry endpoint (also in the `v:2` bundle). The hook/observe client targets it; the bridge does not.
- **slug** — the **server-authoritative** per-project identifier embedded in the bundle's `mcp_url`/`observe_url` (`.../v1/{slug}`). The credential store is keyed per-slug.
- **`projectHash`** — the **client-derived** per-project key under which the hook client already stores state (`~/.unimatrix/<projectHash>/`, the UDS socket + state dir). **Distinct from slug.** Both consumers MUST agree on a single store key (slug or projectHash); the choice is an architecture decision (OQ-6/FR-26).
- **credential store** — the unimatrix-owned, out-of-tree, single global store (under `~/.config/unimatrix/` or `~/.unimatrix/`, mode 0600) holding per-project credential entries. Replaces the in-repo `.claude/settings.local.json` write. **One coherent schema** read by both consumers (FR-23).
- **credential store schema** — one entry per project, containing at minimum `{ mcp_url, observe_url, token, fingerprint }` (reconciled so the hook client reads `observe_url` + `fingerprint`, not `url`, and populates `pinnedFp`). Exact path/key/layout deferred to architecture (OQ-6).
- **`v:2` bundle** — the strict-schema-validated attach bundle (entry #5081, vnc-038 ADR-002) carrying `mcp_url`, `observe_url`, `token`, `fp`. No schema change in this feature. Cloud MCP is bundle-only.
- **cert-pin / fingerprint (`fp`)** — fingerprint-pinning trust model (vnc-038 ADR-002 / F1). Trust is leaf-fingerprint pinning, not CA-trust. `fp` is the expected leaf fingerprint from the bundle; the bridge verifies the presented leaf against it on `secureConnect` (reusing `cert-pin.js`).
- **pinned-flush** — the contract that the token-bearing body/headers are flushed to the wire **only after** the leaf fingerprint matches (`transport-http.js:150-176` pattern). On mismatch the socket is destroyed before any token byte is sent.
- **dumb-client invariant** — the vnc-038 spine: the server is the sole authority on route shape; the client composes no path and derives no slug. The bridge posts `mcp_url` verbatim.

---

## User / Agent Workflows

1. **Bundle attach with MCP restored.** User runs `unimatrix init --bundle <bundle>`. `init` validates the `v:2` bundle, writes the credential to the out-of-tree per-slug store (mode 0600), and writes a `stdio` `unimatrix` entry in `.mcp.json` invoking the bridge. Claude Code spawns the bridge as a stdio MCP server; the bridge resolves the credential from the store, opens a pinned TLS connection to `mcp_url`, and proxies the `context_*` tools. The LLM can now `context_search`/`context_get`/`context_store` over the cloud.
2. **Observe path coexists.** The hook/observe client continues posting hook events to `observe_url`, now resolving its credential from the same out-of-tree store (with `fp` correctly populated). The MCP surface is added alongside — the observe path is untouched in behavior.
3. **Legacy attach (unsupported for MCP).** User runs `unimatrix init --remote <url> --token <tok>`. `init` does not wire the bridge and emits a loud, deterministic message that cloud MCP requires a `v:2` bundle. The observe path on the legacy route is unchanged.
4. **Re-attach / multi-project.** Re-running `init` for the same slug updates that slug's store entry and the `.mcp.json` entry idempotently. Attaching a second project adds a second per-slug entry to the single global store without touching the first.

---

## Constraints

- **Single edge language — pure JS.** Bridge MUST be pure JS; no Python, no third edge language. The Rust binary is the Linux-only server, not the client bridge.
- **Zero runtime dependencies (by decision).** Node stdlib only; no MCP SDK. ass-080 (#777) BUILD/DIY verdict.
- **Dumb-client invariant (vnc-038 spine).** Post `mcp_url` verbatim; compose no path, derive no slug.
- **Cert-pin trust model (vnc-038 ADR-002 / F1).** Complete self-signed handshake (`rejectUnauthorized:false`), verify leaf fingerprint on `secureConnect`, flush token only after pin matches; reuse `cert-pin.js`, never re-implement TLS trust.
- **NFR-06 — no token to logs.** Token absent from all output/loggable surfaces on the remote path.
- **Cloud MCP is bundle-only (OQ-2 → option c, resolves #773).** Legacy env-HTTPS path is not bridged (no `fp` pin) and emits a loud unsupported message; #773 resolved by deprecating env-HTTPS for cloud-attach MCP.
- **#774 merged (PR #778) — Scope A live validation available; bridge validated live in delivery.** rmcp's host-allowlist 403'd remote MCP requests until #774 wired `allowed_hosts` from `UNIMATRIX_PUBLIC_URL` (lesson #5104); with #774 merged (PR #778, commit 913b78cb, COMPLETED 2026-06-18) those requests no longer 403, so Scope-A live end-to-end validation is available and is performed live in delivery. Stub/local remains the fast first tier; Scope B has no #774 dependency.
- **Streamable-HTTP session contract.** `Mcp-Session-Id` capture/replay and SSE (`text/event-stream`) framing per spec.
- **Test infrastructure is cumulative.** Extend the existing hook-client / Layer-2 harness and cert-pin/transport fixtures.
- **Claude Code `.mcp.json` transport types** are `stdio` / `sse` / `streamable-http`; the chosen wiring is `stdio` so Claude Code does no TLS itself.
- **Credential out-of-tree and unimatrix-owned (Scope B).** Store under user home (mode 0600), never in the repo tree, never in `.claude/settings.local.json`. `.claude/settings.json` keeps only hooks. No-cleartext-in-repo, no namespace-squatting; cleartext-at-rest accepted.
- **Per-slug keying, single global store, both consumers one store, one key.** Both the bridge and the hook/observe client resolve from one store, indexed by one agreed key (slug vs projectHash — architecture decides, FR-26).
- **Pre-existing schema mismatch must be reconciled, not ported (FR-23).** Writer emits `{mcp_url, observe_url, token, fingerprint}`; hook client reads `{url, token, timeouts}` and never `fingerprint`. The new store lands ONE coherent schema in which the hook client reads `observe_url` + `fingerprint`.

---

## Dependencies

- **`v:2` bundle + per-slug `.../v1/{slug}` Streamable-HTTP MCP route** — server-ready and frozen (vnc-038, #770; entry #5081). This feature is client-only (Assumption A1).
- **`#774`** (host-allowlist wiring from `UNIMATRIX_PUBLIC_URL`, lesson #5104) — **MERGED** (PR #778, commit 913b78cb, COMPLETED 2026-06-18); **no longer a blocker**. Was a sequencing dependency for Scope A **live** validation only (never a code dependency); with it merged, Scope-A live validation is **unblocked**. Stub/local validation and Scope B never required it.
- **ass-080 (#777)** — research spike grounding the BUILD/DIY (zero-dep) decision and the ~450-LoC budget / hybrid flip-bar.
- **Existing JS modules (reused, not re-implemented):**
  - `cert-pin.js` — `computeFingerprint`, `verifyPeerFingerprint`, `applyCertPin`.
  - `transport-http.js:150-176` — the flush-after-pin reference pattern (observe path; not modified).
  - `init.js` — `init()` / `initRemote()` / `writeRemoteSettingsLocal` (`230-264`) / `writeMcpJson` (`59-104`, idempotency reference) / `gitignoreWarning` (`273-297`, removed) / `printSummary()`.
  - `config.js:276-306` — hook client file-mode `resolve()` (repointed to the out-of-tree store).
  - `bundle.js:67-156` — `decodeBundle` strict validation of `mcp_url`/`observe_url`/`fp`.
- **Node stdlib only:** `http`, `https`, `net`, `crypto`, `fs`, `path`, `os`.
- **External services:** the cloud's per-slug Streamable-HTTP MCP endpoint (`mcp_url`) — reachable only post-#774 for live validation.

---

## NOT in Scope

- At-rest token encryption / OS keychain — Non-Goal; cleartext-at-rest accepted, keychain enterprise-deferred.
- A new MCP SDK / heavyweight dependency (`@modelcontextprotocol/sdk`, bundling `mcp-remote`) — explicitly rejected by decision.
- (No longer excluded) End-to-end validation of Scope A against a **real** cloud — with #774 merged (PR #778), live cloud validation is now **in scope for delivery** and is expected for the Scope-A ACs; it is no longer deferred or #774-blocked. Stub/local is kept as the fast first tier ahead of the live run.
- Server-side changes — the `v:2` bundle and `.../v1/{slug}` route are frozen; #774 is a separate server fix.
- MCP over the legacy `--remote`/`--token` env-HTTPS path — unsupported (bundle-only), loud unsupported message only.
- Reworking the observe/hook path — the observe surface (`transport-http.js` posting to `observe_url`) is unchanged in behavior; only its credential source moves.
- Changing the bundle schema — `v:2` already carries `mcp_url`/`observe_url`/`token`/`fp`.
- `#768` stale remote-mode docs — a separate pre-committed fast-follow, not in this diff.

---

## Open Questions (for architect / human)

Both are **architecture-phase design details**, not scope blockers (per SCOPE Open Questions). Spec defers them; flagged here so they are not lost:

- **OQ-4 (entrypoint shape — architect).** New `unimatrix mcp-bridge` subcommand routed to JS in `bin/unimatrix.js`, vs. a direct `node <bridge-path> ...` command in `.mcp.json` (mirroring the hook `node <client> <EVENT>` shape). The spec requires only that `.mcp.json` invoke the JS bridge (not the Rust binary) with no token (FR-14, FR-17).
- **OQ-6 (store path/layout + key — architect; SR-08).** Exact store path/layout (`~/.config/unimatrix/credentials.json` slug→entry map vs. `~/.unimatrix/<slug>/` dir vs. reuse of `~/.unimatrix/<projectHash>/`) **and** which single key (slug vs projectHash) both consumers index by. Per-slug-keyed + out-of-tree + one-coherent-schema + one-agreed-key are fixed (FR-20, FR-23, FR-24, FR-26); the path/key choice is the open detail. **This is the highest-leverage decision to lock before code (SR-07/SR-08): if the two consumers index by different keys, one silently fails to resolve.**

---

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — returned vnc-038 v:2 bundle ADR (#5081), first-boot-token-never-to-stdout ADR (#5088), bugfix-774 host-allowlist lesson (#5104), vnc-014 clientInfo.name/session ADR (#4355), rmcp Streamable-HTTP session entries; all consistent with SCOPE. No conflicting conventions found. Read-only tier — no storage (spec decisions are feature-specific).
