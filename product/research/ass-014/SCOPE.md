# ASS-014: Cortical Implant — End-to-End Architecture

**Type:** Research Spike (Design Investigation)
**Date:** 2026-03-01
**Status:** Not Started
**Predecessors:**
- `product/research/ass-011/` — Hook-driven orchestration spike (workflow integration design, reactive protocol delivery)
- `product/research/ass-013/` — Tool call observation analysis (telemetry patterns, signal quality)
- Unimatrix entries #190, #191 — claude-flow competitive analysis (delivery patterns, router architecture)

**Purpose:** Define the end-to-end architecture for the "cortical implant" — a single native binary that acts as the universal router for all Claude Code lifecycle hooks, backed by the Unimatrix engine. This spike must produce a unified architectural view that col-006 through col-011 can be scoped against, including data model evolution, transport abstraction, existing feature impact, and distribution strategy. Without this architecture, the delivery features risk being disconnected capabilities rather than a coherent system.

---

## Motivation

Unimatrix has a sophisticated knowledge engine (redb storage, HNSW vectors, confidence evolution, contradiction detection, correction chains) but only one access path — agents must explicitly call MCP tools. Research (ass-011, claude-flow analysis) revealed that most agents don't call `context_briefing`. The knowledge exists but doesn't reach agents.

claude-flow (Ruflo v3.5) solved delivery via Claude Code lifecycle hooks — a single router binary dispatches all hook events, injecting context on every prompt and surviving compaction. But its backend is theater — no real persistence, no learning, no confidence.

The strategic move: build claude-flow's delivery pattern with Unimatrix's real engine. The cortical implant is the bridge — a native binary that lives on every developer machine, handles every hook event, and connects to Unimatrix for reads and writes.

This is not just a hook handler. It is:
- The **universal observer** — sees every tool call, prompt, compaction, session boundary
- The **automatic delivery channel** — injects knowledge without agent cooperation
- The **confidence feedback loop** — infers helpfulness from outcomes, not explicit signals
- The **distribution vehicle** — the component that ships everywhere, the client to both local and future centralized Unimatrix
- The **compaction defense** — preserves critical context when Claude Code compresses conversation history, re-injects knowledge that would otherwise be lost
- The **coordination backbone** — session tracking, agent routing, feature context

Getting the architecture right matters because:
1. The data model must support both MCP (explicit) and hook (automatic) access patterns
2. Existing features (col-002, crt-001, crt-002) may simplify or change shape
3. The transport abstraction determines whether local→centralized is a config change or a rewrite
4. The distribution mechanism determines adoption friction

---

## Research Questions

### RQ-1: Unified Data Model

What data model supports both MCP tools (agent-initiated, explicit) and cortical implant (system-initiated, automatic) without degrading knowledge quality?

- **RQ-1a:** What new data concepts does the implant introduce? (Sessions, injections, routing decisions, structured events.) Which are durable knowledge vs. ephemeral telemetry?
- **RQ-1b:** How do sessions, injections, and events relate to existing EntryRecord schema? Should they be entries with special categories, separate tables, or a hybrid (summaries as entries, raw events as ephemeral)?
- **RQ-1c:** What schema evolution is required from the current 13 tables? New tables, new fields on existing tables, or new indexes? What is the migration path?
- **RQ-1d:** How does mixing ephemeral event data with durable knowledge affect search quality, confidence scoring, and embedding space? What isolation boundaries are needed?
- **RQ-1e:** What is the session data lifecycle? When does a raw session become a summary entry? What gets garbage collected?
- **RQ-1f:** What session state must persist for compaction defense? The implant needs to know: which entries were injected during this session, what the agent's active role/task/feature context is, and what decisions are in play — so it can reconstruct critical context after compaction. Is this stored in Unimatrix (durable), in the implant's process memory (ephemeral daemon), or in a lightweight sidecar file (session-scoped)? What's the data model for "injected context history" that enables re-injection?

**Validation approach:** Propose 2-3 candidate data models. Evaluate each against: write volume projections, search quality impact, migration complexity, and alignment with "it starts with the data model" principle. Produce a recommended schema with migration plan.

### RQ-2: Two-Door Access Pattern

How should the cortical implant and MCP server share the Unimatrix engine without conflicting?

