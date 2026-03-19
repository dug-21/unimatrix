# nan-007: W1-3 Evaluation Harness

## Problem Statement

Every intelligence change in Unimatrix — retrieval model swap, confidence weight
tuning, NLI re-ranking (W1-4), GGUF integration (W2-4), GNN training (W3-1) —
currently ships without quantified evidence of improvement. There is no mechanism
to measure what actually changed in search result quality, catch regressions before
they reach agents, or produce a human-readable artifact that explains why a change
is safe to ship.

W1-4 (NLI cross-encoder) and W2-4 (GGUF) are both blocked on eval results as
explicit gate conditions per the product vision. W3-1 (GNN training) requires
behavioral signal evaluation to validate training label quality. Without nan-007,
these features cannot be responsibly delivered.

## Goals

1. Provide `unimatrix snapshot` — a CLI command that produces a self-contained,
   read-only SQLite copy of the full database suitable for eval (all tables,
   including analytics tables excluded from `unimatrix export`).
2. Provide `unimatrix eval scenarios` — a CLI command that mines real query
   history from a snapshot to produce JSONL eval scenarios.
3. Provide `unimatrix eval run` — a CLI command that replays scenarios through
   multiple configuration profiles in-process, producing per-scenario JSON
   results with P@K, MRR, Kendall tau, and latency metrics.
4. Provide `unimatrix eval report` — a CLI command that aggregates results into
   a Markdown report suitable for human review and PR diff.
5. Provide `UnimatrixUdsClient` (Python) — a test harness client that connects
   to a running daemon's MCP UDS socket, enabling live-path eval and integration
   testing against the production stack without spawning a subprocess.
6. Provide `UnimatrixHookClient` (Python) — a test harness client that sends
   synthetic hook events to the daemon's hook IPC socket, enabling observation
   pipeline testing without requiring Claude Code to be running.

## Non-Goals

- `unimatrix eval live` mode (subcommand form): the UDS client (deliverable 5)
  enables live eval, but a dedicated `eval live` subcommand is deferred. Callers
  use the Python client directly in test scripts for W1-5 and W3-1 needs.
- NLI model integration itself: nan-007 builds the harness that gates W1-4; it
  does not implement the NLI model. Profile TOML supports `[inference]` section
  stubs, but NLI inference code ships in W1-4.
- GGUF model integration: same as NLI — W2-4 ships the GGUF code; nan-007 provides
  the harness to measure it.
- GNN training: W3-1 scope. nan-007 provides `UnimatrixHookClient` as foundation.
- Automated CI gate: the eval report is a human-reviewed artifact. Automated
  quality gates in CI are a future concern, not nan-007 scope.
- Cross-project or multi-deployment eval: single project directory only.
- Web UI or interactive diff viewer for eval results.
- `unimatrix import` of eval results back into the live database.
- Snapshot anonymization (`--anonymize` flag): `agent_id` is role-like metadata,
  not personal identification data. No anonymization pass is required. Accepted
  removal; follow-on feature if ever needed.

## Background Research

### What Already Exists

**`TestHarness` (reusable, in `crates/unimatrix-server/src/test_support.rs`)**

Full `ServiceLayer` construction pattern already exists and is exercised in tests.
It opens a DB via `SqlxStore::open()`, builds a `VectorIndex`, wires
`EmbedServiceHandle`, `RayonPool` (crt-022, landed), `AdaptationService`,
`AuditLog`, `UsageDedup`, and `RateLimitConfig`. The eval engine's
`EvalServiceLayer::from_profile()` is this pattern with two differences:
(1) open read-only, (2) accept profile TOML overrides to `UnimatrixConfig`.

**`kendall_tau()`, `assert_ranked_above()`, ranking helpers
(`crates/unimatrix-engine/src/test_scenarios.rs`)**

Complete ranking metric infrastructure already exists. `kendall_tau()` is
O(n²) and verified via tests. `assert_in_top_k()`, `assert_tau_above()`, and
`assert_confidence_ordering()` are all available. The eval engine reuses these
directly — no new metric code for basic ranking.

**`QueryLogRecord::scan_query_log_by_sessions()` and `insert_query_log()`
(`crates/unimatrix-store/src/query_log.rs`)**

