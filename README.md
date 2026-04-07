# Unimatrix

A self-learning knowledge engine for multi-agent software development. Unimatrix captures decisions, patterns, conventions, and lessons from real work, then delivers that context automatically via Claude Code's hook system — agents do not need to ask for it. Confidence scoring evolves from usage signals, so knowledge that helps gets boosted and knowledge that misleads gets downranked.

Built in Rust. Zero cloud dependency. Ships as a single binary MCP server.

Inspired by and building on patterns from [ruvnet's](https://github.com/ruvnet) work on [claude-flow](https://github.com/ruvnet/claude-flow) and ruvector — particularly the hook-driven delivery architecture and the insight that knowledge engines only matter if knowledge *reaches* agents without their cooperation. Unimatrix pairs that delivery philosophy with an auditable knowledge lifecycle: hash-chained corrections, confidence evolution from real usage signals, and self-maintaining structural coherence.

---

## Why Unimatrix

Multi-agent development creates knowledge that lives in context windows and dies with sessions. Decisions get re-made, patterns get re-discovered, lessons get re-learned.

- **Auditable Knowledge Lifecycle** — Every entry has a SHA-256 content hash. Corrections create hash-chained supersession links. An append-only audit log records every operation with agent identity and session context. You can trace how any piece of knowledge evolved.

- **Invisible Delivery** — Agents do not need to ask for context. Hook-driven integration (Cortical Implant) injects relevant expertise into every prompt automatically via Claude Code's lifecycle hooks. Knowledge reaches agents without their cooperation.

- **Self-Learning** — Confidence scoring evolves from real usage signals: access frequency, helpfulness votes, correction quality, creator trust, freshness decay, and co-access patterns. Entries that help get boosted; entries that mislead get downranked. Adaptive embeddings (MicroLoRA) tune search to project-specific usage patterns.

---

## Core Capabilities

Unimatrix provides these capabilities out of the box.

### Self-Learning Knowledge Engine

Captures decisions, patterns, conventions, procedures, and lessons from real feature work. Seven knowledge categories ensure entries surface in the right context. Confidence scoring combines usage signals, correction quality, creator trust, freshness, helpfulness, and co-access patterns into a composite score that evolves automatically. No manual curation required — the system learns what is useful from how knowledge is accessed and rated.

### Adaptive Embeddings (MicroLoRA)

All-MiniLM-L6-v2 ONNX model runs locally — no API calls, no cloud dependency. A MicroLoRA layer adapts frozen embeddings to project-specific usage patterns. Search relevance improves over time as the system learns which entries are accessed together. 384-dimension vectors with HNSW index for fast approximate nearest-neighbor search.

### Semantic Search with NLI Re-ranking

Natural language queries return entries ranked by NLI (Natural Language Inference) entailment score when an NLI cross-encoder model is present, or by a combination of semantic similarity, confidence score, and co-access affinity when it is not. The NLI re-ranker expands the HNSW candidate pool to `nli_top_k` (default 20), scores each `(query, candidate)` pair on the rayon pool, and sorts by entailment score descending before truncation — measuring whether an entry answers the query rather than merely sharing vocabulary with it. When the NLI model is absent or disabled (`nli_enabled = false`), search falls back to the existing confidence-aware ranking pipeline transparently. Filters by topic, category, tags, and status narrow results without losing semantic ranking. Near-duplicate detection (cosine similarity >= 0.92) prevents redundant entries at write time. Provenance boosting: `lesson-learned` entries get a small ranking boost in search results.

### Hook-Driven Invisible Delivery (Cortical Implant)

Automatic context injection on every prompt via the `UserPromptSubmit` hook. Six hook events drive the integration: `UserPromptSubmit`, `SubagentStart`, `PreCompact`, `PreToolUse`, `PostToolUse`, `Stop`. Subagent injection: when the SM spawns a subagent, the `SubagentStart` hook fires synchronously and injects relevant knowledge into the subagent context before its first token — the subagent does not need to call `context_briefing` manually. `UserPromptSubmit` injection requires at least 5 words in the prompt; shorter inputs (e.g., "yes", "ok continue") are recorded but produce no injection. Compaction resilience: `PreCompact` preserves critical context before Claude Code's context window compaction; the compaction payload is a flat indexed table of active entries (up to k=20) plus a session histogram summary. Closed-loop feedback: the `Stop` hook records session outcomes for confidence evolution. Sub-50ms round-trip budget per hook event. Disk-backed event queue for graceful degradation. Single binary — the `hook` subcommand connects to the running MCP server via Unix domain socket IPC.

### Retrospective Analysis

Analyzes session telemetry for a completed feature cycle and produces the `# Unimatrix Cycle Review —` report. 21 detection rules across 4 categories: agent behavior, friction points, session health, and scope indicators. Rules are domain-aware: each rule guards on `source_domain` as its first filter, so Claude Code rules never fire on events from other domains. A domain pack registry loaded at startup from TOML defines which event types, categories, and detection rules apply to each domain; the "claude-code" pack is always active with no config required. Historical baselines with outlier detection surface anomalies. Evidence synthesis produces actionable findings with supporting data. Lessons and patterns extracted from retrospectives are stored back in the knowledge base with de-duplication via correction chains.

The report header surfaces the feature goal, inferred cycle type (Design, Delivery, Bugfix, Refactor, or Unknown), attribution path used (cycle\_events-first, sessions.feature\_cycle legacy, or content-scan fallback), and an in-progress indicator when no `cycle_stop` event exists. A Phase Timeline table breaks the cycle into per-phase windows showing duration, pass count, agents spawned, records, knowledge throughput, and gate outcome. A "What Went Well" section surfaces non-outlier favorable baseline signals that were previously hidden. Per-finding evidence is rendered as relative-time burst notation (`Timeline: +0m(N) +12m(N▲) …`) rather than raw epoch values. The Knowledge Reuse section splits served entries into cross-feature (from prior cycles) and intra-cycle buckets with a top-entry breakdown. Recommendations appear immediately after the header, before all other sections.

### Contradiction Detection and NLI Edge Classification

After each `context_store`, a fire-and-forget background task runs the NLI cross-encoder on the new entry against its top HNSW neighbors and writes `Contradicts` or `Supports` edges to the knowledge graph (`GRAPH_EDGES`) with `source='nli'`. This replaces the lexical `conflict_heuristic` for new edge creation. A circuit breaker (`max_contradicts_per_tick`, default 10) caps edges written per store call to prevent a single noisy entry from flooding the graph. When NLI is absent, the existing cosine heuristic remains as fallback. Contradictions surface in `context_status` health reports and reduce the coherence health metric (lambda), prompting review.

### Correction Chains with Audit Trails

`context_correct` creates a new entry and deprecates the original, linking them with SHA-256 content hashes (`previous_hash` chain). The append-only audit log records every operation — store, correct, deprecate, quarantine, enroll — with agent identity, session context, and operation outcome. Correction chains are tamper-evident: any break in the hash chain is detectable.

### Coherence Gate (Lambda Health Metric)

Lambda is a composite structural integrity metric [0.0, 1.0] computed from three dimensions: graph quality (weight 0.46 — is the vector index structurally sound?), contradiction density (weight 0.31 — how many unresolved contradictions exist?), and embedding consistency (weight 0.23 — do entries have valid, current embeddings?). When lambda drops below 0.8, maintenance is recommended. A background tick handles maintenance automatically — confidence refresh, graph compaction, co-access cleanup.

`context_status` also reports six graph cohesion metrics computed per-call from the `GRAPH_EDGES` table: connectivity rate (fraction of active entries with at least one non-bootstrap edge), isolated entry count, cross-category edge count, Supports edge count, mean entry degree (in+out), and NLI-inferred edge count (`source='nli'`). These metrics are informational — they do not feed into lambda — but let operators verify whether automated NLI edge inference is producing a connected, cross-category graph that PPR can exploit. Summary format includes a single "Graph cohesion:" line; Markdown format includes a `### Graph Cohesion` sub-section within the Coherence block.

### Content Scanning

Every `context_store` and `context_correct` call scans content for injection patterns (~25+ patterns including prompt injection attempts, system prompt overrides, and encoded payloads) and PII patterns (6+ patterns including emails, phone numbers, API keys, and credentials). Flagged content is rejected with a descriptive error before storage.

### Agent Trust Hierarchy

Four-tier trust model: System > Privileged > Internal > Restricted. Four capabilities gate tool access: `read`, `write`, `search`, `admin`. Unknown agents auto-enroll as Restricted (read + search only) on first contact. Protected agents: `system` and `human` cannot be modified. Self-lockout prevention: an admin cannot remove their own Admin capability. `context_enroll` (Admin-only) manages agent trust levels and capabilities at runtime.

### Knowledge Effectiveness Analysis

Per-entry utility scoring from injection logs and session outcomes. Confidence calibration validation — does predicted quality match actual usefulness? Dead knowledge detection — entries that are never accessed after initial storage.

---

## Getting Started

### Install via npm

```bash
npm install @dug-21/unimatrix
```

Prerequisite: Node.js >= 18.

The npm package includes pre-built binaries for Linux x64. The embedding model downloads automatically on first run (or via `npx unimatrix model-download`).

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

Binary at `target/release/unimatrix-server`.

### Configure MCP Server

Add to `.claude/settings.json`:

```json
{
  "mcpServers": {
    "unimatrix": {
      "command": "npx",
      "args": ["unimatrix"]
    }
  }
}
```

Or for build-from-source:

```json
{
  "mcpServers": {
    "unimatrix": {
      "command": "/path/to/unimatrix-server"
    }
  }
}
```

### Configure Hooks

Add to `.claude/settings.json`:

```json
{
  "hooks": {
    "UserPromptSubmit": [{ "command": "npx unimatrix hook UserPromptSubmit" }],
    "SubagentStart": [{ "command": "npx unimatrix hook SubagentStart" }],
    "PreCompact": [{ "command": "npx unimatrix hook PreCompact" }],
    "PreToolUse": [{ "command": "npx unimatrix hook PreToolUse" }],
    "PostToolUse": [{ "command": "npx unimatrix hook PostToolUse" }],
    "Stop": [{ "command": "npx unimatrix hook Stop" }]
  }
}
```

### Cold Start

A fresh knowledge base returns empty results. Use `/uni-seed` to populate foundational knowledge entries. Use `/uni-init` to configure CLAUDE.md awareness and get agent recommendations.

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

**Get a knowledge briefing for a feature phase:**
```
context_briefing(topic: "crt-027", max_tokens: 1000)
```

---

## Tips for Maximum Value

1. **Start a new session per feature cycle.** Context window pollution across features reduces knowledge quality. Each feature cycle (e.g., `col-015`) should use a fresh Claude Code session.

2. **Use feature cycle naming.** Phase prefix + number: `col-015`, `nan-005`, `vnc-012`. Used in commits, branches, issue tracking, and as the `feature_cycle` parameter in MCP tool calls.

3. **Follow commit message format.** `{prefix}: {description} (#{issue})` — see `/uni-git` for the prefix table.

4. **Category discipline matters.** The right category determines retrieval quality. Decisions (`decision`) are not conventions (`convention`); procedures (`procedure`) are not patterns (`pattern`). Miscategorized entries surface in wrong contexts during semantic search.

5. **Hook latency budget.** Hooks have a sub-50ms round-trip budget. Heavy blocking operations in hook handlers degrade the user experience.

6. **Cold start: use `/uni-seed`.** A fresh knowledge base returns empty search results. `/uni-seed` populates foundational entries before relying on search.

7. **Near-duplicate detection.** Entries with cosine similarity >= 0.92 to existing entries are rejected as duplicates. Rephrase if a legitimate distinct entry is rejected.

8. **Daemon log file is not rotated.** The daemon writes stdout/stderr to `~/.unimatrix/{hash}/unimatrix.log` in append mode. On long-running projects, monitor this file's size and truncate or archive it manually as needed.

9. **NLI model must be downloaded separately.** The NLI cross-encoder model is not bundled and is not downloaded automatically. Run `unimatrix model-download --nli` once after installation. The command prints the SHA-256 hash of the downloaded file — copy that hash into `nli_model_sha256` in your `[inference]` config. The server degrades gracefully to cosine-only search if the model is absent; no error is returned to callers.

10. **Pin `nli_model_sha256` in production.** A replaced or tampered NLI model file is an undetectable model-poisoning attack. Setting `nli_model_sha256` in `[inference]` config causes the server to verify the model file at startup; a mismatch aborts NLI loading (falls back to cosine) and logs a security warning. Production deployments should always set this field.

11. **Run retrospectives to advance the retention window.** Activity data (observations, query_log, sessions) is retained indefinitely for any cycle that has not been reviewed with `context_cycle_review`. The retention K-window only advances past cycles that have a stored review. If retrospectives are skipped, the retention window stalls and raw signal data accumulates without bound. Call `context_cycle_review` after each cycle completes to allow the GC pass to prune older data. `context_status` shows `pending_cycle_reviews` — the list of cycles awaiting review.

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
| `collaborative` | Team-built knowledge, dev/research (default) | 168 h (1 week) |
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
              "pattern", "procedure", "duties", "reference"]
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
# Number of threads dedicated to ML inference (ONNX embedding, NLI cross-encoder, GNN).
# Default: (num_cpus / 2).max(4).min(8) — at least 4 threads, at most 8.
# Valid range: [1, 64]. Out-of-range value aborts startup with a structured error.
rayon_pool_size = 4

