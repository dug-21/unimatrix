# FINDINGS: Build vs. Adopt — Streamable-HTTP MCP transport for the vnc-039 remote bridge

**Spike**: ass-080
**Date**: 2026-06-18
**Approach**: evaluation (build-vs-adopt, security-grounded) with empirical install/dep measurement
**Confidence**: validated (cert-pin gate verified against actual SDK source + Node `https` option surface; dep tree, audit, CVE history, CJS interop measured empirically against `@modelcontextprotocol/sdk@1.29.0`)

---

## Bottom line (read first)

- **Cert-pin gate — PASS (conditional)** for the official `@modelcontextprotocol/sdk`. Its
  `StreamableHTTPClientTransport` delegates *all* network I/O to an injectable
  `fetch?: FetchLike` option. A custom fetch built on `node:https` can carry an
  `https.Agent({ rejectUnauthorized:false })`, hook `secureConnect`, verify the leaf
  fingerprint, and call `req.end(body)` only after the pin matches — flush-after-pin lives
  **inside our custom fetch**, untouched by the SDK. The pin works *because* the SDK never
  owns the socket, not because the SDK supports pinning.
- **No alternative client SDK exists.** Every "lightweight" MCP library
  (mcp-lite, FastMCP, LeanMCP) is **server**-side. The official SDK is the only adopt option;
  the real contest is **adopt-full-SDK vs. DIY vs. hybrid**.
- **Verdict: BUILD (DIY), staying zero-runtime-dependency — keep AC-02.** The adopt path
  costs a **91-package / 25 MB / server-laden** transitive tree to obtain client transport
  logic that, on the cert-pin path, you must *route around* with a hand-written `node:https`
  fetch anyway. The DIY client surface is small and bounded (~400–550 LoC), the SDK gives no
  leverage on the hard part (TLS pinning), and adopting imports the SDK's recurring
  server-transport CVE stream into a client-only bridge. Hybrid is the runner-up, not the pick.

---

## Findings

### Q: Candidate landscape — what exists, and what is its adoption / cadence / ESM-CJS fit / Node floor?

**Answer.** One real candidate: the **official `@modelcontextprotocol/sdk` (TypeScript)**,
measured at **v1.29.0**. Measured facts:

- **License:** `MIT` — compatible with our `MIT OR Apache-2.0`.
- **Node floor:** `engines.node >= 18` — identical to our package.
- **ESM/CJS:** package `type: module` but ships **dual builds**; the `./client/streamableHttp.js`
  export has both `import` (`dist/esm/...`) and `require` (`dist/cjs/...`) paths.
  Empirically, `require('@modelcontextprotocol/sdk/client/streamableHttp.js')` from a CJS
  context **works** and returns `{ StreamableHTTPClientTransport, StreamableHTTPError }`.
  CJS fit with our stdlib-hook client is **not** a blocker.
- **Cadence:** frequent releases (1.29.x as of this spike; the server-side ADR-001 notes the
  Rust `rmcp` sibling releases "~biweekly" and must be exact-pinned). High cadence = an
  **exact-pin + manual-upgrade** burden if adopted, mirroring ADR-001.
- **Alternatives:** `mcp-lite`, `FastMCP`, `LeanMCP`, `mcp-handler` are all **server**
  frameworks — none ship a **client** Streamable-HTTP transport. `mcp-remote` is an end-user
  npx bridge tool (wraps this same SDK), not a library to embed. There is **no lighter
  client-transport library** to adopt.

**Evidence.** `npm install @modelcontextprotocol/sdk@latest --omit=dev`; read
`node_modules/@modelcontextprotocol/sdk/package.json`; `require()` smoke test (succeeded);
repo tree `packages/client/src/client/streamableHttp.ts`; web search of the alternative
landscape (all server-side).

**Recommendation.** Treat the official SDK as the **only** adopt candidate. Don't spend scope
hunting for a thinner client lib — there isn't one. Decide adopt-vs-DIY on this candidate.

---

### Q: THE GATE (dispositive) — can the candidate's client transport accept custom TLS / `https.Agent`, complete a self-signed handshake (`rejectUnauthorized:false`), verify the leaf fingerprint on `secureConnect`, and flush the bearer body ONLY after the pin matches?

