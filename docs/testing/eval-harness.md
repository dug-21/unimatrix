# Evaluation Harness

The evaluation harness is the mandatory gate between any Unimatrix intelligence
change and production. Before a retrieval model swap, confidence weight adjustment,
NLI re-ranking integration, or GGUF model lands, a developer runs the harness to
produce a quantified evidence report. Regressions are caught early; improvements
are demonstrated, not assumed; the human reviewer sees exactly what changed and why.

## Contents

- [When to use it](#when-to-use-it)
- [Overview](#overview)
- [Step 1 — Snapshot the database](#step-1--snapshot-the-database)
- [Step 2 — Extract eval scenarios](#step-2--extract-eval-scenarios)
- [Step 3 — Run the evaluation](#step-3--run-the-evaluation)
- [Step 4 — Generate the report](#step-4--generate-the-report)
- [Writing profile TOMLs](#writing-profile-tomls)
- [Reading the report](#reading-the-report)
- [Hand-authored scenarios](#hand-authored-scenarios)
- [Live-path clients (D5/D6)](#live-path-clients-d5d6)
- [Safety constraints](#safety-constraints)

---

## When to use it

Run the offline harness (D1–D4) before shipping:

| Change type | Gate |
|---|---|
| Retrieval model swap (embedding ONNX file) | D1–D4 required |
| Confidence weight tuning (`ConfidenceWeights`) | D1–D4 required |
| NLI re-ranking integration (W1-4) | D1–D4 required |
| GGUF model integration (W2-4) | D1–D4 required |
| New scoring or ranking logic in `unimatrix-engine` | D1–D4 recommended |
| Schema or storage changes only (no ranking impact) | Optional |
| Documentation changes | Not needed |

Run the live-path clients (D5/D6) before shipping:

| Change type | Gate |
|---|---|
| Hook IPC processing changes (W3-1 GNN training) | D6 required |
| MCP tool response changes | D5 recommended |
| Context injection logic changes | D5 recommended |

---

## Overview

The harness consists of six components:

```
D1  unimatrix snapshot         -- full DB copy (all tables, no daemon writes)
D2  unimatrix eval scenarios   -- mine query_log → JSONL eval scenarios
D3  unimatrix eval run         -- replay scenarios through A/B config profiles
D4  unimatrix eval report      -- aggregate results → Markdown report

D5  UnimatrixUdsClient (Python) -- MCP tool calls over the daemon's UDS socket
D6  UnimatrixHookClient (Python) -- hook IPC calls over the daemon's hook socket
```

D1–D4 are **offline**: they operate against a frozen snapshot, require no running
daemon during evaluation, and produce no writes to any live database. D5–D6 are
**live**: they connect to a running daemon and are used for integration-level
observation and pipeline testing.

---

## Step 1 — Snapshot the database

Take a full, self-contained copy of the active database before touching any code.
The snapshot is the fixed input for all subsequent eval steps — it must not change
between runs.

```bash
unimatrix snapshot --out /tmp/eval/snap.db
```

This runs `VACUUM INTO` against the active database and writes a read-only SQLite
file containing **all tables** (entries, query\_log, graph\_edges, co\_access,
sessions, observations, shadow\_evaluations, and all analytics tables). The
`export` command excludes analytics tables; `snapshot` does not.

The `snapshot` command also copies a sibling `vector/` directory containing the
HNSW index files (`unimatrix.hnsw.graph`, `unimatrix.hnsw.data`,
`unimatrix-vector.meta`) alongside the snapshot database file. When `eval run`
constructs an `EvalServiceLayer` from the snapshot, it automatically loads the
vector index from this sibling directory, restoring full search fidelity. If the
live index is empty or has never been dumped, the `vector/` directory is omitted
and `eval run` falls back to a fresh empty index (backward-compatible with
pre-fix snapshots).

**Flags:**

| Flag | Required | Description |
|---|---|---|
| `--out <path>` | Yes | Destination file. Must not resolve to the active DB path. |
| `--project-dir <path>` | No | Override project directory (default: standard discovery). |

**Snapshot sensitivity warning:** Snapshots contain the full agent interaction
history including query text, retrieved entries, confidence scores, and session
identifiers. Do not commit snapshots to the repository. Store them in a scratch
directory outside the workspace.

**Guard:** The command resolves `--out` via `canonicalize()` and refuses with a
non-zero exit code if the resolved path matches the active database path — including
through symlinks. This prevents accidental self-overwrite of the live database.

---

## Step 2 — Extract eval scenarios

Mine the `query_log` table from the snapshot to produce JSONL eval scenarios.
Each scenario is one real query from a previous agent session, with its actual
retrieved results recorded as soft ground truth.

```bash
unimatrix eval scenarios \
  --db /tmp/eval/snap.db \
  --out /tmp/eval/scenarios.jsonl
```

Filter by retrieval source if needed:

```bash
# Only queries that arrived via MCP (stdio transport)
unimatrix eval scenarios --db /tmp/eval/snap.db --out /tmp/eval/scenarios.jsonl \
  --retrieval-mode mcp

# Only queries that arrived via UDS transport (D5 clients)
unimatrix eval scenarios --db /tmp/eval/snap.db --out /tmp/eval/scenarios.jsonl \
  --retrieval-mode uds

# Both (default)
unimatrix eval scenarios --db /tmp/eval/snap.db --out /tmp/eval/scenarios.jsonl \
  --retrieval-mode all
```

**Flags:**

| Flag | Required | Default | Description |
|---|---|---|---|
| `--db <path>` | Yes | — | Snapshot SQLite file (read-only). |
| `--out <path>` | Yes | — | Output JSONL file. |
| `--retrieval-mode mcp\|uds\|all` | No | `all` | Filter by query source. |
| `--limit <N>` | No | (all) | Maximum number of scenarios to emit. |

**Output format** (one JSON object per line):

```jsonc
{
  "id": "q-a1b2c3d4",
  "query": "what is the confidence scoring formula",
  "context": {
    "agent_id": "uni-rust-dev",
    "feature_cycle": "crt-022",
    "session_id": "sess-abc123",
    "retrieval_mode": "flexible"
  },
  "baseline": {
    "entry_ids": [42, 17, 103],
    "scores": [0.94, 0.87, 0.81]
  },
  "source": "mcp",
  "expected": null          // null for query-log-sourced scenarios
}
```

The `baseline` field records the actual results the current production system
returned for this query. It serves as soft ground truth for P@K and MRR when
`expected` is null. If you have domain knowledge about what the correct results
should be, write [hand-authored scenarios](#hand-authored-scenarios) and set
`expected` to a list of entry IDs instead.

**Note on schema:** The `agent_id` and `feature_cycle` fields in `context` are
populated from `session_id` as a proxy — the `query_log` table does not store these
fields directly. They are informational only.

---

## Step 3 — Run the evaluation

Replay the scenarios in-process through one or more configuration profile TOMLs.
Each profile represents a different configuration of the retrieval stack (different
model, different confidence weights, etc.). One of the profiles should be the
**baseline** (empty TOML = compiled defaults). The rest are candidates.

```bash
unimatrix eval run \
  --db       /tmp/eval/snap.db \
  --scenarios /tmp/eval/scenarios.jsonl \
  --configs  /tmp/eval/baseline.toml,/tmp/eval/candidate.toml \
  --out      /tmp/eval/results/ \
  --k        5
```

This produces one JSON result file per scenario in `--out`, named by scenario ID.

**Flags:**

| Flag | Required | Default | Description |
|---|---|---|---|
| `--db <path>` | Yes | — | Snapshot file. Must not be the live DB. |
| `--scenarios <path>` | Yes | — | JSONL file from `eval scenarios`. |
| `--configs <paths>` | Yes | — | Comma-separated profile TOML paths. First = baseline. |
| `--out <dir>` | Yes | — | Output directory for result JSON files. |
| `--k <N>` | No | `5` | K for P@K (precision at K). |

**What happens internally:**

1. For each profile, an `EvalServiceLayer` is constructed from the snapshot — a
   read-only in-process search stack with the profile's configuration applied. The
   analytics write queue is suppressed; no writes reach the snapshot or the live DB.
2. Each scenario is replayed: the query is re-run against the snapshot's vector
   index with the profile's ranking weights. Results, scores, and latency are
   recorded.
3. Per scenario, the following metrics are computed across profiles:
   - **P@K** — how many of the top K results match ground truth
   - **MRR** — mean reciprocal rank of the first correct result
   - **Kendall tau** — rank correlation between baseline and candidate result lists
   - **MRR delta** and **P@K delta** relative to baseline
   - **Latency overhead** in milliseconds

**Result JSON format** (one file per scenario):

```jsonc
{
  "scenario_id": "q-a1b2c3d4",
  "query": "what is the confidence scoring formula",
  "profiles": {
    "baseline": {
      "entries": [/* ScoredEntry list */],
      "latency_ms": 12,
      "p_at_k": 0.8,
      "mrr": 0.92
    },
    "nli-candidate": {
      "entries": [/* ScoredEntry list */],
      "latency_ms": 38,
      "p_at_k": 0.9,
      "mrr": 0.95
    }
  },
  "comparison": {
    "kendall_tau": 0.87,
    "mrr_delta": 0.03,
    "p_at_k_delta": 0.10,
    "latency_overhead_ms": 26,
    "rank_changes": [
      { "entry_id": 103, "from_rank": 3, "to_rank": 1 }
    ]
  }
}
```

**Guard:** `eval run` applies the same live-DB path guard as `snapshot`. If `--db`
resolves to the active daemon's database path (checked via `canonicalize()`), the
command refuses with a non-zero exit code before opening any pool.

**Memory note:** One `VectorIndex` instance is constructed per profile. For
snapshots with large entry counts, running many profiles simultaneously will increase
memory proportionally. The documented ceiling is 2 profiles × 50k entries ≤ 8 GB RAM.

---

## Step 4 — Generate the report

Aggregate the per-scenario result files into a human-readable Markdown report.

```bash
unimatrix eval report \
  --results   /tmp/eval/results/ \
  --out       /tmp/eval/report.md \
  --scenarios /tmp/eval/scenarios.jsonl   # optional: annotates queries with text
```

**Flags:**

| Flag | Required | Description |
|---|---|---|
| `--results <dir>` | Yes | Directory containing per-scenario JSON files from `eval run`. |
| `--out <path>` | Yes | Output Markdown file. |
| `--scenarios <path>` | No | JSONL file; used to annotate queries in ranking tables. |

The report exits 0 regardless of the regression count. It is a human-reviewed
artifact — there is no automated pass/fail gate.

---

## Writing profile TOMLs

A profile TOML is a named, partial `UnimatrixConfig`. Missing sections fall back
to compiled defaults. The baseline is an empty TOML with only a `[profile]` section.

**Baseline (empty, uses compiled defaults):**

```toml
[profile]
name = "baseline"
description = "Production compiled defaults"
```

**Candidate — confidence weight adjustment:**

```toml
[profile]
name = "higher-freshness"
description = "Increase freshness weight, reduce base weight"

[confidence.weights]
base  = 0.12   # was 0.18
usage = 0.16
fresh = 0.26   # was 0.20
help  = 0.12
corr  = 0.14
trust = 0.12
# Sum must equal 0.92 ± 1e-9. Any other sum is rejected at profile load.
```

**Weight sum invariant:** All six weight fields (`base`, `usage`, `fresh`, `help`,
`corr`, `trust`) must be present and must sum to exactly `0.92 ± 1e-9`. A missing
field or incorrect sum causes `eval run` to fail at profile construction with a
user-readable error naming the expected and actual sums.

**Future: inference overrides** (`[inference]` section) will be used for NLI/GGUF
model path configuration when those features land (W1-4, W2-4). The section is
accepted now but has no effect until the model integration is wired.

---

## Reading the report

The report contains five sections in order:

### 1. Summary

Aggregate P@K, MRR, average latency, and rank change rate per profile, with delta
columns relative to the baseline. A candidate with positive MRR delta and P@K delta
is an improvement.

### 2. Notable Ranking Changes

Queries where result order changed most significantly (sorted by Kendall tau drop).
Each entry shows a side-by-side rank table: baseline result list vs. candidate
result list. This section answers "what specifically changed for each query?"

### 3. Latency Distribution

Percentile table of `latency_ms` per profile. Latency overhead in the candidate
relative to baseline appears here. Acceptable overhead is workload-dependent, but
> 50ms average is worth investigating before shipping.

### 4. Entry-Level Analysis

Which entries gained or lost rank most across all scenarios. The most consistently
promoted entries are the ones the candidate profile believes are more relevant.
The most consistently demoted entries are the ones it believes are less relevant.
Cross-reference against your domain knowledge.

### 5. Zero-Regression Check

**This is the primary shipping gate.** A list of all scenarios where any candidate
profile has lower MRR **or** lower P@K than the baseline. A scenario appears if
*either* metric regresses — not both. When the list is empty, the report prints:

```
No regressions detected.
```

If regressions exist, review each before deciding to ship. A regression in a
low-value scenario (e.g., a one-off query with an unusual phrasing) is different
from a regression in a high-frequency scenario. The harness surfaces the data;
the human decides.

---

## Hand-authored scenarios

Query-log scenarios use the actual retrieved results as soft ground truth. For
cases where you know the correct answers, write hand-authored scenarios:

```jsonc
{
  "id": "manual-001",
  "query": "confidence scoring formula",
  "context": {
    "agent_id": "human",
    "feature_cycle": "w1-4",
    "session_id": "manual",
    "retrieval_mode": "flexible"
  },
  "baseline": null,
  "source": "mcp",
  "expected": [42, 17, 103]   // hard labels — these are the correct entry IDs
}
```

With `expected` set, P@K and MRR are computed against the hard labels. When
`expected` is null (query-log-sourced), they are computed against
`baseline.entry_ids`. Mix both types freely in the same JSONL file.

---

## Live-path clients (D5/D6)

For changes that affect live-daemon behavior (hook processing, observation pipeline,
MCP tool responses), use the Python clients in
`product/test/infra-001/harness/`.

### UnimatrixUdsClient — MCP over UDS (D5)

Connects to the daemon's MCP socket (`~/.unimatrix/<hash>/unimatrix-mcp.sock`)
and exposes the same 12 typed `context_*` methods as `UnimatrixClient`.

```python
from harness.uds_client import UnimatrixUdsClient
from pathlib import Path

mcp_sock = Path.home() / ".unimatrix" / "<project-hash>" / "unimatrix-mcp.sock"

with UnimatrixUdsClient(mcp_sock) as client:
    results = client.context_search(
        query="confidence scoring",
        k=5,
        agent_id="eval-harness",
    )
    print(results)
```

**Wire framing:** Newline-delimited JSON (identical to stdio MCP transport). No
length prefix. Do not use this client with the hook socket — they use different
framing.

**Path limit:** Socket paths may not exceed 103 bytes (OS `AF_UNIX` limit). The
client validates this at construction and raises `ValueError` before connecting.

### UnimatrixHookClient — hook IPC (D6)

Connects to the daemon's hook socket (`~/.unimatrix/<hash>/unimatrix.sock`)
and sends synthetic `HookRequest` messages using the 4-byte big-endian length
prefix + JSON wire protocol.

```python
from harness.hook_client import UnimatrixHookClient
from pathlib import Path
import uuid

hook_sock = Path.home() / ".unimatrix" / "<project-hash>" / "unimatrix.sock"

client = UnimatrixHookClient(hook_sock)
client.connect()
try:
    # Verify the hook socket is alive
    pong = client.ping()
    assert pong.type == "Pong"

    # Simulate a session lifecycle
    sid = str(uuid.uuid4())
    client.session_start(sid, feature_cycle="w1-4", agent_role="uni-rust-dev")
    client.pre_tool_use(sid, tool="Edit", input={"file": "src/lib.rs"})
    client.post_tool_use(sid, tool="Edit", response_size=512, response_snippet="ok")
    client.session_stop(sid, outcome="success")
finally:
    client.disconnect()
```

**Wire framing:** 4-byte big-endian length prefix + JSON body. This is different
from the MCP UDS framing. Do not use this client with the MCP socket.

**Payload limit:** Payloads exceeding 1 MiB (1,048,576 bytes) raise `ValueError`
before any bytes are sent to the socket. The client remains usable after the error.

### Finding the socket paths

The project hash is derived from the project directory. To find the correct paths:

```bash
# Shows active socket paths for the current project
unimatrix status
```

Or locate them directly:

```bash
ls ~/.unimatrix/*/unimatrix.sock       # hook sockets
ls ~/.unimatrix/*/unimatrix-mcp.sock  # MCP UDS sockets
```

### Test suites

Two test suites exercise the clients against a live daemon:

```bash
cd product/test/infra-001

# D5: MCP UDS framing, tool parity, context manager protocol
python -m pytest tests/test_eval_uds.py -v -m integration

# D6: hook framing, session lifecycle, payload limits
python -m pytest tests/test_eval_hooks.py -v -m integration
```

These require a running daemon (`daemon_server` pytest fixture). Run them in the
integration test environment, not in offline CI.

---

## Safety constraints

| Constraint | Description |
|---|---|
| Never commit snapshots | Snapshots contain full agent interaction history. Treat them as sensitive data. |
| Never pass the live DB to `eval run` | The live-DB path guard (FR-44) catches this. If it fires, you are pointing at the wrong `--db` path. |
| `eval report` exits 0 always | Do not use the exit code as a CI gate. The report is a human-reviewed artifact. |
| UDS ≠ hook socket | `UnimatrixUdsClient` and `UnimatrixHookClient` use different framing and different sockets. Connecting to the wrong one produces framing errors, not a clean error message. |
| Weight sum = 0.92 | All six `[confidence.weights]` fields must sum to exactly 0.92 ± 1e-9. |
| `cargo install` for production binary | `cargo build --release` writes to `target/release/` and does NOT update the running daemon. Use `cargo install --path crates/unimatrix-server` to replace the installed binary. Run before deploying a new build; kill the running daemon first. |

---

## Full example walkthrough

```bash
# 0. Build (if needed)
#
# To verify the build or run tests:
#   cargo build --release -p unimatrix-server   -- writes to target/release/, does NOT update the running daemon
#
# To update the installed/running MCP server binary:
#   cargo install --path crates/unimatrix-server -- replaces the binary in $CARGO_HOME/bin/
#   Kill the running daemon before installing; it will not auto-restart.
#
cargo build --release --workspace

# 1. Snapshot — take a copy while the daemon is running normally
unimatrix snapshot --out /tmp/eval/$(date +%Y%m%d)-snap.db

# 2. Extract scenarios from query_log
unimatrix eval scenarios \
  --db /tmp/eval/$(date +%Y%m%d)-snap.db \
  --out /tmp/eval/scenarios.jsonl \
  --retrieval-mode all

# Check how many you got
wc -l /tmp/eval/scenarios.jsonl

# 3. Write your profile TOMLs (see "Writing profile TOMLs" above)
#    /tmp/eval/baseline.toml     -- empty, compiled defaults
#    /tmp/eval/candidate.toml    -- your proposed change

# 4. Run evaluation
unimatrix eval run \
  --db        /tmp/eval/$(date +%Y%m%d)-snap.db \
  --scenarios /tmp/eval/scenarios.jsonl \
  --configs   /tmp/eval/baseline.toml,/tmp/eval/candidate.toml \
  --out       /tmp/eval/results/ \
  --k         5

# 5. Generate report
unimatrix eval report \
  --results   /tmp/eval/results/ \
  --scenarios /tmp/eval/scenarios.jsonl \
  --out       /tmp/eval/report.md

# 6. Review the report — focus on section 5 (Zero-Regression Check)
cat /tmp/eval/report.md
```
