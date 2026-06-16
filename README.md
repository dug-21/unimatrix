# Unimatrix (Alpha release)

Unimatrix is a workflow-aware, self-learning knowledge engine built for agentic workflows such as software delivery. It makes knowledge curation a first-class activity in the workflow itself — not a side effect. Agents search, store, and correct knowledge entries as a normal part of doing work: decisions get attributed, lessons get captured, patterns get refined. Unimatrix makes that knowledge trustworthy, consistent, and — as it learns from actual usage — continuously more relevant.

Two surfaces, both driven by the same engine: agents retrieve knowledge on demand (search, lookup, get), and Unimatrix delivers it proactively — phase-conditioned injections and briefings that surface what matters before agents need to ask. The combination of explicit curation and self-improving delivery is what makes it distinct.

Unimatrix is not an orchestration engine. It does not coordinate agents, schedule
work, or manage workflows. It is a knowledge engine that understands workflow context
— your current phase, the cycle(feature) goal, the agent task, what comes next — and uses that
understanding to surface relevant knowledge at exactly the right moment.

The key mental model: workflow definitions, agent definitions, and skill definitions
are static — they live in your tooling and change infrequently. Architecture
decisions(ADR's), patterns, and lessons-learned are dynamic — they evolve with every
feature, every delivery, every failure. Unimatrix was designed to manage the dynamic
layer. Every architectural pivot, every hard-won lesson, every reusable pattern is
captured, attributed, and when needed, corrected, and only current information is made available to every future agent that needs it.

Built for agentic software delivery. Configurable for any workflow-centric domain.

---

## Getting Started

### Install via npm

> **Platform: Linux x64 and arm64 only.** macOS and Windows are not supported via npm.
> **Hook providers: Claude Code (full), Gemini CLI (v0.31+), and Codex CLI (config ready; live MCP hook testing blocked by Codex upstream bug #16732)**

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

### Container Deployment

> **Platform: Linux x64 and arm64.** Multi-arch images published to GHCR on each release.

```bash
# One-command deployment with persistent data volume
docker run -v unimatrix-data:/data ghcr.io/dug-21/unimatrix

# Or via docker compose
docker compose up
```

The container runs `unimatrix serve --foreground` as PID 1 (non-root, UID 65532). Both ONNX models (embedding + NLI) are baked into the image — no internet access required after pull. Data persists in the `unimatrix-data` named volume at `/data`. Config lives in the data volume; customize via `unimatrix config` or set `UNIMATRIX_CONFIG` for external override.

**HTTPS serving (personal cloud).** To serve the container as a reachable, operator-run cloud over pinned TLS, set two environment variables in `compose.yaml`: `UNIMATRIX_HTTP_ENABLED=true` activates the HTTPS listener (the global binary default `http.enabled` stays `false`), and `UNIMATRIX_PUBLIC_URL` declares the URL clients connect to (e.g. `https://uni.example.com:8443`) — the single knob from which the bundle base-url, the `allowed_hosts` default, and the certificate SAN all derive. The published port is **TLS-only port 8443** — no plaintext port is exposed. On first boot the binary auto-generates both a 32-byte bearer token and a self-signed cert+key (key mode `0600`), persisting them to the data volume; subsequent boots load (not regenerate) them, and an operator may mount their own cert/key read-only to override. The only unauthenticated endpoint on the published port is `GET /health`. Host bind-mounted `/data` must be writable by UID 65532 (`chown 65532` the host directory; named volumes need no setup) — the binary fails loud and actionable if it is not. After the container is up, run `unimatrix client-bundle` to emit the connection bundle for clients.

**Serving multiple projects.** One container can serve N fully-isolated projects from a single bundle and bearer token. The operator declares projects with `unimatrix project register <slug>`; each registered slug is then reachable at `https://host:8443/v1/{slug}/...` with its own database, vector index, hash chain, and analytics under `/data/.unimatrix/{slug}/` — no cross-project read or write. Slugs are operator-declared, never client-minted: a client attaches to an existing slug and never auto-creates a project. A slug must match `^[a-z0-9][a-z0-9-]{0,62}$` (lowercase alphanumeric and hyphen, 1–63 chars, starting alphanumeric) and may not be a reserved route segment (`v1`, `health`, `observe`, `tools`). When no projects are registered (`[[projects]]` absent), the server serves the single default project at `/v1/tools/...` exactly as before — existing single-project installs see zero behavior change.

### Build from Source

Prerequisites:
- Rust 1.89+ (edition 2024)
- ONNX Runtime 1.20.x shared library


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
/uni-init     # Informs claude Unimatrix exists
```

That's it. Unimatrix is ready to use.

#### Remote (HTTP) mode

To wire a project against a networked Unimatrix server instead of a local binary:

```bash
npx @dug-21/unimatrix init --remote https://uni.example.com --token <token>
```

This configures `.claude/settings.json` hooks to invoke the pure-JS HTTP hook client (`node /abs/path/lib/hook-client/index.js <EVENT>`) for the full remote event set, including `PreCompact` and `PostToolUseFailure`. The URL and token are written to `.claude/settings.local.json` (gitignored, per-project) under the `unimatrix.remote` key — never on the hook command line. The environment variables `UNIMATRIX_REMOTE_URL` and `UNIMATRIX_REMOTE_TOKEN` override the file when set. Init validates connectivity with a `Ping` request before writing config. No platform binary or ONNX model is required, so remote mode works on Linux, macOS, and Windows with Node >= 18. `.mcp.json` is skipped in remote mode with an informative message. The merge is idempotent — re-running preserves non-unimatrix hooks and recognizes its own entries.

##### Bundle-driven attach (pinned TLS)

To attach against an HTTPS server that serves a self-signed certificate (the container HTTPS posture above), use the connection bundle the operator emits with `unimatrix client-bundle` instead of passing the URL and token separately:

```bash
npx @dug-21/unimatrix init --remote unimatrix-bundle:<blob>
```

The bundle carries `{base-url, token, cert-fingerprint}` in one opaque string. The client pins the server's exact certificate by its `sha256:` fingerprint (no CA-trust path), so a self-signed cert is trusted by pinning rather than by a certificate authority. A wrong or rotated certificate is rejected with a clear, diagnosable fingerprint-mismatch error directing you to re-bundle. This is a pure-JS, copy-installed remote attach (under 250 KB — no platform binary, no ONNX model) and works on Linux, macOS (Apple Silicon), and Windows with Node >= 18; `init` copies skills and prints the `/uni-init` pointer (it does not append the CLAUDE.md knowledge block — `/uni-init` owns that). Each client instance is bound to exactly one project: a different project means a separate client instance. Multiple distinct LLM CLIs (Claude Code, Codex CLI, Gemini CLI) attach the same server identically — each is a separate client connection.

When the server serves multiple projects, append the operator-supplied slug to the remote URL so the client binds to that one project:

```bash
npx @dug-21/unimatrix init --remote unimatrix-bundle:<blob> --slug <slug>
```

The bundle is cloud-wide (one per container); the slug is per-project and supplied at attach time. The client attaches to an already-registered slug and errors if the slug is unregistered — it never creates a project. Multiple clients may attach the same slug (N clients sharing one project, attributed per session); a single client never spans multiple projects.

When the operator rotates the server certificate, re-run `client-bundle` and re-run `init --remote` on each client with the new bundle. See [docs/cert-rotation.md](docs/cert-rotation.md) for the operator rotation procedure.

### Brownfield Applications

```
/uni-seed     # Interactive skill to review existing project and seed Unimatrix with decisions(ADR's), patterns, procedures
```

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

- **Project knowledge is *dynamic*** - An architecture decision can change from one feature to the next.  Unimatrix makes that 1 operation and always provides the most up to date info to agents. 

- **Auditable Knowledge Lifecycle** — Every entry has a SHA-256 content hash. Corrections create hash-chained supersession links. An append-only audit log records every operation with agent identity and session context. You can trace how any piece of knowledge evolved.

- **Invisible Delivery** — Agents do actively search and use knowledge from Unimatrix.  Hook-driven integration ALSO injects relevant expertise into prompts, subagent spawns and precompaction events automatically, providing a tag team approach focused on ensuring relevant information while ALWAYS being context consumption conscious. Its a tag team approach to provide the best information.

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

Automatic context injection on every prompt via the `UserPromptSubmit` hook. Six hook events drive the integration: `UserPromptSubmit`, `SubagentStart`, `PreCompact`, `PreToolUse`, `PostToolUse`, `Stop`. Subagent injection: when the SM spawns a subagent, the `SubagentStart` hook fires synchronously and injects relevant knowledge into the subagent context before its first token — this combined with a `context_briefing` call on the outset, provides agents with an index of the most relevant artifacts to their goal and task. `UserPromptSubmit` injection requires at least 5 words in the prompt; shorter inputs (e.g., "yes", "ok continue") are recorded but produce no injection. **No guidance is better than misdirection**. Compaction resilience: `PreCompact` preserves critical context before Claude Code's context window compaction; the compaction payload is a flat indexed table of active entries (up to k=20) plus a session histogram summary. Closed-loop feedback: the `Stop` hook records session outcomes for confidence evolution. Sub-50ms round-trip budget per hook event. Disk-backed event queue for graceful degradation. Single binary — the `hook` subcommand connects to the running MCP server via Unix domain socket IPC. Hooks provide the telemetry necessary for Unimatrix to learn.

Multi-provider hook support: Gemini CLI events (`BeforeTool`, `AfterTool`, `SessionEnd`) are normalized to canonical Unimatrix names at the ingest boundary — no downstream code sees provider-specific strings. Codex CLI uses the same event names as Claude Code; the `--provider codex-cli` flag on the `unimatrix hook` subcommand disambiguates attribution. Reference configurations are provided at `.gemini/settings.json` and `.codex/hooks.json`. Codex live MCP hook support is pending resolution of Codex upstream bug #16732.

Remote (HTTP) hook client: for deployments connecting to a networked server, a pure-JS hook client ships in the `@dug-21/unimatrix` npm package (`lib/hook-client/`) — no platform binary or ONNX model required, so it runs on Linux, macOS, and Windows with Node >= 18. It reads hook stdin, builds the same `HookRequest` the Rust hook builds, and POSTs to `{url}/observe` with `Authorization: Bearer <token>`; sync events (`UserPromptSubmit`, `PreCompact`, `SubagentStart`) request `Accept: text/plain` so the server formats injection text. On fire-and-forget events it streams transcript deltas (`[last_offset, file_len)`) in a separate POST so the server's per-session buffer holds the authoritative conversation — bringing remote `PreCompact` restoration to local fidelity. It is fail-open (exit 0 always, never blocks the host CLI), has zero runtime dependencies, and uses a disk-backed event queue for graceful degradation. On Unix the same client also runs in local mode: with no remote config it connects to the daemon's hook IPC socket (`unimatrix.sock`) over a Unix domain socket (UDS), framing `HookRequest` messages byte-identically to the Rust hook. Transport is selected automatically — remote config (the `unimatrix.remote` settings key or `UNIMATRIX_REMOTE_*` env vars) selects HTTP; its absence selects local UDS. UDS local mode is Unix-only, so Windows always uses the HTTP path. Configure remote mode with `npx @dug-21/unimatrix init --remote <url> --token <tok>` (see "Wire into your project"). The hook event set written by `init` covers 8 events by default: `SessionStart`, `Stop`, `UserPromptSubmit`, `PreToolUse` (narrowed to `context_cycle` cycle-event interception — standalone tool observation is no longer registered), `PostToolUse`, `PostToolUseFailure`, `SubagentStart`, `PreCompact`. `SubagentStop` is opt-in: set `unimatrix.hooks.subagent_stop: true` in `.claude/settings.local.json` (default off) to register it.

### Cycle Review Analysis

Analyzes session telemetry for a completed feature cycle and produces the `# Unimatrix Cycle Review —` report. 23 detection rules across 4 categories: agent behavior, friction points, session health, and scope indicators. Includes a `DependencyOnDeprecated` rule (severity: Warning) that fires when any `Prerequisite` edge in the current cycle's entries points to a deprecated source — signaling stale dependency relationships that may need review. Rules are domain-aware: each rule guards on `source_domain` as its first filter, so Claude Code rules never fire on events from other domains. A domain pack registry loaded at startup from TOML defines which event types, categories, and detection rules apply to each domain; the "claude-code" pack is always active with no config required. Historical baselines with outlier detection surface anomalies. Evidence synthesis produces actionable findings with supporting data. Lessons and patterns extracted from retrospectives are stored back in the knowledge base with de-duplication via correction chains.

The report header surfaces the feature goal, inferred cycle type (Design, Delivery, Bugfix, Refactor, or Unknown), attribution path used (cycle\_events-first, sessions.feature\_cycle legacy, or content-scan fallback), and an in-progress indicator when no `cycle_stop` event exists. A Phase Timeline table breaks the cycle into per-phase windows showing duration, pass count, agents spawned, records, knowledge throughput, and gate outcome. A "What Went Well" section surfaces non-outlier favorable baseline signals that were previously hidden. Per-finding evidence is rendered as relative-time burst notation (`Timeline: +0m(N) +12m(N▲) …`) rather than raw epoch values. The Knowledge Reuse section splits served entries into cross-feature (from prior cycles) and intra-cycle buckets with a top-entry breakdown. Recommendations appear immediately after the header, before all other sections.

The review also persists durable per-cycle aggregate columns so cycles can be compared over time rather than recomputed each call: phase durations, transitions, and rework loops (including phases declared but never closed, surfaced as hotspots); the rework/failure session ratio; a knowledge-reuse count over all entries served to the cycle (the union of query and injection logs, not only same-cycle-tagged entries); transcript throughput as a byte total and delta count; content-opaque behavioral-signal counts (e.g. `error`, `refusal`); a compaction count; and two distinct reload metrics — a cross-session `context_reload` (continuity/handoff cost) and a post-compaction within-cycle `compaction_reread` (the compaction tax). The two reload signals are never collapsed into one number. Two presentation-honesty rules govern rendering: a metric whose source data class is empty for the cycle renders **"unavailable"** rather than a `0` indistinguishable from a measured zero, and the behavioral-signal counts render with a coarse/directional qualifier — they are unvalidated, content-opaque regex matches, not exact auditable totals — visually distinct from exactly-counted aggregates such as phase and compaction counts. The throughput unit is bytes, never tokens or cost: these metrics inform the process; they never control execution. An optional `auto_close` parameter writes the `cycle_stop` event synchronously before the review pipeline when the cycle has no stop yet, so a final retrospective can close and review the cycle in one call.

Ahead of the transcript purge that runs at cycle review, the review distills each attributed session's in-memory transcript buffer into a response-transient `transcript_candidates` section. The server *selects* whole marker-matched user/assistant blocks against four marker families (decision phrases, rework signals, lesson markers, phase/gate markers) and attaches them — with per-block provenance, advisory family hints, and per-session loss info (elided bytes, holes, cap-forced truncation) — to the JSON response; the calling agent performs all semantic extraction into `context_store`. The section is additive and absent when no session yields candidates. Selection runs strictly after every transcript lock is released; the 23 detection rules' inputs are unchanged. When a session's buffer is empty or hole-ridden, a labeled degraded fallback reconstructs distillation input from that session's stored observations. Candidates are never persisted — they do not ride the memoized cycle-review record, and reflect call-time buffer content (a memoization hit may return candidates that differ from the cached report).

### Behavioral Signal Delivery

Cycle outcomes recorded via `context_cycle` feed as graph edges, reinforcing co-access signals between entries retrieved during successful delivery phases. Each time a phase completes with a positive outcome, the knowledge retrieved during that phase gains stronger co-access links — future agents entering the same phase surface those entries higher. `context_briefing` operates as a targeted handoff at phase transitions: it uses the current phase and the cycle's history to prioritize knowledge relevant to the agent's declared phase, delivering a structured top-k result set without requiring the agent to search. This goal-conditioned briefing, combined with UDS injection, makes knowledge delivery phase-aware and progressive rather than flat. Reference: crt-046, Group 6.

### Contradiction Detection

After each `context_store`, a background scan checks the new entry against its top HNSW neighbors using cosine similarity. Pairs with similarity >= 0.65 are recorded as `Supports` edges in the knowledge graph. Contradiction density — the ratio of unresolved contradictions to active entries — is one dimension of the Lambda structural health metric, computed periodically and surfaced in `context_status` health reports. When contradictions are identified, `context_correct` is the resolution path: it deprecates the conflicting entry and links the replacement through a hash-chained supersession record. No external model is required for contradiction management.

### Domain-Agnostic Observation Pipeline

Every detection rule carries a `source_domain` guard — a rule fires only for events from its declared domain, never cross-contaminating signals from unrelated systems. Domain packs are registered via `[[observation.domain_packs]]` entries in `config.toml`, specifying the source domain, event types, and applicable knowledge categories. The built-in "claude-code" domain pack is always active and requires no configuration — it covers all Claude Code lifecycle hook events out of the box. Any domain's event stream connects to the learning layer by registering a domain pack; no code changes are required. `source_domain` is validated at both ingest and registration: values must match `^[a-z0-9_-]{1,64}$`. Reference: W1-5, col-023.

Provider identity (`"claude-code"`, `"gemini-cli"`, `"codex-cli"`) is derived from the `--provider` flag or inferred from the hook event name at the ingest boundary and carried through as `source_domain` on observation records. Gemini CLI events with unique names (`BeforeTool`, `AfterTool`, `SessionEnd`) are unambiguously identified without a flag; Codex CLI requires `--provider codex-cli` because it shares event names with Claude Code.

### Correction Chains with Audit Trails

`context_correct` creates a new entry and deprecates the original, linking them with SHA-256 content hashes (`previous_hash` chain). The append-only audit log records every operation — store, correct, deprecate, quarantine, enroll — with agent identity, session context, and operation outcome. Correction chains are tamper-evident: any break in the hash chain is detectable.

### Coherence Gate (Lambda Health Metric)

Lambda is a composite structural integrity metric [0.0, 1.0] computed from three dimensions: graph quality (weight 0.46 — is the vector index structurally sound?), contradiction density (weight 0.31 — how many unresolved contradictions exist?), and embedding consistency (weight 0.23 — do entries have valid, current embeddings?). When lambda drops below 0.8, maintenance is recommended. A background tick handles maintenance automatically — confidence refresh, graph compaction, co-access cleanup.

`context_status` also reports six graph cohesion metrics computed per-call from the `GRAPH_EDGES` table: connectivity rate (fraction of active entries with at least one non-bootstrap edge), isolated entry count, cross-category edge count, Supports edge count, mean entry degree (in+out). Additionally, `stale_dependency_edges` reports the count of `Prerequisite` edges whose source entry has been deprecated — a non-zero value indicates dependency relationships that may reference outdated knowledge. These metrics are informational — they do not feed into lambda — but let operators verify whether automated platform is driving cross-category graph that PPR can exploit. Summary format includes a single "Graph cohesion:" line; Markdown format includes a `### Graph Cohesion` sub-section within the Coherence block.

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

Unimatrix loads configuration from up to two optional TOML files at server startup. When neither file is present, all compiled defaults apply.

- `~/.unimatrix/{project-hash}/config.toml` — **primary** (per-project). Written automatically on first run. This is the canonical config location.
- `~/.unimatrix/config.toml` — **defaults** (global). Optional cross-project defaults; values here apply to all projects unless overridden per-project. List fields (`categories`, `boosted_categories`, `adaptive_categories`, `session_capabilities`) replace the global list entirely — there is no append behavior.

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

# Purge policy for raw session transcript (ephemeral, possibly secret-bearing
# conversation bytes — distinct from distilled knowledge, observations, and the
# audit log, which have their own retention knobs above).
# Values:
#   "PurgeOnCycleClose"  — purge the in-memory transcript buffer at session close,
#                          the staleness sweep, and cycle review
#                          (context_cycle_review). Default, and the only value
#                          the OSS build accepts.
#   { RetainDays = N }   — enterprise retain-N-days seam. REJECTED at startup by
#                          the OSS build with an enterprise-only validation error.
transcript_retention = "PurgeOnCycleClose"

# Maximum accumulated size in bytes of a session's in-memory transcript buffer.
# On overflow the most recent content is kept (ring-tail) and the dropped-byte
# count is recorded as metadata — never as marker bytes in content. Per-session
# bound; aggregate memory is this cap times the number of concurrent sessions.
# Floor: 65536 (64 KiB) — values below abort startup. Default: 4194304 (4 MiB).
transcript_buffer_max_bytes = 4194304

# Per-session cap (bytes) on the transcript candidates a single session may
# contribute to the cycle-review distillation pass. Caps distilled output volume,
# not the raw buffer (which transcript_buffer_max_bytes governs).
# Range: [1, transcript_candidate_cycle_cap_bytes]. Default: 24576 (24 KiB).
transcript_candidate_session_cap_bytes = 24576

# Per-cycle aggregate cap (bytes) on selected candidates across all sessions in
# one cycle-review distillation pass. Truncation is deterministic chronological
# keep-earliest; a cap-forced drop is reported in the candidates loss info.
# Range: [transcript_candidate_session_cap_bytes, usize::MAX]. Default: 262144 (256 KiB).
transcript_candidate_cycle_cap_bytes = 262144

# Hole-fraction threshold for the reconstruction fallback. When the fraction of a
# session's snapshot window lost to holes/elision meets or exceeds this ratio, the
# session is routed through the labeled degraded reconstruction path (stored
# observations) at the documented fidelity floor.
# Range: [0.0, 1.0]. Default: 0.5.
transcript_fallback_hole_fraction = 0.5

# Held-buffer count ceiling. The held-buffer store keeps drained multi-turn
# transcript buffers alive across the per-turn drain so the cycle-review
# distillation path is non-empty. On the (cap+1)th hold the oldest held buffer is
# evicted (every eviction audited). Memory is bounded by transcript_buffer_max_bytes
# times this value. One of two independent reclamation mechanisms (the other is the
# TTL sweep). Range: [1, usize::MAX]. Default: 64.
transcript_hold_max_sessions = 64

# Held-buffer stale-sweep TTL in seconds. A held buffer untouched for longer than
# this is reclaimed by the maintenance-tick stale sweep, independent of whether
# cycle review ever fires. The second independent reclamation mechanism; either
# alone bounds memory. Range: [1, u64::MAX]. Default: 86400 (24 h).
transcript_hold_ttl_secs = 86400
```

```toml
# [transcript_signals] — content-free behavioral-signature classes folded over the
# in-memory transcript stream (sibling to [retention]; #[serde(default)]).
# Each enabled class is a labelled regex matched against raw transcript-delta bytes
# in a single shared scan; only a per-class match COUNT is kept — no transcript
# content is ever stored or surfaced. The counts are DIRECTIONAL, not precise: a
# non-zero count means "this cycle saw matches," not an audited incident total
# (the producer is content-opaque, so the false-positive rate cannot be audited
# after the fact). When omitted, the default catalog ships exactly two domain-neutral
# classes: `error` (index 0 — provider/model hard + overload errors) and
# `refusal` (index 1 — first-person model refusal phrasings). No SDLC literals.
#
# Validation is loud at startup: more than 16 enabled classes, an invalid regex,
# or a duplicate class_name aborts the server with a descriptive error.
[[transcript_signals]]
class_name = "error"      # Stable label; maps to a fixed index by config order.
pattern    = "..."         # Regex compiled once into one shared RegexSet.
enabled    = true          # Disabled entries are parsed but excluded from the scan.
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

```toml
# [http] — HTTPS transport (disabled by default)
[http]
enabled = false               # Set to true to activate the HTTPS listener
content_port = 8443           # TCP port for MCP, health, and observe endpoints
bind_address = "0.0.0.0"     # Bind address (use 0 for OS-assigned port in testing)
max_connections = 32          # Maximum concurrent HTTP sessions

# [tls] — TLS termination (auto-enabled when cert_path and key_path are present)
[tls]
enabled = true                # Set to false for reverse-proxy deployments (plain HTTP)
cert_path = "/path/to/cert.pem"
key_path = "/path/to/key.pem"
```

A 32-byte bearer token is generated automatically at `{data_volume}/token` on first server start with HTTP enabled. The token is printed to stdout once with the `[UNIMATRIX TOKEN]` label. Subsequent starts load it silently. See [docs/client-setup.md](docs/client-setup.md) for client configuration.

**Container HTTPS env vars.** For container deployments, two environment variables drive the HTTPS posture without editing config files (the distroless runtime has no shell, and the global binary default `http.enabled` stays `false`):

- `UNIMATRIX_HTTP_ENABLED=true` — activates the HTTPS listener, container-scoped (an env override of `[http] enabled`).
- `UNIMATRIX_PUBLIC_URL` — the URL clients connect to (e.g. `https://uni.example.com:8443`). A single derivation feeds three consumers: the connection bundle's base-url, the `allowed_hosts` default, and the generated certificate's SAN. When unset, the server uses a loud `https://<EDIT-ME>:8443` placeholder and a permissive-with-warning posture; socket auto-detection is not used.

On first boot with HTTP enabled the binary auto-generates the self-signed cert+key alongside the token (key mode `0600`), with the SAN derived from `UNIMATRIX_PUBLIC_URL` plus the local set (`localhost`, `127.0.0.1`, `0.0.0.0`); the operator may mount their own cert/key read-only to override.

Config files are validated for security at load time: world-writable files abort startup; group-writable files log a warning. `[server] instructions` is scanned for injection patterns before use.

---

## MCP Tool Reference

Unimatrix exposes 14 MCP tools. All tools accept `format: "summary" | "markdown" | "json"` as a common parameter.

| Tool | Purpose | When to Use |
|------|---------|-------------|
| `context_search` | Search for relevant context using natural language. Returns semantically similar entries ranked by relevance. | When you need to find patterns, conventions, or decisions related to a concept. Use when you do NOT know exactly what you are looking for. Key params: `query` (required), `category`, `topic`, `tags`, `k` (default 5), `helpful`. |
| `context_lookup` | Look up context entries by exact filters. Returns entries matching topic, category, tags, status, or ID. | When you KNOW what you are looking for — a specific feature's entries, a category listing, or a known ID. Key params: `topic`, `category`, `tags`, `id`, `status`, `limit` (default 10). |
| `context_get` | Get a specific context entry by its ID. | When you have an entry ID from a previous search or lookup result and need the full content. Key params: `id` (required), `helpful`. |
| `context_store` | Store a new context entry with duplicate detection and content scanning. Each successful non-duplicate store increments the per-session category histogram used by `context_search` for implicit session affinity ranking. | When you discover a pattern, convention, decision, or lesson worth preserving. Key params: `content` (required), `topic` (required), `category` (required), `tags`, `title`, `feature_cycle`, `edges` (optional list of `{edge_type, target_id}` to declare typed relationships at creation time). |
| `context_correct` | Correct an existing entry. Deprecates the original and creates a new entry with a hash-chain link. Automatically redirects all incoming graph edges (excluding `Supersedes`) from the deprecated original to the new entry — no separate `context_edge(mode="redirect")` call required. The original's eligible outgoing graph edges (agent-declared types only; `Supersedes` and tick-generated `CoAccess`/`Informs` excluded) are also carried forward to the new entry by default — no need to re-declare them in `edges`. The response includes an `edges_carried` integer count when one or more outgoing edges are carried (omitted when zero). To intentionally drop an outgoing edge that no longer holds, shed it with `context_edge(mode="remove")` or `context_edge(mode="redirect")` against the new entry id (the deprecated original cannot be edited). Response text includes `"Redirected N incoming edges (M failed, see logs)"` when edges are affected. | When an entry contains wrong or outdated information that should be superseded (not just hidden). Key params: `original_id` (required), `content` (required), `reason`, `edges` (optional list of `{edge_type, target_id}` — edges attach to the new corrected entry, not the deprecated original; passed edges compose additively with carried edges on `(source, target, relation_type)`). |
| `context_deprecate` | Mark an entry as outdated. Entry remains accessible but excluded from default search/lookup. | When knowledge is no longer relevant but should not be deleted (historical record). Key params: `id` (required), `reason`. |
| `context_quarantine` | Quarantine or restore an entry. Quarantined entries are excluded from search and lookup. **Admin only.** | When an entry is suspicious, invalid, or harmful and should be isolated. Use `action: "restore"` to undo. Key params: `id` (required), `action` ("quarantine" or "restore"), `reason`. |
| `context_status` | Get knowledge base health metrics. Shows entry counts, distributions, correction chains, coherence score, security metrics, graph cohesion metrics (connectivity rate, isolated entry count, cross-category edge count, Supports edge count, mean entry degree, and inferred edge count), per-category lifecycle labels (adaptive vs pinned), `pending_cycle_reviews` (cycle IDs that have started within the retention window but have no stored cycle review yet — always computed), and a `curation_health` aggregate block (per-cycle correction rate mean/stddev, source breakdown as agent% and human%, orphan deprecation ratio mean/stddev, and trend direction when at least 6 cycles of snapshot data are available). **Admin only.** | When you need to assess knowledge base health or inspect whether graph edge inference is producing a connected, cross-category graph, identify cycles awaiting retrospective review before signals can be purged, or review curation behavior trends across recent cycles. The `maintain` parameter is accepted but silently ignored — a background tick handles maintenance automatically. Key params: `topic`, `category`, `check_embeddings`. |
| `context_briefing` | Get a knowledge index for a topic or task. Returns up to 20 active entries as a flat indexed table (columns: row, id, topic, category, confidence, snippet). Query derived from: (1) explicit `task` param. Used at the start of a phase or task to get oriented. Call when starting a new 'task' (often with a new subagent). Key params: `topic` (also known as feature_cycle), `task`, `k` (default 20), `max_tokens` (default 3000, range 500-10000). |
| `context_enroll` | Future use |
| `context_cycle` | Signal feature cycle lifecycle events: start, phase transitions, and stop.  | At cycle start/stop and at each phase boundary. Key params: `type` (required: `"start"` \| `"phase-end"` \| `"stop"`), `topic` (required. Topic and feature_cycle are interchangeable), `phase`, `outcome`, `next_phase`, `agent_id`, `goal` (optional, `start` only: 1–2 sentence plain-text statement of feature intent; used as the step-2 query signal by `context_briefing` and hook injection when no explicit `task` is provided; max 1 024 bytes). **Proper use of context_cycle provides unimatrix deep visibility into your workflow**|
| `context_cycle_review` | Analyze observation data for a work cycle (Retrospective). Parses session telemetry, detects hotspots, computes metrics, and renders and stores the `# Unimatrix Cycle Review —` report. Persists durable per-cycle aggregate columns (phase durations/transitions/rework, rework ratio, knowledge-reuse served-count, transcript throughput in bytes, behavioral-signal counts, compaction count, and two distinct reload metrics) for cross-cycle comparison. A metric whose source data is empty for the cycle renders **"unavailable"** rather than a misleading `0`; the content-opaque behavioral-signal counts render with a coarse/directional qualifier, never as exact auditable counts. Ahead of the transcript purge, distills each attributed session's transcript buffer into an additive, response-transient `transcript_candidates` section (whole marker-matched user/assistant blocks + per-session loss info; absent when empty; never persisted) for the calling agent to extract into `context_store`. Use `force=true` to recompute and overwrite the stored record; `auto_close=true` writes the `cycle_stop` event synchronously before the review when the cycle has no stop yet. | After a work cycle completes, to better understand what worked and what didn't during the cycle. Key params: `feature_cycle` (required), `evidence_limit`, `force` (bool, default false — when true, forces recomputation even if a stored record exists), `auto_close` (bool, default false — when true and no `cycle_stop` exists, the cycle is stopped synchronously before the review pipeline), `format` ("markdown" default, "json"). |
| `context_edge` | Standalone edge lifecycle management on existing entries: add a typed relationship, remove one that no longer holds, or redirect one when a target entry is superseded. Pure graph operation — no embedding recompute, no confidence update. Requires `Capability::Write`. Source entry must be active (not quarantined or deprecated). | When you need to declare, retract, or retarget a typed graph relationship between existing entries without creating a new entry version. Primary use case: retargeting `Advances` or `Prerequisite` edges after a supersession. Key params: `mode` (required: `"add"` \| `"remove"` \| `"redirect"`), `source_id` (required), `edge_type` (required — one of 13 agent-meaningful types: `Advances`, `Cites`, `Asserts`, `Mentions`, `Refutes`, `Tests`, `DerivedFrom`, `Motivates`, `About`, `RelatedTo`, `Prerequisite`, `Contradicts`, `Supports`), `target_id` (required), `new_target_id` (required for redirect only). |
| `context_graph` | Graph read operations on the knowledge graph. Seven modes, all requiring `Capability::Read`. `chain`: walks the full supersession history of an entry (ancestors and descendants) via recursive SQL CTE; returns ordered `Vec<EntryRecord>` with per-direction `truncated` flags; 50-hop safety cap. `current`: follows `superseded_by` to the terminal active entry via SQL CTE; 50-hop cap; error if chain terminates at an orphaned deprecated entry. `neighbors`: retrieves entries connected by typed edges; depth=1 queries the live database (all committed writes reflected immediately); depth>1 queries the in-memory graph (may lag recent writes by up to one tick interval); returns flat `Vec<EdgeRecord>`; `Supersedes` excluded from default all-types traversal. `subgraph`: bounded multi-seed BFS returning both nodes and edges (enough to reconstruct the subgraph locally); uses in-memory graph (tick-window staleness applies); 200-node hard cap; `EdgeRecord.direction` is always `"outgoing"` (canonical stored direction); returns `{ nodes, edges, truncated, seed_ids, depth_reached }`. `inverse`: SQL LEFT JOIN antijoin returning active entries of a given `category` that have no incoming edges of ALL specified `missing_edge_types` (AND semantics); queries the live database (no staleness); `limit` default 100, max 500. `filter`: combined category + optional property + optional edge-count filter via correlated SQL subquery against the live database; property filters: `min_age_days`, `min_confidence`, `max_confidence`; edge-count filters: `min_edge_count`, `max_edge_count` (require `edge_types` when present); `limit` default 100, max 500. `path`: shortest BFS path between two entries over the in-memory graph (outgoing edges only, tick-window staleness applies); returns `{ found, from_id, to_id, hops, length }` where `from_id` is not in `hops`; `found: false` when no path exists within depth or when either ID is absent from the current snapshot. | When you need to navigate the knowledge graph without semantic search. Use `chain` to audit correction history; `current` to resolve a deprecated entry to its live successor; `neighbors` for typed-edge neighbor retrieval; `subgraph` to retrieve a bounded evidence or dependency graph from seed entries; `inverse` to find entries of a category with no incoming edges of a given type (orphan/gap detection); `filter` to combine category, property, and edge-count constraints in a single query; `path` to find the shortest typed-edge route between two entries. Key params: `mode` (required: `"chain"` \| `"current"` \| `"neighbors"` \| `"subgraph"` \| `"inverse"` \| `"filter"` \| `"path"`), `id` (chain, current, neighbors), `seed_ids` (subgraph), `from_id` / `to_id` (path), `category` (inverse, filter), `missing_edge_types` (inverse), `edge_types` (neighbors, subgraph, filter, path; absent or empty = all types excluding `Supersedes`), `direction` (`"forward"` \| `"backward"` \| `"both"`, chain and neighbors; ignored for mode semantics in subgraph where all EdgeRecords carry `"outgoing"`), `depth` (neighbors default 1, range 1–10; path default 5, range 1–10), `max_depth` (subgraph default 3, range 1–10), `max_nodes` (subgraph, max 200), `limit` (inverse and filter, default 100, max 500), `resolve_supersessions` (neighbors, subgraph, path — default false; substitutes deprecated endpoints with their terminal active successors), `min_age_days`, `min_confidence`, `max_confidence`, `min_edge_count`, `max_edge_count` (filter mode property and edge-count constraints). |

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
| `serve --daemon` | Start the MCP server as a detached background daemon. Daemonizes (fork/setsid), binds the MCP UDS socket (`unimatrix-mcp.sock`) and hook IPC socket, starts the background tick loop, and exits the launcher process. Activates the HTTPS listener when `[http] enabled = true`. Fails non-zero if a healthy daemon is already running. Linux and macOS only. | `--daemon` |
| `serve --foreground` | Start the MCP server in PID 1 foreground mode. Identical daemon functionality (UDS listener, HTTP listener when enabled, tick loop, ML inference) without fork/setsid. SIGTERM triggers graceful shutdown. Designed for container deployment where the main process must remain PID 1. Mutually exclusive with `--daemon` and `--stdio`. | `--foreground` |
| `serve --stdio` | Start the MCP server in foreground stdio mode. Identical in behavior to the pre-daemon default — the server runs until stdin closes, then performs graceful shutdown and exits. Use for development and testing. | `--stdio` |
| `stop` | Send SIGTERM to the running daemon and wait for it to exit (up to 10 seconds). Exits 0 on success, non-zero if no daemon is running or the PID file is absent/stale. | None |
| `hook <EVENT>` | Handle a lifecycle hook event from Claude Code, Gemini CLI, or Codex CLI. Reads JSON from stdin, connects to the running server via UDS. Provider-specific event names (e.g., Gemini's `BeforeTool`, `AfterTool`, `SessionEnd`) are normalized to canonical Unimatrix names at the ingest boundary. Designed for use in hook configuration files, not direct user invocation. | Event name as positional arg. `--provider <name>` (`claude-code` \| `gemini-cli` \| `codex-cli`) — required for Codex (shares event names with Claude Code); optional for Gemini (inferred from event name); omit for Claude Code (backward-compatible default). |
| `health` | Check daemon liveness by connecting to the MCP UDS socket. Exit 0 when the daemon is running and responsive, exit 1 otherwise. 5-second timeout. No output on success; brief diagnostic on stderr on failure. Used by Docker HEALTHCHECK. | None |
| `client-bundle` | Emit a connection bundle for attaching a remote client to this server over pinned TLS. Reads the data-volume token and the served leaf certificate, and prints a single-line `unimatrix-bundle:` blob carrying `{base-url, token, cert-fingerprint}`. **stdout** is the opaque bundle blob only (pipeable); **stderr** echoes the decoded base-url and `sha256:` cert-fingerprint for the operator to eyeball, with the token redacted (never printed). Pre-tokio synchronous subcommand, like `health` and `version`. Consume the bundle on the client with `init --remote <bundle>`. | `--project-dir <PATH>` |
| `project register <slug>` | Register a project slug for multi-project serving. Creates the per-slug store (DB, vector index, hash chain, analytics) under `/data/.unimatrix/{slug}/` and adds the slug to `[[projects]]` routing, making it reachable at `/v1/{slug}/...`. Rejects a slug that fails the allowlist (`^[a-z0-9][a-z0-9-]{0,62}$`) or equals a reserved route segment (`v1`, `health`, `observe`, `tools`). Errors loudly if the slug is already registered and routing. If the slug was previously de-registered but its data dir persists, register **re-attaches** to the preserved store rather than starting a new chain. | `<slug>` (positional) |
| `project list` | List the registered project slugs. | None |
| `project delete <slug>` | De-register a slug: removes it from `[[projects]]` routing while **preserving** its on-disk data (DB, vector index, hash chain, analytics). Re-registering the same slug re-attaches to the preserved store. To destroy the data as well, pass `--purge`, which requires re-typing the slug via `--confirm <slug>` — the only operation that destroys a hash chain, and it is deliberately loud. | `<slug>` (positional), `--purge`, `--confirm <slug>` |
| `export` | Export the knowledge base to JSONL format (format_version 2, 11 tables). No running server required. | `--output <PATH>` (defaults to stdout), `--skip-quarantined` (omit quarantined entries and their dependents from the export — requires `--confirm`), `--confirm` (acknowledge non-exact snapshot when `--skip-quarantined` is active) |
| `import` | Import a knowledge base from a JSONL export file (accepts format_version 1 and 2). Re-embeds entries and rebuilds vector index. | `--input <PATH>` (required), `--skip-hash-validation`, `--force` (drop existing data including graph_edges, observations, cycle_events, and derived metric tables) |
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

SQLite local database (`unimatrix.db`). 21 tables. Schema version 25. Zero cloud dependency — all data stays on your machine.

### Vector Search

384-dimension HNSW vector index (in-memory, persisted to disk). Dot product similarity.

### Embedding

Local ONNX model (all-MiniLM-L6-v2) via ONNX Runtime. No API calls. MicroLoRA adaptive layer tunes embeddings to project-specific usage.

### Hook Integration

Two hook clients. The single-binary `hook` subcommand communicates with the running MCP server via Unix domain socket (UDS) IPC, with a sub-50ms round-trip budget. The pure-JS, zero-dependency hook client in the `@dug-21/unimatrix` npm package (`lib/hook-client/`) selects its transport from configuration: with remote config present it POSTs `HookRequest` frames to the server's `/observe` endpoint over HTTP with Bearer auth (no binary or ONNX model required); with no remote config it connects on Unix to the daemon's hook IPC socket (`unimatrix.sock`) over UDS, framing `HookRequest` messages byte-identically to the Rust hook (4-byte big-endian length-prefixed frames, 1 MiB cap). UDS local mode is Unix-only; Windows always uses the HTTP path. Across both transports the client streams transcript deltas on fire-and-forget events so the server's per-session buffer stays authoritative, is fail-open (exit 0 always), and keeps bounded client state (per-session `last_offset` plus a disk event queue) under `~/.unimatrix/{project-hash}/hook-client/`. Remote mode is configured via `init --remote` (see "Wire into your project").

### MCP Transport

Two transport surfaces: Unix Domain Socket (local) and HTTPS (network).

**UDS (local):** Daemon mode (default): Unimatrix runs as a persistent background daemon (`unimatrix serve --daemon`) that accepts MCP connections over a Unix Domain Socket (`unimatrix-mcp.sock`, 0600 permissions). Claude Code spawns a lightweight bridge process (the default `unimatrix` invocation) per session; the bridge connects stdin/stdout to the daemon's UDS socket. The daemon survives client disconnection — background tick, vector index, and all in-memory state persist across sessions. Up to 32 concurrent MCP sessions are supported.

**HTTPS (network):** When `[http] enabled = true` in config.toml, the server starts an HTTPS listener on the content port (default 8443). Any MCP-compatible client (Claude Code, Codex CLI, Gemini CLI) can connect over the network using a static bearer token for authentication. TLS termination via rustls is default-on when cert/key paths are configured; set `[tls] enabled = false` for reverse-proxy deployments. A path-dispatching tower service routes requests: `GET /health` (unauthenticated, returns version and schema version), `POST /observe` (remote telemetry — content-negotiated: `Accept: text/plain` returns server-formatted injection text for injection-bearing responses, while `application/json` or no `Accept` header returns the JSON envelope as the default), and `/*` (MCP protocol). When projects are registered, the MCP path carries the project slug — `/v1/{slug}/...` routes to that slug's isolated store, while `/v1/tools/...` (no slug) remains the single-project default; the slug is validated and resolved to a store at the routing edge before any store access, so cross-project mis-targeting is unrepresentable rather than merely rejected. Up to 32 concurrent HTTP sessions (configurable). See [docs/client-setup.md](docs/client-setup.md) for per-client connection instructions.

Foreground mode (container): `unimatrix serve --foreground` runs the full daemon (UDS listener, HTTP listener when enabled, tick loop, ML inference) as PID 1 without fork/setsid. Used by the container `ENTRYPOINT`. SIGTERM triggers graceful shutdown.

Stdio mode (explicit): `unimatrix serve --stdio` starts the server in foreground stdio mode. Identical to the pre-daemon behavior; use for development and testing. HTTP listener does not activate in stdio mode.

The hook IPC socket (`unimatrix.sock`) and the MCP socket (`unimatrix-mcp.sock`) are separate files. Hook IPC uses the existing length-framed binary protocol; MCP sessions use newline-delimited JSON-RPC over the MCP socket or HTTPS.

### Data Layout

```
~/.unimatrix/
  config.toml                # Global config (optional — see Configuration section)
~/.unimatrix/{project-hash}/
  config.toml                # Per-project config override (optional)
  unimatrix.db               # SQLite knowledge database (schema v27)
  unimatrix.pid              # PID file with flock advisory lock
  unimatrix.sock             # Unix domain socket for hook IPC
  unimatrix-mcp.sock         # Unix domain socket for MCP sessions (daemon mode)
  unimatrix.log              # Daemon stdout/stderr log (append mode)
  token                      # Bearer token for HTTPS auth (generated on first run, mode 0600)
  vector/
    unimatrix-vector.hnsw2   # HNSW graph
    unimatrix-vector.meta    # Index metadata
  hook-client/               # Remote HTTP hook client state (mode 0700; created by remote mode only)
    offsets/                 # Per-session transcript last_offset files
    queue/                   # Disk event queue for graceful degradation
    health.json              # Content-free health breadcrumb (no token, no transcript bytes)
~/.cache/unimatrix/models/   # ONNX model files (downloaded once)
```

In a multi-project container, each registered slug gets its own isolated subtree under the data volume — `/data/.unimatrix/{slug}/` holds that project's `unimatrix.db`, `vector/` index, hash chain, and analytics, with no cross-project sharing. A single-project install (no registered slugs) keeps the layout above unchanged.

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

## Security Model

### HTTP Authentication

HTTPS transport uses static bearer token authentication. A 32-byte (256-bit) cryptographically random token is generated on first run, stored at `{data_volume}/token` (mode 0600), and validated on every HTTP request using constant-time comparison (`subtle::ConstantTimeEq`). The `/health` endpoint is the sole unauthenticated path. The `BearerValidator` trait enables enterprise extension (JWT/OAuth) without modifying the core auth path.

### Certificate Pinning

The OSS trust model for the self-signed serving certificate is **fingerprint pinning**, not CA trust. On first boot the server generates a self-signed cert+key and computes its fingerprint as `sha256:<lowercase-hex>` over the served leaf certificate's DER bytes. The `client-bundle` command carries that fingerprint in the connection bundle; the client pins the exact certificate by it (custom `checkServerIdentity` compare, no certificate-authority path), and the fingerprint is computed byte-identically on both stacks. A presented certificate that does not match is rejected with a clear, diagnosable mismatch error. Rotating the certificate therefore requires re-emitting the bundle (`client-bundle`) and re-running `init --remote` on each client — see [docs/cert-rotation.md](docs/cert-rotation.md). CA-trust / SAN-based hostname validation is the enterprise/reverse-proxy path, not the OSS posture.

### Trust Hierarchy

Four-tier model: System > Privileged > Internal > Restricted. Unknown agents auto-enroll as Restricted on first contact (read + search only). `context_enroll` (Admin-only) promotes or modifies agent trust and capabilities. Protected agents: `system` and `human` cannot be modified. Self-lockout prevention: an admin cannot remove their own Admin capability.

### Capabilities

Four capabilities gate tool access: `read`, `write`, `search`, `admin`.

### Project Scoping

Token authorizes, slug scopes data, certificate secures transport — three separate concerns. In OSS single-tenant serving the slug is a **knowledge-integrity boundary, not an access-control boundary**: project identity is taken from the transport (the URL-path slug, validated at the routing edge against `^[a-z0-9][a-z0-9-]{0,62}$` and the reserved set `v1`/`health`/`observe`/`tools`), never from the request payload, so an agent has no field in which to name another project and a wrong slug cannot escape `/data/.unimatrix/{slug}/`. Binding each client to exactly one project removes the ability to mis-target a write into another project's hash chain. The `BearerValidator` trait remains the seam where enterprise adds per-project JWT/RBAC authorization on top of this scoping.

### Content Scanning

Every write operation (`context_store`, `context_correct`) scans content for injection patterns (~25+ patterns including prompt injection, system prompt overrides, and encoded payloads) and PII patterns (6+ patterns including emails, phone numbers, API keys, and credentials). Flagged content is rejected before storage.

### Transcript Handling

Streamed session transcript deltas (raw conversation bytes, possibly secret-bearing) accumulate in a per-session in-memory buffer only — they never reach disk, SQL, or logs. In-memory plus purge-on-close is the secrets guarantee: there is no content scanner for raw transcript. The buffer is bounded by `retention.transcript_buffer_max_bytes` (default 4 MiB; oldest bytes dropped on overflow) and purged at cycle review (after distillation) and the staleness sweep per `retention.transcript_retention`. To keep multi-turn buffers alive across the per-turn drain so the cycle-review distillation path is non-empty, a drained buffer is moved into a bounded held-buffer store rather than freed; it continues merging deltas, is re-adopted on re-registration, and is reclaimed by the held-count cap (`transcript_hold_max_sessions`) or the TTL stale sweep (`transcript_hold_ttl_secs`). Purging a non-empty buffer emits a content-free `transcript_session_purged` audit event (session ID, byte count, timestamp — never content). The buffer has two readers: the server-side PreCompact transcript-tail block, and the cycle-review snapshot seam that feeds the `transcript_candidates` distillation. Distilled candidates are response-transient — selected blocks ride the cycle-review response only and are never written to SQL, files, logs, or the memoized cycle-review record. A server crash loses in-flight transcript by design.

A content-free fold runs alongside the buffer as deltas arrive: it accumulates only a running byte total, a delta count, and per-class behavioral-signature match counts (configured via `[transcript_signals]` — see Configuration). The fold is a scalar counter, never a query over the assembled transcript, and carries no content field, so no transcript bytes escape through it; the resulting class counts are directional, not precise. The held-buffer store (`transcript_hold_max_sessions`/`transcript_hold_ttl_secs`) is a verified startup precondition for this fold: it must be wired and enabled, and startup fails loud if it is not, because the fold must survive to cycle review.

### Audit Trail

Append-only audit log records every operation with agent identity (who performed the action), session context (which session, feature cycle), and operation outcome (success/failure). Each audit row carries four additional compliance columns: `credential_type` (how the caller authenticated — `"none"` for stdio, `"static_token"` for HTTP bearer, `"jwt"` for enterprise JWT), `capability_used` (the capability gate evaluated for the tool call), `agent_attribution` (transport-attested client identity sourced from `clientInfo.name` at the MCP `initialize` handshake — not the spoofable agent-declared `agent_id`), and `metadata` (a JSON object carrying `client_type` and extensible for future keys). Append-only enforcement is maintained by DDL triggers on the `audit_log` table: UPDATE and DELETE operations are rejected at the database level.

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