**Answer: YES — conditionally, and only because the SDK abstains from owning the socket.**

The SDK's `StreamableHTTPClientTransport` does **not** call `node:https` itself and does **not**
hardcode CA-trust. Every request is:

```ts
const response = await (this._fetch ?? fetch)(this._url, {
  ...this._requestInit, method: 'POST', headers, body: JSON.stringify(message), signal
});
```

with the injectable seam typed as:

```ts
export type FetchLike = (url: string | URL, init?: RequestInit) => Promise<Response>;
// StreamableHTTPClientTransportOptions: { fetch?: FetchLike; requestInit?: RequestInit; ... }
```

So the gate is satisfied by **supplying our own `fetch`** that:
1. opens an `https.request({ agent: new https.Agent({ rejectUnauthorized:false }), ... })`
   (or `applyCertPin(...)` exactly as today),
2. buffers `init.body` (`JSON.stringify(message)`) and does **not** flush it,
3. on `socket` → `once('secureConnect')` runs `verifyPeerFingerprint(socket, pinnedFp)`
   (our existing `cert-pin.js`), destroying the socket on mismatch **before** any byte —
   including the `Authorization: Bearer` header — is written,
4. calls `req.end(body)` **only** on pin-OK, then adapts the Node response into a Web
   `Response` (Node 18+ has global `Response`/`Headers`/`ReadableStream`) so the SDK can read
   `response.headers.get('mcp-session-id')` and stream `text/event-stream`.

This is a **byte-for-byte port** of the `transport-http.js:150-176` flush-after-pin pattern into
a `FetchLike`. The SDK never touches TLS, never sees the socket, never gets to run CA
verification first — so the F1 defect (CA-chain rejecting the self-signed leaf before the pin
runs) cannot recur.

**Critical caveat — this disqualifies the *easy* adoption.** Node's **built-in global `fetch`
(undici) does NOT accept a Node `https.Agent`** and gives no `secureConnect` hook. So you
**cannot** just `new StreamableHTTPClientTransport(url, { requestInit: { headers } })` and get
pinning — that path uses global fetch and would either reject the self-signed leaf or (worse)
trust it with no pin. To pass the gate you **must** inject a hand-written `node:https`-based
fetch. At that point the SDK owns only JSON-RPC envelope + session/SSE plumbing; **you still
hand-write the entire hard part (TLS pinning + flush-after-pin + Node→Web Response adapter).**

**Evidence.** SDK source `packages/client/src/client/streamableHttp.ts` (options interface
`fetch?: FetchLike` / `requestInit?: RequestInit`; the three POST/GET call sites all
`(this._fetch ?? fetch)(url, init)` with `body: JSON.stringify(message)`); `FetchLike` type in
`packages/core/src/shared/transport.ts`. Node option-surface verified empirically:
`https.Agent({rejectUnauthorized:false})` accepted; `https.request` emits `socket` (hook point
for `secureConnect`); global `Response`/`ReadableStream`/`Headers` present on Node 18+. Same
mechanism `cert-pin.js`/`transport-http.js` already use today.

**Recommendation.** Record the gate as **PASS-via-injected-custom-fetch, FAIL-via-default-fetch**.
Because the only passing configuration requires us to author the `node:https` fetch + pin +
Response adapter ourselves, the SDK provides **no leverage on the gated requirement** — the
decisive input to the build-vs-adopt call below.

---

### Q: Security & supply-chain posture — transitive tree, audit surface, CVEs, maintainership, pinnability — vs. the current zero-dependency footprint?

**Answer.** The SDK is well-maintained and currently audit-clean, but the tree is **large,
server-weighted, and carries a recurring CVE stream in exactly the transports we would not use.**

Empirical, `@modelcontextprotocol/sdk@1.29.0`, `--omit=dev`:

