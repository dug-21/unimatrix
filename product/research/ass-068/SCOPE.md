# ASS-068: Telemetry Client Architecture — Unifying STDIO + HTTP into One Maintainable Client

## Question

What is the proper architecture for the client-side component that captures hook telemetry and serves injection responses — handling **both** local STDIO/UDS and remote HTTP connections to Unimatrix — optimized for (1) clean architecture, (2) minimal maintenance over time, and (3) excellent performance/reliability? And what is the ordered, independently-shippable migration path from today (all-Rust hook, no HTTP client) to that end-state, with a single-step deployment that installs only the components a given deployment style needs?

## Why It Matters

ass-067 recommended **pure TypeScript** for the *remote* thin client and established the npm packaging precedents. It deliberately left the *local* path as the existing Rust `hook.rs` binary. That leaves us heading toward **two implementations of the same client** — Rust over UDS, TypeScript over HTTP — which duplicates the response-transformation logic (`format_injection`, the SubagentStart envelope, transcript prepend) and creates a permanent parity-maintenance tax between two languages.

This spike extends ass-067 from "what is the remote thin client?" to "what is **the** client?" — one component, two transports, written and maintained once. But "maintained once" has two routes, and the earlier decision picked one without weighing the other: (a) **rewrite the client in TypeScript** — one TS implementation, but the hook logic *and* the wire types now live in TS, divorced from the Rust codebase and re-mirrored by hand; or (b) **keep `hook.rs` as the single Rust source and ship it to the client as WASM** — no reimplementation, no schema drift, because it is literally the same Rust using `wire.rs`. ass-067 chose TS and dropped WASM as "premature" on **build-simplicity** grounds. We pivoted to TS because it seemed to make sense, but under-weighted **long-term maintenance burden** — which is exactly where WASM-binding may win. This spike re-opens WASM as a first-class alternative and makes maintenance a primary scoring axis, not an afterthought.

This session's codebase investigation established the key enabling fact: **all cross-event session state is server-side and untouched by a client rewrite.** `SessionRegistry` (the in-memory session register and signal/rework/topic aggregation) lives in the daemon (`crates/unimatrix-server/src/infra/session.rs`), owned by `server.rs` and Arc-shared into *both* the UDS listener and the HTTP `/observe` router. `hook.rs` is a stateless per-invocation client — it holds no cross-event state. The client and server communicate **only** via the JSON wire types in `crates/unimatrix-engine/src/wire.rs`. So unifying the client is a *port behind a stable wire contract*, not a re-architecture of server state. The server is already transport-symmetric; the client is the asymmetric half.

The single open risk that gates the whole direction is empirical: **per-event process spawn latency for a non-Rust client.** The hook fires on every `PreToolUse`/`PostToolUse` — dozens of times per turn — and `hook.rs` is deliberately lean (sync std I/O, no tokio) for fast spawn. Whether a JS runtime can spawn fast enough on that hot path is unknown and must be measured before committing.

## Bounded Questions

### Q1: Is per-event spawn viable for a non-Rust client? (the gate — PoC required)

The hot-path events (`PreToolUse`, `PostToolUse`) spawn the hook command repeatedly within a single turn. Measure per-invocation spawn-to-exit latency, against the current Rust binary as baseline, for:

- Node.js (cold and warm), invoked via absolute `node /path/to/hook.js` (not `npx` — see ass-067 cold-start finding)
- Bun
- A V8 snapshot / Node Single Executable Application (SEA)
- A `bun build --compile` single-file executable
- A **WASM module instantiated under Node** (WASI P1) per spawn — instantiation + call overhead (the Q2c paths live or die here)
- (Baseline) the current Rust `unimatrix hook` binary

If per-event spawn is too slow to preserve interactive feel (overhead summed across many tool calls, not just a single 500 ms budget per ass-064), evaluate a **long-lived local helper/daemon** the per-event hooks talk to (e.g., a tiny resident process over UDS), and price its complexity and reliability cost. Output a clear **go / no-go on per-event spawn**, with numbers, plus the fallback design if it's no-go.

### Q2: Unified client, dual transport — what is the cleanest shape?

