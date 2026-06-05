# FINDINGS: Telemetry Client Architecture — Unifying STDIO + HTTP into One Maintainable Client

**Spike**: ass-068
**Date**: 2026-06-02
**Approach**: Investigation + evaluation + targeted PoC (Q1 empirical)
**Confidence**: Actionable

---

## Findings

### Q1: Is per-event spawn viable for a non-Rust client? (the gate — PoC required)

**Answer**: Yes — per-event spawn is viable for Node.js. The overhead is real but tolerable. No long-lived daemon fallback is needed.

**Evidence**: Empirical measurements on the development environment (Linux x86_64, Node v24.16.0, Rust 1.95.0), 50 iterations with 5 warmup runs per candidate. Each invocation reads stdin JSON (217 bytes), parses, transforms, and writes stdout — the same work profile as `hook.rs`.

| Candidate | avg (ms) | p50 (ms) | p95 (ms) | p99 (ms) |
|---|---|---|---|---|
| **Rust noop (pure overhead)** | 0.3 | 0.3 | 0.3 | 0.4 |
| **Rust hook-sim (parse+transform)** | 0.3 | 0.3 | 0.3 | 0.4 |
| **Real `unimatrix hook` binary** | 6.0 | 5.8 | 7.4 | 9.0 |
| **Node.js sync hook-sim** | 11.9 | 11.8 | 13.7 | 15.6 |
| **Node SEA (single executable)** | 11.3 | 11.3 | 13.3 | 14.3 |
| **WASM P1 under Node** | 14.8 | 14.5 | 17.3 | 18.1 |
| **Node.js noop (bare startup)** | 9.7 | 9.7 | 11.3 | 11.8 |

Critical finding: **the real Rust binary is 6ms, not 0.3ms**, because it includes project root detection (`detect_project_root` walks up directory tree to `.git`), project hash computation (SHA-256 of path), home directory resolution, socket path construction, and UDS connect attempt. The Node.js overhead (~12ms) is **2x the real Rust binary**, not 40x.

**Cumulative overhead per turn** (16-24 spawns for 8-12 tool calls):

| Client | x16 spawns | x24 spawns | x40 spawns |
|---|---|---|---|
| Rust binary (current) | 97 ms | 146 ms | 243 ms |
| Node.js (remote, minimal work) | 191 ms | 286 ms | 477 ms |
| Node.js (local, full work) | 295 ms | 442 ms | 736 ms |

Context for these numbers:
- **ass-064 per-event sync budget**: 500ms — Node.js at 12ms is well within budget
- **Model API latency per tool call**: 2-30 seconds — hook overhead is <1% of turn time
- **User-perceivable delay**: cumulative ~300ms for a 24-spawn turn is imperceptible against 30-180 seconds of model time in that same turn
- **Fire-and-forget events** (SessionStart, Stop, RecordEvent) dominate — these do not block the model pipeline at all. Only UserPromptSubmit, PreCompact, and SubagentStart are synchronous (typically 3-5 per turn, not 24)

**Node SEA** offers no meaningful improvement (11.3ms vs 11.9ms) — startup time is dominated by V8 initialization, not script loading. The 121MB SEA binary makes it impractical for distribution.

**WASM under Node** is the worst performer (14.8ms) — it adds WASM compilation + instantiation on top of Node.js startup. Running WASM per-spawn is strictly worse than running pure JS per-spawn.

**Go/no-go**: **GO for per-event spawn with Node.js.** The overhead is 2x the Rust binary on the hot path but well within the 500ms sync budget and negligible relative to model API latency. No long-lived daemon fallback is needed. The fire-and-forget dominance (16-20 of 24 spawns are async) means only 3-5 synchronous events actually block, totaling ~36-60ms of sync overhead per turn.

**Recommendation**: Use `node /absolute/path/hook.js <EVENT>` as the hook command (not `npx`, confirming ass-067). Node.js sync I/O (`fs.readFileSync('/dev/stdin')`) outperforms async by a small margin and is simpler. Node SEA and WASM-per-spawn add complexity without meaningful latency improvement.

---

### Q2: Unified client, dual transport — what is the cleanest shape?

**Answer**: **(a) Pure TypeScript rewrite** is the recommended architecture, with `ts-rs` codegen for wire-contract sync (Q3) eliminating the primary maintenance concern that motivated re-opening WASM.

**Evidence**: Multi-axis evaluation of all candidates.