- **Transitive tree:** **17 direct deps**, **91 flat unique packages**, **109 total package
  installs**, **~25 MB on disk** (vs. **0 runtime deps today**). Direct deps include
  `express`, `cors`, `hono`, `@hono/node-server`, `express-rate-limit`, `raw-body`, `jose`,
  `ajv`, `zod`, `pkce-challenge`, `eventsource` — i.e. **server + OAuth machinery the client
  bridge never executes.** The package is import-tree-shakeable at *runtime* but **not
  install-tree-shakeable** (npm installs the whole `dependencies` set regardless of subpath).
- **`npm audit`:** **0 vulnerabilities** at install time (clean *today*).
- **CVE history — the telling part:** recent CVEs are **server-transport** issues:
  - **CVE-2026-25536** (GHSA-345p-7cg4-v4c7, High, CVSS 7.1) — cross-client data leak via
    `StreamableHTTPServerTransport` requestId→stream collision; affects 1.10.0–<1.26.0.
  - **CVE-2025-66414 / DNS-rebinding** — HTTP **server** transport; fixed in ≥1.24.0.
  None touch the **client** transport — but adopting still means tracking, re-auditing, and
  bumping for advisories on code we don't run, plus the blast radius of any future client-side
  or transitive-dep CVE across 91 packages.
