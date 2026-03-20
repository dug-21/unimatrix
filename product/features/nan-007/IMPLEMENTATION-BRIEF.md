# nan-007 Implementation Brief — W1-3: Evaluation Harness

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/nan-007/SCOPE.md |
| Scope Risk Assessment | product/features/nan-007/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/nan-007/architecture/ARCHITECTURE.md |
| Specification | product/features/nan-007/specification/SPECIFICATION.md |
| Risk-Test Strategy | product/features/nan-007/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/nan-007/ALIGNMENT-REPORT.md |

---

## Goal

Provide a complete, offline-first evaluation harness for Unimatrix intelligence changes
so that every retrieval model swap, confidence weight tuning, NLI re-ranking run, or
GGUF integration produces quantified evidence of improvement before it ships. The harness
consists of four offline Rust CLI subcommands (D1–D4) that operate against a frozen
snapshot without a running daemon, and two Python client classes (D5–D6) that connect
to a live daemon's MCP UDS socket and hook IPC socket for live-path evaluation and
observation pipeline testing. D1–D4 gate W1-4 (NLI re-ranking) and W2-4 (GGUF); D5–D6
gate W1-5 and W3-1.

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| snapshot.rs (D1) | pseudocode/snapshot.md | test-plan/snapshot.md |
| eval/profile.rs | pseudocode/eval-profile.md | test-plan/eval-profile.md |
| eval/scenarios.rs (D2) | pseudocode/eval-scenarios.md | test-plan/eval-scenarios.md |
| eval/runner.rs (D3) | pseudocode/eval-runner.md | test-plan/eval-runner.md |
| eval/report.rs (D4) | pseudocode/eval-report.md | test-plan/eval-report.md |
| uds_client.py (D5) | pseudocode/uds-client.md | test-plan/uds-client.md |
| hook_client.py (D6) | pseudocode/hook-client.md | test-plan/hook-client.md |
| main.rs (CLI wiring) | pseudocode/cli-wiring.md | test-plan/cli-wiring.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

