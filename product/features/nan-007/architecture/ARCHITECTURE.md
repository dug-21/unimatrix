# nan-007: W1-3 Evaluation Harness — Architecture

## System Overview

nan-007 adds an offline A/B evaluation subsystem and a live test-harness layer to
Unimatrix. Every intelligence change (NLI re-ranking in W1-4, GGUF in W2-4, GNN
training in W3-1) must produce a quantified evidence report before shipping. Without
this feature those future gates are blocked.

The feature integrates at three seams:

1. **CLI layer** — Two new top-level subcommands (`snapshot`, `eval`) added to the
   `unimatrix` binary following the existing C-10 dispatch ordering rule. `eval` carries
   three nested subcommands (`scenarios`, `run`, `report`), giving a four-level CLI
   surface that matches existing patterns in clap 4.x.

2. **Rust eval engine** — A module tree `crates/unimatrix-server/src/eval/` that
   constructs an `EvalServiceLayer` (extending the `TestHarness` pattern), replays
   scenarios in-process, and computes ranking metrics. All eval code lives in the
   existing `unimatrix-server` crate — no new workspace member is introduced.

3. **Python test-harness layer** — Two new client classes in
   `product/test/infra-001/harness/` that connect to a running daemon's MCP UDS socket
   and hook IPC socket respectively. These enable live-path evaluation and observation
   pipeline testing without spawning a subprocess.

---

## Component Breakdown

### Component 1: `snapshot.rs` — Database Snapshot Command

**Location**: `crates/unimatrix-server/src/snapshot.rs`

**Responsibility**: Produce a self-contained, read-only SQLite copy of the entire
database (all tables, including analytics tables excluded from `export`) via SQLite's
`VACUUM INTO` pragma. This is the only data-collection command — all other eval
components consume the snapshot rather than the live database.

**Key behaviours**:
- Pre-tokio dispatch (C-10 ordering). Uses `block_export_sync` from `export.rs` to
  bridge to async sqlx, then issues `sqlx::query("VACUUM INTO ?").bind(path).execute(&pool).await`
  against a minimal read-only pool opened directly via `SqliteConnectOptions` (no
  migration triggered). ADR-001.
- Validates that the output path does not resolve to the same inode as the active
  database before executing (security requirement, AC-02).
- The `--anonymize` flag is removed from scope — it was explicitly removed in the
  SCOPE.md Resolved Decisions section.