The `query_log` table captures: `session_id`, `query_text`, `ts`,
`result_count`, `result_entry_ids` (JSON array), `similarity_scores` (JSON
array), `retrieval_mode` (string), `source` (string: `"mcp"` or `"uds"`).
The `source` field enables `--retrieval-mode` filtering in scenario extraction.
Writes go through `enqueue_analytics` (fire-and-forget, ~500ms drain) — scenario
extraction operates on historical snapshot data only; no write concern.

**`RetrievalMode` enum and `ServiceSearchParams`
(`crates/unimatrix-server/src/services/search.rs`)**

`RetrievalMode::Strict` (UDS — hard filter, Active-only) vs `Flexible` (MCP —
soft penalty) are already distinct and tested. The eval engine can replay each
scenario with the appropriate mode via `ServiceSearchParams`.

**`UnimatrixConfig` and `UnimatrixConfig::InferenceConfig`
(`crates/unimatrix-server/src/infra/config.rs`)**

The config struct already has `[profile]`, `[confidence]`, `[inference]` sections.
Profile TOML for eval overrides a subset of `UnimatrixConfig` fields. The baseline
profile is an empty TOML (compiled defaults). Candidate profiles specify only
the overrides under test. `ConfidenceWeights` requires all six fields and
validates that they sum to 0.92 ± 1e-9 — the eval profile TOML must respect this
invariant or the config loader will reject it.

**`EmbedServiceHandle` / `RayonPool`
(`crates/unimatrix-server/src/infra/embed_handle.rs`,
`crates/unimatrix-server/src/infra/rayon_pool.rs`)**

crt-022 landed the rayon thread pool for CPU-bound ML inference. The eval engine
uses `EmbedServiceHandle::new()` + `start_loading(config)` pattern from
`TestHarness`. The pool is constructed once per profile per eval run.

**`UnimatrixClient` (Python, `product/test/infra-001/harness/client.py`)**

Complete MCP stdio client exists with all 12 typed tool methods. Uses
newline-delimited JSON (line-by-line reads from stdout). The `UnimatrixUdsClient`
(deliverable 5) connects over `AF_UNIX` instead of managing a subprocess. Wire
protocol is identical — see framing constraint below.

**`HookRequest` / `HookResponse` wire protocol
(`crates/unimatrix-engine/src/wire.rs`)**

Hook IPC uses 4-byte big-endian length prefix + JSON payload (ADR-005).
`write_frame()`, `read_frame()`, `serialize_request()`, `deserialize_response()`
are all public. `MAX_PAYLOAD_SIZE` = 1 MiB. Python `UnimatrixHookClient` wraps
these: struct.pack('>I', len) + json.dumps(request).encode(), then read
4 bytes + payload. The Rust types are fully specified (`HookRequest::Ping`,
`SessionRegister`, `SessionClose`, `RecordEvent`, etc.).

**`run_export()` (`crates/unimatrix-server/src/export.rs`)**

Exports JSONL knowledge records. Not suitable for eval — excludes `query_log`,
`graph_edges`, `sessions`, `shadow_evaluations`, `injection_log`. The snapshot
command is architecturally distinct: `VACUUM INTO 'snapshot.db'` produces a
complete SQLite copy, not a knowledge portability format.

**Existing CLI subcommand structure (`crates/unimatrix-server/src/main.rs`)**

`unimatrix` uses clap with subcommands: `hook`, `export`, `import`, `version`,
`model-download`, `serve`, `stop`. New subcommands `snapshot` and `eval`
(with nested subcommands `scenarios`, `run`, `report`) are added in the same
pattern. Sync subcommands (no tokio runtime) are dispatched before async paths
per C-10.

### Key Technical Findings

**MCP UDS framing is newline-delimited JSON (not length-prefixed)**

ASS-025 stated "likely 4-byte big-endian length prefix" and flagged this as
needing verification. Verification complete: rmcp 0.16.0's `AsyncRwTransport`
uses `JsonRpcMessageCodec` (a `tokio_util` framed codec) that delimits messages
by `\n`. The bridge (`bridge.rs`) is a raw byte forwarder that works precisely
because the UDS framing is identical to stdio framing. `UnimatrixUdsClient`
therefore uses the same newline-delimited JSON as `UnimatrixClient` — the
implementation is a socket-connect variant of the existing client, not a new
framing implementation.

The 4-byte length prefix framing (ADR-005) is **only** the hook IPC protocol
(`unimatrix-engine::wire`), not the MCP transport.

