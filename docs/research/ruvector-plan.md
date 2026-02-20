# Unimatrix: MCP-Enabled Development Knowledge Platform

## Comprehensive Delivery Plan

**Date**: 2026-02-18 (updated 2026-02-19)
**Status**: SUPERSEDED — See `synthesis/research-synthesis-r1.md` and `architecture-patterns/vector-storage-decision.md` for current decisions
**Original Source**: https://github.com/ruvnet/ruvector (79 crates, Rust 2021, MIT)

> **UPDATE 2026-02-19**: Round 1 research (6 reports + synthesis) recommends **direct library dependencies** (hnsw_rs + redb) instead of vendoring ruvector's wrapper code. The ruvector wrapper has confirmed bugs (Issue #134 deadlock, Issue #182 SIGSEGV in PG extension). Writing ~1,200 lines of purpose-built wrapper is less effort than stripping/auditing 2-3K lines of vendored code. The SONA/ReasoningBank approach below remains valid conceptually but should be evaluated independently. See `vector-storage-decision.md` for full analysis.

---

## 1. What We're Building

A Rust-native MCP server that gives Claude Code (and any MCP client) a persistent, learning memory system for software development. It stores patterns, code knowledge, architectural decisions, and bug fixes — then retrieves them semantically and improves its own retrieval quality over time through reinforcement learning.

**One sentence**: ReasoningBank + VectorDB + MCP, all in Rust, shipping as a single binary.

---

## 2. What Already Exists (Verified Real, Not Mocked)

### From ruvector — Production-quality Rust

| Crate | What it provides | Lines (est.) | Quality |
|---|---|---|---|
| `ruvector-core` | SIMD distance (AVX2/512, NEON), 4 quantization tiers, `redb` storage, VectorDB API, HNSW wrapper (`hnsw_rs`) | ~3,000 | Production — real intrinsics, real tests |
| `sona` | ReasoningBank (K-means++ clustering, confidence, consolidation), LoRA (Micro+Base with AVX2), EWC++ (Fisher info, task boundaries), SonaEngine orchestrator, trajectory management | ~2,500 | Production — real algorithms, real tests |
| `mcp-gate` | MCP server (stdio JSON-RPC 2.0, tool registration, initialize/list/call/shutdown, async tokio) | ~800 | Working — wired to TileZero, needs retargeting |
| `rvf` | 18-subcrate framework (adapters, types, crypto, kernel, runtime, wire) | ~5,000+ | Exists — evaluate what's needed |
| `micro-hnsw-wasm` | WASM bindings for HNSW | ~500 | Compiled .wasm exists |

### What's Placeholder (Must Replace)

| Component | Current state | Required action |
|---|---|---|
| `embeddings.rs` HashEmbedding | Character-level hashing, NOT semantic | Replace with real model |
| `embeddings.rs` CandleEmbedding | Deliberate stub, returns error | Implement or remove |
| `agenticdb.rs` embeddings | Same hash placeholder | Wire to real embedding provider |

---

## 3. Architecture

```
┌─────────────────────────────────────────────────────┐
│                   MCP Clients                        │
│  Claude Code ─── npx unimatrix connect ──┐          │
│  Other MCP clients ──────────────────────┤          │
│                                          ▼          │
│                                    stdio bridge      │
│                                   (thin TS client)   │
└──────────────────────────────────────┬──────────────┘
                                       │ HTTP/SSE
┌──────────────────────────────────────▼──────────────┐
│                 unimatrix-server                      │
│            (Rust binary, single process)              │
│                                                      │
│  ┌─────────────┐  ┌──────────────┐  ┌────────────┐  │
│  │  MCP Layer  │  │  MCP Layer   │  │ MCP Layer  │  │
│  │  (memory)   │  │  (learning)  │  │  (admin)   │  │
│  └──────┬──────┘  └──────┬───────┘  └─────┬──────┘  │
│         │                │                 │         │
│  ┌──────▼────────────────▼─────────────────▼──────┐  │
│  │              ReasoningBank                      │  │
│  │  (K-means++, confidence, promotion, dedup)      │  │
│  └──────────────────┬─────────────────────────────┘  │
│                     │                                │
│  ┌──────────────────▼─────────────────────────────┐  │
│  │              SonaEngine                         │  │
│  │  LoRA (micro+base) │ EWC++ │ Trajectories      │  │
│  └──────────────────┬─────────────────────────────┘  │
│                     │                                │
│  ┌─────────┐  ┌────▼────┐  ┌───────────┐           │
│  │  HNSW   │  │  redb   │  │ Embedding │           │
│  │ (index) │  │(storage)│  │ Provider  │           │
│  └─────────┘  └─────────┘  └───────────┘           │
│                                                      │
│  Per-project isolation: separate .db + .hnsw files   │
└──────────────────────────────────────────────────────┘
```

