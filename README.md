# Unimatrix (Alpha release)

Unimatrix is a workflow-aware, self-learning knowledge engine built for agentic
software delivery. It captures the knowledge that emerges from doing work —
decisions, patterns, lessons, conventions — and makes it trustworthy, retrievable,
and continuously improving. As agents move through delivery cycles, Unimatrix learns
what matters at each phase and delivers the right knowledge dynamically, before
agents need to ask for it. Knowledge retention becomes a first-class citizen of the
delivery process, not a side effect.

Unimatrix is not an orchestration engine. It does not coordinate agents, schedule
work, or manage workflows. It is a knowledge engine that understands workflow context
— your current phase, what your team has been doing, what comes next — and uses that
understanding to surface relevant knowledge at exactly the right moment.

The key mental model: workflow definitions, agent definitions, and skill definitions
are static — they live in your tooling and change infrequently. Architecture
decisions, patterns, and lessons-learned are dynamic — they evolve with every
feature, every delivery, every failure. Unimatrix was designed to manage the dynamic
layer. Every architectural pivot, every hard-won lesson, every reusable pattern is
captured, attributed, when needed, corrected, and made available to every future agent that needs it.

Built for agentic software delivery. Configurable for any workflow-centric domain.

This workflow-phase-conditioned delivery means knowledge is surfaced at phase
transitions based on what the engine has learned about each phase — it is not
unconditional injection into every prompt.

---

## Getting Started

### Install via npm

> **Platform: Linux x64 and arm64 only.** macOS and Windows are not supported via npm.

**Prerequisites — both required before installing:**
- Node.js >= 18
- ONNX Runtime 1.20.x shared library installed on the system

**Install ONNX Runtime (Linux):**
```bash
# Download the release for your architecture (x64 or aarch64) from:
# https://github.com/microsoft/onnxruntime/releases
# Extract and install the shared library:
tar xzf onnxruntime-linux-*.tgz
sudo cp onnxruntime-linux-*/lib/libonnxruntime.so* /usr/lib/
sudo ldconfig
```

```bash
npm install @dug-21/unimatrix
```

The embedding model downloads automatically on first run (or via `npx unimatrix model-download`).

### Build from Source

Prerequisites:
- Rust 1.89+ (edition 2024)
- ONNX Runtime 1.20.x shared library

**macOS (Homebrew):**
```bash
brew install onnxruntime
```

**Linux (manual):**
```bash
# Download from https://github.com/microsoft/onnxruntime/releases
# Extract and set ORT_DYLIB_PATH or install to /usr/lib
```

**Devcontainer:** ONNX Runtime pre-installed. No setup needed.

Build:
```bash
cargo build --release --workspace
```

Binary at `target/release/unimatrix`.

### Wire into your project

Run this once from your project root:

```bash
npx unimatrix init
```

This configures everything automatically — MCP server, hooks, skills, and database. It is safe to re-run; existing configuration is preserved.

Then start a Claude Code session and run:

```
/unimatrix-init
```

That's it. Unimatrix is ready to use.

### First Use Examples

**Search for existing knowledge:**
```
context_search(query: "error handling patterns", category: "pattern")
```

**Store a new decision:**
```
context_store(
  content: "Use SQLite for local storage — zero cloud dependency, single-file database.",
  topic: "nxs-008",
  category: "decision",
  title: "Storage backend choice"
)
```

**Get a knowledge briefing before starting implementation:**
```
context_briefing(
  topic: "col-031",
  task: "Implement per-agent rate limiting middleware for the MCP request handler — need patterns for token bucket implementation, existing middleware conventions, and any prior decisions on request-level enforcement"
)
```

**Example of structured 'protocols' for delivery**
```
Read how to use context_cycle commands to gain maximum value in .claude/protocols/uni/README.md
```

---

## Why Unimatrix

Multi-agent development creates knowledge that lives in context windows and dies with sessions. Decisions get re-made, patterns get re-discovered, lessons get re-learned.

- **Auditable Knowledge Lifecycle** — Every entry has a SHA-256 content hash. Corrections create hash-chained supersession links. An append-only audit log records every operation with agent identity and session context. You can trace how any piece of knowledge evolved.

- **Invisible Delivery** — Agents do not need to ask for context. Hook-driven integration injects relevant expertise into every prompt automatically via Claude Code's lifecycle hooks. Knowledge reaches agents without needing to ask.

- **Self-Learning** — Confidence scoring evolves from real usage signals: accesses, helpfulness votes, correction quality, creator trust, and co-access patterns. Entries that help get boosted; entries that mislead get downranked. Adaptive embeddings (MicroLoRA) tune search to project-specific usage patterns.

---

## Core Capabilities