Evaluate architectures for one client that speaks **local UDS** (4-byte length-prefixed JSON framing) and **remote HTTP** (`/observe`), transport selected by config. **No option is pre-favored** — score each against the three goals with **long-term maintenance (single source of truth, drift surface) weighted as primary**, since the prior TS lean under-weighted it:

- **(a) Pure TypeScript rewrite** — one TS client with a transport abstraction (UDS via `net.connect`, HTTP via `fetch`/`http`). Cost: hook logic *and* a `wire.rs` mirror reimplemented in TS, maintained separately from the Rust codebase.
- **(b) The twin (status quo)** — keep Rust `hook.rs` for local + a separate TS client for remote. Two implementations; the trajectory this spike questions.
- **(c) WASM-bound `hook.rs`, repackaged** — compile the *existing* Rust hook logic to WASM, ship via npm, run under Node. **Single Rust source of truth, shared with the server (uses `wire.rs` directly — no schema mirror).** Evaluate the sub-variants, given Node's WASI limits:
  - **(c1) Hybrid** — WASM holds the pure logic (parse, normalize, `build_request`, `format_injection`, response transform); a thin, stable JS shell does the I/O (stdin/stdout, UDS/HTTP transport, fs). Volatile logic stays in Rust; JS is a boring harness over a narrow bytes-in/bytes-out boundary.
  - **(c2) jco / component model** — transpile a WASI-P2 component to ESM via `@bytecodealliance/jco` + `preview2-shim` (richer capabilities, incl. outbound http, shimmed into JS).
  - **(c3) Native addon** — Rust as a Node native module (napi-rs): fastest, single source, but reintroduces the per-platform build matrix.
- **(d)** Any other shape the research surfaces.

Confirm explicitly what stays server-side (the `SessionRegistry` + aggregation + DB-backed observation pipeline) so the recommended client is purely the wire-protocol speaker over two transports.

### Q3: Wire-contract sync — how does a non-Rust client stay in lockstep?

Today `hook.rs` uses the `wire.rs` types directly (compile-time agreement with the server). A non-Rust client loses that guarantee — the duplication **relocates from the formatter to the wire schema.** Evaluate and recommend the lowest-drift approach:

- Codegen TypeScript from Rust (`ts-rs`, or `schemars` → JSON Schema → TS types)
- A language-neutral schema as the single source of truth, both sides generated
- Hand-maintained TS types guarded by contract/round-trip tests

This is a direct lever on Goal 2 (maintenance) — name the mechanism that keeps client and server from drifting. **Note this question largely *dissolves* under the Q2c WASM-binding paths**: a WASM-bound `hook.rs` reuses `wire.rs` directly, so there is no second schema to sync. The drift seam is specific to the TS-rewrite paths (Q2a/b) and counts against them in scoring.

### Q4: Where does response transformation belong?

`format_injection` (byte-budget truncation, header, entry blocks), the SubagentStart `hookSpecificOutput` envelope, `BriefingContent` extraction, and transcript prepend currently live client-side. Resolve where they belong in the target architecture:

- **(a)** Client-side, one implementation in the chosen client language (parity-tested against Rust during migration, then sole owner)
- **(b)** Server pre-formats provider-neutral text; client does only the host-specific stdout serialization (envelope / plain text / silent) — the part that genuinely *must* be client-side

Decide given: the SubagentStart envelope is **host-CLI-specific** (Claude Code vs Gemini vs Codex differ; ADR-006 crt-027), so pure server-side content negotiation is insufficient — but "no server change" is not sacred if relocating formatting cuts duplication. This determines how thin the client can ultimately be.

### Q5: Telemetry capture completeness & reliability across both transports

Inventory the local-only client responsibilities the remote thin-client scope (vnc-024) deferred, and place each in the target architecture, with its reliability requirement (graceful degradation = `exit 0` always, no stdout on failure):