- **Maintainership / bus-factor:** official Anthropic/MCP project, high download volume, active —
  **strong** (the SDK's best attribute and the reason adopt is even debatable).
- **Pinnability:** exact-pin possible and **mandatory** (high cadence), mirroring ADR-001's
  `rmcp` exact-pin discipline — but pinning 91 transitive packages is a lockfile/renovate
  surface we don't have today.

**Evidence.** `npm install … --omit=dev` then `find node_modules -name package.json` (109),
`ls node_modules` (91), `du -sh node_modules` (25 MB), `npm audit --omit=dev` (0 vulns),
`require('@modelcontextprotocol/sdk/package.json').dependencies` (17 keys incl. express/hono/cors),
GitHub Advisory DB for CVE-2026-25536 / CVE-2025-66414.

**Recommendation.** This posture **does not justify breaking zero-dep** for the client bridge.
Going 0 → 91 transitive packages and 25 MB, importing server+OAuth CVE exposure, to obtain
transport logic we must *bypass* on the security-critical (pinning) path, is a poor trade. The
spike's burden was to *justify* breaking AC-02; the evidence does the opposite.

---

### Q: License & footprint — compatible, and what does it do to a package that ships only `optionalDependencies`?

**Answer.** **License is fine; footprint is the problem.** `MIT` is compatible. But the package
today has **zero** `dependencies` (only `optionalDependencies` for platform binaries). Adopting
adds the **first** runtime `dependencies` entry, a lockfile graph of 91 packages, ~25 MB, and an
ongoing audit/upgrade obligation — a qualitative change to the package's posture, not a marginal
one.

**Evidence.** Our `package.json` (`optionalDependencies` only, `license: "MIT OR Apache-2.0"`);
SDK `license: MIT`; 25 MB / 91-package install measured above.

**Recommendation.** License is a non-issue. Footprint is a **decisive con** for adopt and
**neutral** for DIY (stdlib adds nothing).

---

### Q: Honest DIY cost — what surface must a hand-rolled bridge cover, and what's the realistic LoC + spec-tracking burden (anchored to ASS-002's ~750-LoC server figure)?

**Answer.** **Bounded and modest: ~400–550 LoC of net-new client transport code**, materially
*less* than ASS-002's ~750-LoC **server** estimate, because the client side is the smaller half
of the protocol and we **already own** the hardest module (TLS pinning).

Surface the DIY client bridge must cover:
1. **stdio MCP server side** (facing Claude Code): newline-delimited JSON-RPC read/write over
   stdin/stdout. The *easy* half; mirrors the local UDS byte-forwarder house pattern
   (entry #1897 / #2582) — except it terminates+re-frames rather than byte-forwards.
2. **Streamable-HTTP client side** (facing cloud): `initialize` handshake → capture
   `Mcp-Session-Id` from the response → **replay** it on every subsequent request (entry #4708
   semantics) → `tools/list` / `tools/call` request/response correlation by JSON-RPC `id`.
3. **Dual response framing:** parse **`application/json`** (single response) **and**
   **`text/event-stream`** (SSE) by `Content-Type`; an SSE line parser (`event:`/`data:`/`id:`)
   with `Last-Event-ID` for resumable streams.
4. **TLS + pinning:** **already done** — reuse `cert-pin.js` + the `transport-http.js:150-176`
   flush-after-pin pattern verbatim. **~0 net-new LoC** for the gated requirement.
5. **Lifecycle:** timeouts, connect deadline, `DELETE`-session teardown, reconnect/abort,
   1 MiB body guard (reuse `transport-http.js` constants), fail-open classification.

**Rough LoC budget (net-new):** stdio framing ~80; HTTP client + session replay ~120; SSE parser
~90; json/SSE dispatch + correlation ~80; lifecycle/teardown/timeouts ~80; glue/wiring ~60 →
**~400–550 LoC**, plus tests. **Ongoing spec-tracking:** the MCP Streamable-HTTP transport spec
is **stable and small** for the client role (initialize, session header, two content types, SSE);
unlike the server (capabilities negotiation, tool schema generation, concurrency) it is not a
fast-moving surface. Spec-tracking burden is **low**, and it's the *same* protocol the server
side already tracks via `rmcp`, so drift is observable in-house.

**Why this is cheaper here than on the server (ASS-002):** ASS-002 chose `rmcp` to avoid
declarative tool-definition, JSON-Schema generation, proc-macro dispatch, and concurrent
multi-session server state — none of which the **client** bridge implements. The client just
speaks the wire protocol for a single session. The asymmetry that justified *adopt* on the server
is **absent** on the client.

**Evidence.** vnc-039 prior-art entry #5105 (client must translate stdio↔Streamable-HTTP incl.
`Mcp-Session-Id` replay + SSE framing, reuse `cert-pin.js`); entry #4708 (session-id semantics);
entry #1897 / ADR-001 (stdio-bridge house pattern + the server's opposite, justified, adopt call);
`transport-http.js` (existing TLS/timeout/body-guard scaffolding to reuse).

**Recommendation.** **BUILD.** Scope vnc-039 to a hand-rolled pure-Node-stdlib client bridge
(~400–550 LoC + tests), reusing `cert-pin.js` and `transport-http.js` scaffolding. Own the spec
areas in #2–3; the gated TLS requirement carries over for free.

---

### Q: Hybrid option — SDK (or a thin vetted dep) for protocol/framing + custom TLS for pinning. Does a thin dep beat both full-SDK and full-DIY?

**Answer.** **Hybrid is viable and is the runner-up, but does not beat DIY here.** A hybrid would
adopt `@modelcontextprotocol/sdk`'s `StreamableHTTPClientTransport` **only** for JSON-RPC envelope
+ session/SSE plumbing, and inject our `node:https` custom `fetch` (carrying `cert-pin.js`) for
TLS. The split:

- **Dep owns:** JSON-RPC framing, `Mcp-Session-Id` capture/replay, SSE parsing, request
  correlation, reconnection.
- **We own (custom):** the entire `FetchLike` — `https.Agent`, `secureConnect` pin,
  flush-after-pin, **and a Node→Web `Response` adapter** the SDK requires.

**Why it loses to DIY:**
- It still imports the **91-package / 25 MB / server-CVE** tree (Q3) — the dep cost is identical
  to full-adopt because npm can't install-tree-shake.
- The custom `FetchLike` + Node→Web `Response` adapter is **non-trivial glue** (buffer body,
  bridge Node streams to a Web `ReadableStream` for SSE, map headers both directions). That
  adapter is roughly the size of the SSE+dispatch code we'd write in DIY anyway — so hybrid
  **does not eliminate** the hard client code, it **adds** an impedance-matching layer on top of
  it, *plus* the dep tree.
- It couples us to SDK internal expectations (what shape of `Response` the transport reads) — a
  version-upgrade hazard the exact-pin must chase.

A **thin single-purpose vetted dep** (e.g. only an SSE parser) is the *one* place hybrid could pay
off — but an SSE line parser is ~90 LoC of stable, well-understood code; pulling a dep for it is
not worth a new supply-chain entry given the zero-dep posture.

**Evidence.** Same SDK source/dep measurements as Q2–Q3; the `FetchLike`→`Response` contract the
transport consumes (`response.headers.get`, SSE over `response.body`).

**Recommendation.** **Reject hybrid** as the primary. Keep it as the documented fallback **iff**
SSE/session-replay correctness proves harder in delivery than estimated — but note switching to
hybrid then trades correctness risk for the full dep-tree cost **and** still requires the
custom-fetch+Response adapter, so the bar to flip is high.

---

## Unanswered Questions

None. All six SCOPE Investigation Areas and the deliverable's four required outputs (gate verdict
per candidate, build-vs-adopt verdict, hybrid split, AC-02 disposition) are answered with
evidence. The gate was answerable definitively from SDK source + Node's documented option surface;
no live self-signed endpoint was needed to validate the seam (the existing
`cert-pin.js`/`transport-http.js` already prove the runtime pattern end-to-end in-repo).