Unimatrix provides these capabilities out of the box.

### Self-Learning Knowledge Engine

Captures decisions, patterns, conventions, procedures, and lessons from real feature work. Seven knowledge categories ensure entries surface in the right context. Confidence scoring combines usage signals, correction quality, creator trust, freshness, helpfulness, and co-access patterns into a composite score that evolves automatically. No manual curation required — the system learns what is useful from how knowledge is accessed and rated.

### Graph-Enhanced Retrieval

HNSW vector similarity locates an initial candidate pool from the 384-dimension embedding space. Personalized PageRank (PPR) co-access traversal then expands the pool by walking the knowledge graph — surfacing cross-category entries that pure vector search misses, because they were frequently retrieved together with the initial candidates in past sessions. A confirmed +0.0122 MRR improvement comes from this expansion step alone. Phase-conditioned category affinity stratifies results by the current workflow phase: entries from categories that historically appear during the active phase receive a ranking boost, calibrated from the per-(phase, category) frequency table rebuilt each background tick. Co-access ranking promotes entries retrieved together in prior sessions. The three layers compose in sequence: semantic similarity → PPR graph expansion → phase-conditioned and co-access re-ranking. Filters by topic, category, tags, and status apply throughout. Near-duplicate detection (cosine similarity >= 0.92) prevents redundant entries at write time.

### Adaptive Embeddings (MicroLoRA)

All-MiniLM-L6-v2 ONNX model runs locally — no API calls, no cloud dependency. A MicroLoRA layer adapts frozen embeddings to project-specific usage patterns. Search relevance improves over time as the system learns which entries are accessed together. 384-dimension vectors with HNSW index for fast approximate nearest-neighbor search.

### Hook-Driven Invisible Delivery (Cortical Implant)

Automatic context injection on every prompt via the `UserPromptSubmit` hook. Six hook events drive the integration: `UserPromptSubmit`, `SubagentStart`, `PreCompact`, `PreToolUse`, `PostToolUse`, `Stop`. Subagent injection: when the SM spawns a subagent, the `SubagentStart` hook fires synchronously and injects relevant knowledge into the subagent context before its first token — this combined with a `context_briefing` call on the outset, provides agents with an index of the most relevant artifacts to their goal and task. `UserPromptSubmit` injection requires at least 5 words in the prompt; shorter inputs (e.g., "yes", "ok continue") are recorded but produce no injection. **No guidance is better than misdirection**. Compaction resilience: `PreCompact` preserves critical context before Claude Code's context window compaction; the compaction payload is a flat indexed table of active entries (up to k=20) plus a session histogram summary. Closed-loop feedback: the `Stop` hook records session outcomes for confidence evolution. Sub-50ms round-trip budget per hook event. Disk-backed event queue for graceful degradation. Single binary — the `hook` subcommand connects to the running MCP server via Unix domain socket IPC.  Hooks provide the telemetry necessary for Unimatrix to learn.

### Cycle Review Analysis

Analyzes session telemetry for a completed feature cycle and produces the `# Unimatrix Cycle Review —` report. 21 detection rules across 4 categories: agent behavior, friction points, session health, and scope indicators. Rules are domain-aware: each rule guards on `source_domain` as its first filter, so Claude Code rules never fire on events from other domains. A domain pack registry loaded at startup from TOML defines which event types, categories, and detection rules apply to each domain; the "claude-code" pack is always active with no config required. Historical baselines with outlier detection surface anomalies. Evidence synthesis produces actionable findings with supporting data. Lessons and patterns extracted from retrospectives are stored back in the knowledge base with de-duplication via correction chains.

The report header surfaces the feature goal, inferred cycle type (Design, Delivery, Bugfix, Refactor, or Unknown), attribution path used (cycle\_events-first, sessions.feature\_cycle legacy, or content-scan fallback), and an in-progress indicator when no `cycle_stop` event exists. A Phase Timeline table breaks the cycle into per-phase windows showing duration, pass count, agents spawned, records, knowledge throughput, and gate outcome. A "What Went Well" section surfaces non-outlier favorable baseline signals that were previously hidden. Per-finding evidence is rendered as relative-time burst notation (`Timeline: +0m(N) +12m(N▲) …`) rather than raw epoch values. The Knowledge Reuse section splits served entries into cross-feature (from prior cycles) and intra-cycle buckets with a top-entry breakdown. Recommendations appear immediately after the header, before all other sections.

### Behavioral Signal Delivery