# NLI cross-encoder re-ranking and contradiction detection.
# nli_enabled: set to false to disable NLI and use cosine-only search (default: true).
nli_enabled = true
# nli_model_name: "minilm2" (cross-encoder/nli-MiniLM2-L6-H768, ~85MB, default)
#                 "deberta" (cross-encoder/nli-deberta-v3-small, ~180MB)
nli_model_name = "minilm2"
# nli_model_sha256: SHA-256 hash of the downloaded model file. Should be set in
# production — mismatch causes NliServiceHandle to fail and fall back to cosine.
# Obtain the hash by running: unimatrix model-download --nli
nli_model_sha256 = "<hash from model-download --nli>"
# nli_top_k: HNSW candidate pool size for search re-ranking (default: 20, range [1,100]).
nli_top_k = 20
# nli_post_store_k: neighbor count for post-store contradiction detection (default: 10).
nli_post_store_k = 10
# nli_entailment_threshold: minimum entailment score to write a Supports edge (default: 0.6).
nli_entailment_threshold = 0.6
# nli_contradiction_threshold: minimum contradiction score to write a Contradicts edge (default: 0.6).
nli_contradiction_threshold = 0.6
# max_contradicts_per_tick: max edges written per context_store call (default: 10, range [1,100]).
max_contradicts_per_tick = 10
# nli_auto_quarantine_threshold: NLI-origin Contradicts edges trigger auto-quarantine only
# when all edge scores exceed this value. Must be > nli_contradiction_threshold (default: 0.85).
nli_auto_quarantine_threshold = 0.85

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
| `context_search` | Search for relevant context using natural language. Returns semantically similar entries ranked by relevance. When a `session_id` is provided and the session has prior stores, results whose category matches recent session activity receive a small affinity boost (`w_phase_histogram = 0.02`). Cold-start sessions (no prior stores) produce identical results to sessions without a session ID. | When you need to find patterns, conventions, or decisions related to a concept. Use when you do NOT know exactly what you are looking for. Key params: `query` (required), `category`, `topic`, `tags`, `k` (default 5), `helpful`, `session_id`. |
| `context_lookup` | Look up context entries by exact filters. Returns entries matching topic, category, tags, status, or ID. | When you KNOW what you are looking for — a specific feature's entries, a category listing, or a known ID. Key params: `topic`, `category`, `tags`, `id`, `status`, `limit` (default 10). |
| `context_get` | Get a specific context entry by its ID. | When you have an entry ID from a previous search or lookup result and need the full content. Key params: `id` (required), `helpful`. |
| `context_store` | Store a new context entry with duplicate detection and content scanning. Each successful non-duplicate store increments the per-session category histogram used by `context_search` for implicit session affinity ranking. | When you discover a pattern, convention, decision, or lesson worth preserving. Key params: `content` (required), `topic` (required), `category` (required), `tags`, `title`, `feature_cycle`. |
| `context_correct` | Correct an existing entry. Deprecates the original and creates a new entry with a hash-chain link. | When an entry contains wrong or outdated information that should be superseded (not just hidden). Key params: `original_id` (required), `content` (required), `reason`. |
| `context_deprecate` | Mark an entry as outdated. Entry remains accessible but excluded from default search/lookup. | When knowledge is no longer relevant but should not be deleted (historical record). Key params: `id` (required), `reason`. |
| `context_quarantine` | Quarantine or restore an entry. Quarantined entries are excluded from search and lookup. **Admin only.** | When an entry is suspicious, invalid, or harmful and should be isolated. Use `action: "restore"` to undo. Key params: `id` (required), `action` ("quarantine" or "restore"), `reason`. |
| `context_status` | Get knowledge base health metrics. Shows entry counts, distributions, correction chains, coherence score, security metrics, graph cohesion metrics (connectivity rate, isolated entry count, cross-category edge count, Supports edge count, mean entry degree, and NLI-inferred edge count), per-category lifecycle labels (adaptive vs pinned), `pending_cycle_reviews` (cycle IDs that have started within the retention window but have no stored cycle review yet — always computed), and a `curation_health` aggregate block (per-cycle correction rate mean/stddev, source breakdown as agent% and human%, orphan deprecation ratio mean/stddev, and trend direction when at least 6 cycles of snapshot data are available). **Admin only.** | When you need to assess knowledge base health or inspect whether NLI-based edge inference is producing a connected, cross-category graph, identify cycles awaiting retrospective review before signals can be purged, or review curation behavior trends across recent cycles. The `maintain` parameter is accepted but silently ignored — a background tick handles maintenance automatically. Key params: `topic`, `category`, `check_embeddings`. |
| `context_briefing` | Get a knowledge index for a topic or task. Returns a `CONTEXT_GET_INSTRUCTION` header line followed by up to 20 active entries as a flat indexed table (columns: row, id, topic, category, confidence, snippet). Deprecated entries are suppressed. Query derived from: (1) explicit `task` param, (2) active cycle `goal` when set (stored via `context_cycle(start, goal: ...)`), (3) `topic` param fallback. `role` param accepted for backward compatibility but ignored. `UNIMATRIX_BRIEFING_K` env var is deprecated on this path — default k=20 cannot be reduced via env var. | At the start of a phase or task to get oriented. Call after each `context_cycle(type: "phase-end", ...)` to load the knowledge package for the next phase. Gated on `mcp-briefing` feature flag. Key params: `topic` (required fallback), `task`, `session_id`, `k` (default 20), `max_tokens` (default 3000, range 500-10000). |
| `context_enroll` | Enroll or update an agent's trust level and capabilities. **Admin only.** | When managing agent permissions. Protected agents (`system`, `human`) cannot be modified. Self-lockout prevention active. Key params: `target_agent_id` (required), `trust_level` (required), `capabilities` (required). |
| `context_cycle` | Signal feature cycle lifecycle events: start, phase transitions, and stop. Records one append-only event row per call in `CYCLE_EVENTS`; tags subsequent `context_store` entries with the active phase. | At cycle start/stop and at each phase boundary. Key params: `type` (required: `"start"` \| `"phase-end"` \| `"stop"`), `topic` (required), `phase`, `outcome`, `next_phase`, `agent_id`, `goal` (optional, `start` only: 1–2 sentence plain-text statement of feature intent; used as the step-2 query signal by `context_briefing` and hook injection when no explicit `task` is provided; max 1 024 bytes). Phase tokens must be lowercase with no spaces (canonical set: `scope`, `design`, `implementation`, `testing`, `gate-review`). |
| `context_cycle_review` | Analyze observation data for a work cycle. Parses session telemetry, detects hotspots, computes metrics, and renders the `# Unimatrix Cycle Review —` report. Results are memoized: the first call for a cycle computes and stores the full report; subsequent calls return the stored record without recomputation. Use `force=true` to recompute and overwrite the stored record. When the stored record was computed with an older schema version, a version advisory is included in the response. | After a work cycle completes, to extract patterns and lessons. Key params: `feature_cycle` (required), `evidence_limit`, `force` (bool, default false — when true, forces recomputation even if a stored record exists), `format` ("markdown" default, "json"). Report includes: header with goal, cycle type, attribution path, and in-progress indicator; Recommendations (position 2); Phase Timeline table (per-phase duration, passes, agents, knowledge throughput, gate outcome) when `CYCLE_EVENTS` data exists; "What Went Well" section from favorable baseline signals; per-finding burst-notation evidence; Knowledge Reuse split into cross-feature and intra-cycle buckets; `curation_health` block with this cycle's raw correction counts (total, agent-attributed, human-attributed) and orphan deprecation count — plus σ deviation from the rolling 10-cycle baseline when at least 3 prior cycles have snapshot data (annotated with history length, e.g., `"2.1σ (4 cycles of history)"`; raw counts only on cold start). JSON format exposes `goal`, `cycle_type`, `attribution_path`, `is_in_progress`, and `phase_stats` fields. |