---

## Out-of-Scope Discoveries

- **SDK server-transport CVEs are a recurring class (CVE-2026-25536 cross-client data leak;
  CVE-2025-66414 DNS-rebinding).** Not pursued — vnc-039 is client-only. *Why it matters:* if the
  **server** side (`rmcp`, ADR-001) ever considered the TS SDK, or if any future feature embeds
  the SDK server transport, these advisories and ADR-001's "exact-pin + deliberate upgrade"
  discipline apply directly. Flag for whoever owns server-transport dep hygiene.
- **The MCP TS SDK repo became a monorepo** (`packages/{client,server,core,middleware}`),
  splitting transport into `@modelcontextprotocol/core` + per-role packages. Not pursued.
  *Why it matters:* a *future* SDK minor may publish a slimmer client-only package that
  install-tree-shakes the server/OAuth deps — which would materially improve the adopt math and
  could justify revisiting this decision. Worth a 30-minute re-check at vnc-039 delivery time,
  **not** a re-spike.
- **Node's global `fetch` (undici) cannot carry a Node `https.Agent` or expose `secureConnect`.**
  Confirmed here as the reason the "easy" SDK adoption fails the gate. *Why it matters:* a
  reusable trap for **any** future feature tempted to pin a cert behind global fetch — the
  flush-after-pin pattern requires `node:https`, full stop. Candidate lesson for Unimatrix once a
  downstream delivery validates it.

---

## Recommendations Summary

- **Cert-pin gate (`@modelcontextprotocol/sdk`):** PASS **only** via an injected `node:https`
  custom `FetchLike` (default global-fetch path FAILS); the SDK gives zero leverage on pinning —
  we author TLS + flush-after-pin ourselves either way.
- **Candidate landscape:** Official SDK is the *only* adopt option (all "lite" alternatives are
  server-side); decide adopt-vs-DIY on it alone.
- **Security/supply-chain:** Adopt = 0→**91 transitive packages / 25 MB**, server+OAuth code we
  don't run, plus a recurring server-transport CVE stream — **does not justify** breaking zero-dep.
- **License/footprint:** License compatible (MIT); footprint is a decisive con for adopt.
- **DIY cost:** **~400–550 LoC** net-new (stdio framing + HTTP/session-replay + SSE + dispatch +
  lifecycle), **less** than ASS-002's ~750-LoC server figure; TLS pinning reused from
  `cert-pin.js` at ~0 net LoC; spec-tracking burden **low** (client role is small/stable).
- **Hybrid:** Viable but **rejected** — incurs the *full* dep tree yet still requires a custom
  fetch + Node→Web Response adapter, so it adds cost without removing the hard client code.
- **VERDICT: BUILD (DIY), pure Node stdlib.**
- **vnc-039 scope (OQ-1 resolved):** **Keep AC-02 (zero runtime dependencies) — unchanged.**
  vnc-039 hand-rolls the Streamable-HTTP client bridge on `node:https`/`crypto`, reusing
  `cert-pin.js` + `transport-http.js` scaffolding; "no MCP SDK" stays a non-goal **by decision,
  not by default**. Hybrid documented as the only fallback (flip bar: client SSE/session
  correctness proving materially harder than the ~400–550 LoC estimate in delivery).