Cycle outcomes recorded via `context_cycle` feed as graph edges, reinforcing co-access signals between entries retrieved during successful delivery phases. Each time a phase completes with a positive outcome, the knowledge retrieved during that phase gains stronger co-access links — future agents entering the same phase surface those entries higher. `context_briefing` operates as a targeted handoff at phase transitions: it uses the current phase and the cycle's history to prioritize knowledge relevant to the agent's declared phase, delivering a structured top-k result set without requiring the agent to search. This goal-conditioned briefing, combined with UDS injection, makes knowledge delivery phase-aware and progressive rather than flat. Reference: crt-046, Group 6.

### Contradiction Detection

After each `context_store`, a background scan checks the new entry against its top HNSW neighbors using cosine similarity. Pairs with similarity >= 0.65 are recorded as `Supports` edges in the knowledge graph. Contradiction density — the ratio of unresolved contradictions to active entries — is one dimension of the Lambda structural health metric, computed periodically and surfaced in `context_status` health reports. When contradictions are identified, `context_correct` is the resolution path: it deprecates the conflicting entry and links the replacement through a hash-chained supersession record. No external model is required for contradiction management.

### Domain-Agnostic Observation Pipeline

Every detection rule carries a `source_domain` guard — a rule fires only for events from its declared domain, never cross-contaminating signals from unrelated systems. Domain packs are registered via `[[observation.domain_packs]]` entries in `config.toml`, specifying the source domain, event types, and applicable knowledge categories. The built-in "claude-code" domain pack is always active and requires no configuration — it covers all Claude Code lifecycle hook events out of the box. Any domain's event stream connects to the learning layer by registering a domain pack; no code changes are required. `source_domain` is validated at both ingest and registration: values must match `^[a-z0-9_-]{1,64}$`. Reference: W1-5, col-023.

### Correction Chains with Audit Trails

`context_correct` creates a new entry and deprecates the original, linking them with SHA-256 content hashes (`previous_hash` chain). The append-only audit log records every operation — store, correct, deprecate, quarantine, enroll — with agent identity, session context, and operation outcome. Correction chains are tamper-evident: any break in the hash chain is detectable.

### Coherence Gate (Lambda Health Metric)

Lambda is a composite structural integrity metric [0.0, 1.0] computed from three dimensions: graph quality (weight 0.46 — is the vector index structurally sound?), contradiction density (weight 0.31 — how many unresolved contradictions exist?), and embedding consistency (weight 0.23 — do entries have valid, current embeddings?). When lambda drops below 0.8, maintenance is recommended. A background tick handles maintenance automatically — confidence refresh, graph compaction, co-access cleanup.

`context_status` also reports six graph cohesion metrics computed per-call from the `GRAPH_EDGES` table: connectivity rate (fraction of active entries with at least one non-bootstrap edge), isolated entry count, cross-category edge count, Supports edge count, mean entry degree (in+out). These metrics are informational — they do not feed into lambda — but let operators verify whether automated platform is driving cross-category graph that PPR can exploit. Summary format includes a single "Graph cohesion:" line; Markdown format includes a `### Graph Cohesion` sub-section within the Coherence block.

### Content Scanning

Every `context_store` and `context_correct` call scans content for injection patterns (~25+ patterns including prompt injection attempts, system prompt overrides, and encoded payloads) and PII patterns (6+ patterns including emails, phone numbers, API keys, and credentials). Flagged content is rejected with a descriptive error before storage.

### Agent Trust Hierarchy

Four-tier trust model: System > Privileged > Internal > Restricted. Four capabilities gate tool access: `read`, `write`, `search`, `admin`. Unknown agents auto-enroll as Restricted (read + search only) on first contact. Protected agents: `system` and `human` cannot be modified. Self-lockout prevention: an admin cannot remove their own Admin capability. `context_enroll` (Admin-only) manages agent trust levels and capabilities at runtime.  This is mostly unused in currently supported STDIO mode.  More to come

### Knowledge Effectiveness Analysis

Per-entry utility scoring from injection logs and session outcomes. Confidence calibration validation — does predicted quality match actual usefulness? Dead knowledge detection — entries that are never accessed after initial storage.

---

## Tips for Maximum Value

1. **Treat Knowledge Curation as 1st class requirement.**  Agents should be encouraged to search AND store important knowledge future agents should know about their decisions, activities, etc.  

2. **Start a new session per feature cycle.** Context window pollution across features reduces knowledge quality. Each feature cycle (e.g., `col-015`) should use a fresh Claude Code session.

3. **Use `context_cycle` to declare start/top and phase transitions for your workflow.** Eg: `Spec`, `Dev`, `Testing`. The system learns the content categories used in each cycle.  **See `.claude/protocols/uni/README.md` for more details