### Project Isolation (Physical)

```
~/.unimatrix/
├── server.toml                    # Global config
├── projects/
│   ├── claude-flow/
│   │   ├── vectors.redb           # Vector storage
│   │   ├── hnsw.bin               # Serialized HNSW index
│   │   ├── sona.bin               # LoRA weights + EWC state
│   │   └── meta.toml              # Project metadata
│   ├── my-api/
│   │   ├── vectors.redb
│   │   ├── hnsw.bin
│   │   ├── sona.bin
│   │   └── meta.toml
│   └── ...
└── exports/                       # JSONL for git tracking
    ├── claude-flow.jsonl
    └── my-api.jsonl
```

Each project gets its own `redb` database, HNSW index, and SONA state. No cross-contamination. Neural weights from Project A cannot affect Project B.

---

## 4. MCP Tool Surface

### Memory Tools (core)

| Tool | Description | Maps to |
|---|---|---|
| `memory_store` | Store a pattern/knowledge with text + metadata + tags | VectorDB.insert + ReasoningBank.store_pattern |
| `memory_search` | Semantic similarity search, returns ranked results | HNSW.search + ReasoningBank.find_patterns |
| `memory_retrieve` | Get specific entry by key | VectorDB.get |
| `memory_update` | Update metadata/tags on existing entry | VectorDB update + re-index |
| `memory_delete` | Remove entry | VectorDB.delete + HNSW.remove |
| `memory_list` | List entries with optional namespace/tag filter | VectorDB.keys + metadata filter |
| `memory_count` | Count entries, optionally by namespace | VectorDB.len |

### Learning Tools (SONA pipeline)

| Tool | Description | Maps to |
|---|---|---|
| `trajectory_begin` | Start tracking a task trajectory | SonaEngine.begin_trajectory |
| `trajectory_step` | Record an action + reward within trajectory | TrajectoryBuilder.add_step |
| `trajectory_end` | Complete trajectory, trigger learning | SonaEngine.end_trajectory |
| `pattern_search` | Find learned patterns similar to query | ReasoningBank.search |
| `pattern_promote` | Force-promote short-term to long-term | ReasoningBank.promote |
| `consolidate` | Run dedup + prune + EWC consolidation | ReasoningBank.consolidate + EWC.consolidate |
| `quality_report` | Get learning stats: pattern count, avg confidence, hit rate | SonaEngine.stats |

### Admin Tools

| Tool | Description |
|---|---|
| `project_create` | Initialize a new isolated project |
| `project_list` | List all projects |
| `project_switch` | Change active project context |
| `project_export` | Export project memories to JSONL |
| `project_import` | Import JSONL into project |
| `server_status` | Health check, memory usage, index stats |

---

## 5. Crate Layout (New Workspace)