#### Architecture Evaluation

| Axis | (a) TS Rewrite | (b) Twin (status quo) | (c1) WASM Hybrid | (c2) jco/P2 | (c3) napi-rs |
|---|---|---|---|---|---|
| **Maintenance** (primary) | Medium: one TS impl + codegen types from Rust | **Worst**: two impls in two languages | **Best**: one Rust source, zero drift | Good: one Rust source, complex toolchain | Good: one Rust source, per-platform matrix |
| **Per-event latency** | 12ms (GO) | 6ms local / 12ms remote | 15ms (worst) | ~15ms (same as c1) | ~7ms (near-native) |
| **Debuggability** | **Best**: JS stack traces, console.log, node --inspect | Good (two familiar ecosystems) | Poor: WASM stack traces opaque, boundary debugging painful | Poor: jco transpile output is unreadable generated JS | Medium: native crashes harder to diagnose than JS |
| **Deployment ease** | **Best**: zero native deps, single JS file, <50KB | Complex: two install paths | Medium: .wasm + shim, ~2MB | Medium: transpiled ESM + preview2-shim dep | **Worst**: per-platform binaries reintroduced |
| **Runtime footprint** | ~50KB JS | 17MB Rust + 50KB JS | ~2MB .wasm + shim | ~2MB + npm dep | ~5MB per platform |
| **Supply chain** | Minimal: pure JS, no native deps | Mixed: Rust + JS | Medium: wasm toolchain | Higher: jco + preview2-shim deps | Medium: napi-rs + Rust |
| **Build complexity** | Trivial: tsc or none (plain JS) | Two toolchains | High: wasm32-wasip1 target + wasm-opt | **Highest**: wasm32-wasip2 + jco transpile + preview2-shim | High: napi-rs + cross-compilation |

#### Why WASM-binding (c1/c2/c3) does not win despite maintenance advantage

The SCOPE correctly identified that WASM-binding's maintenance advantage is real: `hook.rs` compiled to WASM reuses `wire.rs` directly, eliminating schema drift. However, the PoC (Q1) revealed that this advantage is undercut by three compounding factors:

1. **WASM-per-spawn is the worst latency option** (14.8ms vs 11.9ms for pure JS). The "single Rust source of truth" benefit only applies when running via WASM, but running via WASM costs 25% more per spawn than pure JS. This creates an ironic inversion: the option chosen for maintenance actually performs worse on the hot path.

2. **The maintenance advantage largely dissolves under codegen**. With `ts-rs` (see Q3), TypeScript types are generated from Rust `wire.rs` at build time. The "hand-maintained TS mirror" concern that justified re-opening WASM is addressed by automation — the TS types are derived from the Rust source, not maintained separately. The drift surface shrinks to the ~200 lines of transformation logic (`format_injection`, `build_request`), which is straightforward to parity-test.

3. **Debuggability and the single-maintainer context**. Unimatrix is a single-maintainer OSS project. When a hook fails in a user's environment, a pure JS client produces readable stack traces, accepts `console.log` debugging, and runs under `node --inspect`. A WASM boundary produces opaque `RuntimeError: unreachable` messages. For a project where the maintainer IS the support team, debuggability directly affects maintenance cost — the axis WASM was supposed to win on.