4. **Run Retrospectives** Use `context_cycle_review` to learn about what happened on this feature.  Unimatrix looks for 21 potential hotspots that serve to improve your workflows and stores this summary.  Its also a good opportunity to quality check knowledge stored during the feature_cycle.  Storing the summary also enables the proper release of the telemetry data to avoid unwieldly db growth, while retaining the summary.

5. **Category discipline matters.** The right category determines retrieval quality. Decisions (`decision`) are not conventions (`convention`); procedures (`procedure`) are not patterns (`pattern`). Miscategorized entries surface in wrong contexts during semantic search.

6. **Cold start: use `/uni-seed`.** A fresh knowledge base returns empty search results. `/uni-seed` populates foundational entries before relying on search.

---

## Configuration

Unimatrix loads configuration from two optional TOML files at server startup. When neither file is present, all compiled defaults apply and no existing behavior changes.

- `~/.unimatrix/config.toml` — global config, applies to every project on the machine.
- `~/.unimatrix/{project-hash}/config.toml` — per-project override; values here shadow the global file, which shadows compiled defaults. List fields (`categories`, `boosted_categories`, `adaptive_categories`, `session_capabilities`) replace the global list entirely — there is no append behavior.

Config is loaded once at startup. Changes require a server restart. A malformed file or a security validation failure aborts startup with a descriptive error.

### Profile Presets

The `[profile]` section selects a knowledge-lifecycle preset. Presets encode calibrated confidence weight vectors and freshness half-life values so operators identify their knowledge type rather than tuning ML weights directly.

```toml
[profile]
preset = "collaborative"   # default — matches current compiled behavior
```

| Preset | Best for | Freshness half-life |
|--------|----------|---------------------|
| `collaborative` | Team-built knowledge, dev/research (default) | 8760 h (1 year) |
| `authoritative` | Policy, standards, legal precedents — source trust dominant | 8760 h (1 year) |
| `operational` | Runbooks, incidents, procedures — freshness dominant | 720 h (1 month) |
| `empirical` | Sensor feeds, metrics, time-series — recency critical | 24 h |
| `custom` | Expert use — requires all six weights in `[confidence]` section | set explicitly |

### Key Config Sections

```toml
[knowledge]
# Replace the built-in 7-category list with domain-appropriate categories.
# Values: lowercase, [a-z0-9_-], max 64 chars, up to 64 categories total.
categories = ["lesson-learned", "decision", "convention",
              "pattern", "procedure"]
# Categories surfaced more prominently in search re-ranking (provenance score boost).
boosted_categories = ["lesson-learned"]
# Categories eligible for automated lifecycle management (retention, auto-deprecation).
# All other categories require explicit operator action to deprecate.
# Prerequisite for signal-driven retention (#409). Default: ["lesson-learned"].
adaptive_categories = ["lesson-learned"]
# Freshness decay rate. Overrides the preset's built-in value when set.
freshness_half_life_hours = 8760.0

[server]
# MCP server instructions passed to every connecting agent during the initialize handshake.
# Injection patterns are validated at load time; startup aborts if triggered.
instructions = "..."

[agents]
# Auto-enroll behavior for unknown agents.
# "permissive" grants [Read, Write, Search]; "strict" grants [Read, Search].
default_trust = "permissive"
session_capabilities = ["Read", "Write", "Search"]

[inference]
# Number of threads dedicated to ML inference (ONNX embedding, GNN).
# Default: (num_cpus / 2).max(4).min(8) — at least 4 threads, at most 8.
# Valid range: [1, 64]. Out-of-range value aborts startup with a structured error.
rayon_pool_size = 4

# Session context affinity weights (WA-2).
# w_phase_histogram: boost weight for implicit category histogram signal. Applied inside
# compute_fused_score. Max boost = w_phase_histogram * 1.0 = 0.02 per entry (default: 0.02).
w_phase_histogram = 0.02
# w_phase_explicit: boost weight for explicit phase signal (WA-1 current_phase). Activates
# the PhaseFreqTable — a per-(phase, category) frequency table rebuilt each background tick
# from query_log. Entries accessed frequently in the current phase receive a higher
# phase_explicit_norm contribution. Cold-start guard: when no phase history exists,
# phase_explicit_norm = 0.0 and scores are bit-for-bit identical to pre-col-031.
# Default 0.05 (additive, outside the six-weight sum constraint).
w_phase_explicit = 0.05
# query_log_lookback_days: time window (in days) for the PhaseFreqTable rebuild SQL query.
# Only query_log rows within this window contribute to phase-frequency rankings.
# Default 30. Increasing this window widens the historical signal; decreasing it makes
# rankings more sensitive to recent access patterns.
query_log_lookback_days = 30

[retention]
# Number of completed (reviewed) feature cycles to retain activity data for.
# Observations, query_log, sessions, and injection_log for cycles beyond this
# window are deleted after their cycle_review_index row exists.
# Governs the ceiling for PhaseFreqTable lookback and future GNN training window.
# Range: [1, 10000]. Default: 50.
activity_detail_retention_cycles = 50

# Maximum number of purgeable cycles to process in a single maintenance tick.
# Limits tick budget consumed by GC. Older cycles are processed first.
# Deferred cycles are picked up on the next tick.
# Range: [1, 1000]. Default: 10.
max_cycles_per_tick = 10

# Retention window in days for audit_log rows.
# Audit data is an accountability record, not a learning signal.
# Range: [1, 3650]. Default: 180.
audit_log_retention_days = 180
```

