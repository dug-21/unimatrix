# Unimatrix

A self-learning expertise engine for multi-agent software development. Unimatrix captures the knowledge that emerges from doing work — decisions, patterns, conventions, and lessons — and makes it trustworthy, retrievable, and ever-improving. Agents don't need to ask for context: Unimatrix delivers it automatically via Claude Code's hook system, injecting relevant expertise into every prompt.

Built in Rust. Zero cloud dependency. Ships as a single binary MCP server.

Inspired by and building on patterns from [ruvnet's](https://github.com/ruvnet) work on [claude-flow](https://github.com/ruvnet/claude-flow) and ruvector — particularly the hook-driven delivery architecture and the insight that knowledge engines only matter if knowledge *reaches* agents without their cooperation. Unimatrix pairs that delivery philosophy with an auditable knowledge lifecycle: hash-chained corrections, confidence evolution from real usage signals, and self-maintaining structural coherence.

---

## Features

### Knowledge Engine

- **17-table storage backend** on [redb](https://github.com/cberner/redb) — entries, 5 secondary indexes, vector map, counters, agent registry, audit log, feature tracking, co-access graph, outcome index, observation metrics, signal queue, sessions, and injection log
- **384-dimension vector search** via HNSW (dot product similarity, ef_construction=200, 16 max connections) with filtered search support
- **Local ONNX embedding pipeline** — all-MiniLM-L6-v2 runs on-device via ONNX Runtime. No API calls, no cloud dependency
- **Adaptive embeddings** — MicroLoRA layer (rank 2-8) adapts frozen ONNX embeddings to project-specific usage patterns via contrastive learning (InfoNCE loss), with EWC++ regularization to prevent catastrophic forgetting
- **Near-duplicate detection** — cosine similarity threshold (0.92) prevents redundant entries at write time
- **Schema migration** — scan-and-rewrite migrations triggered automatically on database open. 5 schema versions shipped without breaking changes

### 12 MCP Tools

| Tool | Purpose |
|------|---------|
| `context_search` | Semantic search — natural language queries ranked by similarity + confidence + co-access affinity |
| `context_lookup` | Deterministic retrieval by topic, category, tags, status, or ID |
| `context_get` | Full entry by ID |
| `context_store` | Store new knowledge with duplicate detection and content scanning |
| `context_correct` | Supersede an entry — creates a hash-chained correction link |
| `context_deprecate` | Mark knowledge as outdated |
| `context_quarantine` | Isolate suspicious or invalid entries from search results |
| `context_status` | Health metrics, coherence score, security audit, optional self-maintenance |
| `context_briefing` | Role + task oriented context delivery — duties, conventions, and relevant patterns compiled into one response |
| `context_enroll` | Manage agent trust levels and capabilities at runtime |
| `context_retrospective` | Analyze session telemetry — hotspot detection, evidence synthesis, actionable recommendations |

All tools accept `format`: `summary`, `markdown`, or `json`.

### Confidence & Ranking

Knowledge isn't just stored — it's scored. A six-component additive weighted composite determines each entry's confidence:

| Signal | Weight | How it works |
|--------|--------|-------------|
| Base quality | 0.18 | Status-dependent (Active=0.5, Proposed=0.5, Deprecated=0.2, Quarantined=0.1) |
| Usage frequency | 0.14 | Log-transformed access count, capped at 50 |
| Freshness | 0.18 | Exponential decay with 1-week half-life |
| Helpfulness | 0.14 | Wilson score lower bound (95% CI) with a 5-vote minimum before deviating from neutral |
| Correction quality | 0.14 | Rewards 1-2 corrections (refinement), penalizes 6+ (instability) |
| Creator trust | 0.14 | human=1.0, system=0.7, agent=0.5 |
| Co-access affinity | 0.08 | Query-time boost for entries frequently retrieved together |

Search re-ranking: `0.85 * similarity + 0.15 * confidence + co-access boost (max 0.03) + provenance boost (0.02 for lesson-learned entries)`.

All scoring runs at f64 precision. The Wilson score guard prevents gaming — you can't boost an entry by spamming helpful votes until real usage data accumulates.

### Coherence Gate

A composite health metric (lambda, 0.0-1.0) monitors knowledge base structural integrity across four dimensions:

| Dimension | Weight | What it catches |
|-----------|--------|----------------|
| Confidence freshness | 0.35 | Entries with stale time-decay components |
| Graph quality | 0.30 | Orphaned HNSW nodes from re-embeddings |
| Contradiction density | 0.20 | Rising ratio of quarantined entries |
| Embedding consistency | 0.15 | Drift between stored and re-computed vectors |

When lambda drops below 0.8, `context_status` surfaces maintenance recommendations. With `maintain=true`, it runs inline: batch confidence refresh, HNSW graph compaction (build-new-then-swap), and co-access cleanup. No background threads, no timers — maintenance happens on-demand within the request.

### Security

- **Agent registry** — 4-tier trust hierarchy (System > Privileged > Internal > Restricted) with per-tool capability checks
- **Auto-enrollment** — unknown agents get Restricted access (read + search only) until explicitly promoted
- **Content scanning** — 25+ injection patterns (instruction override, role impersonation, delimiter injection, encoding evasion) and 6+ PII patterns (emails, phone numbers, SSNs, API keys, tokens) checked on every write
- **Append-only audit log** — every operation recorded with agent identity, session context, and outcome
- **Hash-chained corrections** — SHA-256 content hashes with `previous_hash` links create a tamper-evident correction history
- **Input validation** — length limits, character filtering, and pattern matching on all tool parameters
- **Output framing** — read tools separate data from instructions to prevent confused-deputy attacks
- **Protected agents** — `system` and `human` identities are immutable with self-lockout prevention
- **UDS authentication** — peer credential (UID) verification on Unix domain socket connections

### Hook-Driven Delivery (Cortical Implant)

The `unimatrix-server hook` subcommand acts as a universal router for Claude Code lifecycle events. One binary, configured once in `.claude/settings.json`, dispatches all hook events over a Unix domain socket to the running MCP server:

- **Automatic context injection** — `UserPromptSubmit` hook queries Unimatrix for knowledge relevant to the current prompt. Top entries (within a ~350 token budget) are injected into Claude's context before the agent sees it
- **Compaction resilience** — `PreCompact` hook calls `context_briefing` to re-inject critical knowledge (active decisions, conventions, current feature context) when Claude Code compresses the conversation window
- **Closed-loop confidence** — `Stop`/`SubagentStop` hooks determine session outcomes (success/rework/abandoned) and feed implicit helpfulness signals back to the confidence pipeline. Successful sessions bulk-apply `helpful=true` to injected entries; rework is flagged for human review, never auto-downweighted
- **Session lifecycle** — `SessionStart`/`SessionEnd` hooks persist session records with feature attribution, injection history, and outcome tracking. Survives server restart. Stale sessions swept after 24 hours

Transport: length-prefixed JSON over Unix domain socket. Sub-50ms round-trip budget. Graceful degradation via disk-backed event queue when the server is unreachable.

### Observation & Retrospective Intelligence

The `unimatrix-observe` crate provides a full observation pipeline that analyzes Claude Code session telemetry:

- **21 detection rules** across 4 categories — agent behavior (7), friction points (4), session health (5), scope indicators (5)
- **Historical baselines** — mean + stddev across prior feature cycles with 1.5-sigma outlier detection. Four arithmetic guard modes handle edge cases (NoVariance, NewSignal)
- **Evidence synthesis** — timestamp clustering (30s sliding window), sequence pattern extraction, top-file ranking, and human-readable narrative generation
- **Actionable recommendations** — template-based suggestions for recognized hotspot patterns (permission retries, coordinator respawns, sleep workarounds, compile cycles)
- **Lesson-learned auto-persistence** — retrospective findings are automatically stored as knowledge entries (`category: lesson-learned`) with system trust, embedded for semantic search, and provenance-boosted in future queries
- **De-duplication** — repeated retrospectives for the same feature cycle supersede prior entries via correction chains

This closes the learning loop: observation feeds retrospective analysis, which produces lesson-learned entries, which rank higher in future searches, which influence future agent behavior.

---

## Architecture

### 8 Crates

| Crate | Role |
|-------|------|
| `unimatrix-store` | redb storage engine — 17 tables, 5 schema versions, secondary indexes, scan-and-rewrite migration |
| `unimatrix-vector` | HNSW vector index — 384d embeddings, dot product, filtered search, build-new-then-swap compaction |
| `unimatrix-embed` | ONNX embedding pipeline — all-MiniLM-L6-v2, lazy background loading, HuggingFace Hub model caching |
| `unimatrix-core` | Shared traits (`EntryStore`, `VectorStore`, `IndexStore`), async wrappers, unified error types |
| `unimatrix-engine` | Business logic — confidence scoring, co-access, project detection, wire protocol, UDS transport, auth |
| `unimatrix-adapt` | Adaptive embeddings — MicroLoRA, InfoNCE contrastive learning, EWC++ regularization, prototype centroids |
| `unimatrix-observe` | Observation pipeline — JSONL parsing, feature attribution, 21 detection rules, baselines, synthesis, narratives |
| `unimatrix-server` | Binary — MCP server (stdio) + hook subcommand (UDS). 12 tools, agent registry, content scanning, session lifecycle |

### Data Layout

```
~/.unimatrix/{project-hash}/
  unimatrix.redb             # 17-table knowledge database
  unimatrix.pid              # PID file with flock advisory lock
  unimatrix.sock             # Unix domain socket for hook IPC
  vector/
    unimatrix-vector.hnsw2   # HNSW graph
    unimatrix-vector.meta    # index metadata
  observation/
    {session-id}.jsonl        # per-session telemetry

~/.cache/unimatrix/models/   # ONNX model files (downloaded once)
```

---

## Getting Started

### Prerequisites

- **Rust 1.89+** (edition 2024)
- **ONNX Runtime 1.20.x** shared library

#### Installing ONNX Runtime

**macOS (Homebrew):**
```bash
brew install onnxruntime
export ORT_LIB_LOCATION=$(brew --prefix onnxruntime)/lib
export ORT_PREFER_DYNAMIC_LINK=1
```

**Linux (manual):**
```bash
wget https://github.com/microsoft/onnxruntime/releases/download/v1.20.1/onnxruntime-linux-x64-1.20.1.tgz
tar xzf onnxruntime-linux-x64-1.20.1.tgz
sudo cp onnxruntime-linux-x64-1.20.1/lib/* /usr/local/lib/
sudo ldconfig
export ORT_LIB_LOCATION=/usr/local/lib
export ORT_PREFER_DYNAMIC_LINK=1
```

**Devcontainer:** ONNX Runtime is pre-installed. `.cargo/config.toml` sets the environment automatically.

### Build

```bash
cargo build --release
```

Binary: `target/release/unimatrix-server`

### Configure MCP

Add to your project's `.claude/settings.json`:

```json
{
  "mcpServers": {
    "unimatrix": {
      "command": "/path/to/unimatrix-server",
      "args": []
    }
  }
}
```

### Configure Hooks

Add the cortical implant hooks to `.claude/settings.json`:

```json
{
  "hooks": {
    "UserPromptSubmit": [
      { "command": "/path/to/unimatrix-server hook UserPromptSubmit" }
    ],
    "PreCompact": [
      { "command": "/path/to/unimatrix-server hook PreCompact" }
    ],
    "PreToolUse": [
      { "command": "/path/to/unimatrix-server hook PreToolUse" }
    ],
    "PostToolUse": [
      { "command": "/path/to/unimatrix-server hook PostToolUse" }
    ],
    "Stop": [
      { "command": "/path/to/unimatrix-server hook Stop" }
    ]
  }
}
```

### Interact

Claude Code discovers the `context_*` tools automatically. The server instructions guide agents to search before implementing and store reusable findings:

```
# Search for relevant knowledge
context_search(query: "error handling conventions", category: "convention")

# Store a decision
context_store(
  topic: "authentication",
  category: "decision",
  content: "Use JWT with RS256 for API auth. Tokens expire after 1 hour.",
  title: "Auth token strategy"
)

# Get a role briefing
context_briefing(role: "developer", task: "implement user registration endpoint")

# Correct outdated knowledge
context_correct(
  original_id: 42,
  content: "Use JWT with ES256 — 3x faster verification on our infra.",
  reason: "Benchmarked RS256 vs ES256"
)

# Analyze a feature cycle
context_retrospective(feature_cycle: "col-010b", evidence_limit: 3)
```

### CLI

```
unimatrix-server [--project-dir <PATH>] [--verbose]
unimatrix-server hook <EVENT>
```

| Flag | Effect |
|------|--------|
| `--project-dir <PATH>` | Override automatic project root detection |
| `--verbose`, `-v` | Debug-level logging (stderr) |
| `hook <EVENT>` | Handle a Claude Code lifecycle hook event |

---

## Tests

```bash
cargo test
```

1,500+ tests across 8 crates. Embedding model tests are gated behind `#[ignore]` — run with `cargo test -- --ignored` to include them.

---

## Knowledge Categories

Eight built-in categories: `outcome`, `lesson-learned`, `decision`, `convention`, `pattern`, `procedure`, `duties`, `reference`. The allowlist is extensible at runtime.

---

## Project Structure

```
crates/
  unimatrix-store/       # redb storage engine
  unimatrix-vector/      # HNSW vector index
  unimatrix-embed/       # ONNX embedding pipeline
  unimatrix-core/        # shared traits + async wrappers
  unimatrix-engine/      # confidence, co-access, wire protocol, auth
  unimatrix-adapt/       # adaptive embedding (MicroLoRA, EWC++)
  unimatrix-observe/     # observation pipeline + retrospective intelligence
  unimatrix-server/      # MCP server + hook binary
product/
  PRODUCT-VISION.md      # full roadmap
  features/              # per-feature documentation
  research/              # research spikes and analysis
patches/
  anndists/              # local fix for anndists edition 2024 compat
```

---

## Acknowledgments

Unimatrix's hook-driven delivery architecture draws directly from [ruvnet's](https://github.com/ruvnet) pioneering work on [claude-flow](https://github.com/ruvnet/claude-flow) (Ruflo) and ruvector. The core insight — that agent knowledge systems only deliver value when knowledge reaches agents automatically, without requiring explicit tool calls — shaped the entire Cortical Implant design. The adaptive embedding pipeline builds on patterns explored in ruvector's vector search architecture. We learned from both systems and are grateful for the open exploration that made this work possible.

---

## License

MIT OR Apache-2.0
