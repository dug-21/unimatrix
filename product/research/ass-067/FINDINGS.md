# ASS-067 FINDINGS: Packaging, Installation, and Thin Client Architecture

**Spike**: ass-067
**Date**: 2026-05-31
**Approach**: Investigation + evaluation (code+ecosystem)
**Confidence**: Directional

---

## Q1: What does `npm install unimatrix` actually install?

**Answer**: The existing `@dug-21/unimatrix` npm package installs a JS shim (`bin/unimatrix.js`), a `postinstall.js` that downloads the ONNX model, a `lib/` directory with `init.js`, `merge-settings.js`, and `resolve-binary.js`, plus bundled skills and protocols. Platform-specific Rust binaries ship via `optionalDependencies` (`@dug-21/unimatrix-linux-x64`, `@dug-21/unimatrix-linux-arm64`), each containing the `unimatrix` binary and `libonnxruntime.so`. This is the esbuild/biome pattern, already implemented and shipping.

**Evidence**:

Package structure (from `packages/unimatrix/package.json`):
- Root package: `@dug-21/unimatrix` -- JS shim, init logic, skills, protocols
- Platform packages: `@dug-21/unimatrix-linux-x64` and `@dug-21/unimatrix-linux-arm64` with `os`/`cpu` fields
- `bin/unimatrix.js` routes `init` to JS implementation, all other subcommands to the native binary via `resolve-binary.js`
- `postinstall.js` triggers `unimatrix model-download` after binary resolution
- The release pipeline (`.github/workflows/release.yml`) builds both platform binaries, bundles `libonnxruntime.so`, runs smoke tests, and publishes to npm

Current size budget:
- `unimatrix` release binary (stripped): ~17 MB (Linux x64), ~35 MB debug
- `libonnxruntime.so.1.20.1`: ~14 MB
- ONNX model (`all-MiniLM-L6-v2`): ~87 MB (downloaded at postinstall, not bundled in tarball)
- Total per-platform npm package: ~31 MB (binary + libonnxruntime)

Missing platforms: macOS (arm64, x64) and Windows are not yet shipped. ASS-014 identified these as P0/P0/P2 respectively. The `resolve-binary.js` PLATFORMS map only contains `linux-x64` and `linux-arm64`.

**Recommendation**: The existing npm packaging infrastructure is sound and matches industry best practice. Extend to macOS arm64 (P0) and macOS x64 (P1) by adding `@dug-21/unimatrix-darwin-arm64` and `@dug-21/unimatrix-darwin-x64` platform packages. No package splitting needed.

---

## Q2: What is the best-suited architecture for the thin client?

**Answer**: Pure TypeScript is the correct architecture. WASM is premature (Node.js WASI Preview 2 is not supported, Preview 1 is still experimental), native Rust binaries would duplicate the existing platform-specific distribution complexity without benefit, and hybrid adds maintenance burden for zero advantage given the thin client's narrow scope.

### Architecture evaluation

| Architecture | Distribution | Performance | Maintenance | Size | Verdict |
|---|---|---|---|---|---|
| **Pure TypeScript** | Trivial (npm, no native deps) | Sufficient (JSON+HTTP) | Low (one language) | <50 KB | **Recommended** |
| Rust to WASM | Single .wasm, but Node.js WASI P2 NOT supported | Overkill for JSON+HTTP | Medium (Rust build step) | ~1-2 MB | Premature |
| Rust native binary | 5 platform packages | Fastest | High (cross-compilation matrix) | ~3-5 MB | Redundant |
| Hybrid (TS + WASM core) | npm + .wasm | Mixed | High (two runtimes) | ~1 MB | Overcomplicated |

### Why WASM is premature (critical finding)

ASS-014 recommended WASM for Phase 3, but the ecosystem has not matured as expected:

1. **Node.js WASI status**: As of Node.js v26.2.0 (May 2026), WASI support covers only `"unstable"` and `"preview1"`. WASI Preview 2 is NOT supported. GitHub issue nodejs/node#55396 remains open with no assigned milestone. The `uvwasi` project has stalled.

2. **JCO/Component Model alternative**: The Bytecode Alliance's `jco` tool can transpile WASM components with WASI P2 to ECMAScript modules using `@bytecodealliance/preview2-shim`. Viable but not simpler than pure TS for a client that does JSON parsing and HTTP.

3. **Prisma precedent**: Prisma migrated *away* from native Rust binaries to TypeScript+WASM, achieving 90% smaller bundles (14 MB to 1.6 MB) and 3.4x faster queries by eliminating cross-language serialization. Their lesson: the overhead of crossing the Rust-to-JS boundary exceeds any performance gain for I/O-bound work. The thin client is entirely I/O-bound.

### Why Pure TypeScript is sufficient

The thin client's responsibilities (from vnc-024 SCOPE.md and `hook.rs` analysis):

1. **Request construction**: Parse Claude Code's hook stdin JSON -> construct `HookRequest` JSON. Pure JSON-to-JSON transformation.
2. **HTTP transport**: POST to `{server_url}/observe` with Bearer auth. Standard fetch/undici.
3. **Response transformation** (the critical gap vnc-024 identified):
   - `HookResponse::Entries` -> formatted plain text via `format_injection()` logic
   - `HookResponse::BriefingContent` -> extract `content` field, optionally prepend transcript block
   - SubagentStart -> wrap in `hookSpecificOutput` JSON envelope
   - `HookResponse::Ack` / `HookResponse::Error` -> silent or stderr
4. **Event name normalization**: 12 static string mappings.
5. **Graceful degradation**: Server unreachable -> exit 0, no stdout.

None are computationally intensive. JSON parsing in V8 is faster than JSON parsing in WASM for payloads under 1 MB.

### Latency analysis

| Architecture | Overhead | Within budget? |
|---|---|---|
| TS | <1ms (Node.js already running) | Yes |
| WASM | ~5-10ms (instantiation + boundary) | Yes |
| Native | ~3-5ms (process spawn) | Yes |

**Recommendation**: Build as pure TypeScript (`@dug-21/unimatrix-client`). Zero native dependencies. Runs on Node.js >=18. Revisit WASM when Node.js ships WASI Preview 2.

---

## Q3: What does `init` do in each tier?

**Answer**: Both tiers use the same `npx unimatrix init` command with a `--remote` flag. The init flow is already 80% implemented in `packages/unimatrix/lib/init.js` for local mode.

### Local mode (`npx unimatrix init`) -- existing, needs minor updates

| Step | Current | Needed |
|---|---|---|
| Project root detection | Implemented | No change |
| Binary resolution | Implemented | No change |
| MCP registration in .mcp.json | stdio transport | No change |
| Hook installation in .claude/settings.json | 7 events | Add PreCompact, PostToolUseFailure |
| Skill copying | 10 skills | No change |
| Database pre-creation | Via `unimatrix version` | No change |
| CLAUDE.md configuration | Not implemented (prints instruction) | Should append knowledge block directly |

### Remote mode (`npx unimatrix init --remote <url> --token <token>`) -- new

| Step | Implementation |
|---|---|
| Project root detection | Reuse `detectProjectRoot()` |
| Auth handshake | POST `{url}/observe` with `{"type": "Ping"}` + Bearer token -> validate connectivity + auth |
| MCP registration in .mcp.json | stdio transport to thin client binary |
| Hook installation | `"type": "command"` pointing to `node /absolute/path/hook.js <EVENT>` |
| Skill copying | Same skills from thin client package |
| CLAUDE.md configuration | Same knowledge block |
| Token storage | `.claude/settings.local.json` (gitignored) or `~/.unimatrix/remote.json` |

