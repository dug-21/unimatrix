# SPECIFICATION: nan-007 — Evaluation Harness (W1-3)

**Feature ID**: nan-007
**Phase**: Nanoprobes
**Upstream scope**: `product/features/nan-007/SCOPE.md`
**Risk assessment**: `product/features/nan-007/SCOPE-RISK-ASSESSMENT.md`
**Research foundation**: `product/research/ass-025/RECOMMENDATIONS.md`

---

## Objective

Provide a complete evaluation harness for Unimatrix intelligence changes — retrieval
model swaps, confidence weight tuning, NLI re-ranking, and GGUF integration — so that
every change is measured against real query scenarios before reaching agents.
Regressions are caught before production; improvements are demonstrated, not assumed;
the human reviewer sees exactly what changed and why.

The harness consists of six deliverables: four offline Rust CLI subcommands (D1–D4)
that operate without a running daemon against a frozen snapshot, and two Python
clients (D5–D6) that connect to a live daemon's sockets for live-path validation.
D1–D4 gate W1-4 and W2-4. D5–D6 extend the harness for W1-5 and W3-1.

---

## Functional Requirements

### D1 — `unimatrix snapshot`

**FR-01**: The system shall provide a `unimatrix snapshot` CLI subcommand that
produces a self-contained SQLite copy of the active database using `VACUUM INTO`.

**FR-02**: The subcommand shall accept `--out <path>` (required) specifying the output
file path, and `--project-dir <path>` (optional, defaults to the standard project
directory discovery logic).

**FR-03**: The snapshot shall include all database tables without exception: `entries`,
`entry_tags`, `co_access`, `feature_entries`, `outcome_index`, `agent_registry`,
`audit_log`, `counters`, `graph_edges`, `sessions`, `observations`, `query_log`,
`shadow_evaluations`, `injection_log`, `observation_metrics`, `topic_deliveries`. This
is an unconditional full copy — not a subset.

**FR-04**: The subcommand shall refuse with a non-zero exit code and a descriptive
error message when `--out` resolves (after symlink resolution) to the same file path as
the active daemon's database. The error message shall name both paths.

**FR-05**: The `snapshot` subcommand shall be dispatched before the tokio runtime
(consistent with the C-10 pre-async dispatch ordering rule already established for
`export`). Implementation shall use either rusqlite synchronously or a minimal
`block_on` wrapper, not a full tokio runtime.

**FR-06**: The snapshot file produced shall be a valid SQLite3 file openable by any
SQLite reader without migration, schema modification, or write access.

### D2 — `unimatrix eval scenarios`

**FR-07**: The system shall provide a `unimatrix eval scenarios` CLI subcommand
(nested under an `eval` subcommand group) that mines the `query_log` table from a
snapshot database and writes eval scenarios in JSONL format.

**FR-08**: The subcommand shall accept: `--db <path>` (required, path to snapshot
SQLite file), `--out <path>` (required, output JSONL file), `--limit <N>` (optional,
maximum scenario count), `--retrieval-mode mcp|uds|all` (optional, default `all`).

**FR-09**: Each output JSONL line shall be a valid scenario object with the following
fields: `id` (string, unique within the file), `query` (string), `context` (object
with `agent_id`, `feature_cycle`, `session_id`, `retrieval_mode`), `baseline` (object
with `entry_ids` array of integers and `scores` array of floats), `source` (string,
`"mcp"` or `"uds"`), `expected` (null for query-log-sourced scenarios).

**FR-10**: The `--retrieval-mode mcp` flag shall filter to scenarios where
`query_log.source = "mcp"` only; `--retrieval-mode uds` to `source = "uds"` only;
`--retrieval-mode all` returns both without filtering.

**FR-11**: The subcommand shall open the snapshot database in read-only mode
(`?mode=ro` or `SqliteConnectOptions::read_only(true)`). It shall produce no writes
to the snapshot.

**FR-12**: Hand-authored scenarios in the same JSONL format (with `expected` set to a
list of entry IDs rather than null, and `baseline` absent or null) shall be accepted
as valid input to `unimatrix eval run` without modification.

### D3 — `unimatrix eval run`

**FR-13**: The system shall provide a `unimatrix eval run` CLI subcommand that replays
eval scenarios through multiple configuration profiles in-process (Rust, no spawned
server, no IPC), producing one JSON result file per scenario.

**FR-14**: The subcommand shall accept: `--db <path>` (required, snapshot), `--scenarios
<path>` (required, JSONL file), `--configs <comma-separated paths>` (required, one or
more profile TOML files), `--out <dir>` (required, output directory for result JSONs),
`--k <N>` (optional, K for P@K, default 5).

**FR-44**: `eval run` shall resolve `--db` via `std::fs::canonicalize()` and compare
the resolved path against the active daemon's database path (also resolved via
`canonicalize()`). If the two paths are identical, the subcommand shall refuse with a
non-zero exit code and a descriptive error message that names both resolved paths. An
error from `canonicalize()` on either path shall cause the subcommand to fail with a
descriptive error rather than proceed. This mirrors the path-guard already required for
`unimatrix snapshot` (FR-04 / NFR-06).