**`context_search` vs `context_lookup`**: `context_search` uses semantic similarity (natural language). `context_lookup` uses exact filters (topic, category, tags, status). Use search when exploring; use lookup when you know what you want.

**`context_correct` vs `context_deprecate` vs `context_quarantine`**: `context_correct` supersedes with a new version (hash-chained). `context_deprecate` marks as outdated (no replacement). `context_quarantine` isolates from all results (Admin-only, reversible).

---

## Skills Reference

Unimatrix includes 14 Claude Code skills. Skills are platform-native `/command` files installed via the npm package or by copying `.claude/skills/` directories to the target repository.

Skills that interact with the MCP server require the server to be running and configured.

| Skill | Purpose | When to Use |
|-------|---------|-------------|
| `/uni-query-patterns` | Search for patterns, procedures, and conventions before work. (MCP) | Before designing or implementing any component. |
| `/uni-store-adr` | Store an architectural decision record in Unimatrix. (MCP) | After each design decision during architecture work. |
| `/uni-store-pattern` | Store a reusable implementation pattern. (MCP) | When you discover a gotcha, trap, or reusable solution. |
| `/uni-store-procedure` | Store or update a technical procedure (how-to). (MCP) | During retrospectives when a technique has evolved. |
| `/uni-store-lesson` | Store a lesson learned from a failure or unexpected issue. (MCP) | After bugfixes, gate failures, or rework cycles. |
| `/uni-record-outcome` | Record a feature or bugfix outcome. (MCP) | At the end of every session (design, delivery, bugfix, retrospective). |
| `/uni-knowledge-search` | Interactive semantic search across knowledge. (MCP) | When exploring a topic or looking for related entries. |
| `/uni-knowledge-lookup` | Interactive deterministic lookup by exact filters. (MCP) | When you know what you want — a specific feature, category, or ID. |
| `/uni-review-pr` | PR security review and merge readiness check. | After delivery or bugfix opens a PR. Can be invoked standalone. |
| `/uni-retro` | Post-merge retrospective — extract patterns, procedures, lessons. (MCP) | After a feature PR is merged. |
| `/uni-git` | Git workflow conventions (branch naming, commit prefixes, PR templates). | For consistent git conventions. Contributor/developer-focused. |
| `/uni-release` | Version bump, changelog generation, tag, and release pipeline. | When creating a new release. |
| `/uni-init` | Initialize Unimatrix in a repository — CLAUDE.md setup + agent recommendations. | First-time setup of a repo to use Unimatrix. |
| `/uni-seed` | Populate foundational knowledge through human-directed exploration. (MCP) | After installation, to seed the knowledge base before relying on search. |

