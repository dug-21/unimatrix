# ASS-025: Evaluation Harness — Research Spike & Recommendations

**Status**: Research complete. Ready for design (ASS-025 scope).
**Feeds**: W1-3 feature delivery
**Date**: 2026-03-19

---

## Context

The product vision originally scoped W1-3 as a ~1-week offline eval harness
covering four capabilities: snapshot, scenario extraction, eval run, and report.

This spike was commissioned to:
1. Audit current export and test infrastructure against what W1-3 actually needs
2. Determine how to source real live data from a running deployment
3. Expand scope to include live path simulation (MCP + UDS) and A/B testing
4. Assess whether the full scope fits a single feature delivery

**Conclusion**: All six deliverables are architecturally coherent and should ship
together in a single W1-3 delivery. Estimated effort: 1.5–2 weeks. The offline
eval (deliverables 1–4) is the critical path that gates W1-4 and W2-4. The live
simulation layer (deliverables 5–6) extends the harness for ongoing use through
W1-5, W3-1, and beyond.

---

## Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Snapshot scope | Full DB copy (`VACUUM INTO`) | Eval needs analytics tables (query_log, graph_edges, co_access, shadow_evaluations) that the existing export excludes. Full copy is simpler and safer. |
| Eval engine architecture | Both Rust in-process + Python subprocess | In-process (Rust `TestHarness` pattern) for fast offline A/B comparison; Python subprocess for live path validation covering the full MCP stack |
| Live vs. offline modes | Both | `eval run` (offline, frozen snapshot) and `eval live` (live simulation against running daemon) as distinct modes |
| Report format | Markdown | Checks into repo, diffs in PRs, no toolchain dependency |
| Binary architecture | Single binary, new subcommands | Consistent with the "single binary" non-negotiable. `snapshot` and `eval` added as CLI subcommands of `unimatrix`. |
| Delivery model | Single W1-3 feature | Six deliverables, mixed Rust + Python, ~1.5–2 weeks. Offline path gates W1-4; live path extends value for W1-5 and W3-1. |

---

## What Already Exists

### Reusable Infrastructure

| Component | Location | Reusable For |
|-----------|----------|-------------|
| `run_export()` | `crates/unimatrix-server/src/export.rs` | Knowledge table reference; snapshot replaces it for eval |
| `TestHarness` | `crates/unimatrix-server/src/test_support.rs` | Core eval engine — in-process `ServiceLayer` construction |
| `UnimatrixClient` (Python) | `product/test/infra-001/harness/client.py` | MCP stdio simulation (live eval path, already complete) |
| `kendall_tau()`, `assert_ranked_above()` | `crates/unimatrix-engine/src/test_scenarios.rs` | Ranking comparison metrics |
| `EntryProfile`, `RetrievalScenario` | `crates/unimatrix-engine/src/test_scenarios.rs` | Synthetic scenario construction |
| `QueryLogRecord::scan_query_log_by_sessions()` | `crates/unimatrix-store/src/query_log.rs` | Mining real query scenarios |
| `RetrievalMode` enum | `crates/unimatrix-server/src/services/search.rs` | Distinguishes strict (UDS hook) vs. flexible (MCP) paths |
| `ServiceSearchParams` | `crates/unimatrix-server/src/services/search.rs` | Full search parameterization per profile |
| `EmbedServiceHandle` / `RayonPool` | `crates/unimatrix-server/src/infra/` | Inference for eval (crt-022 landed) |

### Key Gap: Export ≠ Snapshot

`unimatrix export` produces JSONL knowledge records for portability. This is not
what eval needs. Eval needs a working SQLite file that the Rust `ServiceLayer` can
open and run searches against. Specifically, the current export **excludes** these
tables that eval requires:

- `query_log` — the natural source for real test scenarios
- `graph_edges` — typed relationship graph (crt-021), affects re-ranking
- `sessions` — feature cycle context for scenario enrichment
- `shadow_evaluations` — NLI scoring history (needed for W1-4 eval baseline)
- `injection_log` — what was injected and when

`unimatrix snapshot` is a new command, conceptually different from `export`.

### Key Gap: No UDS Client in Test Harness