**FR-15**: The system shall provide `EvalServiceLayer::from_profile(db_path,
profile_config)` that: (a) opens the snapshot database in read-only mode, (b)
constructs a `ServiceLayer` with profile-specified overrides applied to
`UnimatrixConfig`, (c) builds the vector index from the snapshot without triggering
migration, and (d) disables the analytics write queue so that no `enqueue_analytics`
calls attempt writes during eval.

**FR-16**: The analytics write queue shall be disabled at `EvalServiceLayer`
construction — not merely blocked by the read-only SQLite mode. The in-memory queue
must be no-op'd or suppressed entirely (SR-07 compliance). A no-op `AnalyticsMode`
variant or equivalent design shall be chosen by the architect; this requirement is
the enforcement target.

**FR-17**: Profile TOML format shall be a named subset of `UnimatrixConfig` with a
required `[profile]` section containing `name` (string) and optional `description`
(string). The baseline profile is an empty TOML file (uses compiled defaults). A
candidate profile specifies only the config fields being tested under `[confidence]`,
`[inference]`, or any other `UnimatrixConfig` section.

**FR-18**: When a candidate profile overrides `[confidence]` weights, all six weight
fields must be present and must sum to 0.92 ± 1e-9, or `EvalServiceLayer` construction
shall fail with a structured error that names the invariant and the actual sum.
This error must be user-readable, not a raw serde parse failure (SR-08 compliance).

**FR-19**: The eval runner shall compute the following metrics per scenario per profile:
P@K (precision at K), MRR (mean reciprocal rank). For cross-profile comparison per
scenario: Kendall tau (rank correlation between profile result lists), rank change list
(entries that moved positions), MRR delta, P@K delta, and latency overhead in
milliseconds.

**FR-20**: P@K computation shall use dual-mode semantics: for query-log-sourced
scenarios (`expected = null`), `baseline.entry_ids` is soft ground truth; for
hand-authored scenarios (`expected` is a list), `expected` is used as hard labels.

**FR-21**: Each per-scenario result JSON in `--out` shall contain: `scenario_id`,
`query`, `profiles` (object keyed by profile name, each with `entries`, `latency_ms`,
`p_at_k`, `mrr`), and `comparison` (object with `kendall_tau`, `rank_changes`,
`mrr_delta`, `p_at_k_delta`, `latency_overhead_ms`).

**FR-22**: The existing `kendall_tau()` function and ranking helpers in
`unimatrix-engine::test_scenarios` shall be reused directly by the eval runner's
metrics computation. No duplicate metric implementation is permitted.

**FR-23**: `EvalServiceLayer::from_profile()` shall validate any configured model paths
at construction time and return a structured error if a required model file is absent
or unreadable. Construction shall not panic under any profile configuration (SR-09
compliance).

**FR-24**: `SqlxStore::open()` (which triggers schema migration) shall not be called
on a snapshot database. The eval runner shall use a raw `sqlx::SqlitePool` constructed
with `SqliteConnectOptions::read_only(true)`, keeping eval infrastructure
self-contained with no `SqlxStore` API modification.

### D4 — `unimatrix eval report`

**FR-25**: The system shall provide a `unimatrix eval report` CLI subcommand that
reads result JSON files from the `--results` directory and writes a Markdown report.

**FR-26**: The subcommand shall accept: `--results <dir>` (required), `--out <path>`
(required, output Markdown file), `--scenarios <path>` (optional, JSONL file for
annotating queries with their text in the report).

**FR-27**: The Markdown report shall contain, in order:
1. A summary table with per-profile aggregate P@K, MRR, average latency, and rank
   change rate, plus a delta column relative to the baseline profile.
2. A notable ranking changes section: queries where result order changed most
   significantly (sorted by Kendall tau drop), each showing a side-by-side rank table
   of baseline vs. candidate profile entry lists.
3. A latency distribution section: histogram or percentile table of `latency_ms` per
   profile.
4. An entry-level analysis section: which entries gained or lost rank most across all
   scenarios (most promoted and most demoted entries).
5. A zero-regression check section: explicit list of all scenarios where any candidate
   profile has lower MRR or P@K than the baseline profile.

**FR-28**: The zero-regression check section shall produce an explicit empty-list
indicator when no regressions exist. The zero-regression check list is the artifact
the human reads to decide whether the change is safe to ship.

**FR-29**: The report subcommand shall not contain any automated pass/fail gate logic.
It produces a human-reviewed artifact only. No exit code based on regression count
is returned (SR-06 constraint).

### D5 — `UnimatrixUdsClient` (Python)

**FR-30**: The system shall provide a Python class `UnimatrixUdsClient` in
`product/test/infra-001/harness/uds_client.py` that connects to a running daemon's
MCP UDS socket via `socket.AF_UNIX`.

**FR-31**: `UnimatrixUdsClient.__init__` shall accept `socket_path: str | Path` and
`timeout: float` (default `DEFAULT_TIMEOUT`). It shall validate that the socket path
does not exceed 103 bytes before attempting connection (C-08 / FR-20 from SCOPE.md).

**FR-32**: The client shall expose `connect()`, `disconnect()`, and the context manager
protocol (`__enter__` / `__exit__`). `connect()` shall open the `AF_UNIX` socket and
complete the MCP `initialize` handshake. `disconnect()` shall send MCP `shutdown` and
close the socket.