```
unimatrix/
├── Cargo.toml                          # Workspace root
├── crates/
│   │
│   │── ruvector-core/                  # VENDORED from ruvector
│   │   ├── src/
│   │   │   ├── distance.rs            # SIMD distance (keep as-is)
│   │   │   ├── simd_intrinsics.rs     # AVX2/512/NEON (keep as-is)
│   │   │   ├── quantization.rs        # 4 tiers (keep as-is)
│   │   │   ├── storage.rs             # redb backend (keep as-is)
│   │   │   ├── vector_db.rs           # VectorDB API (keep as-is)
│   │   │   ├── index/hnsw.rs          # hnsw_rs wrapper (keep as-is)
│   │   │   ├── index/flat.rs          # Flat index fallback (keep as-is)
│   │   │   ├── embeddings.rs          # REPLACE hash with real provider
│   │   │   ├── types.rs               # (keep as-is)
│   │   │   ├── error.rs               # (keep as-is)
│   │   │   └── lib.rs                 # (trim unused modules)
│   │   └── Cargo.toml                 # (trim unused deps)
│   │
│   ├── sona/                           # VENDORED from ruvector
│   │   ├── src/
│   │   │   ├── engine.rs              # SonaEngine (keep as-is)
│   │   │   ├── reasoning_bank.rs      # ReasoningBank (keep, extend)
│   │   │   ├── lora.rs               # MicroLoRA + BaseLoRA (keep as-is)
│   │   │   ├── ewc.rs                # EWC++ (keep as-is)
│   │   │   ├── trajectory.rs          # Trajectory management (keep as-is)
│   │   │   ├── types.rs              # (keep as-is)
│   │   │   └── lib.rs
│   │   └── Cargo.toml
│   │
│   ├── unimatrix-server/              # NEW — the MCP server
│   │   ├── src/
│   │   │   ├── main.rs               # Entry point, CLI args, daemonize
│   │   │   ├── server.rs             # HTTP/SSE + stdio MCP transport
│   │   │   ├── router.rs             # JSON-RPC method dispatch
│   │   │   ├── tools/
│   │   │   │   ├── memory.rs          # memory_* tool implementations
│   │   │   │   ├── learning.rs        # trajectory_*, pattern_*, consolidate
│   │   │   │   └── admin.rs           # project_*, server_status
│   │   │   ├── project.rs             # Project isolation manager
│   │   │   ├── config.rs              # server.toml parsing
│   │   │   └── lib.rs
│   │   └── Cargo.toml                 # deps: axum, tokio, serde_json, ruvector-core, sona
│   │
│   ├── unimatrix-embeddings/          # NEW — real embedding provider
│   │   ├── src/
│   │   │   ├── lib.rs                # EmbeddingProvider trait
│   │   │   ├── api.rs                # OpenAI/Cohere/Voyage API calls
│   │   │   ├── local.rs              # ort (ONNX Runtime) for local models
│   │   │   └── cache.rs              # LRU cache for repeated texts
│   │   └── Cargo.toml                # deps: reqwest, ort, lru
│   │
│   └── unimatrix-client/             # NEW — thin stdio-to-HTTP bridge
│       ├── src/
│       │   └── main.rs               # stdin → HTTP POST, SSE → stdout
│       ├── package.json              # For npx unimatrix connect
│       └── Cargo.toml
│
├── tests/
│   ├── integration/                   # End-to-end MCP protocol tests
│   └── benchmarks/                    # HNSW search, embedding, storage perf
│
└── docker/
    ├── Dockerfile                     # Multi-stage: build → runtime
    └── docker-compose.yml             # Server + optional embedding model
```

### Vendoring Strategy

**Why vendor, not git submodule**: The ruvector workspace is 79 crates. We need ~4. Vendoring the specific crates lets us:
- Trim unused feature flags and dependencies
- Modify `embeddings.rs` without forking the whole repo
- Keep the workspace minimal and buildable
- Pin exact versions without upstream churn

**What to vendor**:
- `ruvector-core` — strip to: distance, simd, quantization, storage, vector_db, index, types, error
- `sona` — keep entire crate (engine, reasoning_bank, lora, ewc, trajectory, types)
- Reference `mcp-gate` patterns but rewrite for our tool surface

**What NOT to vendor**:
- `rvf/` (18 subcrates) — evaluate later for V2
- `ruvector-cluster`, `ruvector-raft` — distributed features not needed V1
- `ruvector-graph`, `ruvector-gnn` — graph analysis not needed V1
- All `-wasm` crates — we're building a native server, not WASM
- All `-node` crates — NAPI not needed (we use HTTP bridge)

---

## 6. Embedding Strategy

### V1: API-based (ship fast)

```rust
// unimatrix-embeddings/src/api.rs
pub struct ApiEmbeddingProvider {
    client: reqwest::Client,
    model: String,       // "text-embedding-3-small"
    dimensions: usize,   // 1536
    api_key: String,
    cache: LruCache<u64, Vec<f32>>,  // hash(text) → embedding
}

impl EmbeddingProvider for ApiEmbeddingProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
    fn dimensions(&self) -> usize;
}
```

