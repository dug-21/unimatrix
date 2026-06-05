# ASS-067: Packaging, Installation, and Init — Two-Tier Developer Experience

## Question

What is the complete packaging, distribution, and initialization story for Unimatrix across both deployment tiers (local/stdio and personal-cloud/thin-client) — and what does this mean for the W2 roadmap?

## Why It Matters

Unimatrix has zero adoption today. The path from "never heard of it" to "working in my repo" must be frictionless. Two deployment tiers exist with different packaging needs:

1. **Local mode** (`npm install unimatrix`): Full server runs locally via stdio MCP. Everything in one package — server binary, hooks, skills. Zero external dependencies beyond Claude Code.

2. **Personal cloud mode** (`docker run unimatrix` + `npm install unimatrix-tc`): Server runs remotely in Docker. Each repo installs a thin client that handles hook installation, MCP registration, and HTTP transport to the remote server.

Both tiers need an `init` command that performs appropriate setup: hook installation, MCP server registration in settings.json, skill loading, CLAUDE.md configuration. The developer experience must be seamless in both cases — same `init`, different wiring underneath.

The thin client became critical path when vnc-024 research discovered that raw curl/HTTP against `/observe` cannot replicate what the local hook binary does — the hook binary performs client-side response transformation (JSON→text, `hookSpecificOutput` envelope formatting, transcript block prepending) that synchronous hooks (UserPromptSubmit, PreCompact, SubagentStart) depend on. No client = feature loss on remote. The thin client's sole purpose: **ensure HTTP Unimatrix has full parity with STDIO Unimatrix — now and in the future.**

This is also the technology that preserves the free remote observation path (no Agent SDK credits required), which ass-066 identified as essential to the OSS/personal-cloud value proposition.

## Bounded Questions

### Q1: What does `npm install unimatrix` actually install?

- Full Rust binary? Platform-specific optionalDependencies (like esbuild, turbo)?
- WASM build of the full server? (feasible? size implications?)
- Native binary via npm postinstall download (like Prisma)?
- What is the package size budget? What's acceptable for a devDependency?
- How does this relate to the existing `cargo install unimatrix` path?
- Do we ship one package or split (e.g., `@unimatrix/server` + `@unimatrix/cli`)?

### Q2: What is the best-suited architecture for the thin client?

The thin client exists for one reason: ensure HTTP/remote Unimatrix does not lose any functionality the STDIO/local version has. Given that:

- JS/TypeScript runtime is already installed on every target (Claude Code, Codex, Gemini are all JS/TS)
- The client must perform response transformation (JSON→text, hookSpecificOutput formatting, transcript prepending)
- The client must handle synchronous hook responses within Claude Code's latency budget
- The client must be distributable via npm (existing release channel)

Evaluate architectures with open debate:
- **Pure TypeScript**: simplest distribution, no compilation, full ecosystem access. But: performance for any future computation needs?
- **Rust compiled to WASM** (ASS-014 approach): single artifact, platform-agnostic, fast. But: WASI maturity in Node.js? Debugging story?
- **Rust native binary** (platform-specific optionalDependencies): fastest execution. But: cross-compilation matrix, heavier distribution.
- **Hybrid** (TS shell + WASM/native core for hot paths): best of both?

For each: size budget, capabilities, latency profile, maintenance burden, distribution complexity.

### Q3: What does `init` do in each tier?

For local mode (`unimatrix init`):
- Register MCP server in Claude Code settings.json (stdio transport)
- Install hooks (which hook events? all or subset?)
- Load skills (which skills?)
- Create/configure CLAUDE.md knowledge block
- Any per-project configuration?

For remote mode (`unimatrix-tc init`):
- Register thin client as MCP server in settings.json (stdio transport, proxies to HTTP)
- OR: register hooks pointing to remote `/observe` endpoint directly (no MCP proxy)?
- Install hooks configured for HTTP transport
- Auth handshake with remote server (register project, get project-hash)
- Load skills (same skills? subset? fetched from server?)
- CLAUDE.md configuration (same or different?)

Are these the same command with different flags, or distinct CLIs?