`UnimatrixClient` spawns `unimatrix serve --stdio`. No client exists that:
- Connects to a running daemon's MCP UDS socket
- Sends synthetic hook/lifecycle events to the hook IPC socket

Both gaps addressed by deliverables 5 and 6.

---

## The Six Deliverables

### Deliverable 1: `unimatrix snapshot`

**CLI**:
```
unimatrix snapshot --out eval/snapshot-2026-03-19.db [--anonymize] [--project-dir ...]
```

**Mechanism**: `VACUUM INTO 'snapshot.db'` — SQLite's atomic, WAL-safe copy
pragma. Creates a defragmented, self-consistent copy of the full database at the
moment of execution.

**All tables included**: entries, entry_tags, co_access, feature_entries,
outcome_index, agent_registry, audit_log, counters, graph_edges, sessions,
observations, query_log, shadow_evaluations, injection_log, observation_metrics,
topic_deliveries — the complete state.

**`--anonymize` flag**: Replace `agent_id` and `session_id` values with
SHA-256-seeded consistent pseudonyms before the copy. Same input always maps to
the same pseudonym (preserves co-access relationship patterns). Uses a random
salt generated at snapshot time and stored in a companion `.meta` file alongside
the snapshot. Applied via a post-copy pass on the snapshot file (not the live DB).

**Safety**: CLI must refuse if `--out` path resolves to the same file as the
active daemon's DB path (W1-3 security requirement).

**No running server required**: Opens DB directly via `SqlxStore::open()`, same
as `run_export()`.

---

### Deliverable 2: `unimatrix eval scenarios`

**CLI**:
```
unimatrix eval scenarios \
  --db snapshot.db \
  --source query_log \
  --out scenarios.jsonl \
  [--limit 500] \
  [--retrieval-mode mcp|uds|all]
```

**Mechanism**: Mines `query_log` and joins with `entries` to produce eval
scenarios. The `source` field in `query_log` (`"mcp"` or `"uds"`) enables
filtering by path. The `result_entry_ids` at time-of-query becomes soft ground
truth.

**Scenario format** (extends product vision format):
```json
{
  "id": "qlog-4921",
  "query": "integration test fixture initialization",
  "context": {
    "agent_id": "anon-3a2f",
    "feature_cycle": "crt-022",
    "session_id": "anon-8b1c",
    "retrieval_mode": "flexible"
  },
  "baseline": {
    "entry_ids": [45, 12, 3],
    "scores": [0.91, 0.87, 0.83]
  },
  "source": "mcp",
  "expected": null
}
```

`baseline.entry_ids` and `baseline.scores` are the actual results at query time
— "before the change, this is what was returned." `expected` is null unless
hand-authored. Delta comparison between profiles is the primary value even
without hard labels.

Also supports **hand-authored scenarios** in the same format (omit `baseline`,
set `expected` to the list of entry IDs that must appear in results).

**Important**: `query_log` writes go through `enqueue_analytics` (eventual
consistent, 500ms drain). Scenario extraction operates on historical snapshot
data only — no immediate write-back concern.

---

### Deliverable 3: `unimatrix eval run`

**CLI**:
```
unimatrix eval run \
  --db snapshot.db \
  --scenarios scenarios.jsonl \
  --configs baseline.toml,candidate.toml \
  --out results/ \
  [--k 5]
```

**Architecture**: Rust in-process. Opens the snapshot DB, constructs one
`ServiceLayer` per profile config using the `TestHarness` pattern, then replays
each scenario through each profile. No spawned server. No network. Same code
path as production search.

**Profile TOML format** — a subset of `UnimatrixConfig` with named overrides:
```toml
[profile]
name = "nli-enabled"
description = "W1-4 NLI cross-encoder re-ranking at top-20"

[inference]
nli_model = "/path/to/models/nli-MiniLM2-L6-H768.onnx"
nli_rerank_top_k = 20

[confidence]
weights = { freshness = 0.35, graph = 0.30, contradiction = 0.20, embedding = 0.15 }
```

The baseline profile is an empty TOML (uses current config defaults). The
candidate profile specifies only the overrides under test. This means every
W1/W2/W3 intelligence feature becomes a single profile TOML diff.