---

## Knowledge Categories

Unimatrix uses 7 built-in knowledge categories. Category discipline matters for retrieval quality — miscategorized entries surface in wrong contexts during semantic search.

| Category | Description | Example |
|----------|-------------|---------|
| `lesson-learned` | Lessons from failures, gate rejections, unexpected issues. | "Always verify hook latency after adding new UDS handlers — we hit 200ms in col-008." |
| `decision` | Architectural and design decisions (ADRs). | "Use SQLite for local storage — single-file, zero cloud dependency, bundled via rusqlite." |
| `convention` | Project conventions and rules agents should follow. | "All MCP tool handlers follow the execution order: identity -> capability -> validation -> category -> scanning -> business logic -> format -> audit." |
| `pattern` | Reusable implementation patterns, gotchas, and solutions. | "Do not hold Store lock across async boundaries — use spawn_blocking for all Store calls." |
| `procedure` | Step-by-step technical procedures (how-to). | "How to add a new MCP tool: 1. Define params struct, 2. Implement handler, 3. Add validation, 4. Add audit event." |
| `duties` | Role duties for `context_briefing` orientation. | "Architect duties: read SCOPE.md, decompose into components, define integration surface, produce ADRs." |
| `reference` | General reference material. | "ONNX Runtime 1.20.x compatibility matrix for supported platforms." |