### Q4: How does this interact with existing infrastructure?

- Current `unimatrix hook` binary — does it become part of the npm package? Or replaced by the thin client?
- Current `unimatrix-server` MCP registration — manual today. How does `init` automate it?
- vnc-024 (remote observation config) — does the thin client subsume this entirely?
- ASS-014 Phase 3 WASM architecture — does this spike validate, refine, or supersede it?
- The `/observe` endpoint (vnc-022) — is it the thin client's transport or do we need something else?

### Q5: What are the npm packaging patterns and constraints?

- Platform-specific binary distribution: how do esbuild, turbo, Prisma, Biome do it? What's current best practice?
- WASM distribution via npm: size limits, loading patterns, Node.js WASI status in 2026
- Monorepo vs separate packages: `@unimatrix/server`, `@unimatrix/client`, `@unimatrix/cli`?
- Version coupling between server and thin client — how to handle?
- Does the server Docker image need to publish compatible thin client versions?

### Q6: What does this mean for the W2 roadmap?

- Is the thin client a prerequisite for other W2 features?
- What's the sequencing: thin client before or after vnc-024?
- Does this change the vnc-024 scope (thin client replaces curl-based hooks)?
- Estimated effort for each package (full npm, thin client, init commands)?
- What gates must pass before npm publish (security, size, cross-platform testing)?

## Approach

**Investigation + evaluation.** Internal architecture analysis (current hook binary, MCP registration, server packaging), external ecosystem research (npm binary distribution patterns, WASM in npm 2026), and architectural sketching of the two-tier init flow.

**Breadth: `code+ecosystem`.** Deep investigation of npm binary distribution patterns (esbuild, Prisma, Biome, turbo for precedent). Internal analysis of current hook/server/MCP architecture.

**Confidence required: `directional`.** Recommendation on packaging approach and roadmap placement. No PoC required — but should be specific enough to write a feature spec from.

**Constraints classification:**
- **Hard**: npm is the existing release channel — already in use, not a new decision
- **Hard**: Node.js/JS runtime is already installed on every target machine (Claude Code, Codex CLI, and Gemini CLI are all JS/TypeScript-based)
- **Hard**: Thin client must work without ONNX/redb/HNSW locally (intelligence is server-side)
- **Hard**: Single binary story for local mode preserved (no multi-process dance)
- **Hard**: The thin client's sole purpose is ensuring HTTP/remote Unimatrix has full parity with STDIO/local functionality — now and in the future. No feature loss on remote.
- **Hypothesis**: WASM is the right thin client format (challengeable — JS/TS native, Rust native binary, or hybrid are all viable. Open debate.)

**Dependencies:**
- ASS-014: WASM cortical implant architecture (foundational design)
- ASS-066: Session hosting research (tiering decision — free vs enterprise)
- vnc-022: `/observe` endpoint (thin client transport target)
- vnc-024: Remote observation config (may be subsumed by thin client)

## What the Output Should Be

- **Packaging recommendation**: What to ship, how to ship it, size budgets
- **Init flow design**: Step-by-step for both tiers, with specific files modified
- **Thin client architecture decision**: WASM vs native, capabilities needed, size target
- **Roadmap placement**: Where thin client fits in W2, what it blocks/unblocks
- **vnc-024 relationship**: Subsumed, complementary, or orthogonal

## Known Constraints

- Claude Code settings.json has a specific MCP server registration format that init must target
- Hook installation requires writing to `.claude/settings.json` or `.claude/settings.local.json`
- The existing `unimatrix hook` binary is ~5MB compiled Rust — shipping this in npm adds weight
- WASI support in Node.js (via `node:wasi`) is still experimental as of Node 22
- npm package size soft limit is 50MB; hard limit varies by registry

## Prior Art

- ASS-014: WASM cortical implant architecture (Phase 3 design)
- ASS-066: Session hosting + tiering (free observation vs enterprise session hosting)
- vnc-022: `/observe` endpoint (shipped)
- vnc-024: Remote observation client configuration (in progress)
- esbuild, Prisma, Biome, turbo: npm binary distribution precedents