**`transport-io` feature is enabled; `transport-async-rw` is not explicitly
listed but is available via `(OwnedReadHalf, OwnedWriteHalf)` blanket impl**

The `run_session()` in `mcp_listener.rs` calls `server.serve((read_half,
write_half))` which routes through `IntoTransport<_, _, TransportAdapterAsyncRW>`.
This blanket impl is in `rmcp::transport::async_rw` and applies whenever
`transport-async-rw` is enabled. The feature is transitively pulled in despite
not being listed explicitly in Cargo.toml.

**Read-only snapshot enforcement requires a SQLite open-mode URI**

`SqlxStore::open()` triggers migration (this is how schema upgrades work). The
eval engine must not migrate a snapshot. Two options: (a) a new
`SqlxStore::open_readonly(path)` function that uses `?mode=ro` in the SQLite
connection URI, or (b) skip `SqlxStore` entirely and use a raw `sqlx::SqlitePool`
with `SqliteConnectOptions::read_only(true)`. The eval engine needs enough of the
store API to run searches (`scan_entries`, vector map reads, co-access reads) —
a minimal read-only pool is sufficient and avoids modifying `SqlxStore`.

**`unimatrix snapshot` is a sync subcommand (no tokio runtime)**

Like `export`, snapshot opens the DB directly. `VACUUM INTO` is a SQLite pragma
that can be executed via a synchronous rusqlite connection, avoiding the tokio
runtime entirely. This is consistent with the C-10 dispatch ordering rule.
Alternatively, a minimal tokio block (like `block_export_sync`) is acceptable
for sqlx-based `VACUUM INTO` execution.

**Crate placement: no new crate needed**

ASS-025 proposed a possible `crates/unimatrix-eval/` crate. Codebase analysis
confirms the single-binary principle and the precedent of `export.rs` and
`test_support.rs` both living in `unimatrix-server`. The eval infrastructure
belongs in `crates/unimatrix-server/src/eval/` as a module tree, not a new
workspace member.

**Hook IPC socket vs MCP socket are separate**

Two distinct socket files per daemon:
- `unimatrix-mcp.sock` — MCP UDS (newline-delimited JSON via rmcp)
- `unimatrix-hook.sock` (or equivalent) — hook IPC (4-byte length prefix)

`UnimatrixUdsClient` connects to the MCP socket.
`UnimatrixHookClient` connects to the hook socket.

## Proposed Approach

### Deliverable 1: `unimatrix snapshot`

New file `crates/unimatrix-server/src/snapshot.rs`. New `Snapshot` subcommand
in `main.rs` `Command` enum. Sync path (dispatched before tokio runtime). Opens
DB directly, executes `VACUUM INTO 'output.db'`. Validates that output path
does not resolve to the active DB path (security requirement). `--anonymize`
flag runs a post-copy pass on the snapshot file replacing `agent_id` and
`session_id` values with SHA-256-seeded consistent pseudonyms using a random
salt stored in a companion `.meta` JSON file.

### Deliverable 2: `unimatrix eval scenarios`

New subcommand `eval scenarios` (nested under an `Eval` subcommand group).
Implementation in `crates/unimatrix-server/src/eval/scenarios.rs`. Opens
snapshot DB read-only, scans `query_log`, joins with `entries` to produce
JSONL output. Supports `--source mcp|uds|all`, `--limit N`. Produces the
scenario format defined in ASS-025 (id, query, context, baseline, source,
expected). Also accepts hand-authored scenarios in the same format.

### Deliverable 3: `unimatrix eval run`

New subcommand `eval run`. Implementation in
`crates/unimatrix-server/src/eval/runner.rs`. Constructs one `ServiceLayer`
per profile config via `EvalServiceLayer::from_profile()` (extends `TestHarness`
pattern, read-only DB open). Replays each scenario through each profile. Writes
per-scenario JSON to `--out` directory. Computes P@K, MRR, kendall_tau (reused
from `unimatrix_engine::test_scenarios`), rank change list, latency delta.

Profile TOML is a subset of `UnimatrixConfig` with a required `[profile] name`
field. Baseline profile is an empty TOML file. Candidate profile specifies only
the overrides under test.

### Deliverable 4: `unimatrix eval report`

New subcommand `eval report`. Implementation in
`crates/unimatrix-server/src/eval/report.rs`. Reads result JSONs from
`--results` directory, aggregates into Markdown report with: summary table,
notable ranking changes sorted by Kendall tau drop, latency distribution,
entry-level analysis (most promoted/demoted entries), zero-regression check list.