- **RQ-2a:** What operations does each access path need? (MCP: 12 existing tools. Implant: prompt-scoped search, session-aware briefing, category-filtered match, injection recording, event recording, session lifecycle.)
- **RQ-2b:** Where is the shared boundary? Can both paths use the same Rust traits (EntryStore, VectorStore, IndexStore) from `unimatrix-core`? What needs to be factored out of `unimatrix-server` into shared libraries?
- **RQ-2c:** How do concurrent reads/writes work? redb supports concurrent readers but single writer. If both MCP server and implant open the database, who writes? Options: (1) implant is read-only, queues writes for MCP server, (2) implant opens its own redb write transaction (redb handles locking), (3) implant writes to a staging area that the server merges.
- **RQ-2d:** Does the implant link `unimatrix-core`/`unimatrix-store`/`unimatrix-embed` directly, or does it communicate via IPC to the running server? Trade-offs: direct linking = fast but two processes with database access; IPC = clean separation but added latency and server dependency.
- **RQ-2e:** What happens when the MCP server isn't running? (Hook fires but no server.) Should the implant degrade gracefully — skip injection, queue events for later — or does it need standalone capability?
- **RQ-2f:** How does the PreCompact hook work as an access pattern? This is the most latency-critical hook — it must synchronously return content to inject into the compacted window before Claude Code proceeds. What does the implant query? Options: (1) call context_briefing with session's role/task context, (2) re-inject the N highest-confidence entries from this session's injection history, (3) reconstruct a "session state snapshot" (active feature, recent decisions, in-progress work). What's the token budget (<2000 tokens per PRODUCT-VISION.md)? How does prioritization work — active decisions > conventions > recent injections? Does the implant need a pre-computed "compaction payload" updated on every prompt cycle, or can it compute on-demand within the latency budget?

**Validation approach:** Prototype the two leading transport options (direct redb access vs. Unix domain socket to server). Measure latency against <50ms target. Evaluate write contention under realistic hook frequency. Produce a recommended access pattern with failure mode analysis.

### RQ-3: Existing Feature Impact Assessment

What changes in already-completed features when the cortical implant becomes the universal observer?

- **RQ-3a: col-002 (Retrospective Pipeline)** — The JSONL telemetry hooks become part of the cortical implant. Does the `unimatrix-observe` crate's JSONL parser remain the primary analysis path, or does structured data from the implant replace it? What is the migration: deprecate JSONL, keep as compatibility, or dual-write?
- **RQ-3b: col-002b (Detection & Baselines)** — Baseline comparison currently uses MetricVectors computed from JSONL. If session data comes from Unimatrix directly, does the MetricVector computation simplify? Do the 21 detection rules need different input formats?
- **RQ-3c: crt-001 (Usage Tracking)** — USAGE_LOG records MCP tool retrievals. Hook-injected knowledge (col-007) is "usage" but doesn't go through MCP. How does USAGE_LOG accommodate hook injections? New `source` field? Separate table? Or does the implant maintain its own injection log that feeds the same confidence pipeline?
- **RQ-3d: crt-002 (Confidence Evolution)** — Helpful/unhelpful signals currently come from explicit MCP tool parameters. col-009 provides implicit signals from session outcomes. Should implicit and explicit signals have different weights in the confidence formula? How does bulk session-end signaling interact with per-retrieval signaling?
- **RQ-3e: crt-004 (Co-Access Boosting)** — Currently tracks entries retrieved together via MCP tools. Hook injections create a new co-access pattern — entries injected into the same prompt are "co-accessed." Should this feed the same CO_ACCESS table?
- **RQ-3f: col-001 (Outcome Tracking)** — Outcome entries are currently stored by agents via MCP. col-010 session lifecycle could auto-generate outcome entries from session end signals. Does this complement or replace agent-stored outcomes?
- **RQ-3g: vnc-003 (context_briefing)** — The existing briefing tool is the closest analog to compaction defense — it already produces role+task-scoped knowledge bundles. Does PreCompact hook simply call briefing internally, or does it need a different query pattern? Briefing returns an unordered knowledge bag; compaction defense may need a prioritized, token-budgeted payload with session-specific state (injected entries, active work context) that briefing doesn't currently include. What evolution of the briefing interface (or a new hook-specific query interface) is needed?

**Validation approach:** For each affected feature, produce a delta document: what changes, what stays, what simplifies. Identify any rework that blocks col-006 implementation vs. rework that can happen incrementally after delivery features ship.

### RQ-4: Transport Abstraction & Security

How should the cortical implant communicate with Unimatrix, and how does the design support the transition from local to centralized deployment?