Note: pseudocode and test-plan files are produced in Session 2 Stage 3a. The Component
Map lists expected components from the architecture — actual file paths are filled during
delivery. The Cross-Cutting Artifacts section tracks files that don't belong to a single
component but are consumed by specific stages.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| VACUUM INTO implementation method | Use rusqlite synchronous `Connection::execute()` — single DDL statement, no tokio runtime, dispatched pre-tokio per C-10. Also applies live-DB path guard via `canonicalize` on both `snapshot --out` and `eval run --db`. | OQ-2 / Architecture | product/features/nan-007/architecture/ADR-001-vacuum-into-rusqlite-sync.md |
| Analytics write queue in EvalServiceLayer | `AnalyticsMode::Suppressed` — eval runner constructs a raw read-only `sqlx::SqlitePool`, never calls `SqlxStore::open()`, never spawns the drain task, never calls `enqueue_analytics`. `AnalyticsMode::Live` exists for future use only. | SR-07 / ADR-002 | product/features/nan-007/architecture/ADR-002-analytics-mode-suppression.md |
| Accessing `kendall_tau()` from production binary code | Enable `features = ["test-support"]` on `unimatrix-engine` in `unimatrix-server/Cargo.toml`. No duplication of metric code. Comment marks the feature as "production-safe; required by eval runner". | OQ-3 / ADR-003 | product/features/nan-007/architecture/ADR-003-test-support-feature-for-eval.md |
| Crate placement for eval infrastructure | Module tree inside `crates/unimatrix-server/src/eval/`. No new workspace crate. Consistent with `export.rs` and `test_support.rs` precedents. | OQ-4 / ADR-004 | product/features/nan-007/architecture/ADR-004-no-new-eval-crate.md |
| CLI structure for `unimatrix eval` | Nested clap 4.x enum: `Command::Eval { command: EvalCommand }` with inner `EvalCommand::{ Scenarios, Run, Report }`. `snapshot` is a peer `Command::Snapshot` variant, not nested under eval. Entire `Eval` arm dispatched pre-tokio; async work uses `block_export_sync`. | OQ-1 / ADR-005 | product/features/nan-007/architecture/ADR-005-nested-eval-subcommand-clap4.md |
| Hook socket path | `ProjectPaths.socket_path` is the hook IPC socket (`{data_dir}/unimatrix.sock`). `ProjectPaths.mcp_socket_path` is the MCP UDS socket (`{data_dir}/unimatrix-mcp.sock`). No new `ProjectPaths` field needed. | OQ-3 answer / Architecture | product/features/nan-007/architecture/ARCHITECTURE.md |
| Snapshot anonymization (`--anonymize`) | Removed from scope. `agent_id` is role-like metadata; no anonymization pass required. Snapshots must not be committed to the repo; CLI `--help` must warn about content sensitivity (NFR-07). See ALIGNMENT-REPORT.md VARIANCE-01 — accepted by human. | SCOPE.md / ALIGNMENT-REPORT.md | — |
| `eval run` live-DB path guard (FR-44) | `eval run --db` resolves the supplied path via `canonicalize()` and compares against the active daemon's DB path from `ProjectPaths`. Match returns `EvalError::LiveDbPath` before any pool is opened. Mirrors the snapshot guard. Closes ALIGNMENT-REPORT.md VARIANCE-02. | VARIANCE-02 / FR-44 / ADR-001 | product/features/nan-007/architecture/ADR-001-vacuum-into-rusqlite-sync.md |
| `AnalyticsMode` canonical name | `AnalyticsMode::Suppressed` is the canonical name throughout all documents (ALIGNMENT-REPORT.md noted an inconsistency in the spec's domain model which used `Disabled`). Implementers use `Suppressed` exclusively. | Vision alignment / ALIGNMENT-REPORT.md | product/features/nan-007/architecture/ADR-002-analytics-mode-suppression.md |
| Vector index per profile vs. shared | One `VectorIndex` instance per `EvalServiceLayer` per profile. Index is not shared across profiles because each profile may differ in weight configuration. Memory implication documented in CLI help text. NFR-03 sets measurable limit: 2 profiles, 50k entries, 8 GB RAM. | OQ-5 / Architecture | product/features/nan-007/architecture/ARCHITECTURE.md |
| MCP UDS framing | Newline-delimited JSON (`\n`-terminated), identical to stdio transport. No length prefix. rmcp 0.16.0 `JsonRpcMessageCodec` confirmed. `UnimatrixUdsClient` is a socket-connect variant of `UnimatrixClient`. | SCOPE.md / Architecture | — |

---

## Files to Create / Modify

### Rust — `crates/unimatrix-server/`

| File | Action | Summary |
|------|--------|---------|
| `src/snapshot.rs` | Create | `run_snapshot(project_dir, out)` — sync, rusqlite, `VACUUM INTO`, live-DB path guard |
| `src/eval/mod.rs` | Create | Re-exports, `EvalCommand` enum, `run_eval_command()` dispatcher |
| `src/eval/profile.rs` | Create | `EvalProfile`, `EvalServiceLayer`, `AnalyticsMode`, `EvalError` |
| `src/eval/scenarios.rs` | Create | `run_scenarios()` — query_log scan, JSONL output, source filter |
| `src/eval/runner.rs` | Create | `run_eval()` — per-profile ServiceLayer construction, metric computation, result JSON output |
| `src/eval/report.rs` | Create | `run_report()` — aggregate results to Markdown, five sections, zero-regression check |
| `src/main.rs` | Modify | Add `Command::Snapshot` and `Command::Eval { command: EvalCommand }` variants; dispatch arms in sync block |
| `Cargo.toml` | Modify | Add `features = ["test-support"]` to `unimatrix-engine` dependency; add comment marking it production-safe |

### Python — `product/test/infra-001/`

| File | Action | Summary |
|------|--------|---------|
| `harness/uds_client.py` | Create | `UnimatrixUdsClient` — MCP over AF_UNIX, newline-delimited JSON, 12 typed tool methods, context manager |
| `harness/hook_client.py` | Create | `UnimatrixHookClient` — hook IPC over AF_UNIX, 4-byte BE length prefix + JSON, 5 typed methods |
| `tests/test_eval_uds.py` | Create | D5 test suite: connection lifecycle, tool parity, concurrent clients, source=uds in query_log |
| `tests/test_eval_hooks.py` | Create | D6 test suite: session round-trips, status visibility, keyword population, invalid payload rejection |

---

## Data Structures

### Rust

```rust
// eval/profile.rs
pub enum AnalyticsMode {
    Live,       // normal ServiceLayer — drain task active (future use)
    Suppressed, // no drain task; enqueue_analytics calls are no-ops
}

pub struct EvalProfile {
    pub name: String,
    pub description: Option<String>,
    pub config_overrides: UnimatrixConfig,  // subset; empty = compiled defaults
}

pub struct EvalServiceLayer {
    inner: ServiceLayer,
    db_path: PathBuf,
    profile_name: String,
    // analytics_mode is always Suppressed for eval; tracked for future Live variant
}

pub enum EvalError {
    ModelNotFound(PathBuf),
    ConfigInvariant(String),  // human-readable: "expected 0.92, got 0.91"
    LiveDbPath { supplied: PathBuf, active: PathBuf },
    Io(std::io::Error),
    Store(/* store error type */),
}

// eval/scenarios.rs
pub struct ScenarioRecord {
    pub id: String,
    pub query: String,
    pub context: ScenarioContext,
    pub baseline: Option<ScenarioBaseline>,
    pub source: String,          // "mcp" | "uds"
    pub expected: Option<Vec<u64>>,
}

pub struct ScenarioContext {
    pub agent_id: String,
    pub feature_cycle: String,
    pub session_id: String,
    pub retrieval_mode: String,  // "flexible" | "strict"
}

pub struct ScenarioBaseline {
    pub entry_ids: Vec<u64>,
    pub scores: Vec<f32>,  // parallel to entry_ids; lengths must match
}

// eval/runner.rs
pub struct ScenarioResult {
    pub scenario_id: String,
    pub query: String,
    pub profiles: HashMap<String, ProfileResult>,
    pub comparison: ComparisonMetrics,
}

pub struct ProfileResult {
    pub entries: Vec<ScoredEntry>,
    pub latency_ms: u64,
    pub p_at_k: f64,
    pub mrr: f64,
}

pub struct ScoredEntry {
    pub id: u64,
    pub title: String,
    pub final_score: f64,
    pub similarity: f64,
    pub confidence: f64,
    pub status: String,
    pub nli_rerank_delta: Option<f64>,
}

pub struct ComparisonMetrics {
    pub kendall_tau: f64,
    pub rank_changes: Vec<RankChange>,
    pub mrr_delta: f64,
    pub p_at_k_delta: f64,
    pub latency_overhead_ms: i64,
}

pub struct RankChange {
    pub entry_id: u64,
    pub from_rank: usize,  // 1-indexed, position in baseline list
    pub to_rank: usize,    // 1-indexed, position in candidate list
}

// main.rs — clap enum additions
#[derive(Debug, Subcommand)]
enum Command {
    // ... existing variants ...
    Snapshot { out: PathBuf },
    Eval { #[command(subcommand)] command: EvalCommand },
}

#[derive(Debug, Subcommand)]
enum EvalCommand {
    Scenarios { db: PathBuf, source: ScenarioSource, limit: Option<usize>, out: PathBuf },
    Run { db: PathBuf, scenarios: PathBuf, configs: String, out: PathBuf, k: usize },
    Report { results: PathBuf, scenarios: Option<PathBuf>, out: PathBuf },
}
```

### Python

```python
# harness/uds_client.py
class UnimatrixUdsClient:
    MAX_SOCKET_PATH_BYTES = 103
    def __init__(self, socket_path: str | Path, timeout: float = DEFAULT_TIMEOUT): ...
    def connect(self) -> None: ...      # validates path, opens AF_UNIX, MCP initialize
    def disconnect(self) -> None: ...   # MCP shutdown, close socket
    def __enter__(self): ...
    def __exit__(self, *args): ...
    # 12 typed tool methods: context_search, context_store, context_lookup,
    # context_get, context_correct, context_deprecate, context_status,
    # context_briefing, context_quarantine, context_enroll, context_cycle,
    # context_cycle_review

# harness/hook_client.py
class UnimatrixHookClient:
    MAX_PAYLOAD_SIZE = 1_048_576  # 1 MiB
    def __init__(self, socket_path: str | Path, timeout: float = DEFAULT_TIMEOUT): ...
    def ping(self) -> HookResponse: ...
    def session_start(self, session_id: str, feature_cycle: str, agent_role: str) -> HookResponse: ...
    def session_stop(self, session_id: str, outcome: str) -> HookResponse: ...
    def pre_tool_use(self, session_id: str, tool: str, input: dict) -> HookResponse: ...
    def post_tool_use(self, session_id: str, tool: str,
                      response_size: int, response_snippet: str) -> HookResponse: ...
    # _send(payload: bytes) raises ValueError if len > MAX_PAYLOAD_SIZE before any write
    # Wire: struct.pack('>I', len) + json payload; struct.unpack('>I', 4-byte header) for reads
```

---

## Function Signatures

```rust
// snapshot.rs
pub fn run_snapshot(
    project_dir: Option<&Path>,
    out: &Path,
) -> Result<(), Box<dyn std::error::Error>>;

// eval/profile.rs
impl EvalServiceLayer {
    pub async fn from_profile(
        db_path: &Path,
        profile: &EvalProfile,
    ) -> Result<Self, EvalError>;
}

// eval/scenarios.rs
pub fn run_scenarios(
    db: &Path,
    source: ScenarioSource,   // enum: Mcp | Uds | All
    limit: Option<usize>,
    out: &Path,
) -> Result<(), Box<dyn std::error::Error>>;

// eval/runner.rs
pub fn run_eval(
    db: &Path,
    scenarios: &Path,
    configs: &[PathBuf],
    k: usize,
    out: &Path,
) -> Result<(), Box<dyn std::error::Error>>;

// eval/report.rs
pub fn run_report(
    results: &Path,
    scenarios: Option<&Path>,
    out: &Path,
) -> Result<(), Box<dyn std::error::Error>>;

// main.rs dispatcher
fn run_eval_command(
    cmd: EvalCommand,
    project_dir: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>>;
```

---

## Constraints

| # | Constraint |
|---|-----------|
| C-01 | No new workspace crate. All Rust eval modules live in `crates/unimatrix-server/src/eval/` alongside `export.rs` and `snapshot.rs`. |
| C-02 | `SqlxStore::open()` must not be called on a snapshot database. Use raw `sqlx::SqlitePool` with `SqliteConnectOptions::read_only(true)`. |
| C-03 | `AnalyticsMode::Suppressed` at `EvalServiceLayer` construction — drain task never spawned, `enqueue_analytics` is no-op. `?mode=ro` is a secondary layer, not the primary guard (SR-07 / ADR-002). |
| C-04 | `UnimatrixUdsClient` uses newline-delimited JSON (`\n`), not a length prefix. Hook IPC only uses 4-byte BE length prefix. |
| C-05 | `UnimatrixHookClient` write framing: `struct.pack('>I', len(payload)) + payload`. Read framing: `struct.unpack('>I', 4-byte-header)[0]` then read payload. `MAX_PAYLOAD_SIZE` = 1 MiB; `ValueError` raised client-side before any send. |
| C-06 | Profile TOML with `[confidence]` overrides must supply all six weight fields summing to 0.92 ± 1e-9. Failure returns `EvalError::ConfigInvariant(String)` with a user-readable message naming expected/actual sums. |
| C-07 | `unimatrix eval report` must not exit with non-zero code based on regression count. No CI gate logic in this subcommand. |
| C-08 | MCP UDS socket path must not exceed 103 bytes. `UnimatrixUdsClient` validates before `connect()` and raises a descriptive error. |
| C-09 | `snapshot` is dispatched pre-tokio (C-10 ordering). Uses `block_export_sync` + async sqlx (ADR-001: rusqlite was removed in nxs-011; VACUUM INTO goes through sqlx via `block_export_sync`). `eval scenarios` and `eval run` also use `block_export_sync` bridge for async sqlx within the sync dispatch arm. |
| C-10 | Eval test suites extend existing `TestHarness` fixtures. `kendall_tau()` from `unimatrix-engine::test_scenarios` is reused directly — no duplicate metric code. |
| C-11 | All Python additions go in `product/test/infra-001/harness/` (clients) and `product/test/infra-001/tests/` (test suites). |
| C-12 | No `--anonymize` flag. Snapshots must not be committed to the repo. CLI `--help` must include a warning about snapshot content sensitivity (NFR-07). |
| C-13 | `eval run --db` and `unimatrix snapshot --out` must both apply `std::fs::canonicalize()` path guard against the active daemon DB path before any I/O. |
| C-14 | `EvalServiceLayer::from_profile()` validates model paths at construction and returns `EvalError::ModelNotFound` — never panics on a bad profile (SR-09 / ADR-001). |
| C-15 | `ConfidenceWeights` sum invariant: if candidate profile omits any of the six weight fields, or the sum is not 0.92 ± 1e-9, reject at profile load time with a user-readable error (not a raw serde failure). |

---

## Dependencies

### Rust (no new external crates)

| Component | Location | Role in nan-007 |
|-----------|----------|----------------|
| `sqlx::SqlitePool` + `SqliteConnectOptions` | `crates/unimatrix-store` | Read-only snapshot pool in `eval/` and `VACUUM INTO` in `snapshot.rs` (via `block_export_sync`) |
| `TestHarness::new()` pattern | `crates/unimatrix-server/src/test_support.rs` | Construction model for `EvalServiceLayer::from_profile()` |
| `block_export_sync()` | `crates/unimatrix-server/src/export.rs` | Async bridge for `snapshot`, `eval scenarios`, and `eval run` |
| `kendall_tau()`, `assert_ranked_above()` | `crates/unimatrix-engine/src/test_scenarios.rs` | Metric computation (via `test-support` feature) |
| `QueryLogRecord::scan_query_log_by_sessions()` | `crates/unimatrix-store/src/query_log.rs` | Scenario extraction from snapshot |
| `RetrievalMode`, `ServiceSearchParams` | `crates/unimatrix-server/src/services/search.rs` | Search replay per profile |
| `UnimatrixConfig`, `ConfidenceWeights`, `InferenceConfig` | `crates/unimatrix-server/src/infra/config.rs` | Profile TOML loading and invariant validation |
| `EmbedServiceHandle`, `RayonPool` | `crates/unimatrix-server/src/infra/` | Inference for eval profiles (crt-022) |
| `HookRequest`, `HookResponse`, wire framing | `crates/unimatrix-engine/src/wire.rs` | Wire protocol spec for `UnimatrixHookClient` |

`Cargo.toml` change required: add `features = ["test-support"]` to `unimatrix-engine` dependency with a comment: `# production-safe; required by eval runner for kendall_tau and ranking metrics`.

### Python (standard library only — no new dependencies)

| Module | Use |
|--------|-----|
| `socket` | `AF_UNIX SOCK_STREAM` for both UDS and hook clients |
| `struct` | `pack('>I', ...)` / `unpack('>I', ...)` for hook framing |
| `json` | Wire serialization / deserialization |
| `pathlib` | Path handling |

### Existing Python infrastructure consumed

| Component | Location | Role |
|-----------|----------|------|
| `UnimatrixClient` | `product/test/infra-001/harness/client.py` | Framing reference and API surface for `UnimatrixUdsClient` |
| `daemon_server` fixture (entry #1928) | `product/test/infra-001/suites/conftest.py` | Daemon lifecycle for D5/D6 test suites |

---

## NOT in Scope

- `unimatrix eval live` subcommand — deferred; callers use `UnimatrixUdsClient` directly.
- NLI model integration — W1-4. Profile TOML `[inference]` section is a stub only.
- GGUF model integration — W2-4.
- GNN training — W3-1. `UnimatrixHookClient` is the foundation.
- Automated CI gate logic — the eval report is a human-reviewed artifact only.
- Cross-project or multi-deployment eval — single project directory only.
- Web UI or interactive diff viewer — report is Markdown only.
- `unimatrix import` of eval results — no write-back to the live DB.
- Snapshot anonymization (`--anonymize` flag) — explicitly removed from scope.
- New database tables, columns, or schema migrations — none.
- `SqlxStore::open_readonly()` — eval runner uses raw `sqlx::SqlitePool` directly.

---

## Alignment Status

**Overall**: PASS with two resolved WARNs.

| Variance | Status | Resolution |
|----------|--------|-----------|
| WARN-01: `--anonymize` flag removed from scope | Accepted by human | Snapshots must not be committed to the repo; CLI `--help` must include content-sensitivity warning (NFR-07). Follow-on feature if ever needed. |
| WARN-02: `eval run` missing snapshot-path guard | Resolved — added as FR-44 / AC-16 in spec, and documented in ADR-001 | `eval run --db` applies `canonicalize()` path guard before any pool is opened; `EvalError::LiveDbPath` is the error variant. |
| Naming inconsistency: `AnalyticsMode::Suppressed` vs `Disabled` | Resolved | `AnalyticsMode::Suppressed` is the canonical name across all documents and implementation. |

All six SCOPE.md deliverables are addressed in full across all source documents. All
four SCOPE.md open questions are answered in ADR-001 through ADR-005. No unrequired
scope additions were introduced.