Supported providers (from ruvector's existing `ApiEmbedding`):
- OpenAI: `text-embedding-3-small` (1536d), `text-embedding-3-large` (3072d)
- Cohere: `embed-english-v3.0` (1024d)
- Voyage: `voyage-2` (1024d)

### V2: Local models (no API dependency)

```rust
// unimatrix-embeddings/src/local.rs
pub struct OrtEmbeddingProvider {
    session: ort::Session,           // ONNX Runtime
    tokenizer: tokenizers::Tokenizer,
    dimensions: usize,
}
```

Use `ort` crate with a small model like `all-MiniLM-L6-v2` (384d, ~80MB).
This eliminates the API key requirement and works offline.

### Cache Layer (Both V1 and V2)

```rust
// unimatrix-embeddings/src/cache.rs
// LRU cache with xxhash keys
// Avoid re-embedding identical text (e.g., same error message, same function name)
// Expected hit rate: 60-80% in development workflows (repeated patterns)
```

---

## 7. Development-Scoped ReasoningBank

The existing `sona/reasoning_bank.rs` is generic. For a development platform, we scope the pattern taxonomy:

### Pattern Categories

```rust
pub enum DevPatternCategory {
    // Code patterns
    BugFix { language: String, error_type: String },
    Implementation { feature_type: String },
    Refactoring { pattern: String },

    // Architecture decisions
    ArchitectureDecision { context: String, decision: String, consequences: String },
    DesignPattern { pattern_name: String, applicability: String },

    // Debugging knowledge
    ErrorSolution { error_signature: String, root_cause: String, fix: String },
    PerformanceFix { bottleneck: String, optimization: String },

    // Project knowledge
    CodebasePattern { path_pattern: String, convention: String },
    DependencyInsight { package: String, insight: String },
    TestStrategy { component: String, approach: String },
}
```

### Trajectory Definition for Dev Tasks

```rust
pub struct DevTrajectory {
    pub task_type: TaskType,          // bug_fix, feature, refactor, debug
    pub files_touched: Vec<String>,
    pub steps: Vec<DevStep>,
    pub outcome: Outcome,             // success, partial, failure
    pub quality_score: f32,           // 0.0 - 1.0
    pub duration_ms: u64,
}

pub struct DevStep {
    pub action: DevAction,            // search, read, edit, test, commit
    pub target: String,               // file path, search query, etc.
    pub reward: f32,                  // positive if moved toward solution
    pub observation: String,          // what was learned
}
```

### Promotion Rules (Dev-Specific)

| Rule | Threshold | Rationale |
|---|---|---|
| Minimum uses before promotion | 3 | Don't promote one-off patterns |
| Quality threshold | 0.7 (not 0.6) | Higher bar for code knowledge |
| Bug fix patterns | Auto-promote on 2nd success | Bug fixes are high-value |
| Architecture decisions | Manual promotion only | These need human validation |
| Confidence decay | -0.05/week unused | Stale patterns lose relevance |
| Dedup similarity | 0.92 (not 0.95) | More aggressive dedup for code |

---

## 8. Transport: HTTP/SSE + Stdio Bridge

### Server (HTTP/SSE)

```rust
// unimatrix-server/src/server.rs
// Uses axum for HTTP
// Endpoint: POST /mcp — JSON-RPC 2.0 request/response
// Endpoint: GET /mcp/sse — Server-Sent Events for streaming
// Health: GET /health

async fn handle_mcp_request(
    State(state): State<AppState>,
    Json(request): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    match request.method.as_str() {
        "initialize" => handle_initialize(&state),
        "tools/list" => handle_tools_list(&state),
        "tools/call" => handle_tools_call(&state, request.params),
        "shutdown" => handle_shutdown(&state),
        _ => JsonRpcResponse::error(-32601, "Method not found"),
    }
}
```

### Thin Client (stdio bridge)

```typescript
// unimatrix-client/src/main.ts
// npx unimatrix connect
// Reads JSON-RPC from stdin, POSTs to server, writes response to stdout
// This is what Claude Code talks to

import { createInterface } from 'readline';

const SERVER_URL = process.env.UNIMATRIX_URL || 'http://localhost:3077';

const rl = createInterface({ input: process.stdin });
for await (const line of rl) {
    const response = await fetch(`${SERVER_URL}/mcp`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: line,
    });
    const json = await response.json();
    process.stdout.write(JSON.stringify(json) + '\n');
}
```

### Claude Code Integration

```bash
# Add as MCP server
claude mcp add unimatrix -- npx unimatrix connect

# Or direct if server is local
claude mcp add unimatrix -- unimatrix-server --stdio
```

---

## 9. Build Phases

### Phase 1: Foundation (Week 1-2)

**Goal**: Rust binary that starts, accepts MCP connections, stores/retrieves vectors.

| Task | Source | Effort |
|---|---|---|
| Create workspace, vendor `ruvector-core` + `sona` | ruvector repo | 2h |
| Strip unused modules from `ruvector-core` | Trim Cargo.toml features | 2h |
| Replace `embeddings.rs` hash with `ApiEmbeddingProvider` | New code | 4h |
| Build `unimatrix-server` crate with axum | Reference `mcp-gate` patterns | 8h |
| Implement `memory_store`, `memory_search`, `memory_retrieve` | Wire VectorDB + HNSW | 6h |
| Implement `memory_delete`, `memory_list`, `memory_count` | Wire VectorDB | 3h |
| Project isolation (per-project redb + hnsw) | New `project.rs` | 4h |
| Thin client (`npx unimatrix connect`) | New TS, ~50 lines | 2h |
| Docker build | Multi-stage Dockerfile | 2h |
| **Total** | | **~33h** |

**Deliverable**: Working MCP server with semantic memory. `claude mcp add unimatrix` works.

### Phase 2: Learning (Week 3-4)

**Goal**: ReasoningBank + SONA pipeline active. Patterns improve over time.

| Task | Source | Effort |
|---|---|---|
| Wire `SonaEngine` into server state | Connect existing crate | 4h |
| Implement `trajectory_begin/step/end` MCP tools | New tool handlers | 6h |
| Implement `pattern_search`, `consolidate`, `quality_report` | Wire ReasoningBank | 4h |
| Dev-scoped pattern categories | Extend `reasoning_bank.rs` | 6h |
| Dev-scoped promotion rules | Configure thresholds | 3h |
| Confidence decay/boost on retrieval | Extend store logic | 3h |
| SONA state persistence (save/load LoRA + EWC) | Serialization | 4h |
| End-to-end integration tests | Test full pipeline | 6h |
| **Total** | | **~36h** |

**Deliverable**: Server learns from development trajectories. Patterns promote, decay, consolidate.

### Phase 3: Production Hardening (Week 5-6)

**Goal**: Reliable, performant, deployable.

| Task | Source | Effort |
|---|---|---|
| Local embedding model via `ort` (all-MiniLM-L6-v2) | New `local.rs` | 8h |
| Embedding cache (LRU + disk) | New `cache.rs` | 4h |
| JSONL export/import for git tracking | New tool handlers | 4h |
| `project_create/list/switch/export/import` admin tools | New admin handlers | 6h |
| Graceful shutdown, signal handling | Server hardening | 2h |
| Logging (tracing), metrics endpoint | Standard infra | 3h |
| Benchmark suite (HNSW search latency, embed throughput) | New tests | 4h |
| Documentation (README, MCP tool reference) | Docs | 4h |
| CI (cargo test, cargo clippy, docker build) | GitHub Actions | 3h |
| **Total** | | **~38h** |

**Deliverable**: Production-ready v1.0. Docker image, local embeddings, CI green.

### Phase 4: Advanced (V2, Post-Launch)

| Feature | Description |
|---|---|
| Cross-project knowledge | Opt-in Global Pool, explicit promotion |
| Federated learning | SONA's federated training pipeline (already in crate) |
| Quantized storage | Enable Int4/Product quantization for large indexes |
| SSE streaming | Stream search results for large result sets |
| Plugin system | Load custom pattern extractors |
| RVF framework | Evaluate `rvf/` subcrates for advanced indexing |
| Distributed mode | `ruvector-cluster` + `ruvector-raft` for multi-node |

---

## 10. Key Dependencies (Workspace Cargo.toml)

```toml
[workspace]
resolver = "2"
members = [
    "crates/ruvector-core",
    "crates/sona",
    "crates/unimatrix-server",
    "crates/unimatrix-embeddings",
]

[workspace.dependencies]
# Storage
redb = "2.1"
hnsw_rs = "0.3"

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
bincode = "2.0.0-rc.3"
rkyv = "0.8"
toml = "0.8"

# Async
tokio = { version = "1.41", features = ["full"] }
axum = "0.7"
reqwest = { version = "0.12", features = ["json"] }

# SIMD
simsimd = "5.9"

# Embeddings
ort = { version = "2.0", optional = true }
tokenizers = { version = "0.20", optional = true }

# Utilities
thiserror = "2.0"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
dashmap = "6.0"
lru = "0.12"
xxhash-rust = { version = "0.8", features = ["xxh3"] }
uuid = { version = "1.0", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
```

---

## 11. Risk Register

| Risk | Impact | Mitigation |
|---|---|---|
| `hnsw_rs` has a local patch in ruvector Cargo.toml | Build failure | Vendor the patched version, or use `instant-distance` as alternative |
| `redb` version mismatch between vendored crates | Compile error | Pin exact version in workspace |
| SIMD intrinsics don't compile on all targets | Limited platform support | Fallback paths exist in `simd_intrinsics.rs` already |
| `ort` (ONNX Runtime) has complex native deps | Hard to build in CI | Feature-gate behind `local-embeddings`, default to API |
| ReasoningBank K-means++ is O(n*k) on each consolidation | Slow for large pattern sets | Cap cluster count, run consolidation on schedule not per-request |
| Embedding API rate limits | Slow ingestion | LRU cache + batch embedding + backoff |
| HNSW index grows unbounded | Memory pressure | Periodic rebuild with quantized vectors, cap per-project |

---

## 12. Success Criteria

### V1 Launch (Phase 1-3 Complete)

- [ ] `claude mcp add unimatrix -- npx unimatrix connect` works end-to-end
- [ ] Store 10,000 patterns, search returns top-10 in <50ms
- [ ] Patterns persist across server restarts
- [ ] Project isolation: two projects share zero state
- [ ] Learning: patterns promoted after 3 successful uses
- [ ] Learning: stale patterns decay, duplicates merge
- [ ] JSONL export round-trips without data loss
- [ ] Docker image builds and runs on linux/amd64 + linux/arm64
- [ ] Local embedding model works without API key
- [ ] Server starts in <2 seconds

### Performance Targets

| Metric | Target | How (from ruvector) |
|---|---|---|
| Vector search (10K vectors, 768d) | <10ms | HNSW + SIMD distance |
| Vector insert | <1ms | redb + index update |
| Embedding (API) | <200ms | OpenAI latency |
| Embedding (local) | <50ms | ort + all-MiniLM-L6-v2 |
| Consolidation (1000 patterns) | <500ms | K-means++ |
| LoRA adaptation | <1ms | MicroLoRA AVX2 |
| Server memory (10K patterns) | <200MB | Quantized vectors |
| Binary size | <50MB | LTO, single codegen unit |

---

## 13. How This Answers the Original Question

> "Build workspace to include all ruvector rvf crates. Wire ReasoningBank methodology,
> scoped/refined for a development platform, layer MCP interface on top."

**Yes, this works.** Specifically:

1. **Workspace**: Vendor `ruvector-core` + `sona` (not all 79 crates — just the ones that matter)
2. **Learning capability**: SONA engine (LoRA + EWC++ + ReasoningBank) is real, production Rust
3. **ReasoningBank scoped for dev**: Add dev-specific pattern categories, promotion rules, trajectory definitions
4. **MCP interface**: Adapt `mcp-gate` patterns into `unimatrix-server` with 20 dev-focused tools
5. **Rust as storage**: `redb` for persistence, HNSW for indexing, per-project physical isolation

**The only new code**: ~2,000 lines for the MCP server + tools, ~500 lines for embedding provider, ~200 lines for project isolation, ~50 lines for the thin client. Everything else is vendored and wired.

**Total new code**: ~3,000 lines
**Total vendored code**: ~6,000 lines
**Ratio**: 2:1 existing-to-new

This is assembly with a focused MCP layer, not a ground-up build.
