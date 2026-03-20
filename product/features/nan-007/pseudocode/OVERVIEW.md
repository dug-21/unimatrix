# nan-007 Pseudocode Overview — W1-3 Evaluation Harness

## Components Involved

| Component | File | Why Touched |
|-----------|------|-------------|
| snapshot.rs | `crates/unimatrix-server/src/snapshot.rs` | New file: VACUUM INTO, live-DB guard, sqlx + block_export_sync |
| eval/profile.rs | `crates/unimatrix-server/src/eval/profile.rs` | New file: EvalProfile, EvalServiceLayer, AnalyticsMode, EvalError |
| eval/scenarios.rs | `crates/unimatrix-server/src/eval/scenarios.rs` | New file: query_log scan → JSONL |
| eval/runner.rs | `crates/unimatrix-server/src/eval/runner.rs` | New file: per-profile replay, metric computation |
| eval/report.rs | `crates/unimatrix-server/src/eval/report.rs` | New file: Markdown aggregation, five sections |
| eval/mod.rs | `crates/unimatrix-server/src/eval/mod.rs` | New file: re-exports + EvalCommand enum + dispatcher |
| uds_client.py | `product/test/infra-001/harness/uds_client.py` | New file: UnimatrixUdsClient over AF_UNIX |
| hook_client.py | `product/test/infra-001/harness/hook_client.py` | New file: UnimatrixHookClient over AF_UNIX |
| main.rs | `crates/unimatrix-server/src/main.rs` | Modified: Command::Snapshot + Command::Eval variants |

## Data Flow (Offline, D1–D4)

```
snapshot --out snap.db
  resolve out_path via canonicalize()
  compare against ProjectPaths.db_path (same? → EvalError, non-zero exit)
  block_export_sync {
    open raw SqlitePool (ro: false, source DB) [no migration]
    VACUUM INTO out_path
  }
  snap.db now exists: valid SQLite, all tables, no WAL

eval scenarios --db snap.db --out scenarios.jsonl
  resolve db via canonicalize()
  compare against ProjectPaths.db_path (same? → error)
  block_export_sync {
    open raw SqlitePool (ro: true, snap.db) [no migration]
    scan query_log → join entries → filter by source
    write one JSONL line per row
  }

eval run --db snap.db --scenarios scenarios.jsonl --configs a.toml,b.toml --out results/
  resolve db via canonicalize()
  compare against ProjectPaths.db_path (same? → EvalError::LiveDbPath)
  for each profile TOML:
    EvalServiceLayer::from_profile(db, profile)  [async, via block_export_sync]
      validate ConfidenceWeights sum (0.92 ± 1e-9)
      validate model paths → EvalError::ModelNotFound if absent
      open raw SqlitePool (ro: true) [no migration, AnalyticsMode::Suppressed]
      build VectorIndex from snapshot
  for each scenario in scenarios.jsonl:
    for each EvalServiceLayer:
      replay search via ServiceLayer.search
      record ProfileResult {entries, latency_ms, p_at_k, mrr}
    compute ComparisonMetrics across profiles
    write results/{scenario_id}.json

eval report --results results/ --out report.md
  read all *.json from results/
  aggregate → five Markdown sections
  exit 0 always (no CI gate logic)
```

## Data Flow (Live, D5–D6)

```
UnimatrixUdsClient(mcp_socket_path)
  validate socket path ≤ 103 bytes
  __enter__: open AF_UNIX SOCK_STREAM → MCP initialize handshake (newline-delimited JSON)
  12 typed tool methods → tools/call JSON-RPC → newline-terminated response
  __exit__: MCP shutdown → close socket

UnimatrixHookClient(socket_path)
  __enter__: open AF_UNIX SOCK_STREAM
  send: struct.pack('>I', len) + json_bytes; validate len ≤ 1 MiB before send
  recv: read 4 bytes → unpack length → read N bytes → parse JSON
  5 typed methods: ping, session_start, session_stop, pre_tool_use, post_tool_use
```

## Shared Types (Rust)

Defined in `eval/profile.rs`, used across all eval modules:

```
AnalyticsMode {
    Live,        -- drain task active (future use only)
    Suppressed,  -- no drain task, no enqueue_analytics calls (always used in nan-007)
}

EvalProfile {
    name: String,
    description: Option<String>,
    config_overrides: UnimatrixConfig,  -- subset; empty = compiled defaults
}

EvalServiceLayer {
    pool: SqlitePool,         -- read_only(true), raw (no migration)
    vector_index: Arc<VectorIndex>,
    embed_handle: EmbedServiceHandle,
    rayon_pool: Arc<RayonPool>,
    adapt_svc: Arc<AdaptationService>,
    profile_name: String,
    db_path: PathBuf,
    -- analytics_mode is always Suppressed; tracked structurally via absence of drain task
}

EvalError {
    ModelNotFound(PathBuf),
    ConfigInvariant(String),        -- "confidence weights sum to X, expected 0.92 ± 1e-9"
    LiveDbPath { supplied: PathBuf, active: PathBuf },
    Io(std::io::Error),
    Store(StoreError),
    ProfileNameCollision(String),   -- two profile TOMLs share same [profile].name
    InvalidK(usize),                -- k == 0
}
```

Defined in `eval/scenarios.rs`:

```
ScenarioRecord {
    id: String,
    query: String,
    context: ScenarioContext { agent_id, feature_cycle, session_id, retrieval_mode },
    baseline: Option<ScenarioBaseline { entry_ids: Vec<u64>, scores: Vec<f32> }>,
    source: String,           -- "mcp" | "uds"
    expected: Option<Vec<u64>>,
}
```

Defined in `eval/runner.rs`:

```
ScenarioResult {
    scenario_id: String,
    query: String,
    profiles: HashMap<String, ProfileResult>,
    comparison: ComparisonMetrics,
}

ProfileResult {
    entries: Vec<ScoredEntry>,
    latency_ms: u64,
    p_at_k: f64,
    mrr: f64,
}

ScoredEntry {
    id: u64, title: String, final_score: f64, similarity: f64,
    confidence: f64, status: String, nli_rerank_delta: Option<f64>,
}

ComparisonMetrics {
    kendall_tau: f64,
    rank_changes: Vec<RankChange { entry_id: u64, from_rank: usize, to_rank: usize }>,
    mrr_delta: f64,       -- candidate.mrr - baseline.mrr
    p_at_k_delta: f64,    -- candidate.p_at_k - baseline.p_at_k
    latency_overhead_ms: i64,
}
```

Defined in Python (uds_client.py / hook_client.py):

```
MCPResponse: {id, result, error, raw}   -- same as client.py MCPResponse
HookResponse: {type: str, ...}          -- typed wrapper around hook IPC JSON
```

## Sequencing Constraints (Build Order)

1. `eval/profile.rs` — defines shared error types and EvalServiceLayer; must be compiled before runner and scenarios
2. `eval/scenarios.rs` — depends on profile types and ScenarioRecord shape
3. `eval/runner.rs` — depends on profile.rs (EvalServiceLayer, EvalError) and scenarios.rs (ScenarioRecord)
4. `eval/report.rs` — depends only on ScenarioResult JSON schema; no Rust type dependency on runner
5. `snapshot.rs` — standalone, depends only on sqlx, project, and block_export_sync from export.rs
6. `eval/mod.rs` — depends on all four eval sub-modules
7. `main.rs` modifications — depend on eval/mod.rs and snapshot.rs
8. Python clients — independent of Rust; depend only on stdlib and running daemon

## Integration Surface (from ARCHITECTURE.md)

All names below are directly from the architecture Integration Surface table:

| Name | Module | Signature |
|------|---------|-----------|
| `run_snapshot` | `snapshot.rs` | `fn(project_dir: Option<&Path>, out: &Path) -> Result<(), Box<dyn Error>>` |
| `EvalServiceLayer::from_profile` | `eval/profile.rs` | `async fn(db_path: &Path, profile: &EvalProfile) -> Result<EvalServiceLayer, EvalError>` |
| `run_scenarios` | `eval/scenarios.rs` | `fn(db: &Path, source: ScenarioSource, limit: Option<usize>, out: &Path) -> Result<(), Box<dyn Error>>` |
| `run_eval` | `eval/runner.rs` | `fn(db: &Path, scenarios: &Path, configs: &[PathBuf], k: usize, out: &Path) -> Result<(), Box<dyn Error>>` |
| `run_report` | `eval/report.rs` | `fn(results: &Path, scenarios: Option<&Path>, out: &Path) -> Result<(), Box<dyn Error>>` |
| `run_eval_command` | `eval/mod.rs` | `fn(cmd: EvalCommand, project_dir: Option<&Path>) -> Result<(), Box<dyn Error>>` |
| `Command::Snapshot` | `main.rs` | `Snapshot { out: PathBuf }` |
| `Command::Eval` | `main.rs` | `Eval { #[command(subcommand)] command: EvalCommand }` |

## Key Constraints Summary

- C-01: No new workspace crate. All Rust eval in `src/eval/`.
- C-02: Raw `sqlx::SqlitePool` with `read_only(true)` — never `SqlxStore::open()` on snapshot.
- C-03: `AnalyticsMode::Suppressed` always; drain task never spawned.
- C-04: UDS client uses `\n`-delimited JSON, NOT length prefix.
- C-05: Hook client uses 4-byte BE length prefix + JSON.
- C-06: ConfidenceWeights sum invariant: six fields must sum to 0.92 ± 1e-9.
- C-07: `eval report` exits 0 regardless of regression count.
- C-08: MCP UDS socket path ≤ 103 bytes; validated before connect().
- C-09: snapshot + eval commands dispatched pre-tokio; async via block_export_sync.
- C-13: Both `snapshot --out` and `eval run --db` apply canonicalize() path guard.

## Knowledge Stewardship

Queried: /uni-query-patterns for "evaluation harness patterns conventions" (category: pattern) — 5 results returned; none directly applicable to offline eval harness. Closest: #426 (shadow-mode evaluation pipeline, crt-007) and #724 (behavior-based ranking tests). Patterns are consumer-facing; eval harness is an infrastructure producer. No deviation from established patterns.
Queried: /uni-query-patterns for "nan-007 architectural decisions" (category: decision, topic: nan-007) — 5 results: ADR-001 (VACUUM INTO + sqlx + block_on, #2602), ADR-002 (AnalyticsMode::Suppressed, #2585), ADR-003 (test-support feature gate, #2586), ADR-004 (eval in unimatrix-server, #2587), ADR-005 (nested eval subcommand via clap, #2588). All five ADRs directly inform this pseudocode; followed without deviation.
Queried: /uni-query-patterns for "snapshot vacuum database patterns" — 5 results; #1097 (snapshot isolation for knowledge export, ADR) is the most relevant prior art. nan-007 VACUUM INTO approach extends this pattern to full-DB copy rather than transaction-scoped export.
Queried: /uni-query-patterns for "block_export_sync async bridge pattern" — 5 results; #2126 (block_in_place not Handle::current().block_on) and #1758 (extract spawn_blocking body into named sync helper) are relevant. block_export_sync follows the established bridge pattern.
Stored: nothing novel to store — pseudocode agents are read-only; patterns are consumed not created