**FR-33**: The wire protocol shall be newline-delimited JSON, identical to
`UnimatrixClient` (which uses stdio). The `_send` and `_read_response` logic shall
differ only in transport setup (socket vs. subprocess pipe). No length prefix is used
for MCP messages over UDS.

**FR-34**: `UnimatrixUdsClient` shall expose the same 12 typed tool methods as
`UnimatrixClient`: `context_search`, `context_store`, `context_lookup`, `context_get`,
`context_correct`, `context_deprecate`, `context_status`, `context_briefing`,
`context_quarantine`, `context_enroll`, `context_cycle`, `context_cycle_review`.

**FR-35**: A test suite `product/test/infra-001/tests/test_eval_uds.py` shall be
provided covering: UDS connection lifecycle, tool call parity between `UnimatrixUdsClient`
and `UnimatrixClient` for equivalent queries, concurrent client behavior (multiple UDS
clients against a single daemon), and validation that UDS-sourced queries appear as
`source="uds"` in `query_log`.

### D6 — `UnimatrixHookClient` (Python)

**FR-36**: The system shall provide a Python class `UnimatrixHookClient` in
`product/test/infra-001/harness/hook_client.py` that connects to the daemon's hook
IPC socket via `socket.AF_UNIX` using the 4-byte big-endian length-prefix + JSON
body wire protocol defined in `unimatrix_engine::wire`.

**FR-37**: `UnimatrixHookClient` shall expose typed methods: `ping()`,
`session_start(session_id, feature_cycle, agent_role)`,
`session_stop(session_id, outcome)`, `pre_tool_use(session_id, tool, input)`,
`post_tool_use(session_id, tool, response_size, response_snippet)`. Each method shall
return a typed `HookResponse` object.

**FR-38**: Write framing shall be `struct.pack('>I', len(payload)) + payload` where
`payload = json.dumps(request).encode()`. Read framing shall be: read 4 bytes,
`struct.unpack('>I', header)[0]` for length, then read exactly that many bytes as
JSON body.

**FR-39**: The client shall reject payloads exceeding `MAX_PAYLOAD_SIZE` (1 MiB) with
a descriptive `ValueError` before sending, not after.

**FR-40**: A test suite `product/test/infra-001/tests/test_eval_hooks.py` shall be
provided covering: session lifecycle round-trips (start, pre-tool-use, post-tool-use,
stop), verification that a session record is visible in the database after
`session_stop` (confirmed via `UnimatrixUdsClient.context_status`), session keyword
population (col-022 keywords field), and invalid payload rejection (oversized payload,
malformed JSON).

**FR-41**: The hook socket path convention shall be confirmed by the architect before
implementation. The path is expected to follow `{project_dir}/.unimatrix/{hash}/unimatrix-hook.sock`
(matching `ProjectPaths`), but this must be verified against `ProjectPaths` and
documented in the architecture output before the implementer codes the path discovery.

### CLI Registration

**FR-42**: The subcommands `snapshot` and `eval` (with nested subcommands `scenarios`,
`run`, `report`) shall be registered in the `unimatrix` binary's clap command enum
and visible in `unimatrix --help` and `unimatrix eval --help` output.

**FR-43**: All new Rust modules (`snapshot.rs`, `eval/mod.rs`, `eval/scenarios.rs`,
`eval/runner.rs`, `eval/report.rs`) shall reside in
`crates/unimatrix-server/src/`. No new workspace crate is introduced.

---

## Non-Functional Requirements

**NFR-01 (Performance — snapshot)**: `unimatrix snapshot` against a database of up
to 50,000 entries shall complete in under 60 seconds. `VACUUM INTO` SQLite performance
is the bounding factor; no additional performance constraint is imposed beyond this.

**NFR-02 (Performance — eval run)**: `unimatrix eval run` shall process at least
5 scenarios per second per profile on reference hardware (a developer laptop with
at least 4 cores). For 500 scenarios and 2 profiles this bounds the run to under
3 minutes. The HNSW index load time per profile contributes to startup cost but
not per-scenario cost.

**NFR-03 (Memory — vector index)**: If the architect determines that loading a
separate HNSW vector index per profile is prohibitive for multi-profile runs against
large snapshots (SR-03), the eval runner shall share a single read-only index across
all `EvalServiceLayer` instances. This is an architectural decision; this requirement
sets the constraint that memory usage must not prevent a 2-profile run against a
snapshot of up to 50,000 entries on a machine with 8 GB RAM.

**NFR-04 (Correctness — read-only enforcement)**: The snapshot database shall remain
byte-for-byte identical before and after `unimatrix eval run`. This is enforced at
the SQLite layer by `?mode=ro` and additionally by the analytics queue suppression
required by FR-16. The SHA-256 hash of the snapshot file must not change after an
eval run.

**NFR-05 (Correctness — metric reproducibility)**: For identical input (same
scenarios, same profile, same snapshot), `unimatrix eval run` shall produce identical
numeric results across repeated invocations on the same machine. Non-determinism in
ranking due to float tie-breaking must be documented if it cannot be eliminated.