### Deliverable 5: `UnimatrixUdsClient` (Python)

New file `product/test/infra-001/harness/uds_client.py`. Connects to the
daemon's MCP UDS socket via `socket.AF_UNIX`. Wire protocol is identical to
stdio: newline-delimited JSON (same `_send`/`_read_response` logic, different
connection setup). Exposes the same 12 typed tool methods as `UnimatrixClient`.
Supports `connect()`/`disconnect()` and context manager protocol. New test suite
`product/test/infra-001/tests/test_eval_uds.py`.

### Deliverable 6: `UnimatrixHookClient` (Python)

New file `product/test/infra-001/harness/hook_client.py`. Connects to the
daemon's hook IPC socket via `socket.AF_UNIX`. Wire protocol: 4-byte BE length
prefix + JSON body (matches `unimatrix_engine::wire` exactly). Typed methods
for `ping()`, `session_start()`, `session_stop()`, `pre_tool_use()`,
`post_tool_use()`. New test suite
`product/test/infra-001/tests/test_eval_hooks.py`.

## Acceptance Criteria

- AC-01: `unimatrix snapshot --out <path>` creates a valid SQLite file at `<path>`
  containing all tables present in the source database (entries, query_log,
  graph_edges, co_access, sessions, shadow_evaluations, and all others).
- AC-02: `unimatrix snapshot` refuses with a non-zero exit code and an error
  message when `--out` resolves to the same file as the active daemon's database.
- AC-03: `unimatrix eval scenarios --db <snapshot>` produces JSONL where each
  line is a valid scenario with `id`, `query`, `context`, `baseline.entry_ids`,
  `baseline.scores`, and `source` fields.
- AC-04: `unimatrix eval scenarios --retrieval-mode uds` filters to UDS-sourced
  scenarios (`source="uds"`); `--retrieval-mode mcp` filters to MCP-sourced;
  `--retrieval-mode all` returns both.
- AC-05: `unimatrix eval run --db <snapshot> --scenarios <file> --configs baseline.toml,candidate.toml`
  opens the snapshot in read-only mode (`?mode=ro`) and produces no writes to
  the snapshot database.
- AC-05b: `unimatrix eval run` refuses with a non-zero exit code when `--db`
  resolves (via `canonicalize`) to the same path as the active daemon's database,
  matching the guard already required on `unimatrix snapshot`.
- AC-06: `unimatrix eval run` produces one JSON result file per scenario in `--out`
  containing `profiles`, `comparison.kendall_tau`, `comparison.mrr_delta`,
  `comparison.p_at_3_delta`, and `comparison.latency_overhead_ms`.
- AC-07: `unimatrix eval run` P@K uses `baseline.entry_ids` as soft ground truth
  for query_log-sourced scenarios; uses `expected` as hard labels for hand-authored
  scenarios.
- AC-08: `unimatrix eval report --results <dir>` produces a Markdown file with
  a summary table, notable ranking changes section, and a zero-regression check
  section.
- AC-09: `unimatrix eval report` zero-regression check lists all scenarios where
  the candidate profile has lower MRR or P@K than baseline; the list is empty
  when no regressions exist.
- AC-10: `UnimatrixUdsClient` connects to a running daemon's MCP UDS socket,
  completes the MCP initialize handshake, and executes all 12 `context_*` tool
  methods with the same results as `UnimatrixClient` for equivalent queries.
- AC-11: `UnimatrixUdsClient` supports the context manager protocol
  (`__enter__`/`__exit__`) with automatic connect/initialize/disconnect lifecycle.
- AC-12: `UnimatrixHookClient` connects to the daemon's hook socket, sends a
  `Ping` request, and receives a `Pong` response.
- AC-13: `UnimatrixHookClient.session_start()` followed by `session_stop()`
  results in a session record visible in the database (verifiable via
  `context_status`).
- AC-14: `UnimatrixHookClient` rejects payloads exceeding `MAX_PAYLOAD_SIZE`
  (1 MiB) with a descriptive error before sending.
- AC-15: `unimatrix snapshot`, `unimatrix eval scenarios`, `unimatrix eval run`,
  and `unimatrix eval report` are all registered as subcommands in the `unimatrix`
  binary (visible in `--help` output).

## Constraints