**The c2 (jco/component model) path** is additionally disqualified because Node.js still has no native WASI P2 support (confirmed: nodejs/node#55396 open, no milestone as of June 2026). The `@bytecodealliance/preview2-shim` is viable but adds a dependency for capability shimming that pure JS does not need. The transpiled ESM output from jco is machine-generated and unreadable — debugging requires source maps back to Rust, adding toolchain complexity.

**The c3 (napi-rs native addon) path** reintroduces the per-platform binary matrix that the current `optionalDependencies` structure already handles for the server. Adding a second set of per-platform packages for the client doubles the release matrix. Latency would be near-native (~7ms), but the 5ms improvement over pure JS does not justify the distribution complexity for a single-maintainer project.

#### Confirmation: client holds no cross-event state

Verified in code: `SessionRegistry` in `crates/unimatrix-server/src/infra/session.rs` owns all cross-event state — injection history, rework tracking, topic tallies, session metadata. The `hook.rs` `run()` function creates a fresh `LocalTransport`, reads stdin, builds a request, sends it, writes stdout, and exits. No state persists between invocations. The client is a stateless wire-protocol adapter.

**Recommendation**: Use architecture (a) — pure TypeScript rewrite with a transport abstraction layer. UDS transport via Node.js `net.connect` (length-prefixed framing), HTTP transport via `http`/`https` built-in modules. Single implementation, two transports selected by config. Wire types generated from Rust via `ts-rs`.

---

### Q3: Wire-contract sync — how does a non-Rust client stay in lockstep?

**Answer**: Use `ts-rs` to codegen TypeScript type definitions from the Rust `wire.rs` serde types. Guard with round-trip contract tests.

**Evidence**:

The `wire.rs` file defines 6 types (`HookInput`, `HookRequest`, `HookResponse`, `ImplantEvent`, `EntryPayload`, `TransportError`) with 20 serde annotations including `#[serde(tag = "type")]` for internally-tagged enums, `#[serde(flatten)]`, `#[serde(default)]`, and `#[serde(skip_serializing_if)]`. These are exactly the annotations `ts-rs` supports with its `serde-compat` feature (enabled by default).

**Codegen approach comparison**:

| Approach | Drift risk | Automation | Maturity |
|---|---|---|---|
| `ts-rs` (derive macro -> .ts files) | **Lowest**: compile-time binding, serde-compat parses tags | `cargo test` exports .ts files automatically | v12.0+, actively maintained, 5k+ GitHub stars |
| `schemars` -> JSON Schema -> TS types | Low: two-step, JSON Schema is intermediate | Requires extra tooling (`json-schema-to-typescript`) | Mature but more complex pipeline |
| Hand-maintained TS types + contract tests | **Highest**: manual sync, tests catch after the fact | None | N/A |

**ts-rs integration for `wire.rs`**: Add `#[derive(TS)]` and `#[ts(export)]` to `HookInput`, `HookRequest`, `HookResponse`, `ImplantEvent`, and `EntryPayload`. Running `cargo test` generates `bindings/HookRequest.ts`, `bindings/HookResponse.ts`, etc. These generated files become the TS client's type definitions — imported directly, never hand-edited.

The generated types handle the critical serde patterns:
- `#[serde(tag = "type")]` on `HookRequest`/`HookResponse` -> TypeScript discriminated unions with `type` literal field
- `#[serde(default)]` -> optional fields with `?`
- `#[serde(skip_serializing_if = "Option::is_none")]` -> conditional inclusion
- `#[serde(flatten)]` on `HookInput.extra` -> intersection type

**Contract test layer**: In addition to codegen, add a `tests/wire_contract.rs` that serializes every `HookRequest` variant to JSON, writes to a fixture file, and a corresponding `wire_contract.test.ts` that deserializes the same fixtures and validates structure. This catches behavioral mismatches that type-level codegen cannot (e.g., serialization of `None` vs omission).

**Note on WASM dissolution**: Under Q2c paths, this question dissolves entirely because WASM reuses `wire.rs` directly. Since Q2a was selected, Q3 becomes load-bearing — and `ts-rs` codegen addresses it sufficiently.

**Recommendation**: Add `ts-rs` as a dev-dependency to `unimatrix-engine`. Derive `TS` on all wire types. Generate TypeScript bindings as part of `cargo test`. CI gate: generated .ts files must be committed and match cargo-test output (diff check). Add JSON fixture round-trip contract tests for behavioral parity.

---

### Q4: Where does response transformation belong?

**Answer**: **(b) Server pre-formats text; client does only host-specific stdout serialization.** Move `format_injection` and `BriefingContent` assembly to the server. The client's only formatting job is the SubagentStart `hookSpecificOutput` envelope — which genuinely must be client-side because it is host-CLI-specific.

**Evidence**:

Current client-side transformation responsibilities in `hook.rs`:

| Responsibility | Lines | Host-specific? | Server-movable? |
|---|---|---|---|
| `format_injection()` — byte-budget truncation, header, entry blocks | ~40 | No | **Yes** |
| `format_entry_block()` — single entry formatting | ~8 | No | **Yes** |
| `prepend_transcript()` — combine transcript + briefing | ~10 | No | **Yes** |
| `extract_transcript_block()` — read transcript tail, parse JSONL, format | ~80 | Yes (reads local file) | Partially (see Q5) |
| `write_stdout_subagent_inject()` — `hookSpecificOutput` JSON envelope | ~12 | **Yes** (Claude Code-specific) | **No** |
| `write_stdout()` — route response to stdout by type | ~25 | **Yes** (host envelope contracts differ) | **No** |
| `build_exchange_pairs()` — JSONL transcript parsing | ~150 | Yes (reads local file format) | Partially |

The transformation that belongs server-side is content formatting — `format_injection` and `prepend_transcript`. These are:
- Not host-specific (same text format regardless of Claude Code / Codex / Gemini)
- Already running server-side data (the `Entries` response items come from the server)
- The primary source of parity risk if duplicated in TS

Server-side pre-formatting means `/observe` returns formatted plain text for sync events, not raw JSON `HookResponse::Entries`. The client receives text and wraps it in the host-appropriate envelope:
- **UserPromptSubmit**: plain text to stdout
- **SubagentStart**: wrap in `{"hookSpecificOutput": {"hookEventName": "SubagentStart", "additionalContext": "<text>"}}`
- **PreCompact**: plain text to stdout (transcript prepend moves server-side via `transcript_excerpt` field in `CompactPayload`)

**Implementation**: Add an `Accept: text/plain` header to sync HTTP requests. When the server sees `Accept: text/plain`, it calls `format_injection` internally and returns formatted text. When it sees `Accept: application/json` (or no header), it returns the current JSON envelope. UDS client sends JSON and formats locally (backward compatible). New TS client sends HTTP with `Accept: text/plain` and receives pre-formatted text.

**What stays client-side**:
1. Host-CLI envelope serialization (SubagentStart `hookSpecificOutput`, future Codex/Gemini equivalents)
2. Stdout/stderr routing (which responses produce output vs. silent)
3. Exit code management (always 0)

**Recommendation**: Add server-side content negotiation to `/observe` for sync events. Return pre-formatted text when `Accept: text/plain`. This reduces the TS client's transformation surface to ~40 lines of host-envelope logic — trivially parity-testable. The Rust `hook.rs` local path continues formatting client-side (no regression) until it is retired.

---

### Q5: Telemetry capture completeness & reliability across both transports

**Answer**: Three local-only responsibilities require explicit placement in the unified architecture. All are resolvable without signal loss.

**Evidence**: Inventory of local-only client responsibilities from `hook.rs` analysis:

#### 1. Project-root detection + project-hash computation

**Current**: `hook.rs` — `detect_project_root()` + `compute_project_hash()`. Used to compute the UDS socket path (`~/.unimatrix/{hash}/unimatrix.sock`).

**Placement**:
- **Local transport**: client computes project hash to find the socket. Stays in the client — unavoidable for UDS path discovery.
- **Remote transport**: **not needed by the client**. The remote server already knows which project it serves (configured at `init --remote`). The client sends the project root path in `SessionRegister.cwd`, and the server computes/validates the hash. Project-root detection is still useful for `init --remote` (to determine where to write `.mcp.json` and `.claude/settings.json`), but not needed per-event.
- **Reliability**: No signal loss. Remote events carry `cwd` in `SessionRegister`; server-side project identification is authoritative.

#### 2. Disk event-queue fallback

**Current**: `EventQueue` in `crates/unimatrix-engine/src/event_queue.rs` (550 lines). When UDS server is unavailable, fire-and-forget events are written to `~/.unimatrix/{hash}/event-queue/pending-{timestamp}.jsonl`. Replayed on next successful connect.

**Placement**:
- **Local transport**: Port to TS. The event queue is 550 lines of Rust but conceptually simple: append JSON lines to a file, rotate on count, prune on age, replay oldest-first. TS equivalent: ~150 lines using `fs.appendFileSync`. Directory: same `~/.unimatrix/{hash}/event-queue/` path.
- **Remote transport**: **Different mechanism.** HTTP failures are transient (server unreachable, network blip). Two options: (a) same disk queue, or (b) in-memory retry with exponential backoff since HTTP failures are typically shorter-lived than "server not started yet" UDS failures.
- **Recommendation**: Implement disk queue for both transports. It is simple, bounded, and covers both "server restarting" and "network blip" cases uniformly. The TS implementation is straightforward — `fs.appendFileSync` + `fs.readdirSync` + file rotation.
- **Reliability**: Full parity. Events queued during server downtime are replayed on next successful connection, same as today.

#### 3. Transcript-tail reading for PreCompact

**Current**: `extract_transcript_block()` in `hook.rs`. Reads the last 12KB of the transcript JSONL file, parses into exchange pairs, formats a restoration block within 3000 bytes.

**Placement**: This is the most complex local-only responsibility. Three options:

**(a) Client reads transcript, sends in `CompactPayload.transcript_excerpt`**: The wire protocol already has this field (forward-compatible, added in vnc-022). The TS client reads the transcript file (same `extract_transcript_block` logic), populates `transcript_excerpt`, and the server uses it for compaction defense. Requires porting ~230 lines of JSONL parsing logic to TS.

**(b) Server buffers transcript via observation events**: The server already observes every tool call and prompt via RecordEvent. Instead of reading the transcript file, the server reconstructs the recent conversation from its own observation data. This is the #670 approach. Eliminates client-side transcript reading entirely.

**(c) Remote clients skip transcript excerpt**: For HTTP clients, `transcript_excerpt` stays None (current behavior). The server falls back to its briefing-only compaction defense. Signal degradation but functional.

- **Recommendation**: Option (a) for local, option (c) for remote initially, option (b) as the strategic target (#670). The TS client ports `extract_transcript_block` for local mode (it has `transcript_path` from stdin). Remote mode sends `transcript_excerpt: null` — the server handles this gracefully today. When #670 ships, both paths converge on server-side reconstruction and `extract_transcript_block` is retired from the client.
- **Reliability**: Local: full parity via transcript_excerpt. Remote: graceful degradation (briefing-only compaction) until #670. No signal loss — the server always produces a compaction defense response; transcript excerpt improves its quality but is not required.

**Recommendation**: All three local-only responsibilities are resolvable. Project-hash: client for local, server for remote. Event queue: port to TS for both transports (~150 lines). Transcript reading: port for local, skip for remote, retire when #670 ships. Full pipeline fidelity maintained on both transports.

---

### Q6: Single-step deployment with componentized packaging

**Answer**: One npm package (`@dug-21/unimatrix`) with conditional component installation via `init` flags. No separate `@dug-21/unimatrix-client` package — the client JS is bundled in the root package.

**Evidence**:

The ass-067 recommendation of a separate `@dug-21/unimatrix-client` package was premised on the twin architecture (Q2b) where the TS client is a standalone tool. Under the unified architecture (Q2a), the client JS is small enough (<50KB) to bundle directly in the root package, simplifying the install story.

**Package structure**:

```
@dug-21/unimatrix                    # Single package
  bin/unimatrix.js                   # Existing CLI shim (routes to native or JS)
  lib/init.js                        # Existing init (extended with --remote)
  lib/hook-client/                   # NEW: TS hook client (~50KB)
    index.js                         # Hook entry point
    transform.js                     # Host-envelope serialization (~40 lines)
    transport-uds.js                 # UDS transport (length-prefix framing)
    transport-http.js                # HTTP transport (fetch/http)
    event-queue.js                   # Disk event queue
    types.js                         # ts-rs generated wire types
  optionalDependencies:
    @dug-21/unimatrix-linux-x64      # Rust server binary + libonnxruntime
    @dug-21/unimatrix-linux-arm64
    @dug-21/unimatrix-darwin-arm64   # NEW (macOS)
    @dug-21/unimatrix-darwin-x64     # NEW (macOS)
```

**Init flow by deployment style**:

| Command | What installs | What runs |
|---|---|---|
| `npx @dug-21/unimatrix init` | Root package + platform binary + ONNX model | MCP: `unimatrix` binary (stdio). Hooks: `unimatrix hook <EVENT>` (Rust UDS) |
| `npx @dug-21/unimatrix init --remote <url> --token <tok>` | Root package only (platform binary is optional, not downloaded if missing) | MCP: `unimatrix` binary if available, else remote MCP-over-HTTP (future). Hooks: `node /path/to/lib/hook-client/index.js <EVENT>` (TS HTTP) |

The key insight: `optionalDependencies` with `os`/`cpu` fields means npm automatically skips platform binaries on platforms without a matching package. A macOS user running `init --remote` gets zero native binaries — just the JS. A Linux user running `init` (local) gets the platform binary automatically.

**Remote-only install size**: ~200KB (root package JS + skills + protocols). No 31MB platform binary, no 87MB ONNX model.

**Platform gap disposition** (macOS/Windows from ass-067 Q1):
- macOS arm64/x64: Add platform packages with cross-compiled server binary. Required for local mode on macOS. Remote mode works today without them.
- Windows: P2 priority. Remote mode works today (Node.js + HTTP). Local mode requires Windows UDS support (available in Node.js but not in the Rust server's Unix-specific socket code).

**"Single binary" principle (PRODUCT-VISION.md principle 6)**:
Current wording: "Single binary, zero required infrastructure. Container is optional. Daemon + UDS works without it."

This principle describes the **server deployment model**, not the client. The client is an adapter, not infrastructure. The principle accurately reflects the server story (one `unimatrix` binary, no database to configure, no container required). Recommend updating to: "Single binary server, zero required infrastructure. The client is an adapter — JS for hooks, the binary for MCP. Container is optional."

**Recommendation**: Bundle the TS hook client in the existing `@dug-21/unimatrix` package under `lib/hook-client/`. No separate package needed. `init --remote` installs only the JS client. `init` (local) installs the platform binary via `optionalDependencies`. Update PRODUCT-VISION.md principle 6 wording to clarify server vs. client distinction.

---

### Q7: Migration roadmap — all-Rust to end-state

**Answer**: Five independently-shippable chunks, ordered by dependency and risk. vnc-024 becomes chunk 1 (reframed). Total: ~6-8 weeks.

**Evidence**: Decomposition based on the Q2a (TS rewrite) architecture decision, Q3 (ts-rs codegen), Q4 (server-side formatting), and Q6 (bundled packaging).

#### Chunk 1: Wire contract codegen + server-side content negotiation (1-2 weeks)

**Deliverable**: `ts-rs` integration on `wire.rs` types; `/observe` returns formatted text for `Accept: text/plain` on sync events; JSON fixture round-trip contract tests.

**This IS vnc-024 (#672), reframed.** vnc-024's original scope (curl config, installation, HTTP hook investigation) is subsumed. Its remaining value is wire contract documentation + server-side format handling. Reframe the issue description to match.

**Dependencies**: None — builds on shipped vnc-022 (`/observe` endpoint).
**Gates**: Generated .ts types compile. Contract test fixtures pass round-trip in both Rust and TS. `/observe` with `Accept: text/plain` returns formatted text identical to `format_injection` output.
**Risk**: Low. Server-side change is additive (new content negotiation path). `ts-rs` is a dev-only dependency.
**Reversible**: Fully. `ts-rs` is dev-only. Content negotiation is a new code path, not a modification.

#### Chunk 2: TS hook client — HTTP transport (remote) (2-3 weeks)

**Deliverable**: `lib/hook-client/` in the npm package. Reads stdin JSON, normalizes event name, constructs `HookRequest`, POSTs to `/observe` with Bearer auth, transforms response to stdout (host-envelope serialization). HTTP transport only. `init --remote` configures hooks to use `node /path/to/hook-client/index.js <EVENT>`.

**Dependencies**: Chunk 1 (wire types, content negotiation).
**Gates**: Byte-identical output vs Rust `hook.rs` for the same inputs on all 13 event types (parity test suite). Size < 100KB. Node 18/20/22/24 compatibility. Graceful degradation: server unreachable -> exit 0, no stdout.
**Risk**: Medium. Response transformation parity is the primary risk — mitigated by the parity test suite and server-side formatting (Q4) reducing client-side transformation to ~40 lines.
**Reversible**: Fully. `init --remote` is a new flag; removing the TS client leaves the existing local path untouched.
**Effort**: ~200 lines transport, ~40 lines transform, ~60 lines normalize/build_request, ~80 lines CLI entry point, ~150 lines event queue, ~200 lines tests = ~730 lines total.

#### Chunk 3: TS hook client — UDS transport (local alternative) (1-2 weeks)

**Deliverable**: `transport-uds.js` — Node.js `net.connect` with 4-byte BE length-prefix framing (matching `wire.rs` framing protocol). Transport selected by config (socket path present -> UDS, server URL present -> HTTP). Event queue shared across transports.

**Dependencies**: Chunk 2 (hook client framework).
**Gates**: Round-trip parity with Rust `hook.rs` over UDS. Length-prefix framing byte-identical to `wire.rs` `write_frame`/`read_frame`. Latency: < 20ms per invocation including project-root detection.
**Risk**: Low. UDS transport is straightforward — `net.connect` + `Buffer` for framing. Project root detection and hash computation port directly from the existing `init.js` `detectProjectRoot()`.
**Reversible**: Fully. UDS transport is a new module alongside HTTP.

#### Chunk 4: `init` unification + transcript reader + packaging (1-2 weeks)

**Deliverable**: `init.js` extended with `--remote` flag (per ass-067 Q3 design). Transcript reader (`extract_transcript_block` equivalent) for PreCompact local path. `init` selects hook command based on mode: local -> Rust binary (if available) or TS+UDS, remote -> TS+HTTP. Platform detection logic for graceful fallback.

**Dependencies**: Chunks 2+3.
**Gates**: `npx @dug-21/unimatrix init` on Linux produces working local setup (unchanged). `npx @dug-21/unimatrix init --remote <url> --token <tok>` produces working remote setup. macOS `init --remote` works without platform binary.
**Risk**: Low. Init logic is JavaScript, already 80% implemented.
**Reversible**: `--remote` flag is additive.

#### Chunk 5: Rust `hook.rs` retirement gate (1 week, deferred until validated)

**Deliverable**: Decision gate — retire Rust `hook.rs` or keep as high-performance local option. If retire: `init` on local switches to TS+UDS client. If keep: dual-track remains but as a deliberate choice, not an accidental twin.

**Dependencies**: Chunks 1-4 fully validated in production.
**Gates**:
- Parity test suite passes for all 13 events on both transports
- Latency regression acceptable (Node.js ~12ms vs Rust ~6ms per spawn — measured, not estimated)
- Event queue replay works identically
- PreCompact transcript reading produces equivalent output
- 2+ weeks of production usage with zero regressions

**Risk**: Low by this point — all prior chunks validate the replacement incrementally.
**Reversible**: Yes — the Rust binary remains compiled and available. Retirement is a config change in `init.js`, not code deletion. The binary can be reinstated by changing the hook command back.

**When to retire `hook.rs`**: When the TS client has been the default for new installations for 4+ weeks with zero signal-loss incidents. The binary continues to be built (it is the MCP server entrypoint) — only the `hook` subcommand routing in `init.js` changes.

#### Timeline and effort summary

| Chunk | Effort | Cumulative | What ships |
|---|---|---|---|
| 1: Wire codegen + content negotiation | 1-2 weeks | 1-2 weeks | ts-rs types, server-side formatting, contract tests |
| 2: TS HTTP client | 2-3 weeks | 3-5 weeks | Remote hook client, `init --remote` |
| 3: TS UDS client | 1-2 weeks | 4-7 weeks | Local TS alternative, transport abstraction |
| 4: Init unification + packaging | 1-2 weeks | 5-8 weeks | Unified init, transcript reader, macOS remote |
| 5: Retirement gate | 1 week | 6-9 weeks | Decision: retire or keep Rust hook path |

#### Conditional breakdown (if WASM had won Q2)

For completeness, had Q2c been selected, the chunks would differ:

| Chunk (WASM path) | Effort | Notes |
|---|---|---|
| 1: WASM build pipeline | 2-3 weeks | wasm32-wasip1 target, wasm-opt, JS shim for I/O |
| 2: WASM HTTP transport shim | 1-2 weeks | JS shell calls fetch, passes bytes to WASM |
| 3: WASM UDS transport shim | 1-2 weeks | JS shell calls net.connect, passes bytes to WASM |
| 4: Init + packaging | 1-2 weeks | Same as TS path but with .wasm artifact |
| 5: Retirement gate | 1 week | Same criteria |

Total: 6-10 weeks — similar to the TS path. The WASM path is not cheaper; it trades TS porting effort for WASM toolchain setup effort. The TS path wins on debuggability and deployment simplicity, while the WASM path wins on type-level drift elimination (partially offset by ts-rs codegen).

#### Dispositions

- **vnc-024 (#672)**: Becomes Chunk 1. Reframe issue description from "thin client installation" to "wire contract codegen + server-side content negotiation." Reduce scope to: ts-rs integration, Accept header content negotiation on /observe, contract test fixtures.
- **ASS-014 WASM Phase 3**: Superseded for the client. The TS client replaces the WASM cortical implant concept. ASS-014's Phase 1-2 (bundled subcommand, npm distribution) remain valid and are shipping. Phase 3's WASM client is replaced by the TS client with HTTP transport — achieving the same goal (lightweight remote client, no platform binary needed) with better tooling.
- **PRODUCT-VISION.md principle 6**: Update wording to: "Single binary server, zero required infrastructure. The client is an adapter — JS for hooks, the binary for MCP. Container is optional."

---

## Unanswered Questions

1. **Bun runtime latency**: Bun was not available in the test environment. Bun's startup is reported as 2-5ms (vs Node.js 10ms). If validated, `bun /path/to/hook.js` could halve spawn overhead. Not blocking — Node.js latency is acceptable — but worth measuring when Bun is available.

2. **Claude Code HTTP hook handler behavior** (carried from ass-067): Does `"type": "http"` pass raw response body to stdout? Server-side content negotiation (Q4) makes this question less critical but still relevant for future zero-client-code remote hooks.

3. **Codex/Gemini CLI hook timeout budgets** (carried from ass-067): Whether `node /path/to/hook.js <EVENT>` works within their specific timeout budgets needs validation per-client.

4. **#670 server-side transcript reconstruction**: The strategic replacement for client-side transcript reading (Q5). Design and effort not scoped here.

5. **macOS cross-compilation CI**: Adding `@dug-21/unimatrix-darwin-arm64` and `@dug-21/unimatrix-darwin-x64` platform packages requires cross-compilation in the release pipeline. Effort not estimated here.

---

## Out-of-Scope Discoveries

1. **Node.js V8 startup is the dominant cost**: 10ms of the 12ms per-spawn is V8 initialization, not script execution. Any approach that spawns Node.js per-event pays this cost. A long-lived Node.js helper daemon (resident process over UDS) would amortize V8 startup across all events in a session — reducing per-event overhead to <1ms. This is the fallback if latency requirements tighten. Not needed today.

2. **Node SEA is impractical for distribution**: The 121MB binary (embedding the entire V8 engine) produces negligible latency improvement (11.3ms vs 11.9ms). Distribution cost far exceeds benefit. Do not pursue.

3. **WASM-per-spawn is an anti-pattern**: WASM compilation + instantiation on every spawn adds ~3ms over pure JS for zero benefit (the work is I/O-bound JSON transformation). WASM should only be considered for long-lived processes where compilation is amortized. This finding contradicts the ASS-014 Phase 3 assumption that per-event WASM would be viable.

4. **`init.js` is missing PreCompact and PostToolUseFailure**: Confirmed from ass-067 out-of-scope finding. `HOOK_EVENTS` array in `packages/unimatrix/lib/init.js` line 9 lists 7 events but omits PreCompact and PostToolUseFailure. Bug — fix in Chunk 4 or sooner.

5. **Fire-and-forget dominance reduces sync overhead concern**: Of ~24 hook spawns per turn, only 3-5 are synchronous (UserPromptSubmit, PreCompact, SubagentStart). The remaining 16-20 (SessionStart, Stop, RecordEvent, generic PreToolUse/PostToolUse) are fire-and-forget. Sync overhead per turn is ~36-60ms for Node.js, not 286ms. This significantly reduces the latency concern.

---

## Recommendations Summary

- **Q1 (Spawn latency gate)**: GO for per-event Node.js spawn. 12ms avg per invocation, well within 500ms sync budget. No daemon fallback needed. Use `node /absolute/path/hook.js <EVENT>` (sync I/O).
- **Q2 (Unified client architecture)**: Pure TypeScript rewrite (Q2a) with transport abstraction. WASM-binding (Q2c) rejected on latency (worst performer), debuggability (opaque errors), and dissolution of its primary advantage by ts-rs codegen.
- **Q3 (Wire contract sync)**: Use `ts-rs` to codegen TypeScript types from Rust `wire.rs` serde types. Augment with JSON fixture round-trip contract tests. CI-gated: generated types must match cargo-test output.
- **Q4 (Response transformation)**: Move `format_injection` to server via `Accept: text/plain` content negotiation on `/observe`. Client does only host-specific envelope serialization (~40 lines).
- **Q5 (Telemetry completeness)**: Project-hash: client for local, server for remote. Event queue: port to TS (~150 lines). Transcript reading: port for local, skip for remote, retire when #670 ships. No signal loss.
- **Q6 (Deployment packaging)**: Bundle TS client in existing `@dug-21/unimatrix` package under `lib/hook-client/`. No separate package. `init --remote` installs only JS. Update PRODUCT-VISION.md principle 6 to clarify server-vs-client distinction.
- **Q7 (Migration roadmap)**: Five chunks over 6-9 weeks. vnc-024 becomes Chunk 1 (reframed as wire codegen + content negotiation). Rust `hook.rs` retirement is Chunk 5, gated on 4+ weeks production usage. ASS-014 Phase 3 WASM superseded by TS client.