Hook command comparison:
```
# Local:  LD_LIBRARY_PATH=/path/to/bin /path/to/unimatrix hook UserPromptSubmit
# Remote: node /path/to/unimatrix-client/bin/hook.js UserPromptSubmit
```

**Recommendation**: Extend existing `init.js` with `--remote <url> --token <token>`. Keep hook commands as `"type": "command"` for response transformation client-side. Add PreCompact and PostToolUseFailure to hook event list.

---

## Q4: How does this interact with existing infrastructure?

**Answer**: The thin client subsumes vnc-024's client-side scope entirely. Local `unimatrix hook` binary unchanged. `/observe` endpoint is the transport target with no server changes. ASS-014 Phase 3 WASM design replaced by TS thin client.

### Current `unimatrix hook` binary (hook.rs, 4,832 lines)

What the thin client replicates (subset):
- stdin parsing, event normalization, provider detection (straightforward TS port)
- Request construction (simplified -- no project hash, no UDS socket path)
- Response transformation (the critical value-add)

What the thin client does NOT need:
- Project root detection or hash computation (server handles)
- Event queue (HTTP retries or drop, no local filesystem queue)

### vnc-024 interaction

The thin client IS vnc-024's Option C (thin wrapper), implemented in TypeScript. With the thin client, vnc-024's remaining scope shrinks to:
- Wire contract documentation (keep)
- Timeout validation (keep)
- Everything else (curl config, installation, HTTP hook type investigation) -- subsumed

### ASS-014 interaction