- Project-root detection + project-hash computation (local attribution) — needed where?
- The disk **event-queue fallback** (resilience when the server is mid-restart) — both transports? client or relocated?
- Transcript-tail reading for `PreCompact` (`extract_transcript_block`, exchange-pair builder) — and the `CompactPayload.transcript_excerpt` seam: does the client populate it, or does the server buffer (the unbuilt #670)?

The recommendation must preserve full pipeline fidelity on **both** transports — no signal loss remote vs local.

### Q6: Single-step deployment with componentized packaging

What packaging + `init` delivers **one command per deployment style**, installing only what that style needs? Build on ass-067 Q1/Q5:

- **Local**: client + Rust server binary (+ ONNX model via postinstall)
- **Remote**: client only (tens of KB) — no server binary, no model, no ONNX
- One package with conditional/optional components vs. separate packages (`@dug-21/unimatrix` + `@dug-21/unimatrix-client`)
- How `npx @dug-21/unimatrix init` (local) vs `... init --remote <url> --token <token>` (remote) selects and fetches only the necessary artifacts
- Disposition of the **macOS/Windows platform gaps** (ass-067 Q1) under the chosen shape
- Confirm the correct framing of the "single binary" principle: single-binary **server** + zero required infrastructure; the client is an adapter, not infrastructure. Note whether `PRODUCT-VISION.md` principle 6 wording needs to change to reflect "single installation."

### Q7: Migration roadmap — all-Rust → end-state

Decompose the path from today (all-Rust hook over UDS, **no HTTP client at all**) to the recommended end-state into ordered, **independently-shippable** chunks. For each chunk: user-facing deliverable, dependencies, parity/acceptance gates, risk, rough effort, and **what is reversible** (so a failed Q1 latency gate doesn't strand the migration). **The chunk breakdown is conditional on Q2** — a WASM-repackage path may be *less* work than a TS rewrite (it reuses `hook.rs` rather than porting it); surface both breakdowns if the architecture choice is close. Must explicitly address:

- Does **vnc-024 (#672)** become chunk 1 — the HTTP client — reframed as step 1 of unification rather than a permanent twin?
- At what point (if ever) is the Rust `hook.rs` retired, and what parity gates guard that cutover? (Byte/semantic parity vectors as a *temporary migration gate*, not permanent dual-maintenance.)
- Where do the local-only pieces (event queue, UDS length-prefix framing, transcript reader) get ported, and in which chunk?

## Decision Criteria

This is a genuinely multi-factor decision — no single axis dominates, and the strong options trade off against each other. Score every Q2 candidate across **all** of:

1. **Long-term maintenance** *(primary)* — single source of truth, drift surface, number of languages/toolchains to keep in sync. (Where the prior TS lean fell short.)
2. **Per-event latency & reliability** — hot-path spawn/instantiation overhead (Q1), graceful degradation, behavior under a down server.
3. **Client-side architecture fit** — runtime footprint on the developer's machine, host-CLI differences (Claude Code / Codex / Gemini hook contracts), debuggability across any language boundary, security/supply-chain surface.
4. **Deployment ease** — single-step install per style, only-necessary-components per style, platform coverage (macOS/Windows gaps), and the upgrade / version-coupling story.

Name the trade-offs explicitly rather than collapsing them to one number — e.g., WASM-binding's maintenance win vs. its debuggability and build/deployment cost; a native addon's latency win vs. the per-platform matrix it reintroduces; a TS rewrite's deployment simplicity vs. its maintenance/drift cost. The recommendation is the best *balance* for Unimatrix's single-maintainer, OSS, personal-cloud context — state the weighting used.

## Approach

**Investigation + evaluation + targeted PoC.** Internal analysis of the current `hook.rs` client surface and the `wire.rs` contract; external ecosystem research on JS runtime startup, single-file-executable tooling, and codegen options. **Unlike ass-067, this spike requires a small empirical PoC** — the Q1 spawn-latency measurement is the gating input and cannot be answered from literature alone.

**Breadth: `code+ecosystem`, thorough.** Bun/Node SEA/snapshot startup characteristics; `ts-rs`/`schemars` codegen maturity; npm conditional-component packaging precedents (esbuild, Prisma, Biome, turbo, bun).

**Confidence required: `actionable`.** Specific enough to write a design brief and a chunked delivery roadmap from, with a clear go/no-go on per-event spawn.

**Constraints classification:**
- **Hard**: Client and server communicate only via `wire.rs` JSON types (length-prefixed over UDS, plain JSON over HTTP `/observe`). The client conforms; the wire contract is the boundary.
- **Hard**: All cross-event session state is server-side (`SessionRegistry`) and out of scope to change — the client holds no cross-event state.
- **Hard**: Full pipeline fidelity on both transports — no remote-vs-local signal loss.
- **Hard**: Graceful degradation — the hook process exits 0 regardless of server state; no stdout on failure (ass-064, FR-03.7).
- **Hard**: A JS/TS runtime is already present on every target (Claude Code, Codex, Gemini) — whatever ships must run under Node without adding a new runtime dependency.
- **Open — do NOT pre-decide**: WASM is a **first-class alternative**, not ruled out. ass-067 dropped it as "premature" on build-simplicity grounds; this spike re-opens it on maintenance grounds. The real question is whether `hook.rs`'s I/O profile (stdio + fs + UDS/HTTP) is best served by WASI P1 + a thin JS I/O shell, jco/P2-shim, or a native addon — and at what per-spawn-latency and maintenance cost vs. a TS rewrite.
- **Hard**: SubagentStart stdout requires the `hookSpecificOutput` envelope; plain text is silently dropped (ADR-006 crt-027).
- **No preferred answer**: The end-state is genuinely open. Candidates — TS rewrite (Q2a), the twin (Q2b), WASM-bound `hook.rs` (Q2c: hybrid / jco / native) — are scored primarily on long-term maintenance (single source of truth) and the Q1 latency/instantiation result. Correct for the prior bias: TS optimized build-simplicity; this spike optimizes maintenance + reliability.

**Dependencies / prior art to build on:**
- **ass-067** — pure-TS thin-client recommendation + packaging precedents (direct parent; this spike extends it)
- **ass-066** — session hosting (`unimatrix run`); the *programmatic* surface, adjacent and **out of scope** here
- **ass-064** — remote telemetry + MCP transport unification; 500 ms sync budget
- **ASS-014** — WASM cortical implant (prior centralized-deployment design); confirm client-side disposition (deferred/superseded by TS)
- **vnc-022** — `/observe` endpoint (shipped); `CompactPayload.transcript_excerpt` forward-compat seam
- **vnc-024 (#672)** — in-flight thin-client scope; this spike's roadmap determines its disposition
- **ADR-006 crt-027** — SubagentStart `hookSpecificOutput` envelope
- This session's codebase investigation: `hook.rs` ownership inventory; server-side `SessionRegistry`

## Out of Scope

- **Session hosting / `unimatrix run` (ass-066)** — the programmatic-session surface. The recommended client architecture must not *preclude* it, but designing it is separate.
- Enterprise / multi-user credential handling.
- Server-side intelligence-pipeline changes (the client migration is wire-compatible by construction).
- **Implementing** the migration — this spike produces the architecture + roadmap; delivery is separate sessions.

## What the Output Should Be

- **Architecture recommendation**: the proper client for telemetry capture + dual transport (UDS/HTTP), with the formatting-ownership (Q4) and wire-sync (Q3) decisions resolved — explicitly comparing **TS-rewrite vs WASM-bound `hook.rs`** on long-term maintenance, scored against the three goals.
- **Go/no-go on per-event spawn** (Q1): measured numbers, recommended runtime/packaging, and the long-lived-helper fallback design if no-go.
- **Single-step deployment recommendation** (Q6): componentized packaging + `init` that installs only what each style needs.
- **Migration roadmap** (Q7): ordered, independently-shippable chunks from all-Rust → end-state, with gates, effort, and reversibility — and vnc-024's place in it.
- **Dispositions**: vnc-024 (#672), the `PRODUCT-VISION.md` "single binary" principle wording, and ASS-014 WASM (confirm deferral/supersession for the client).

## Known Constraints

- `hook.rs` is ~4,800 lines, stateless per-invocation, sync std I/O, no tokio — lean by design for fast spawn.
- The hot path (`PreToolUse`/`PostToolUse`) spawns the hook many times per turn; cumulative overhead matters more than any single-event budget.
- `npx` cold start adds 200–500 ms — hook commands must use an absolute runtime/executable path (ass-067).
- npm is the existing release channel; the current `optionalDependencies` platform-package structure is sound (ass-067).
- macOS (arm64/x64) and Windows platform packages are not yet shipped (ass-067 Q1).