```toml
# [observation] — domain pack registry (optional; omit for Claude Code-only deployments)
# The "claude-code" pack is always loaded as the built-in default.
# Domain pack changes require a server restart — runtime re-registration is not supported.
[[observation.domain_packs]]
source_domain = "sre"                    # Must match ^[a-z0-9_-]{1,64}$
event_types   = ["incident_opened", "incident_resolved", "alert_fired"]
categories    = ["runbook", "post-mortem"]
# Built-in domain packs (claude-code) register detection rules as Rust code.
# External packs declare threshold or temporal-window rules as TOML descriptors.
```

Config files are validated for security at load time: world-writable files abort startup; group-writable files log a warning. `[server] instructions` is scanned for injection patterns before use.

---

## MCP Tool Reference

Unimatrix exposes 12 MCP tools. All tools accept `format: "summary" | "markdown" | "json"` as a common parameter.

| Tool | Purpose | When to Use |
|------|---------|-------------|
| `context_search` | Search for relevant context using natural language. Returns semantically similar entries ranked by relevance. | When you need to find patterns, conventions, or decisions related to a concept. Use when you do NOT know exactly what you are looking for. Key params: `query` (required), `category`, `topic`, `tags`, `k` (default 5), `helpful`. |
| `context_lookup` | Look up context entries by exact filters. Returns entries matching topic, category, tags, status, or ID. | When you KNOW what you are looking for — a specific feature's entries, a category listing, or a known ID. Key params: `topic`, `category`, `tags`, `id`, `status`, `limit` (default 10). |
| `context_get` | Get a specific context entry by its ID. | When you have an entry ID from a previous search or lookup result and need the full content. Key params: `id` (required), `helpful`. |
| `context_store` | Store a new context entry with duplicate detection and content scanning. Each successful non-duplicate store increments the per-session category histogram used by `context_search` for implicit session affinity ranking. | When you discover a pattern, convention, decision, or lesson worth preserving. Key params: `content` (required), `topic` (required), `category` (required), `tags`, `title`, `feature_cycle`. |
| `context_correct` | Correct an existing entry. Deprecates the original and creates a new entry with a hash-chain link. | When an entry contains wrong or outdated information that should be superseded (not just hidden). Key params: `original_id` (required), `content` (required), `reason`. |
| `context_deprecate` | Mark an entry as outdated. Entry remains accessible but excluded from default search/lookup. | When knowledge is no longer relevant but should not be deleted (historical record). Key params: `id` (required), `reason`. |
| `context_quarantine` | Quarantine or restore an entry. Quarantined entries are excluded from search and lookup. **Admin only.** | When an entry is suspicious, invalid, or harmful and should be isolated. Use `action: "restore"` to undo. Key params: `id` (required), `action` ("quarantine" or "restore"), `reason`. |
| `context_status` | Get knowledge base health metrics. Shows entry counts, distributions, correction chains, coherence score, security metrics, graph cohesion metrics (connectivity rate, isolated entry count, cross-category edge count, Supports edge count, mean entry degree, and inferred edge count), per-category lifecycle labels (adaptive vs pinned), `pending_cycle_reviews` (cycle IDs that have started within the retention window but have no stored cycle review yet — always computed), and a `curation_health` aggregate block (per-cycle correction rate mean/stddev, source breakdown as agent% and human%, orphan deprecation ratio mean/stddev, and trend direction when at least 6 cycles of snapshot data are available). **Admin only.** | When you need to assess knowledge base health or inspect whether graph edge inference is producing a connected, cross-category graph, identify cycles awaiting retrospective review before signals can be purged, or review curation behavior trends across recent cycles. The `maintain` parameter is accepted but silently ignored — a background tick handles maintenance automatically. Key params: `topic`, `category`, `check_embeddings`. |
| `context_briefing` | Get a knowledge index for a topic or task. Returns up to 20 active entries as a flat indexed table (columns: row, id, topic, category, confidence, snippet). Query derived from: (1) explicit `task` param. Used at the start of a phase or task to get oriented. Call when starting a new 'task' (often with a new subagent). Key params: `topic` (also known as feature_cycle), `task`, `k` (default 20), `max_tokens` (default 3000, range 500-10000). |
| `context_enroll` | Future use |
| `context_cycle` | Signal feature cycle lifecycle events: start, phase transitions, and stop.  | At cycle start/stop and at each phase boundary. Key params: `type` (required: `"start"` \| `"phase-end"` \| `"stop"`), `topic` (required. Topic and feature_cycle are interchangeable), `phase`, `outcome`, `next_phase`, `agent_id`, `goal` (optional, `start` only: 1–2 sentence plain-text statement of feature intent; used as the step-2 query signal by `context_briefing` and hook injection when no explicit `task` is provided; max 1 024 bytes). **Proper use of context_cycle provides unimatrix deep visibility into your workflow**|
| `context_cycle_review` | Analyze observation data for a work cycle (Retrospective). Parses session telemetry, detects hotspots, computes metrics, and renders and stores the `# Unimatrix Cycle Review —` report. Use `force=true` to recompute and overwrite the stored record. | After a work cycle completes, to better understand what worked and what didn't during the cycle. Key params: `feature_cycle` (required), `evidence_limit`, `force` (bool, default false — when true, forces recomputation even if a stored record exists), `format` ("markdown" default, "json"). |