Phase 1-2 designs remain valid. Phase 3 WASM specifics replaced by TS thin client (WASI P2 didn't materialize).

### /observe endpoint (vnc-022) interaction

No changes needed. Thin client POSTs `HookRequest` JSON, receives `HookResponse` JSON, transforms client-side.

**Recommendation**: Ship thin client as vnc-024 resolution. Reduce vnc-024 to wire contract docs only (~1 week).

---

## Q5: npm packaging patterns and constraints

**Answer**: The existing package follows esbuild/biome optionalDependencies pattern. Thin client should be separate, zero-native-dependency package.

### Industry survey

| Tool | Pattern | Binary Size | Notes |
|---|---|---|---|
| esbuild | optionalDependencies | ~9 MB each | Original pioneer, 28+ platform packages |
| Biome | optionalDependencies | ~12 MB each | Same pattern |
| Turbo | optionalDependencies | ~15 MB each | Go binary |
| Codex CLI | optionalDeps -> dist-tags | ~206 MB pre-split | Hit registry limits |
| Prisma | WASM migration | 14->1.6 MB | Moved FROM native TO TS+WASM |

### Recommended package structure

```
@dug-21/unimatrix              # Full server (existing)
  optionalDependencies:
    @dug-21/unimatrix-linux-x64
    @dug-21/unimatrix-linux-arm64
    @dug-21/unimatrix-darwin-arm64  (NEW)
    @dug-21/unimatrix-darwin-x64   (NEW)

@dug-21/unimatrix-client       # Thin client (NEW, <100 KB)
  bin/unimatrix-client.js       # CLI entry point
  lib/hook.js                   # Hook handler (stdin -> HTTP -> stdout)
  lib/transform.js              # Response transformation
  lib/transport.js              # HTTP transport to /observe
  lib/normalize.js              # Event name normalization
  lib/init-remote.js            # Remote init logic
  (NO optionalDependencies -- zero native code)
```

### Version coupling

Wire protocol Ping/Pong, not npm version locks. Thin client sends Ping on init, server responds with Pong + `server_version`. Major mismatch: error. Minor: warn.

**Recommendation**: Ship `@dug-21/unimatrix-client` as separate package. Version coupling via wire protocol, not npm locks.

---

## Q6: W2 roadmap placement

**Answer**: Thin client is prerequisite for personal-cloud parity. Total W2 impact: 4-5 weeks.

### Dependency chain

```
vnc-022 (shipped) -> /observe endpoint exists
        |
vnc-024 (reduced) -> wire contract docs (1 week)
        |
THIN CLIENT -> @dug-21/unimatrix-client (2-3 weeks)
        |
Remote init -> --remote flag in init.js (1 week)
        |
Session hosting -> unimatrix run (ASS-066, 2-3 weeks)
```

### Effort breakdown

| Component | Effort | Notes |
|---|---|---|
| Response transformation (`transform.js`) | 3-4 days | Port from hook.rs, ~200 lines TS |
| HTTP transport (`transport.js`) | 2-3 days | Node.js built-in `http`/`https` |
| Hook CLI handler (`hook.js`) | 3-4 days | Simplified port of `run()` + `build_request()` |
| Event name normalization | 1 day | 12 static mappings |
| Remote init (`init-remote.js`) | 3-4 days | Ping handshake, hook config, token storage |
| npm packaging + CI | 2-3 days | Package.json, bin, release workflow |
| Testing | 3-4 days | Unit + integration |
| **Total** | **~2.5-3 weeks** | |

### Gates before npm publish

1. Response transformation parity: byte-identical output vs Rust for same inputs
2. Round-trip validation: all 13 event types, thin client matches local binary output
3. Size gate: <500 KB (`npm pack --dry-run`)
4. Cross-platform: Node.js 18, 20, 22
5. Security: no eval, no dynamic require, no unexpected network calls

**Recommendation**: Sequence as vnc-024-reduced (1w) -> thin client (2-3w) -> remote init (1w). Ship before session hosting.

---

## Unanswered Questions

1. **Claude Code HTTP hook handler behavior**: Does `"type": "http"` pass raw response body to stdout, or extract/transform? Thin client sidesteps this with `"type": "command"`, but if HTTP hooks do passthrough, server-side content negotiation becomes a future optimization.

2. **Codex/Gemini CLI hook timeout budgets**: Whether `node /path/to/hook.js <EVENT>` works within their timeout budgets needs validation. Should be fast (<5ms startup) but needs measurement.

3. **Token storage security**: `.claude/settings.local.json` (gitignored, per-project), `~/.unimatrix/remote.json` (per-user), or `UNIMATRIX_TOKEN` env var? Different security/convenience tradeoffs.

4. **Transcript excerpt for PreCompact over HTTP**: Should thin client read transcript file and populate `transcript_excerpt` on CompactPayload, or let server generate briefing without it?

---

## Out-of-Scope Discoveries

1. **PreCompact and PostToolUseFailure missing from init**: `merge-settings.js` installs hooks for 7 events but omits PreCompact and PostToolUseFailure. These are active hook events handled by the Rust binary. Bug in existing init -- fix regardless of thin client.

2. **`npx` cold start concern**: `npx` resolves packages before executing, adding 200-500ms. Hook commands should use absolute path to `node` + script, not `npx`.

3. **Codex CLI split-package lessons**: Codex CLI initially split into per-platform optionalDependencies then consolidated due to global install issues on Windows. Unimatrix's devDependency model avoids this.

4. **JCO transpilation as future WASM bridge**: `@bytecodealliance/jco` can transpile WASM components to ESM. Could enable Rust-compiled thin client as JS in the future. Not needed now.

---

## Recommendations Summary

| Question | Recommendation |
|---|---|
| Q1: npm install content | Existing structure correct. Add macOS platforms. |
| Q2: Thin client architecture | **Pure TypeScript** -- no WASM, no native binary. <50 KB. |
| Q3: Init flow | Extend `init.js` with `--remote` flag. Both tiers share core flow. |
| Q4: Infrastructure interaction | Thin client subsumes vnc-024 client scope. Local binary unchanged. |
| Q5: npm packaging | Separate `@dug-21/unimatrix-client` package. Zero native deps. |
| Q6: W2 roadmap | vnc-024-reduced (1w) -> thin client (2-3w) -> remote init (1w). ~4-5w total. |