**Why sqlx + block_export_sync, not rusqlite**: rusqlite was fully removed from all
crates in nxs-011 (PR #299). Reintroducing it would undo that migration and introduce
a second bundled SQLite binary. `VACUUM INTO` is valid SQL that sqlx executes directly.
The `block_export_sync` wrapper (already used by `export.rs`, `eval scenarios`, and
`eval run`) is the established pattern for bridging the pre-tokio CLI dispatch to async
sqlx. ADR-001.

---

### Component 2: `eval/scenarios.rs` — Scenario Extraction

**Location**: `crates/unimatrix-server/src/eval/scenarios.rs`

**Responsibility**: Open a snapshot database in read-only mode, scan `query_log`,
join with `entries` to build the scenario baseline, and write JSONL to the output
path. Supports `--source mcp|uds|all` filtering on the `source` field in `query_log`.

**Key behaviours**:
- Opens snapshot via raw `sqlx::SqlitePool` with `SqliteConnectOptions::read_only(true)`.
  Does NOT call `SqlxStore::open()` (which triggers migration).
- Runs inside a `block_on` wrapper (same pattern as `export.rs`) since it needs async
  sqlx queries but is dispatched from the sync branch of `main()`.
- Produces JSONL where each line matches the scenario schema defined in ASS-025:
  `id`, `query`, `context`, `baseline.entry_ids`, `baseline.scores`, `source`,
  `expected` (null for query_log-sourced, non-null for hand-authored).

---

### Component 3: `eval/runner.rs` — In-Process Eval Engine

**Location**: `crates/unimatrix-server/src/eval/runner.rs`

**Responsibility**: Construct one `EvalServiceLayer` per profile config, replay each
scenario through each profile in-process, compute per-scenario metrics (P@K, MRR,
Kendall tau, latency delta), and write JSON result files.

**Key behaviours**:
- `EvalServiceLayer::from_profile(db_path, profile_toml)` — wraps the `TestHarness`
  construction pattern with three modifications: (1) read-only DB via raw pool, (2)
  `AnalyticsMode::Suppressed` passed at construction to disable the analytics write
  queue (SR-07 mitigation, ADR-002 below), and (3) a live-DB path guard identical to
  the one in `unimatrix snapshot`: `db_path` is resolved via `std::fs::canonicalize()`
  and compared against the active daemon's DB path (obtained from `ProjectPaths`); if
  they resolve to the same path, `EvalError::LiveDbPath` is returned before any pool
  is opened. This prevents `eval run` from accidentally operating against the live
  database instead of a snapshot (ADR-001).
- Profile TOML is a subset of `UnimatrixConfig`. An empty TOML is the baseline.
  Candidate profiles specify only the fields under test. `ConfidenceWeights` sum
  invariant (0.92 ± 1e-9) is validated at profile load time with a user-readable error
  that names the invariant and the computed sum (SR-08 mitigation).
- Model paths in `[inference]` sections are validated at `EvalServiceLayer` construction
  time; a missing model returns a structured `EvalError::ModelNotFound` rather than
  panicking at inference time (SR-09 mitigation).
- Vector index is loaded once per `EvalServiceLayer` instance (one per profile). The
  per-profile memory footprint is acceptable for the expected snapshot sizes; the
  architecture does not share the index across profiles because each profile may have
  different index state from different weight configurations (SR-03 analysis: documented
  limit is noted in the eval CLI help text — large snapshots + many candidate profiles
  require proportionally more memory).
- Metrics computation delegates to `unimatrix_engine::test_scenarios::kendall_tau()`
  and sibling functions. These are accessible because `unimatrix-server` adds
  `unimatrix-engine` as a dependency with the `test-support` feature enabled at build
  time for the `eval` module (ADR-003 below).

---

### Component 4: `eval/report.rs` — Markdown Report Generation

**Location**: `crates/unimatrix-server/src/eval/report.rs`

**Responsibility**: Read per-scenario JSON result files from `--results` directory,
aggregate into a Markdown report with summary table, notable ranking changes, latency
distribution, entry-level promotion/demotion analysis, and zero-regression check.

**Key behaviours**:
- Sync path dispatched before tokio runtime (C-10). Pure filesystem reads and string
  formatting — no async, no database.
- Zero-regression check (AC-09) lists all scenarios where the candidate profile has
  lower MRR or P@K than baseline.

---

### Component 5: `eval/profile.rs` — Profile Config Type

**Location**: `crates/unimatrix-server/src/eval/profile.rs`

**Responsibility**: Defines `EvalProfile` (parsed from profile TOML), `AnalyticsMode`
enum, and the `EvalServiceLayer` struct. Centralises the analytics suppression decision
and the `ConfidenceWeights` validation error path.

```rust
pub enum AnalyticsMode {
    Live,       // normal SqlxStore behaviour — drain task active
    Suppressed, // no drain task; enqueue_analytics calls are no-ops
}

pub struct EvalServiceLayer {
    inner: ServiceLayer,
    db_path: PathBuf,
    profile_name: String,
}

impl EvalServiceLayer {
    pub async fn from_profile(
        db_path: &Path,
        profile: &EvalProfile,
    ) -> Result<Self, EvalError>;
}
```

---

### Component 6: `UnimatrixUdsClient` (Python)

**Location**: `product/test/infra-001/harness/uds_client.py`

**Responsibility**: Connect to a running daemon's MCP UDS socket over `AF_UNIX`, issue
the MCP initialize handshake, and expose the same 12 typed `context_*` tool methods as
`UnimatrixClient`. Enables live-path eval and integration testing without subprocess
management.

**Wire protocol**: Newline-delimited JSON over `AF_UNIX SOCK_STREAM` (identical to
stdio transport). The rmcp `JsonRpcMessageCodec` uses `\n` as delimiter — confirmed
in SCOPE.md Technical Findings. No length prefix for MCP.

**Socket path validation**: Client validates that the supplied path does not exceed 103
bytes before connecting (C-08 / FR-20 constraint).

---

### Component 7: `UnimatrixHookClient` (Python)

**Location**: `product/test/infra-001/harness/hook_client.py`

**Responsibility**: Connect to the daemon's hook IPC socket over `AF_UNIX`, send
synthetic `HookRequest` messages using the 4-byte BE length prefix + JSON framing
defined in `unimatrix_engine::wire`, and receive `HookResponse` replies.

**Wire protocol**: `struct.pack('>I', len(payload)) + payload` for writes;
`struct.unpack('>I', header)[0]` + payload read for responses. `MAX_PAYLOAD_SIZE`
= 1 MiB enforced client-side before sending (AC-14).

**Socket path**: Callers supply the path explicitly — either constructed from
`ProjectPaths.socket_path` convention or from the `daemon_server` pytest fixture.
See Open Question 3 answer below.

---

## Component Interactions

```
CLI (main.rs)
  │
  ├── Snapshot (pre-tokio, block_export_sync)
  │     └── sqlx pool (ro, raw) → VACUUM INTO → snapshot.db
  │
  └── Eval subcommand group (sync dispatch, async internals)
        ├── scenarios → raw SqlitePool (ro) → query_log scan → JSONL
        ├── run → EvalServiceLayer × N profiles → per-scenario JSON results
        │           └── ServiceLayer (read-only, analytics suppressed)
        │                 ├── EmbedServiceHandle (ONNX, RayonPool)
        │                 └── VectorIndex (HNSW, loaded from snapshot)
        └── report → JSON result files → Markdown report

Python Test Layer
  ├── UnimatrixUdsClient → AF_UNIX → mcp_socket_path → MCP tool calls
  └── UnimatrixHookClient → AF_UNIX → socket_path → HookRequest/Response
```

Data flows in one direction during eval:

1. `snapshot` produces `snapshot.db` (read-only by convention, enforced by `?mode=ro`)
2. `eval scenarios` reads `snapshot.db` → writes `scenarios.jsonl`
3. `eval run` reads `snapshot.db` + `scenarios.jsonl` → writes `results/*.json`
4. `eval report` reads `results/*.json` → writes `report.md`

No step writes back to the snapshot. No step requires a running daemon.

Steps 5 (UDS client) and 6 (hook client) are independent and require a live daemon.

---

## Technology Decisions

### VACUUM INTO: sqlx + block_export_sync wrapper (ADR-001)

`VACUUM INTO` is issued via sqlx using the `block_export_sync` helper from `export.rs`.
Rationale: rusqlite was fully removed from all crates in nxs-011 (PR #299) and cannot
be reintroduced. `VACUUM INTO` is valid SQL that sqlx executes directly. The
`block_export_sync` wrapper (a current-thread tokio runtime bridging sync CLI dispatch
to async sqlx) is the established pattern used by `export`, `eval scenarios`, and
`eval run`. `VACUUM INTO` is SQLite-specific; a future Postgres migration would require
a full snapshot command rewrite regardless of this choice.

### AnalyticsMode suppression in EvalServiceLayer (ADR-002)

The analytics write queue is suppressed at `EvalServiceLayer` construction time, not
at the SQLite layer. The SQLite `?mode=ro` enforcement alone does not prevent the
in-memory `analytics_tx` channel from accepting enqueued events — only the drain task's
subsequent write attempt would fail. Suppression at construction avoids spurious
shed-counter increments, eliminates the drain task tokio spawn entirely, and is the
explicit design decision mandated by SR-07.

### test_scenarios module via `test-support` feature (ADR-003)

`kendall_tau()` and ranking helpers live in `unimatrix_engine::test_scenarios`, gated
by `#[cfg(any(test, feature = "test-support"))]` in `crates/unimatrix-engine/src/lib.rs`.
The eval runner is production binary code (not `#[cfg(test)]`), so these functions
must be accessible via the `test-support` feature. `unimatrix-server/Cargo.toml` adds
`unimatrix-engine` with `features = ["test-support"]` for the eval binary targets.

### No new workspace crate (ADR-004)

All eval modules live in `crates/unimatrix-server/src/eval/` as a module tree. The
single-binary principle is maintained. This is the same precedent as `export.rs` and
`test_support.rs`. A separate `unimatrix-eval` crate would require a new workspace
member and a circular dependency through `unimatrix-server`'s `ServiceLayer`.

### clap 4.x nested eval subcommand dispatch (ADR-005)

The `Eval` variant in the `Command` enum carries a nested `EvalCommand` enum with
`Scenarios`, `Run`, and `Report` variants. Clap 4.x supports this via
`#[command(subcommand)]` on the inner field. The outer dispatch arm in `main()` matches
`Some(Command::Eval { command: eval_cmd })` and handles it as a sync path (C-10: before
tokio runtime). The inner `EvalCommand::Run` requires async sqlx, so it uses
`block_export_sync` (the same helper already in `export.rs`) to bridge from sync to
async without initialising an outer runtime.

### rmcp exact version pin (SR-01 mitigation)

`rmcp = { version = "=0.16.0", ... }` is already in `crates/unimatrix-server/Cargo.toml`
as an exact pin. The `transport-async-rw` blanket impl (required for UDS `serve()`) is
a transitive feature of `=0.16.0`. No Cargo.toml change is needed; the existing pin
satisfies SR-01. A smoke integration test that exercises the UDS `serve()` path covers
compile-time breakage detection.

---

## Integration Points

### Existing infrastructure reused

| Component | Location | How Used |
|-----------|----------|----------|
| `TestHarness::new()` pattern | `src/test_support.rs` | Basis for `EvalServiceLayer::from_profile()` |
| `block_export_sync()` | `src/export.rs` | Reused by `eval scenarios` and `eval run` for async bridge |
| `kendall_tau()`, `assert_ranked_above()` | `unimatrix-engine/src/test_scenarios.rs` | Metric computation in eval runner |
| `QueryLogRecord::scan_query_log_by_sessions()` | `unimatrix-store/src/query_log.rs` | Scenario extraction from snapshot |
| `RetrievalMode`, `ServiceSearchParams` | `src/services/search.rs` | Search parameterisation per profile |
| `UnimatrixConfig`, `ConfidenceWeights` | `src/infra/config.rs` | Profile TOML loading and invariant validation |
| `EmbedServiceHandle`, `RayonPool` | `src/infra/` | Inference for eval profiles |
| `HookRequest`, `HookResponse`, wire framing | `unimatrix-engine/src/wire.rs` | Hook client wire protocol |
| `UnimatrixClient` (Python) | `product/test/infra-001/harness/client.py` | Model for UDS client implementation |

### Acceptance group separation (SR-04 mitigation)

- **Offline group (D1–D4)**: `snapshot`, `eval scenarios`, `eval run`, `eval report`.
  No running daemon required. Validated independently. These gate W1-4 and W2-4.
- **Live group (D5–D6)**: `UnimatrixUdsClient`, `UnimatrixHookClient`. Require a live
  daemon and the `daemon_server` pytest fixture. These gate W1-5 and W3-1 integration
  testing. A D5/D6 daemon fixture failure cannot block D1–D4 acceptance.

---

## Integration Surface

| Integration Point | Type / Signature | Source | Notes |
|-------------------|------------------|--------|-------|
| `EvalServiceLayer::from_profile` | `async fn(db_path: &Path, profile: &EvalProfile) -> Result<EvalServiceLayer, EvalError>` | `eval/profile.rs` (new) | Opens ro pool; suppresses analytics |
| `AnalyticsMode` | `enum { Live, Suppressed }` | `eval/profile.rs` (new) | Passed at ServiceLayer construction |
| `EvalProfile` | `struct { name: String, description: Option<String>, config_overrides: UnimatrixConfig }` | `eval/profile.rs` (new) | Parsed from profile TOML |
| `EvalError` | `enum` with `ModelNotFound`, `ConfigInvariant(String)`, `LiveDbPath`, `Io(...)`, `Store(...)` | `eval/profile.rs` (new) | Structured errors; no panics. `LiveDbPath` is returned when `db_path` canonicalizes to the active daemon DB. |
| `run_snapshot` | `fn(project_dir: Option<&Path>, out: &Path) -> Result<(), Box<dyn Error>>` | `snapshot.rs` (new) | Pre-tokio; sqlx + block_export_sync; no migration |
| `run_scenarios` | `fn(db: &Path, source: ScenarioSource, limit: Option<usize>, out: &Path) -> Result<(), Box<dyn Error>>` | `eval/scenarios.rs` (new) | block_on wrapper |
| `run_eval` | `fn(db: &Path, scenarios: &Path, configs: &[PathBuf], k: usize, out: &Path) -> Result<(), Box<dyn Error>>` | `eval/runner.rs` (new) | block_on wrapper |
| `run_report` | `fn(results: &Path, scenarios: Option<&Path>, out: &Path) -> Result<(), Box<dyn Error>>` | `eval/report.rs` (new) | Sync; filesystem only |
| `ScenarioRecord` | `struct { id: String, query: String, context: ScenarioContext, baseline: Option<Baseline>, source: String, expected: Option<Vec<u64>> }` | `eval/scenarios.rs` (new) | JSONL line format |
| `ScenarioResult` | `struct { scenario_id: String, query: String, profiles: HashMap<String, ProfileResult>, comparison: Comparison }` | `eval/runner.rs` (new) | Per-scenario JSON output |
| `UnimatrixUdsClient` | Python class; `connect()`, `disconnect()`, `__enter__`, `__exit__`, 12 `context_*` typed methods | `uds_client.py` (new) | newline-delimited JSON over AF_UNIX |
| `UnimatrixHookClient` | Python class; `ping()`, `session_start()`, `session_stop()`, `pre_tool_use()`, `post_tool_use()` | `hook_client.py` (new) | 4-byte BE + JSON over AF_UNIX |
| `Command::Snapshot` | clap `Subcommand` variant: `Snapshot { out: PathBuf }` | `main.rs` | Added to existing enum |
| `Command::Eval` | clap `Subcommand` variant: `Eval { command: EvalCommand }` | `main.rs` | Nested subcommand |
| `EvalCommand` | `enum { Scenarios { ... }, Run { ... }, Report { ... } }` | `main.rs` | Three-level CLI |

---

## Open Question Answers

### 1. VACUUM INTO: sync rusqlite vs async sqlx

**Decision: sqlx + block_export_sync (revised).**

The original decision (rusqlite synchronous) was invalidated by nxs-011 (PR #299),
which fully removed rusqlite from all crates. Reintroducing it would undo that
migration. The revised decision uses `sqlx::query("VACUUM INTO ?").bind(path).execute(&pool).await`
inside `block_export_sync` — the same pattern already used by `export.rs`, `eval
scenarios`, and `eval run`. `VACUUM INTO` is valid SQL that sqlx executes directly; no
rusqlite dependency is required. A minimal read-only `SqlitePool` opened via raw
`SqliteConnectOptions` (not `SqlxStore::open()`) avoids triggering migrations.
Reference: ADR-001.

### 2. Nested eval subcommand structure in clap 4.x

**Decision: Confirmed. Pattern is `Command::Eval { command: EvalCommand }` with inner
`#[command(subcommand)]` field.**

In clap 4.x, nested subcommands work by embedding a second `Subcommand` enum inside a
variant. The dispatch in `main()` reaches into the outer match arm then inner-matches
on `eval_cmd`. C-10 dispatch ordering is preserved because the entire `Eval` arm is
placed in the sync dispatch block alongside `Hook`, `Export`, `Import`, `Version`, and
`ModelDownload` — before the tokio runtime init. The `EvalCommand::Run` variant
internally uses `block_export_sync` (creating a current-thread tokio runtime if not
already inside one) to bridge to async sqlx for its DB queries and `ServiceLayer`
construction. This matches how `export.rs` handles it. The `EvalCommand::Report` and
`EvalCommand::Scenarios` variants are also sync at the outer level and use
`block_export_sync` for their async needs. Reference: ADR-005.

### 3. Hook socket path

**Decision: `ProjectPaths.socket_path` is the hook IPC socket.**

The `socket_path` field in `ProjectPaths` resolves to
`{data_dir}/unimatrix.sock`. This is the hook IPC socket — confirmed in
`crates/unimatrix-engine/src/project.rs`. The MCP UDS socket is `mcp_socket_path` at
`{data_dir}/unimatrix-mcp.sock`. The hook socket path does NOT need a new
`ProjectPaths` field. `UnimatrixHookClient` and `UnimatrixUdsClient` accept the socket
path as a constructor argument; callers supply it from `ProjectPaths.socket_path` and
`ProjectPaths.mcp_socket_path` respectively. The `daemon_server` pytest fixture already
exposes both paths. No `ProjectPaths` struct changes required.

---

## Risk Mitigations

| Risk ID | Severity | Mitigation |
|---------|----------|------------|
| SR-01 | High | rmcp already pinned to `=0.16.0` in Cargo.toml. Smoke integration test exercises UDS `serve()` path at compile + runtime. |
| SR-02 | Med | Document snapshot operating mode: snapshot is taken against the database file path, not against a live daemon. WAL-mode SQLite provides isolation during VACUUM INTO. Document that taking a snapshot while the daemon is writing does not corrupt either database — WAL provides this guarantee. |
| SR-03 | Med | One `VectorIndex` per `EvalServiceLayer` per profile. Document the memory implication in CLI help text. The design does not share indexes because each profile can differ in index state. |
| SR-04 | High | D1–D4 acceptance criteria are validated independently with no running daemon. D5–D6 have separate criteria gated on `daemon_server` fixture. |
| SR-05 | Med | Hook socket path resolved: `ProjectPaths.socket_path`. No new field needed. |
| SR-07 | High | `AnalyticsMode::Suppressed` at `EvalServiceLayer` construction eliminates the drain task and no-ops `enqueue_analytics`. SQLite `?mode=ro` is a secondary enforcement layer. |
| SR-10 | High | `eval run` resolves `db_path` via `canonicalize()` and compares against the active daemon DB path from `ProjectPaths` before opening any pool. If they match, `EvalError::LiveDbPath` is returned. Mirrors the identical guard in `unimatrix snapshot` (AC-02). |
| SR-08 | Med | `ConfidenceWeights` validation returns a user-readable error naming the invariant (expected 0.92, got X) rather than a raw serde parse failure. |
| SR-09 | Med | `EvalServiceLayer::from_profile()` validates model paths at construction, returning `EvalError::ModelNotFound` before any inference attempt. |

---

## Module Tree

```
crates/unimatrix-server/src/
  snapshot.rs              -- D1: VACUUM INTO, sqlx + block_export_sync, pre-tokio
  eval/
    mod.rs                 -- re-exports, EvalCommand enum
    profile.rs             -- EvalProfile, EvalServiceLayer, AnalyticsMode, EvalError
    scenarios.rs           -- D2: query_log scan → JSONL
    runner.rs              -- D3: in-process A/B replay, metrics
    report.rs              -- D4: Markdown aggregation, zero-regression check
  main.rs                  -- Command::Snapshot + Command::Eval added

product/test/infra-001/
  harness/
    uds_client.py          -- D5: UnimatrixUdsClient
    hook_client.py         -- D6: UnimatrixHookClient
  tests/
    test_eval_uds.py       -- D5 test suite
    test_eval_hooks.py     -- D6 test suite
```