**Per-scenario output** (written to `results/`):
```json
{
  "scenario_id": "qlog-4921",
  "query": "integration test fixture initialization",
  "profiles": {
    "baseline": {
      "entries": [
        {"id": 45, "title": "...", "final_score": 0.91, "similarity": 0.91,
         "confidence": 0.73, "status": "active"}
      ],
      "latency_ms": 14,
      "p_at_3": 1.0,
      "mrr": 1.0
    },
    "candidate": {
      "entries": [
        {"id": 45, "title": "...", "final_score": 0.89, "similarity": 0.91,
         "confidence": 0.73, "nli_rerank_delta": -0.02, "status": "active"}
      ],
      "latency_ms": 312,
      "p_at_3": 1.0,
      "mrr": 1.0
    }
  },
  "comparison": {
    "kendall_tau": 0.95,
    "rank_changes": [],
    "mrr_delta": 0.0,
    "p_at_3_delta": 0.0,
    "latency_overhead_ms": 298
  }
}
```

**Metrics computed per scenario**:
- P@K (precision at K) — using `baseline.entry_ids` as soft ground truth, or
  `expected` if hand-authored
- MRR (mean reciprocal rank)
- Kendall tau between profiles (rank correlation)
- Rank change list (entries that moved positions)
- Latency delta

**Snapshot read-only enforcement**: The eval engine opens the snapshot DB with
a read-only SQLite URI (`?mode=ro`). No analytics writes. No usage recording.
No mutation of any kind.

---

### Deliverable 4: `unimatrix eval report`

**CLI**:
```
unimatrix eval report \
  --results results/ \
  --out report.md \
  [--scenarios scenarios.jsonl]
```

**Output**: Markdown report with:

1. **Summary table** — per-profile aggregate P@K, MRR, avg latency, rank change rate:
   ```
   | Metric         | baseline | candidate | delta  |
   |---------------|----------|-----------|--------|
   | P@3            | 0.82     | 0.86      | +4.9%  |
   | MRR            | 0.74     | 0.79      | +6.8%  |
   | Avg latency    | 18ms     | 330ms     | +312ms |
   | Rank changes   | —        | 127/500   | 25.4%  |
   ```

2. **Notable ranking changes** — queries where result order changed, sorted by
   impact (size of Kendall tau drop):
   ```
   ### Query: "integration test fixture setup" (tau=0.42)
   | Rank | baseline          | candidate              |
   |------|-------------------|------------------------|
   | 1    | entry-45 (0.91)   | entry-12 (nli=0.94)    |
   | 2    | entry-12 (0.87)   | entry-45 (nli=0.71)    |
   ```

3. **Latency distribution** — histogram of latency_ms per profile. Critical for
   the W1-4 gate condition (NLI latency overhead must be acceptable).

4. **Entry-level analysis** — which entries gained / lost rank most across all
   scenarios. Surfaces entries that the candidate profile consistently promotes
   or demotes, enabling spot-checking for correctness.

5. **Zero-regression check** — explicit list of scenarios where the candidate
   profile degraded results (MRR or P@K worse than baseline). Gate condition:
   this list should be empty or explainable.

The report is the artifact the human reviews to decide: does this change
actually improve things?

---

### Deliverable 5: UDS Live Client (`UnimatrixUdsClient`)

**Location**: `product/test/infra-001/harness/uds_client.py`

**Purpose**: Connect to a running daemon's MCP UDS socket rather than spawning
a subprocess. Enables eval and testing against the production path without
disrupting existing stdio tests.

```python
class UnimatrixUdsClient:
    """MCP client over Unix domain socket (daemon mode).

    Connects to a running unimatrix daemon's MCP socket.
    Same tool API surface as UnimatrixClient.
    """
    def __init__(self, socket_path: str | Path, timeout: float = DEFAULT_TIMEOUT): ...
    def connect(self): ...          # AF_UNIX connect + MCP initialize
    def disconnect(self): ...       # MCP shutdown + close socket
    def __enter__(self) / __exit__: ...

    # Same typed tool methods as UnimatrixClient:
    # context_search, context_store, context_lookup, context_get,
    # context_correct, context_deprecate, context_status, context_briefing,
    # context_quarantine, context_enroll, context_cycle, context_cycle_review
```