The `outcome` category has been retired: cycle outcomes are now recorded as structured events in `CYCLE_EVENTS` via `context_cycle`, not as knowledge base entries. Attempting to store an entry with `category = "outcome"` returns an `InvalidCategory` error.

The default category list can be replaced at startup via `[knowledge] categories` in `~/.unimatrix/config.toml`. The 7 built-in categories cover the primary use cases for software delivery; operators targeting other domains can supply a domain-appropriate list.

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
| `model-download` | Download ONNX model(s) to cache. Without flags, downloads the embedding model (used by npm postinstall). With `--nli`, downloads the configured NLI cross-encoder model and prints its SHA-256 hash for config pinning. | `--nli` (NLI model), `--nli-model minilm2\|deberta` (model variant, default `minilm2`) |
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
| `unimatrix-server` | MCP server — tool handlers, hook IPC, agent registry, audit, content scanning |

---

## Security Model

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

### NLI Model Integrity

SHA-256 hash pinning for the NLI cross-encoder model file. When `nli_model_sha256` is set in `[inference]` config, the model file is verified before the ONNX session is constructed. A mismatch transitions `NliServiceHandle` to Failed, logs a security warning, and falls back to cosine-only search — the server continues operating. A tampered model file without hash pinning is an undetectable model-poisoning attack. Production deployments must set `nli_model_sha256`; obtain the hash by running `unimatrix model-download --nli`.

---

## Acknowledgments

Unimatrix's hook-driven delivery architecture draws directly from [ruvnet's](https://github.com/ruvnet) pioneering work on [claude-flow](https://github.com/ruvnet/claude-flow) (Ruflo) and ruvector. The core insight — that agent knowledge systems only deliver value when knowledge reaches agents automatically, without requiring explicit tool calls — shaped the entire Cortical Implant design. The adaptive embedding pipeline builds on patterns explored in ruvector's vector search architecture. We learned from both systems and are grateful for the open exploration that made this work possible.

---

## License

MIT OR Apache-2.0