**Binary architecture**: Single binary, new subcommands only. No new workspace
crate. Eval modules live in `crates/unimatrix-server/src/eval/` as a module
tree alongside existing `export.rs` and `snapshot.rs`.

**Snapshot read-only enforcement**: The eval engine must open the snapshot with
SQLite `?mode=ro` or `SqliteConnectOptions::read_only(true)`. `SqlxStore::open()`
triggers migration and must not be called on a snapshot. A new
`SqlxStore::open_readonly()` function or a raw pool constructed with
`SqliteConnectOptions` is required.

**No migration on snapshot**: The snapshot is a frozen copy. The eval engine
must not run `SqlxStore::open()` (which triggers migration) on it. This is
enforced at the SQLite layer via read-only mode — writes will fail at the OS
level regardless.

**MCP UDS framing is newline-delimited JSON**: The `UnimatrixUdsClient` uses the
same line-by-line JSON protocol as `UnimatrixClient`. The rmcp `AsyncRwTransport`
`JsonRpcMessageCodec` uses `\n` as delimiter. No length-prefix is used for MCP.
(The 4-byte length prefix in `unimatrix_engine::wire` is for hook IPC only.)

**Hook IPC framing is 4-byte BE length prefix + JSON**: `UnimatrixHookClient`
must use `struct.pack('>I', len(payload)) + payload` for writes and
`struct.unpack('>I', header)[0]` for reads. Maximum payload is 1 MiB.

**`ConfidenceWeights` sum invariant**: Profile TOML with custom confidence
weights must have all six fields summing to 0.92 ± 1e-9 or config loading fails.
Baseline profile (empty TOML) uses compiled defaults; this constraint applies
only to candidate profiles overriding the `[confidence]` section.

**Test infrastructure is cumulative**: The eval test suites extend existing
fixtures and helpers. `TestHarness` is extended, not replaced. The existing
`kendall_tau()` and ranking helpers in `unimatrix-engine::test_scenarios` are
reused directly.

**Tokio runtime constraint for snapshot**: Like `export`, the `snapshot`
subcommand must be dispatched before the tokio runtime (C-10 ordering rule). If
`VACUUM INTO` requires async sqlx, use a minimal `block_on` wrapper as
`run_export` does; or use rusqlite synchronously.

**Socket path length limit**: The MCP UDS socket path must not exceed 103 bytes
(C-08 / FR-20). `UnimatrixUdsClient` should validate path length before
connecting.

**Python harness location**: All Python additions go in
`product/test/infra-001/harness/` (existing convention). Test files go in
`product/test/infra-001/tests/`.

## Resolved Decisions (pre-design)

- **Read-only store API**: No `SqlxStore` API change. The eval runner constructs
  a raw `sqlx::SqlitePool` with `SqliteConnectOptions::read_only(true)` directly.
  This keeps eval infrastructure self-contained.
- **P@K dual-mode semantics**: Confirmed. `baseline.entry_ids` is soft ground
  truth for query_log-sourced scenarios; `expected` is hard labels for
  hand-authored scenarios.
- **Daemon fixture**: `daemon_server` pytest fixture (starts server in daemon mode,
  yields socket paths) is the correct approach for D5/D6 test suites.
- **Snapshot anonymization**: Removed from scope. `agent_id` is role-like metadata,
  not personal identification data. No anonymization pass required.

## Open Questions for Architect

1. **`VACUUM INTO` sync vs async**: Recommend rusqlite (synchronous, pre-tokio,
   consistent with `export`'s pattern). Architect to confirm or choose sqlx async
   with `block_on`.

2. **Nested `eval` subcommand structure**: Three-level CLI (`unimatrix eval
   scenarios|run|report`) requires `Eval` subcommand with nested `EvalCommand`
   variants in clap 4.x. Architect to confirm this pattern works and document
   the dispatch ordering relative to C-10.

3. **Hook socket path discovery**: `UnimatrixHookClient` needs the hook socket
   path. Architect to confirm the path convention (likely
   `{project_dir}/.unimatrix/{hash}/unimatrix-hook.sock`) matches `ProjectPaths`.

## Tracking

https://github.com/dug-21/unimatrix/issues/321

---

## Knowledge Stewardship

Research foundation: ASS-025 (`product/research/ass-025/RECOMMENDATIONS.md`).
Key correction from ASS-025: MCP UDS framing is newline-delimited JSON (same as
stdio transport), not 4-byte length-prefixed. The length prefix is hook IPC only.
