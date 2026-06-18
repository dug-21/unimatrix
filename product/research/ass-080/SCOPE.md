# ass-080 — Build vs. Adopt: Streamable-HTTP MCP transport for the vnc-039 remote bridge

> Resolves **vnc-039 OQ-1**. The remote MCP bridge must speak a full MCP Streamable-HTTP
> transport over a self-signed, fingerprint-pinned HTTPS connection. The vnc-039 scope
> currently *defaults* this to a hand-rolled, pure-Node-stdlib implementation and lists "no
> MCP SDK" as a non-goal — yet the server side (ASS-002 / ADR-001) made the **opposite**
> call for the same protocol, rejecting DIY for spec-tracking burden and adopting `rmcp`.
> This spike settles build-vs-adopt for the client with a security-grounded recommendation
> **before** the vnc-039 scope is finalized. Scoped in a uni-zero session 2026-06-18.

## Question

Should the vnc-039 bridge **hand-roll** a Streamable-HTTP MCP client on Node stdlib, or
**adopt** an existing MCP SDK/library — and is any such SDK **secure and constraint-compatible
enough** for our use? Specifically: does any candidate clear the non-negotiable **cert-pin
trust model**, and if so, does its security/supply-chain posture justify breaking the package's
current zero-runtime-dependency footprint?

## Why It Matters

- **It is the single biggest cost, risk, and maintenance-surface decision in vnc-039.** The
  zero-dependency posture is cheap for the observe path (POST JSON to `observe_url`) but
  *expensive and correctness-risky* for a full MCP transport: session lifecycle,
  `Mcp-Session-Id` capture/replay, `application/json` **and** `text/event-stream` (SSE)
  framing, request/response correlation, timeouts/reconnection.
- **The project already made this decision once — the other way — on the server.** ASS-002 /
  ADR-001 evaluated the Rust MCP landscape and chose `rmcp` over a ~750-LoC DIY precisely to
  avoid "spec-tracking burden." The client is now drifting toward DIY for the same wire
  protocol by default rather than by decision. That inconsistency must be resolved
  consciously, not inherited.
- **It gates the vnc-039 scope.** Build-vs-adopt determines vnc-039's diff size, its test
  surface, and whether the "zero runtime dependencies" constraint (AC-02) survives. The
  human has stated they are open to an SDK *if one is secure enough* — this spike produces
  the evidence for that call.

## The Bridge's Dual Role (frames every candidate evaluation)

The bridge sits between two MCP transports and must satisfy **both** sides:

```
Claude Code  ──stdio JSON-RPC──►  [ vnc-039 bridge ]  ──Streamable-HTTP (pinned TLS)──►  cloud
             (bridge = MCP server)                     (bridge = MCP client)
```

A candidate must serve as an MCP **stdio server** (facing Claude Code) *and* an MCP
**Streamable-HTTP client** (facing the cloud) — or at minimum the harder client side — while
honoring the cert-pin trust model. An SDK that does the protocol but cannot pin a self-signed
leaf is **disqualified** (or relegated to a hybrid).

## Investigation Areas (bounded)

1. **Candidate landscape.** The official `@modelcontextprotocol/sdk` (TypeScript) and any
   viable alternatives. For each: adoption, maintenance cadence, release/version-pinning
   story, ESM/CJS compatibility with the existing CJS stdlib hook client, and Node version
   floor.
2. **Cert-pin compatibility — THE GATE (dispositive).** Can the candidate's HTTP /
   Streamable-HTTP client transport accept a **custom TLS / `https.Agent`** so the bridge can:
   complete a self-signed handshake (`rejectUnauthorized: false`), verify the leaf fingerprint
   on `secureConnect` (`verifyPeerFingerprint`), and flush the bearer-bearing body **only after**
   the pin matches — exactly the `transport-http.js` / `cert-pin.js` pattern? If the SDK
   hardcodes CA-trust with no TLS injection seam, it **cannot** satisfy our model → DIY or
   hybrid is forced. Answer this first; it may end the evaluation early.
3. **Security & supply-chain posture.** Full transitive dependency tree (depth + count),
   `npm audit` surface, known CVEs, maintainership/bus-factor, and whether transitive versions
   can be pinned. Compare head-to-head against the **current zero-dependency footprint**. The
   spike must *justify breaking* zero-dep, not assume it.
4. **License & footprint.** License compatibility (project is MIT/Apache 2.0); install size;
   bundling implications for a package that today ships only `optionalDependencies` (platform
   binaries).
5. **Honest DIY cost.** Enumerate the surface a hand-rolled bridge must cover (initialize →
   session id → `tools/list` / `tools/call`; `Mcp-Session-Id` replay; json + SSE framing;
   error/timeout/correlation) and give a realistic LoC + ongoing spec-tracking estimate.
   Anchor to ASS-002's ~750-LoC server DIY figure, adjusted for the client side.
6. **Hybrid option.** SDK (or a minimal, single-purpose vetted dep) for protocol/framing +
   **custom TLS for pinning**, if the SDK exposes a transport seam. Evaluate whether a thin
   vetted dependency beats both the full SDK and full DIY.

## Deliverable (`FINDINGS.md`)

An **ADR-ready recommendation** that resolves vnc-039 OQ-1:

- **Go/no-go on the cert-pin gate** for each candidate (dispositive — lead with it).
- **Build vs. adopt** verdict. If **adopt**: which library, and the explicit
  security/constraint justification (cert-pin viability, dep tree, audit surface, license,
  footprint, Node/CJS fit). If **build**: a scoped surface estimate (LoC + the spec areas to
  own) and the maintenance burden accepted.
- If **hybrid**: the exact split (what the dep owns vs. what stays custom) and why.
- A clear statement of what changes in the vnc-039 scope as a result (AC-02 zero-dep
  constraint kept / relaxed / replaced).

## Constraints & Prior Art

- **Cert-pin trust model is non-negotiable** (vnc-038 ADR-002; `cert-pin.js`;
  `transport-http.js:150-176` flush-after-pin). Any candidate must bend to it; the model does
  not bend to the candidate.
- **Current posture is zero runtime dependencies** — the package ships only
  `optionalDependencies`. Breaking that is permitted *only* if this spike justifies it on
  security + correctness grounds.
- **Single edge language (JS/TS); MIT/Apache 2.0 licensing.**
- **Prior art — this is the client mirror of a settled server decision:** ASS-002 (Rust MCP
  SDK landscape → `rmcp`, DIY rejected for spec-tracking burden) / ADR-001. Also: the stdio
  bridge house pattern (Unimatrix entry #1897), `Mcp-Session-Id` session semantics (entry
  #4708), and the MCP Streamable-HTTP transport spec.

## Out of Scope

- **Building the bridge** — that is vnc-039 delivery. This spike informs it, does not start it.
- **Server-side changes** and **#774** (host-allowlist) — orthogonal; this spike has no #774
  dependency.
- **Credential relocation (vnc-039 Scope B)** — independent of the transport decision.

## Dependency

**Blocks finalizing the vnc-039 scope (OQ-1).** No dependency on #774. Single bounded spike →
execute via `uni-spike-researcher`.

## Tracking

GitHub Issue: (created with this scope). Advances goal #4946 (`personal-cloud`). Feeds
vnc-039 (#775) OQ-1; related #773.