**NFR-06 (Security — snapshot path validation)**: The snapshot path comparison
(FR-04) shall use `std::fs::canonicalize` on both paths before comparison to prevent
symlink-based bypass. An error from `canonicalize` (e.g., source DB not found) shall
cause the subcommand to fail with a descriptive error rather than proceed.

**NFR-07 (Security — no credentials in snapshot)**: The snapshot command shall
document in its `--help` text that the snapshot contains all database content,
including `agent_id` and `session_id` values, and should be stored accordingly. No
automated scrubbing of PII is in scope (snapshot anonymization is explicitly removed).

**NFR-08 (Compatibility — rmcp version)**: `rmcp` shall be pinned to an exact version
(`=0.16.0` or whatever the workspace currently pins). A compile-time smoke test or
integration test shall exercise the UDS `serve()` path directly so that a framing
change in a future rmcp version is detected loudly at the test layer (SR-01 mitigation).

**NFR-09 (Compatibility — Python)**: `UnimatrixUdsClient` and `UnimatrixHookClient`
shall be compatible with the Python version already required by
`product/test/infra-001`. No new Python dependencies beyond the standard library
(`socket`, `struct`, `json`, `pathlib`) are permitted for the two client modules.

**NFR-10 (Maintainability — cumulative test infrastructure)**: The eval test suites
shall extend existing `TestHarness` and fixtures. The `daemon_server` pytest fixture
(entry #1928) is the correct approach for D5/D6 test setup. No isolated scaffolding
that duplicates existing test infrastructure is permitted.

---

## Acceptance Criteria

Acceptance criteria are divided into two independent groups. Group 1 (offline, D1–D4)
has no daemon dependency and can be validated with a snapshot database alone.
Group 2 (live, D5–D6) requires a running daemon with known socket paths via the
`daemon_server` pytest fixture.

### Group 1: Offline Acceptance (D1–D4 — no daemon required)

**AC-01**: `unimatrix snapshot --out <path>` creates a valid SQLite file at `<path>`
containing all tables present in the source database (entries, query_log, graph_edges,
co_access, sessions, shadow_evaluations, and all others present in the schema).
*Verification*: Open the snapshot with a SQLite reader; run `SELECT name FROM
sqlite_master WHERE type='table'` and confirm all expected table names are present.

**AC-02**: `unimatrix snapshot` refuses with a non-zero exit code and an error message
when `--out` resolves (via `canonicalize`) to the same file as the active daemon's
database.
*Verification*: Run `unimatrix snapshot --out <live-db-path>`; assert exit code != 0
and stderr contains both resolved paths.

**AC-03**: `unimatrix eval scenarios --db <snapshot>` produces JSONL where each line
is a valid scenario object with `id`, `query`, `context`, `baseline.entry_ids`,
`baseline.scores`, and `source` fields.
*Verification*: Parse each line of output as JSON; assert all required fields are
present and have correct types.

**AC-04**: `unimatrix eval scenarios --db <snapshot> --retrieval-mode uds` produces
only scenarios where `source = "uds"`; `--retrieval-mode mcp` produces only
`source = "mcp"`; `--retrieval-mode all` produces both.
*Verification*: Run each flag variant against a snapshot with known `query_log` rows
of both source types; assert the `source` field of every output line matches the filter.

**AC-05**: `unimatrix eval run --db <snapshot> --scenarios <file> --configs
baseline.toml,candidate.toml` opens the snapshot in read-only mode (`?mode=ro`) and
produces no writes to the snapshot database.
*Verification*: Record the SHA-256 hash of the snapshot before `eval run`; assert the
hash is unchanged after `eval run` completes.

**AC-06**: `unimatrix eval run` produces one JSON result file per scenario in `--out`
containing `profiles`, `comparison.kendall_tau`, `comparison.mrr_delta`,
`comparison.p_at_k_delta`, and `comparison.latency_overhead_ms`.
*Verification*: Parse each result JSON; assert all required fields are present and
numeric.

**AC-07**: `unimatrix eval run` uses `baseline.entry_ids` as soft ground truth for
P@K on query-log-sourced scenarios (`expected = null`), and uses `expected` as hard
labels for hand-authored scenarios.
*Verification*: Run eval with a hand-authored scenario that has a known `expected`
list and confirm P@K is computed against `expected`; run with a query-log scenario
and confirm P@K uses `baseline.entry_ids`.

**AC-08**: `unimatrix eval report --results <dir>` produces a Markdown file with a
summary table, notable ranking changes section, latency distribution section,
entry-level analysis section, and a zero-regression check section.
*Verification*: Run on a prepared results directory; assert the output file contains
all five section headers.

**AC-09**: The zero-regression check section lists all scenarios where the candidate
profile has lower MRR or P@K than baseline; the section contains an explicit
empty-list indicator when no regressions exist.
*Verification*: Run with results from a candidate profile that is known to degrade
one scenario; assert that scenario appears in the zero-regression list. Run with
results where no degradation exists; assert the empty-list indicator is present.

**AC-15**: `unimatrix snapshot`, `unimatrix eval scenarios`, `unimatrix eval run`,
and `unimatrix eval report` are all registered as subcommands in the `unimatrix`
binary and visible in `--help` output.
*Verification*: `unimatrix --help` contains `snapshot`; `unimatrix eval --help`
contains `scenarios`, `run`, and `report`.

**AC-16**: `unimatrix eval run --db <active-db>` returns a non-zero exit code with a
descriptive error message when `--db` resolves (via `canonicalize`) to the same file
as the active daemon's database.
*Verification*: Invoke `unimatrix eval run --db <live-db-path> --scenarios <file>
--configs <profile> --out <dir>`; assert exit code != 0 and stderr contains a message
naming both resolved paths (FR-44).

### Group 2: Live Acceptance (D5–D6 — daemon required via `daemon_server` fixture)

**AC-10**: `UnimatrixUdsClient` connects to a running daemon's MCP UDS socket,
completes the MCP `initialize` handshake, and executes all 12 `context_*` tool
methods with the same results as `UnimatrixClient` for equivalent queries.
*Verification*: `test_eval_uds.py` — start daemon with `daemon_server` fixture;
run a query via `UnimatrixUdsClient`; run the same query via `UnimatrixClient` via
stdio; assert results are identical.

**AC-11**: `UnimatrixUdsClient` supports the context manager protocol
(`__enter__` / `__exit__`) with automatic connect/initialize/disconnect lifecycle.
*Verification*: `test_eval_uds.py` — use `with UnimatrixUdsClient(...) as client:`
block; assert connection is established on entry and closed on exit without explicit
`connect()` / `disconnect()` calls.

**AC-12**: `UnimatrixHookClient` connects to the daemon's hook socket, sends a `Ping`
request, and receives a `Pong` response within the configured timeout.
*Verification*: `test_eval_hooks.py` — `hook_client.ping()` returns a `HookResponse`
with `type = "Pong"`.

**AC-13**: `UnimatrixHookClient.session_start()` followed by `session_stop()` results
in a session record visible in the database (verifiable via `context_status`).
*Verification*: `test_eval_hooks.py` — call `session_start` then `session_stop` via
hook client; call `context_status` via `UnimatrixUdsClient`; assert the session
appears in status output.

**AC-14**: `UnimatrixHookClient` rejects payloads exceeding `MAX_PAYLOAD_SIZE` (1 MiB)
with a descriptive `ValueError` before sending.
*Verification*: `test_eval_hooks.py` — construct a payload of 1,048,577 bytes; assert
that calling any `UnimatrixHookClient` method raises `ValueError` before any bytes are
sent on the socket.

---

## Domain Models

### EvalScenario

The unit of evaluation work. One row in the JSONL scenario file.

```
EvalScenario {
    id:               string          -- unique identifier, e.g. "qlog-4921"
    query:            string          -- natural language query text
    context:          ScenarioContext -- enrichment from query_log / session join
    baseline:         ScenarioBaseline | null  -- soft ground truth (query-log scenarios)
    source:           "mcp" | "uds"  -- retrieval path that produced this scenario
    expected:         int[] | null   -- hard labels (hand-authored scenarios only)
}

ScenarioContext {
    agent_id:         string
    feature_cycle:    string
    session_id:       string
    retrieval_mode:   "flexible" | "strict"
}

ScenarioBaseline {
    entry_ids:  int[]    -- ordered list of entry IDs as returned at query time
    scores:     float[]  -- similarity scores parallel to entry_ids
}
```

A scenario is **query-log-sourced** when `baseline` is non-null and `expected` is
null. P@K uses `baseline.entry_ids` as soft ground truth.

A scenario is **hand-authored** when `expected` is a non-null list and `baseline` may
be absent. P@K uses `expected` as hard labels.

### EvalProfile

A named configuration override applied to `UnimatrixConfig` for one eval run
participant.

```
EvalProfile {
    name:        string   -- required; used as the key in result JSON "profiles" object
    description: string?  -- optional human label
    overrides:   UnimatrixConfig subset  -- fields to override from compiled defaults
}
```

The **baseline profile** is an empty TOML file: zero overrides, all compiled defaults.
The **candidate profile** is a TOML file specifying only the change under test.

### EvalResult

The per-scenario output produced by `unimatrix eval run` for one scenario.

```
EvalResult {
    scenario_id:  string
    query:        string
    profiles:     Map<profile_name: string, ProfileResult>
    comparison:   ComparisonMetrics
}

ProfileResult {
    entries:        ScoredEntry[]  -- result list in ranked order
    latency_ms:     int
    p_at_k:         float          -- precision at K
    mrr:            float          -- mean reciprocal rank
}

ScoredEntry {
    id:              int
    title:           string
    final_score:     float
    similarity:      float
    confidence:      float
    status:          string
    nli_rerank_delta: float?       -- present only when NLI re-ranking was active
}

ComparisonMetrics {
    kendall_tau:         float     -- rank correlation, candidate vs. baseline
    rank_changes:        RankChange[]
    mrr_delta:           float     -- candidate.mrr - baseline.mrr
    p_at_k_delta:        float     -- candidate.p_at_k - baseline.p_at_k
    latency_overhead_ms: int       -- candidate.latency_ms - baseline.latency_ms
}

RankChange {
    entry_id:  int
    from_rank: int     -- position in baseline result list (1-indexed)
    to_rank:   int     -- position in candidate result list (1-indexed)
}
```

### EvalReport

The Markdown artifact produced by `unimatrix eval report`, aggregated over all
`EvalResult` files in the results directory. Not a Rust struct — a rendered document
with five sections (see FR-27).

### EvalServiceLayer

A restricted `ServiceLayer` variant used during `eval run`. Constructed by
`EvalServiceLayer::from_profile(db_path, profile_config)`.

```
EvalServiceLayer {
    snapshot_pool:     SqlitePool (read_only = true)
    vector_index:      Arc<VectorIndex>
    embed_handle:      EmbedServiceHandle
    rayon_pool:        Arc<RayonPool>
    adaptation_svc:    AdaptationService
    analytics_mode:    AnalyticsMode::Suppressed  -- SR-07: queue suppressed, not just blocked
}
```

### UnimatrixUdsClient

Python class that implements the MCP JSON-RPC client over `AF_UNIX SOCK_STREAM`,
using newline-delimited JSON framing (identical to `UnimatrixClient` over stdio).

### UnimatrixHookClient

Python class that implements the hook IPC client over `AF_UNIX SOCK_STREAM`, using
4-byte big-endian length-prefix + JSON body framing (matching `unimatrix_engine::wire`).

---

## Ubiquitous Language

| Term | Meaning in this feature |
|------|------------------------|
| snapshot | A `VACUUM INTO` full copy of the active SQLite database; includes all tables including analytics tables excluded by `export`; read-only input to all eval commands |
| eval scenario | One unit of evaluation: a query, its historical result set (soft ground truth), optional hard labels, and retrieval path metadata |
| query-log-sourced scenario | A scenario mined from `query_log`; `baseline.entry_ids` is soft ground truth; `expected` is null |
| hand-authored scenario | A scenario written by a human with explicit `expected` entry IDs as hard labels |
| baseline profile | The empty TOML profile that uses all compiled `UnimatrixConfig` defaults; the "before" in an A/B comparison |
| candidate profile | A TOML profile specifying the change under test (e.g., new confidence weights, NLI model path); the "after" |
| profile TOML | A file that is a subset of `UnimatrixConfig` plus a required `[profile] name` field |
| EvalServiceLayer | A read-only, analytics-disabled `ServiceLayer` constructed for one profile during eval run |
| soft ground truth | `baseline.entry_ids`: the results that were returned at query time; a proxy for "what the system thought was correct before this change" |
| hard labels | `expected`: explicit entry IDs that should appear in results; authoritative for hand-authored scenarios |
| P@K | Precision at K: the fraction of the top-K result entries that appear in the ground truth set |
| MRR | Mean reciprocal rank: the reciprocal of the rank of the first relevant result |
| Kendall tau | Rank correlation coefficient between two result lists; 1.0 = identical order, -1.0 = reversed |
| zero-regression check | The section of the eval report listing scenarios where the candidate profile scored lower than baseline; empty means no regressions |
| analytics queue suppression | Disabling the `enqueue_analytics` write path at `EvalServiceLayer` construction so that eval replay produces zero analytics writes |
| MCP UDS framing | Newline-delimited JSON (`\n`-terminated), identical to stdio MCP transport; used by `UnimatrixUdsClient` |
| hook IPC framing | 4-byte big-endian length prefix + JSON body; used by `UnimatrixHookClient`; defined in `unimatrix_engine::wire` |
| offline eval | D1–D4: snapshot + scenario extraction + eval run + report; requires no running daemon |
| live eval | D5–D6: connects to a running daemon's sockets; requires `daemon_server` fixture |
| `daemon_server` fixture | pytest fixture (entry #1928) that starts the unimatrix daemon, yields socket paths, and tears down on exit; required for Group 2 acceptance |

---

## User Workflows

### Workflow 1: A/B comparison for W1-4 (NLI) gate evaluation

1. Before implementing W1-4, take a snapshot: `unimatrix snapshot --out eval/pre-nli.db`
2. Extract scenarios from production history: `unimatrix eval scenarios --db eval/pre-nli.db --limit 500 --out eval/scenarios.jsonl`
3. Author `eval/baseline.toml` (empty file) and `eval/nli.toml` with `[inference] nli_model = "..."`.
4. After implementing W1-4 on a branch: `unimatrix eval run --db eval/pre-nli.db --scenarios eval/scenarios.jsonl --configs eval/baseline.toml,eval/nli.toml --out eval/results-nli/`
5. Generate report: `unimatrix eval report --results eval/results-nli/ --out eval/report-nli.md`
6. Human reads the report. Zero-regression check empty + P@K delta positive → gate passes → ship W1-4.

### Workflow 2: Confidence weight tuning evaluation

1. Author a candidate profile with `[confidence]` overrides (all six weights, summing to 0.92).
2. Run `eval run` with the confidence candidate profile against a snapshot.
3. Read report to see which entries moved rank and whether aggregate metrics improved.

### Workflow 3: Observation pipeline validation (D6 hook client)

1. Start daemon via `daemon_server` fixture.
2. Instantiate `UnimatrixHookClient` with the hook socket path.
3. Call `session_start`, several `pre_tool_use` / `post_tool_use`, then `session_stop`.
4. Use `UnimatrixUdsClient.context_status` to confirm the session record and any observation data is present.

### Workflow 4: Integration test against live daemon (D5 UDS client)

```python
with UnimatrixUdsClient(socket_path) as client:
    results = client.context_search(query="fixture initialization", k=5)
    assert len(results) > 0
```

---

## Constraints

**C-01 (Binary architecture)**: No new workspace crate. Eval modules live in
`crates/unimatrix-server/src/eval/` as a module tree alongside existing `export.rs`
and `snapshot.rs`. The `unimatrix` binary remains the single binary.

**C-02 (No migration on snapshot)**: `SqlxStore::open()` triggers schema migration and
must not be called on a snapshot database. The eval runner shall use a raw
`sqlx::SqlitePool` with `SqliteConnectOptions::read_only(true)`. This is enforced at
the SQLite layer — writes will fail at the OS level regardless — but also enforced by
code structure to prevent future drift.

**C-03 (Analytics queue suppression — SR-07)**: The analytics write queue must be
disabled at `EvalServiceLayer` construction, not merely blocked by read-only SQLite.
The in-memory `enqueue_analytics` path must be no-op'd so that eval replay produces
no in-memory queue population. An `AnalyticsMode::Suppressed` variant (or equivalent
named design) is the required architectural mechanism.

**C-04 (MCP UDS framing)**: `UnimatrixUdsClient` uses newline-delimited JSON (`\n`),
not a length prefix. This matches rmcp 0.16.0's `AsyncRwTransport` /
`JsonRpcMessageCodec`. The 4-byte big-endian length prefix framing is for hook IPC
only.

**C-05 (Hook IPC framing)**: `UnimatrixHookClient` uses `struct.pack('>I', len(payload))
+ payload` for writes, `struct.unpack('>I', header)[0]` for read length. Maximum
payload is 1 MiB (`MAX_PAYLOAD_SIZE` from `unimatrix_engine::wire`).

**C-06 (ConfidenceWeights sum invariant)**: Profile TOML with `[confidence]` overrides
must supply all six weight fields summing to 0.92 ± 1e-9. Config loading shall reject
a partial or non-summing override with a user-readable error (SR-08). The baseline
empty-TOML profile is exempt.

**C-07 (No CI gate logic in report)**: `unimatrix eval report` shall not exit with a
non-zero code based on the zero-regression count. No automated pass/fail gate logic
belongs in this subcommand. CI gate automation is explicitly out of scope (SR-06).

**C-08 (Socket path length)**: The MCP UDS socket path must not exceed 103 bytes.
`UnimatrixUdsClient` shall validate path length before attempting `connect()` and
raise a descriptive error if the limit is exceeded.

**C-09 (Pre-async dispatch ordering)**: `snapshot` is a sync subcommand dispatched
before the tokio runtime, consistent with C-10 ordering rule already established for
`export`. If `VACUUM INTO` requires sqlx async, a minimal `block_on` wrapper is
acceptable; a full tokio runtime is not.

**C-10 (Cumulative test infrastructure)**: Eval test suites extend existing fixtures
and helpers. `TestHarness` is extended, not replaced. `kendall_tau()` and ranking
helpers in `unimatrix-engine::test_scenarios` are reused directly. No isolated
scaffolding that duplicates existing test infrastructure is introduced.

**C-11 (Python harness location)**: All Python additions go in
`product/test/infra-001/harness/`. Test files go in
`product/test/infra-001/tests/`. No Python files are added elsewhere.

**C-12 (No snapshot anonymization)**: The `--anonymize` flag is removed from scope.
`agent_id` is role-like metadata, not personal identification data. No anonymization
pass is implemented.

**C-13 (Hook socket path must be confirmed)**: The hook socket path convention is an
open question (see Open Questions). The architect must confirm `ProjectPaths` exposes
the hook socket path before implementation of `UnimatrixHookClient`.

---

## Dependencies

### New Rust dependencies

None. All required Rust infrastructure already exists in the workspace (rusqlite for
sync snapshot, sqlx for read-only pool, rayon pool from crt-022, ranking metrics from
unimatrix-engine).

### Existing Rust components consumed

| Component | Location | Role |
|-----------|----------|------|
| `TestHarness` | `crates/unimatrix-server/src/test_support.rs` | Pattern for `EvalServiceLayer::from_profile()` |
| `kendall_tau()`, `assert_ranked_above()` | `crates/unimatrix-engine/src/test_scenarios.rs` | Metric computation (reused directly) |
| `QueryLogRecord::scan_query_log_by_sessions()` | `crates/unimatrix-store/src/query_log.rs` | Scenario extraction from snapshot |
| `RetrievalMode` enum | `crates/unimatrix-server/src/services/search.rs` | Strict vs. Flexible path replay |
| `ServiceSearchParams` | `crates/unimatrix-server/src/services/search.rs` | Full search parameterization per profile |
| `EmbedServiceHandle` / `RayonPool` | `crates/unimatrix-server/src/infra/` | Inference for eval (crt-022) |
| `UnimatrixConfig` / `InferenceConfig` | `crates/unimatrix-server/src/infra/config.rs` | Config override base for profile TOML |
| `run_export()` | `crates/unimatrix-server/src/export.rs` | Dispatch pattern reference for snapshot (sync, pre-tokio) |
| `HookRequest` / `HookResponse` wire protocol | `crates/unimatrix-engine/src/wire.rs` | Wire protocol spec for `UnimatrixHookClient` |
| `SqlxStore` (read path) | `crates/unimatrix-store/src/` | `scan_entries`, vector map reads, co-access reads for EvalServiceLayer |

### Existing Python components consumed

| Component | Location | Role |
|-----------|----------|------|
| `UnimatrixClient` | `product/test/infra-001/harness/client.py` | Framing reference and API surface for `UnimatrixUdsClient` |
| `daemon_server` fixture | `product/test/infra-001/suites/conftest.py` | Daemon lifecycle management for D5/D6 tests (entry #1928) |

### External services

None. No network calls. All eval is in-process or via local Unix domain sockets.

---

## NOT in Scope

The following are explicitly excluded:

- **`unimatrix eval live` subcommand**: UDS client (D5) enables live-path eval, but
  a dedicated `eval live` subcommand is deferred. Callers use `UnimatrixUdsClient`
  directly in Python test scripts.
- **NLI model integration**: nan-007 builds the harness that gates W1-4; it does not
  implement the NLI model. Profile TOML supports `[inference]` section stubs, but
  NLI inference code is W1-4.
- **GGUF model integration**: W2-4. nan-007 provides the harness; GGUF code is W2-4.
- **GNN training**: W3-1. `UnimatrixHookClient` is the foundation; training loop is W3-1.
- **Automated CI gate**: The eval report is a human-reviewed artifact. Automated
  quality gates in CI (non-zero exit on regression) are future scope.
- **Cross-project or multi-deployment eval**: Single project directory only.
- **Web UI or interactive diff viewer**: Report is Markdown only.
- **`unimatrix import` of eval results**: No write-back of eval results to the live DB.
- **Snapshot anonymization (`--anonymize` flag)**: Removed from scope. `agent_id` is
  role-like metadata; no anonymization pass is required.
- **Schema changes**: This feature adds no new database tables, columns, or migrations.
- **`SqlxStore` API modifications**: The eval runner constructs a raw `sqlx::SqlitePool`
  directly; `SqlxStore::open_readonly()` is not added.

---

## Open Questions for Architect

**OQ-1 (hook socket path — SR-05, High priority)**: Does `ProjectPaths` currently
expose the hook socket path? If not, does `ProjectPaths` need a new field, or does the
hook socket path follow a derivable convention from the MCP socket path? This must be
resolved before D6 implementation. The spec assumes the path is
`{project_dir}/.unimatrix/{hash}/unimatrix-hook.sock` but this is unverified.

**OQ-2 (VACUUM INTO sync vs. async)**: Recommend rusqlite (synchronous, pre-tokio,
consistent with `export.rs` pattern). Architect to confirm or choose sqlx async with
`block_on`. This decision affects crate dependencies for `snapshot.rs`.

**OQ-3 (kendall_tau accessibility — Assumptions table, SR-05)**: `kendall_tau()` in
`unimatrix-engine::test_scenarios` may be gated behind `#[cfg(test)]` or a
`test-support` feature flag. The eval runner must call it from production binary code.
Architect to verify accessibility and design the feature flag or module restructure
if needed before FR-22 can be implemented.

**OQ-4 (nested eval subcommand dispatch — C-09 compatibility)**: Three-level CLI
(`unimatrix eval scenarios|run|report`) requires an `Eval` subcommand with nested
`EvalCommand` variants in clap 4.x. Architect to confirm dispatch ordering relative
to the C-10 pre-async rule (specifically: does the `eval` subcommand group get
dispatched pre-tokio or post-tokio, given that `eval run` needs an async context
for sqlx pool construction?).

**OQ-5 (vector index sharing across profiles — SR-03)**: Should `EvalServiceLayer`
load a separate HNSW index per profile (simpler isolation) or share a single read-only
index across all profiles (lower memory)? Architect to estimate memory cost at 50,000
entries and recommend one approach. NFR-03 sets the constraint; this question sets
the design target.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for "eval harness snapshot scenario extraction ranking
  metrics acceptance criteria" — found entry #749 (Calibration Scenario Builder Pattern,
  tagged calibration/kendall-tau/ranking/unimatrix-engine). This confirms the existing
  `kendall_tau()` and `CalibrationScenario` infrastructure in `unimatrix-engine` and
  directly informed FR-22 (reuse directive) and OQ-3 (accessibility concern). Also
  found entry #425 (Three-Slot Model Registry pattern), relevant context for W1-4/W2-4
  integration via profile TOML but not directly applicable to this spec.
- Queried: `/uni-query-patterns` for "acceptance criteria verification method split
  offline live daemon fixture pattern" — found entry #1928 (daemon fixture pattern)
  referenced in SR-04. This grounded the Group 1 / Group 2 AC split (SR-04
  recommendation) and the `daemon_server` fixture requirement in D5/D6 test suites.
- No new knowledge stored — risks and patterns are feature-specific to nan-007;
  cross-feature generalizations are for the retro to promote.
