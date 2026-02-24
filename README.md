# Unimatrix

A self-learning context engine that serves as the knowledge backbone for multi-agent development orchestration. Unimatrix accumulates conventions, decisions, patterns, and process intelligence across feature cycles, then delivers the right context to the right agent at the right workflow moment.

## What's Built

Unimatrix ships as an MCP server that Claude Code (or any MCP client) connects to over stdio. The stack:

| Crate | Role |
|-------|------|
| `unimatrix-store` | redb-backed entry store with 10 tables, secondary indexes, schema migration |
| `unimatrix-vector` | HNSW approximate nearest-neighbor search (384-d embeddings, dot product) |
| `unimatrix-embed` | Local ONNX sentence embedding pipeline (all-MiniLM-L6-v2 default, no API calls) |
| `unimatrix-core` | Integration traits, async wrappers, unified error types |
| `unimatrix-server` | MCP server binary — 8 tools, agent registry, audit log, content scanning |

### MCP Tools

**Knowledge operations:**

| Tool | What it does |
|------|-------------|
| `context_search` | Semantic search — natural language query ranked by similarity |
| `context_lookup` | Deterministic filter — by topic, category, tags, status, or ID |
| `context_get` | Retrieve a single entry by ID |
| `context_store` | Store a new knowledge entry (with near-duplicate detection at 0.92 threshold) |
| `context_correct` | Supersede an entry with a correction (creates chain link) |
| `context_deprecate` | Mark an entry as deprecated |
| `context_status` | Health metrics — counts, distributions, correction chains, security stats |
| `context_briefing` | Compiled orientation for a role + task — conventions, duties, and relevant patterns in one call |

All tools accept a `format` parameter: `summary` (default), `markdown`, or `json`.

### Security

- **Agent registry** with 4 trust levels (System > Privileged > Internal > Restricted)
- Unknown agents auto-enroll as Restricted (read + search only)
- Content scanning (~35 regex patterns) blocks prompt injection, PII, and credential leaks
- Input validation on all parameters (length limits, pattern matching)
- Append-only audit log for every operation
- Content hash chains (SHA-256) for tamper detection

### Knowledge Categories

Eight built-in categories: `outcome`, `lesson-learned`, `decision`, `convention`, `pattern`, `procedure`, `duties`, `reference`.

---

## Prerequisites

- **Rust 1.89+** (edition 2024)
- **ONNX Runtime 1.20.x** shared library

### Installing ONNX Runtime

The embedding pipeline links dynamically against ONNX Runtime. You need the shared library installed.

**macOS (Homebrew):**
```bash
brew install onnxruntime
export ORT_LIB_LOCATION=$(brew --prefix onnxruntime)/lib
export ORT_PREFER_DYNAMIC_LINK=1
```

**Linux (manual):**
```bash
# Download ONNX Runtime 1.20.1 (adjust URL for your arch)
wget https://github.com/microsoft/onnxruntime/releases/download/v1.20.1/onnxruntime-linux-x64-1.20.1.tgz
tar xzf onnxruntime-linux-x64-1.20.1.tgz
sudo cp onnxruntime-linux-x64-1.20.1/lib/* /usr/local/lib/
sudo ldconfig

export ORT_LIB_LOCATION=/usr/local/lib
export ORT_PREFER_DYNAMIC_LINK=1
```

**Devcontainer:** ORT is pre-installed. The `.cargo/config.toml` sets the defaults automatically.

---

## Build

```bash
cargo build --release
```

The binary is at `target/release/unimatrix-server`.

---

## Usage with Claude Code

### 1. Configure MCP

Add Unimatrix as an MCP server in your project's `.claude/settings.json`:

```json
{
  "mcpServers": {
    "unimatrix": {
      "command": "/absolute/path/to/unimatrix-server",
      "args": []
    }
  }
}
```

Replace the path with wherever your built binary lives. For development in this repo:

```json
{
  "mcpServers": {
    "unimatrix": {
      "command": "cargo",
      "args": ["run", "--release", "--bin", "unimatrix-server"]
    }
  }
}
```

### 2. Start Claude Code

Launch Claude Code in your project directory as usual. Unimatrix auto-detects the project root (walks up looking for `.git/`) and creates its data directory at:

```
~/.unimatrix/{project-hash}/
  unimatrix.redb           # knowledge database
  vector/
    unimatrix-vector.hnsw2  # HNSW graph
    unimatrix-vector.meta   # index metadata
```

The embedding model downloads on first use to `~/.cache/unimatrix/models/`.

### 3. Interact

Claude Code sees the 8 `context_*` tools automatically. The server's `instructions` field tells the agent:

> *"Before starting implementation, architecture, or design tasks, search for relevant patterns and conventions using the context tools. Apply what you find. After discovering reusable patterns or making architectural decisions, store them for future reference."*

Example agent interactions:

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

# Get a role briefing before starting work
context_briefing(role: "developer", task: "implement user registration endpoint")

# Correct outdated knowledge
context_correct(
  original_id: 42,
  content: "Use JWT with ES256 (not RS256) — better performance on our infra.",
  reason: "Benchmarked RS256 vs ES256, ES256 is 3x faster for verification"
)
```

### CLI Options

```
unimatrix-server [--project-dir <PATH>] [--verbose]
```

| Flag | Effect |
|------|--------|
| `--project-dir <PATH>` | Override automatic project root detection |
| `--verbose`, `-v` | Enable debug-level logging (to stderr) |

---

## Running Tests

```bash
cargo test
```

371+ tests across all crates. Some embedding tests require model download (gated behind `#[ignore]` — run with `cargo test -- --ignored` to include them).

---

## Project Structure

```
crates/
  unimatrix-store/     # redb storage engine
  unimatrix-vector/    # HNSW vector index
  unimatrix-embed/     # ONNX embedding pipeline
  unimatrix-core/      # integration traits + async wrappers
  unimatrix-server/    # MCP server binary
product/
  PRODUCT-VISION.md    # full roadmap
  features/            # feature documentation per milestone
patches/
  anndists/            # local fix for anndists edition 2024 compat
```

---

## License

MIT OR Apache-2.0