**`context_search` vs `context_lookup`**: `context_search` uses semantic similarity (natural language). `context_lookup` uses exact filters (topic, category, tags, status). Use search when exploring; use lookup when you know what you want.

**`context_correct` vs `context_deprecate` vs `context_quarantine`**: `context_correct` supersedes with a new version (hash-chained). `context_deprecate` marks as outdated (no replacement). `context_quarantine` isolates from all results (Admin-only, reversible).

---

## Skills Reference

Unimatrix ships 10 Claude Code skills via the npm package. Skills are platform-native `/command` files installed automatically by `npx unimatrix init`.

Skills marked (MCP) require the server to be running and configured.

| Skill | Purpose | When to Use |
|-------|---------|-------------|
| `/uni-init` | Initialize Unimatrix in a repository — CLAUDE.md setup + agent orientation recommendations. | First-time setup of a repo. |
| `/uni-seed` | Populate foundational knowledge through human-directed, gated exploration. (MCP) | After installation, before relying on search. |
| `/uni-retro` | Post-merge retrospective — extracts patterns, procedures, and lessons from shipped features. (MCP) | After a feature PR is merged. |
| `/uni-knowledge-search` | Semantic search across Unimatrix knowledge. (MCP) | Exploring a topic, finding related decisions or patterns. |
| `/uni-knowledge-lookup` | Deterministic lookup by feature, category, or entry ID. (MCP) | When you know what you want. |
| `/uni-query-patterns` | Query component patterns and conventions before designing or implementing. (MCP) | Before writing pseudocode or code. |
| `/uni-store-adr` | Store an architectural decision record. (MCP) | After each design decision. |
| `/uni-store-lesson` | Store a lesson learned from a failure or gate rejection. (MCP) | After bugfixes and unexpected issues. |
| `/uni-store-pattern` | Store a reusable implementation pattern. (MCP) | When a gotcha or reusable solution emerges. |
| `/uni-store-procedure` | Store or update a technical how-to procedure. (MCP) | When a technique evolves or is discovered. |

---

## Knowledge Categories

Unimatrix uses 5 built-in knowledge categories. Category discipline matters for retrieval quality — miscategorized entries surface in wrong contexts during semantic search.

| Category | Description | Example |
|----------|-------------|---------|
| `lesson-learned` | Lessons from failures, gate rejections, unexpected issues. | "Always verify hook latency after adding new UDS handlers — we hit 200ms in col-008." |
| `decision` | Architectural and design decisions (ADRs). | "Use SQLite for local storage — single-file, zero cloud dependency, bundled via rusqlite." |
| `convention` | Project conventions and rules agents should follow. | "All MCP tool handlers follow the execution order: identity -> capability -> validation -> category -> scanning -> business logic -> format -> audit." |
| `pattern` | Reusable implementation patterns, gotchas, and solutions. | "Do not hold Store lock across async boundaries — use spawn_blocking for all Store calls." |
| `procedure` | Step-by-step technical procedures (how-to). | "How to add a new MCP tool: 1. Define params struct, 2. Implement handler, 3. Add validation, 4. Add audit event." |


The default category list can be replaced at startup via `[knowledge] categories` in `~/.unimatrix/config.toml`. The 5 built-in categories cover the primary use cases for software delivery; operators targeting other domains can supply a domain-appropriate list.

---

## CLI Reference