**Wire protocol**: MCP JSON-RPC over `AF_UNIX SOCK_STREAM`. The `rmcp` library
uses length-prefixed framing on the UDS socket (same as SSE transport adaption).
The client must match this framing — not line-delimited JSON (which is the stdio
transport format).

**`eval live` mode**: With the UDS client, eval scenarios can be replayed against
a live running daemon:
```
unimatrix eval live \
  --socket ~/.unimatrix/.../unimatrix-mcp.sock \
  --scenarios scenarios.jsonl \
  --out live-results/
```
This exercises the full production stack (auth, rate limiting, usage recording,
background tick interactions) with real production knowledge state. Not a frozen
snapshot — results reflect the live system.

**Test additions**: New `test_eval_uds.py` suite covering:
- UDS connection lifecycle (connect, initialize, shutdown)
- Tool call parity with stdio client (same query → same results)
- Concurrent client behavior (multiple UDS clients vs. single daemon)
- Path distinction validation: `source="uds"` in query_log for UDS-sourced queries

---

### Deliverable 6: Hook IPC Simulation Client (`UnimatrixHookClient`)

**Location**: `product/test/infra-001/harness/hook_client.py`

**Purpose**: Send synthetic lifecycle and observation events to the UDS hook
socket. Enables testing the observation pipeline, session management, and
eventually the GNN training signal pipeline without requiring Claude Code to
actually be running.

```python
class UnimatrixHookClient:
    """Send hook events to the running daemon's hook socket.

    Uses the HookRequest/HookResponse wire format from unimatrix-engine::wire.
    """
    def __init__(self, socket_path: str | Path, timeout: float = DEFAULT_TIMEOUT): ...

    def session_start(self, session_id: str, feature_cycle: str,
                      agent_role: str = "assistant") -> HookResponse: ...
    def session_stop(self, session_id: str, outcome: str | None = None) -> HookResponse: ...
    def pre_tool_use(self, session_id: str, tool: str,
                     input: dict) -> HookResponse: ...
    def post_tool_use(self, session_id: str, tool: str,
                      response_size: int,
                      response_snippet: str | None = None) -> HookResponse: ...
    def ping(self) -> HookResponse: ...
```

**Wire protocol**: JSON `HookRequest` sent over `AF_UNIX`, `HookResponse` read
back. Max payload: `MAX_PAYLOAD_SIZE` (from `unimatrix_engine::wire`). Unlike
MCP, this is not JSON-RPC — it's a simpler request/response envelope.

**What this enables**:

1. **Synthetic session sequences** for observation pipeline testing:
   ```python
   hook.session_start("test-session-001", "crt-022")
   hook.pre_tool_use("test-session-001", "Bash", {"command": "grep -r foo ."})
   hook.post_tool_use("test-session-001", "Bash", response_size=1200)
   hook.pre_tool_use("test-session-001", "Grep", {"pattern": "foo"})
   hook.post_tool_use("test-session-001", "Grep", response_size=340)
   hook.session_stop("test-session-001", outcome="complete")
   ```

2. **Co-access accumulation**: Drive co-access counts for specific entry pairs to
   test W1-3 eval scenarios that rely on co-access boost signal.

3. **W1-5 observation pipeline testing**: Inject generalized event types once
   W1-5 ships (new event schema). The hook client becomes the primary way to
   generate domain-specific observation data in tests.

4. **W3-1 training signal**: Generate synthetic behavioral patterns (retrieval →
   successful completion vs. retrieval → re-search) to test GNN training label
   quality before deploying on real production data.

**Test additions**: New `test_eval_hooks.py` suite covering:
- Session lifecycle round-trips (start → tool calls → stop)
- Observation record creation in DB (validate via `context_status`)
- Session keyword extraction (col-022 keywords field populated)
- Invalid payload rejection (MAX_PAYLOAD_SIZE, malformed JSON)

---

## A/B Testing Workflow

For each W1/W2/W3 intelligence feature, the eval cycle is:

```
Before the feature branch:
  unimatrix snapshot --out eval/pre-nli.db --anonymize

Extract scenarios from production history:
  unimatrix eval scenarios --db eval/pre-nli.db --limit 500 --out eval/scenarios.jsonl

Implement the feature (W1-4 NLI in this example).

Author baseline and candidate profiles:
  # baseline.toml — empty (current defaults)
  # nli.toml — [inference] nli_model = "..." nli_rerank_top_k = 20

Run comparison:
  unimatrix eval run \
    --db eval/pre-nli.db \
    --scenarios eval/scenarios.jsonl \
    --configs eval/baseline.toml,eval/nli.toml \
    --out eval/results-nli/

Generate human report:
  unimatrix eval report --results eval/results-nli/ --out eval/report-nli.md

Decision: Does the report show measurable improvement? Acceptable latency?
Gate passed → ship W1-4. Gate failed → tune parameters, re-run.
```

This workflow is identical for W2-4 (GGUF), W3-1 (GNN weights), and any future
intelligence investment. The profile TOML is the only thing that changes.

---

## Data Sources for Scenarios

| Tier | Source | Effort | Quality |
|------|--------|--------|---------|
| 1 | `query_log` (real production queries) | Zero curation | High volume, soft ground truth |
| 2 | `sessions` + `query_log` join (grouped by feature cycle) | Low | Adds work context |
| 3 | `observations` + `query_log` join (behavioral success signals) | Medium | Enables implicit ground truth |
| 4 | Hand-authored scenarios | High per scenario | Hard labels, critical path coverage |

Tier 1 is sufficient to gate W1-4. Tier 3 becomes valuable for W3-1 GNN training
signal evaluation. Tier 4 should cover the known-critical queries that must rank
correctly under any intelligence change.

**Anonymization note**: The `--anonymize` flag on `unimatrix snapshot` enables
scenario sets to be checked into the repository as regression fixtures. Same
agent_id always maps to same pseudonym (SHA-256 with random snapshot salt),
preserving co-access relationship patterns.

---

## Implementation Notes for the Architect

### Rust Crate Placement

- `snapshot` subcommand: `crates/unimatrix-server/src/snapshot.rs` — parallel
  to `export.rs`. No new crate.
- `eval run/scenarios/report` subcommands: consider a new `crates/unimatrix-eval/`
  crate to keep eval infrastructure separate from server runtime. The eval crate
  depends on `unimatrix-store` and `unimatrix-server` (for `ServiceLayer`). Or
  add to `unimatrix-server` as additional modules — consistent with single-binary
  principle, avoids a new workspace member.
- Python additions: `product/test/infra-001/harness/` (existing convention).

### ServiceLayer Construction for Eval

`TestHarness::new()` already shows the pattern. The eval engine needs a
`EvalServiceLayer::from_profile(db_path, profile_config)` that:
1. Opens DB in read-only mode (`?mode=ro`)
2. Constructs `ServiceLayer` with profile-specified overrides
3. Builds vector index from DB (no write, no migration)
4. Returns a ready-to-search service handle

Key constraint: **never trigger migration** on a snapshot DB. Use
`SqlxStore::open_readonly()` or equivalent, not the standard open path.

### UDS Wire Framing

The MCP UDS transport uses `rmcp`'s async framing. The Python `UnimatrixUdsClient`
must match this framing exactly. Check `rmcp 0.16.0` source for the exact length
prefix format used on `UnixStream` (likely 4-byte big-endian length + JSON body,
but verify against actual transport code before implementing).

### Read-Only Snapshot Guarantee

Both `eval run` and `eval live` (when operating against a snapshot) must open
the DB with `?mode=ro` to enforce the read-only constraint at the SQLite layer.
This prevents accidental analytics writes during eval — especially important
because `ServiceLayer` construction wires up the analytics write queue.

### Test Infra Extension

Per Unimatrix convention: test infrastructure is cumulative. The eval harness
extends `TestHarness` and `CalibrationScenario`/`RetrievalScenario` — it does
not create new isolated scaffolding. The existing `kendall_tau()` and ranking
assertion helpers are reused directly in the eval engine's metrics computation.