- **RQ-4a:** What transport trait/interface abstracts over local (redb/socket) and remote (HTTPS/gRPC)? What operations must it support? (Sync query with response, fire-and-forget event, batch operations.)
- **RQ-4b:** How does the implant authenticate to Unimatrix? Local: file permissions, process lineage verification (extending vnc-004's `/proc/{pid}/cmdline` pattern). Remote: API keys, OAuth 2.1 tokens, mTLS? Design the auth context interface that supports both.
- **RQ-4c:** How does the implant know WHICH Unimatrix instance to connect to? Local: project hash discovery (same as MCP server's `~/.unimatrix/{project_hash}/`). Remote: endpoint URL from config. What's the discovery/config mechanism?
- **RQ-4d:** What is the threat model for the implant specifically? It runs as a hook process (ephemeral, inherits Claude Code's environment). Can a malicious hook config hijack it? Can a rogue implant poison the knowledge base? How does this interact with the existing AGENT_REGISTRY trust hierarchy?
- **RQ-4e:** How does the transport handle the MCP server not being available? (Local: server process died. Remote: network partition.) Graceful degradation strategy — what works offline, what queues, what fails silently.

**Validation approach:** Define the transport trait in Rust pseudocode. Implement local variant. Sketch remote variant interface. Produce threat model document for implant-specific attack surface. Evaluate against existing Security Cross-Cutting Concerns in PRODUCT-VISION.md.

### RQ-5: Distribution & Deployment

How does the cortical implant get onto every machine that needs it, across platforms, with minimal friction?

- **RQ-5a:** What is the binary format? Single native binary (like the MCP server) or wrapper? What are the platform targets? (Linux x86_64, Linux aarch64, macOS x86_64, macOS aarch64, Windows x86_64 — same as Claude Code's supported platforms.)
- **RQ-5b:** What distribution mechanism? Options: (1) `npm install -g @unimatrix/cortical` with platform-specific binaries embedded (esbuild/turbo/biome model), (2) `cargo install unimatrix-hook`, (3) GitHub Releases with platform binaries + install script, (4) Homebrew/apt/winget, (5) bundled with MCP server binary as a subcommand (`unimatrix hook ...`).
- **RQ-5c:** How does the implant get configured in `.claude/settings.json`? Manual setup? `unimatrix init` command (alc-003) that writes hook configuration? Self-configuring on first MCP server startup?
- **RQ-5d:** How does versioning work? The implant and MCP server share Rust libraries — must they be the same version? What happens on version mismatch? (Implant v0.3 talking to server v0.4.)
- **RQ-5e:** What is the update story? npm handles updates naturally. Cargo requires explicit `cargo install --force`. How do teams ensure consistent implant versions across developers?
- **RQ-5f:** What is the dev container / Codespaces story? The current Unimatrix dev environment builds from source. For external adoption, the implant must be pre-built or trivially installable in container environments.

**Validation approach:** Evaluate distribution options against: installation friction (steps to working), update friction (steps to current version), platform coverage, binary size, and team consistency. Prototype the npm-with-native-binary approach (most promising for ubiquity). Produce a recommended distribution strategy with packaging plan.

---

## Deliverables

| ID | Deliverable | Format | Answers |
|----|-------------|--------|---------|
| D14-1 | Unified Data Model Proposal | `findings/data-model.md` | RQ-1 |
| D14-2 | Access Pattern Architecture | `findings/access-pattern.md` | RQ-2 |
| D14-3 | Existing Feature Impact Assessment | `findings/impact-assessment.md` | RQ-3 |
| D14-4 | Transport & Security Design | `findings/transport-security.md` | RQ-4 |
| D14-5 | Distribution Strategy | `findings/distribution.md` | RQ-5 |
| D14-6 | Architecture Synthesis | `findings/synthesis.md` | All RQs — unified architecture document that col-006–011 scope against |
| D14-7 | col-006–011 Scoping Recommendations | `findings/feature-scoping.md` | Per-feature impact of architecture decisions, revised descriptions, dependency updates |

---

## Success Criteria

| Criterion | Measurement |
|-----------|-------------|
| Data model covers both access paths | Sessions, injections, events modeled. No knowledge quality degradation from ephemeral data. Clear durable/ephemeral boundary. Compaction defense state (injection history, session context) modeled with clear lifecycle. |
| Transport abstraction supports local and remote | Trait defined in Rust pseudocode. Local variant prototyped. Remote variant interface sketched. <50ms local latency confirmed. |
| Existing feature impact quantified | Every completed col/crt feature assessed. Rework items classified as blocking vs. incremental. No silent breakage. |
| Security model extends to implant | Threat model documented. Auth context interface defined. Graceful degradation on server unavailability. |
| Distribution mechanism identified | One recommended approach with packaging prototype or proof of concept. Installation steps documented. Platform matrix confirmed. |
| col-006–011 can be scoped against architecture | Each delivery feature has revised description reflecting architectural decisions. Dependencies and interfaces explicit. |

---

## Constraints

- **No production code changes.** This is research and architecture — outputs are documents and pseudocode, not implementation.
- **No new phases or milestones.** Architecture informs col-006–011 scoping within existing M5.
- **Preserve genericity.** Data model and transport must remain domain-agnostic (per ASS-009). Hook-driven delivery is a capability, not a domain-specific feature.
- **Backward compatibility.** Existing MCP tools, existing data, existing agent workflows must not break. The implant is additive — agents that don't have it installed still work via explicit MCP calls.
- **Scope boundary:** This spike covers architecture and impact assessment. It does NOT produce implementation briefs, pseudocode, or test plans for col-006–011. Those follow in the normal feature workflow after this spike's findings are accepted.

---

## Relationship to Roadmap

```
ASS-014 (this spike)
   │
   │  architecture decisions feed into:
   │
   ├──► col-006: Hook Transport Layer (Cortical Implant)
   │     │  ← transport abstraction, binary architecture, distribution
   │     │
   │     ├──► col-007: Context Injection
   │     │     ← data model (injection tracking), interface (prompt-scoped search)
   │     ├──► col-008: Compaction Resilience
   │     │     ← interface (session-aware briefing), data model (session state)
   │     ├──► col-009: Closed-Loop Confidence
   │     │     ← data model (implicit signals), impact on crt-002
   │     ├──► col-010: Session Lifecycle
   │     │     ← data model (sessions), impact on col-002
   │     └──► col-011: Semantic Agent Routing
   │           ← interface (category-filtered search), data model (routing decisions)
   │
   ├──► col-002/crt-001/crt-002 rework assessment
   │     ← impact assessment identifies what changes, when
   │
   └──► PRODUCT-VISION.md updates
         ← revised col-006–011 descriptions, data model notes
```

---

## Open Questions

1. **redb concurrent access model** — redb v3.1.x supports multiple concurrent readers and a single writer per database file. Can the implant and MCP server coexist as reader+writer, or do we need a write coordinator? What does redb's locking behavior look like under hook-frequency writes?

2. **Hook process lifetime** — Claude Code hooks are ephemeral shell processes. Does the implant start fresh on every hook event (cold start penalty) or can it maintain a long-running daemon? If daemon, how does it lifecycle with Claude Code sessions? If cold start, can we meet <50ms with redb open + HNSW load?

3. **Embedding at hook time** — col-007 context injection needs semantic search, which needs to embed the prompt. ONNX runtime initialization takes ~200ms. Can the implant amortize this across hook calls? This may force the daemon architecture or require a pre-warmed embedding service.

4. **JSONL telemetry transition** — col-002's observation hooks already deploy. When the implant absorbs this role, is there a migration period where both paths coexist? Or does col-006 ship as a replacement, with col-002's JSONL path deprecated?

5. **npm binary packaging precedent** — esbuild, turbo, biome all ship native binaries via npm. What's the current best practice for Rust binaries in npm packages? Is there an established crate or toolchain for this?

6. **Claude Code hook event schema stability** — Hooks receive data via environment variables and stdin. Is this interface stable across Claude Code versions? What's the contract? Does Anthropic document hook event schemas?

7. **Session identity** — How does the implant know which session it's in? Claude Code doesn't expose a session ID to hooks (or does it?). If not, how do we correlate hook events across a session? Process tree inspection? PID of parent Claude Code process?

8. **Compaction defense depth** — claude-flow archives full conversation transcripts to SQLite for post-compaction restoration. Unimatrix's approach would be knowledge-level rather than transcript-level — re-injecting relevant entries rather than raw conversation history. But is knowledge-level sufficient? When an agent is mid-task with complex state (partial implementation, debugging context, file change history), can entry-level re-injection recover that working context? Or does the implant need to track and re-inject "session working state" — a richer concept than knowledge entries alone? This determines whether compaction defense is a briefing variant or a fundamentally new capability.

9. **Compaction frequency as signal** — Compaction events are themselves telemetry. Frequent compaction in a session may signal: long-running complex task, agent struggling with scope, or knowledge injection volume too high (ironic — injecting too much context causes the very compaction it tries to survive). Should the implant adapt injection volume based on compaction frequency? Does this feed the retrospective pipeline (col-002)?

## Tracking

- GitHub Issue: TBD (create on spike start)
- Unimatrix entries: #190 (claude-flow analysis), #191 (delivery gap analysis)