The `unimatrix` binary (or `npx unimatrix`) serves as both the MCP server and the hook handler.

### Default Mode (no subcommand)

Bridge mode. Connects to the running daemon's MCP socket and bridges stdin/stdout to it. If no daemon is running, auto-starts one (waits up to 5 seconds for the socket to appear) before bridging. This is what the MCP server configuration invokes — no change to `.mcp.json` is required.

### Subcommands

| Subcommand | Description | Key Flags |
|------------|-------------|-----------|
| `serve --daemon` | Start the MCP server as a detached background daemon. Daemonizes (fork/setsid), binds the MCP UDS socket (`unimatrix-mcp.sock`) and hook IPC socket, starts the background tick loop, and exits the launcher process. Fails non-zero if a healthy daemon is already running. Linux and macOS only. | `--daemon` |
| `serve --stdio` | Start the MCP server in foreground stdio mode. Identical in behavior to the pre-daemon default — the server runs until stdin closes, then performs graceful shutdown and exits. Use for development and testing. | `--stdio` |
| `stop` | Send SIGTERM to the running daemon and wait for it to exit (up to 10 seconds). Exits 0 on success, non-zero if no daemon is running or the PID file is absent/stale. | None |
| `hook <EVENT>` | Handle a Claude Code lifecycle hook event. Reads JSON from stdin, connects to the running server via UDS. Designed for use in `.claude/settings.json` hook configuration, not direct user invocation. | Event name as positional arg. |
| `export` | Export the knowledge base to JSONL format. No running server required. | `--output <PATH>` (defaults to stdout) |
| `import` | Import a knowledge base from a JSONL export file. Re-embeds entries and rebuilds vector index. | `--input <PATH>` (required), `--skip-hash-validation`, `--force` (drop existing data) |
| `version` | Print version and exit. With `--project-dir`, also initializes the database. | `--project-dir <PATH>` |
| `model-download` | Download the ONNX embedding model to cache. Used by npm postinstall; safe to run manually to pre-warm the model cache. | None |
| `snapshot` | Create a self-contained SQLite copy of the active database using `VACUUM INTO`. Includes all tables (entries, query_log, graph_edges, co_access, sessions, and all analytics tables). Refuses with a non-zero exit code if `--out` resolves to the same path as the live database. | `--out <PATH>` (required), `--project-dir <PATH>` |
| `eval scenarios` | Mine the `query_log` table from a snapshot and write eval scenarios in JSONL format. Each scenario includes query text, retrieval context, baseline result set (soft ground truth), and source path (`mcp` or `uds`). | `--db <PATH>` (required), `--out <PATH>` (required), `--retrieval-mode mcp\|uds\|all` (default `all`), `--limit <N>` |
| `eval run` | Replay eval scenarios through one or more configuration profile TOML files in-process, producing one JSON result file per scenario. Computes P@K, MRR, Kendall tau, rank change list, CC@k (Category Coverage at k), ICD (Intra-query Category Diversity), and latency delta per scenario per profile. Opens snapshot read-only; produces no writes to the snapshot. | `--db <PATH>` (required), `--scenarios <PATH>` (required), `--configs <TOML,...>` (required), `--out <DIR>` (required), `--k <N>` (default 5) |
| `eval report` | Aggregate per-scenario JSON result files into a Markdown report. Report contains: summary table (P@K, MRR, CC@k, ICD with delta columns), notable ranking changes, latency distribution, entry-level analysis, distribution analysis (CC@k range per profile and top improved/degraded scenarios by CC@k), and zero-regression check section. Human-reviewed artifact only — no automated pass/fail gate. | `--results <DIR>` (required), `--out <PATH>` (required), `--scenarios <PATH>` (optional, annotates queries) |

### Global Flags

| Flag | Description |
|------|-------------|
| `--project-dir <PATH>` | Override automatic project root detection. |
| `--verbose` / `-v` | Enable debug-level logging to stderr. |

---

## Architecture Overview

Unimatrix is a 9-crate Rust workspace that ships as a single binary.

### Storage

SQLite local database (`unimatrix.db`). 21 tables. Schema version 18. Zero cloud dependency — all data stays on your machine.

### Vector Search

384-dimension HNSW vector index (in-memory, persisted to disk). Dot product similarity.

### Embedding

Local ONNX model (all-MiniLM-L6-v2) via ONNX Runtime. No API calls. MicroLoRA adaptive layer tunes embeddings to project-specific usage.

### Hook Integration

Single binary. The `hook` subcommand communicates with the running MCP server via Unix domain socket (UDS) IPC. Sub-50ms round-trip budget.

### MCP Transport

Daemon mode (default): Unimatrix runs as a persistent background daemon (`unimatrix serve --daemon`) that accepts MCP connections over a Unix Domain Socket (`unimatrix-mcp.sock`, 0600 permissions). Claude Code spawns a lightweight bridge process (the default `unimatrix` invocation) per session; the bridge connects stdin/stdout to the daemon's UDS socket. The daemon survives client disconnection — background tick, vector index, and all in-memory state persist across sessions. Up to 32 concurrent MCP sessions are supported.

Stdio mode (explicit): `unimatrix serve --stdio` starts the server in foreground stdio mode. Identical to the pre-daemon behavior; use for development and testing.

The hook IPC socket (`unimatrix.sock`) and the MCP socket (`unimatrix-mcp.sock`) are separate files. Hook IPC uses the existing length-framed binary protocol; MCP sessions use newline-delimited JSON-RPC over the MCP socket.

### Data Layout

```
~/.unimatrix/
  config.toml                # Global config (optional — see Configuration section)
~/.unimatrix/{project-hash}/
  config.toml                # Per-project config override (optional)
  unimatrix.db               # SQLite knowledge database (schema v18)
  unimatrix.pid              # PID file with flock advisory lock
  unimatrix.sock             # Unix domain socket for hook IPC
  unimatrix-mcp.sock         # Unix domain socket for MCP sessions (daemon mode)
  unimatrix.log              # Daemon stdout/stderr log (append mode)
  vector/
    unimatrix-vector.hnsw2   # HNSW graph
    unimatrix-vector.meta    # Index metadata
~/.cache/unimatrix/models/   # ONNX model files (downloaded once)
```

### Crate Workspace (9 crates)

| Crate | Responsibility |
|-------|---------------|
| `unimatrix-store` | SQLite storage engine — entries, indexes, audit log, migrations |
| `unimatrix-vector` | HNSW vector index — build, search, persist, compact |
| `unimatrix-embed` | ONNX embedding pipeline — model loading, tokenization, inference |
| `unimatrix-core` | Core traits, domain types, async wrappers, query filters |
| `unimatrix-engine` | Shared business logic — confidence scoring, project paths, search ranking |
| `unimatrix-adapt` | Adaptive embedding pipeline — MicroLoRA training, state persistence |
| `unimatrix-observe` | Observation pipeline — hotspot detection, metric computation, retrospective analysis |
| `unimatrix-learn` | Shared ML infrastructure — training reservoirs, EWC++ state, neural models, model versioning |
| `unimatrix` | MCP server — tool handlers, hook IPC, agent registry, audit, content scanning |

---

## Security Model (Mostly Future Use)

### Trust Hierarchy

Four-tier model: System > Privileged > Internal > Restricted. Unknown agents auto-enroll as Restricted on first contact (read + search only). `context_enroll` (Admin-only) promotes or modifies agent trust and capabilities. Protected agents: `system` and `human` cannot be modified. Self-lockout prevention: an admin cannot remove their own Admin capability.

### Capabilities

Four capabilities gate tool access: `read`, `write`, `search`, `admin`.

### Content Scanning

Every write operation (`context_store`, `context_correct`) scans content for injection patterns (~25+ patterns including prompt injection, system prompt overrides, and encoded payloads) and PII patterns (6+ patterns including emails, phone numbers, API keys, and credentials). Flagged content is rejected before storage.

### Audit Trail

Append-only audit log records every operation with agent identity (who performed the action), session context (which session, feature cycle), and operation outcome (success/failure).

### Hash-Chained Corrections

SHA-256 content hashes with `previous_hash` links create tamper-evident correction chains. Any break in the chain is detectable.

### Observation Ingest Constraints

Three hard limits apply to all observation events before any processing:

- **Payload size**: events with a payload exceeding 64 KB are rejected with a `PayloadTooLarge` error.
- **JSON nesting depth**: payloads nested more than 10 levels deep are rejected with a `NestingTooDeep` error.
- **Source domain format**: `source_domain` must match `^[a-z0-9_-]{1,64}$` at both domain pack registration and event ingest. Invalid values are rejected with an `InvalidSourceDomain` error — they do not silently coerce or pass through.

---

## Acknowledgments

Unimatrix's hook-driven delivery architecture draws directly from [ruvnet's](https://github.com/ruvnet) pioneering work on [claude-flow](https://github.com/ruvnet/claude-flow) (Ruflo) and ruvector. The core insight — that agent knowledge systems only deliver value when knowledge reaches agents automatically, without requiring explicit tool calls — shaped the entire Cortical Implant design. The adaptive embedding pipeline builds on patterns explored in ruvector's vector search architecture. We learned from both systems and are grateful for the open exploration that made this work possible.

---

## License

MIT OR Apache-2.0
