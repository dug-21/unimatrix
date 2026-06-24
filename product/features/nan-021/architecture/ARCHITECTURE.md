# nan-021 Architecture — HTTPS-Bridge Integration Fixture

> **Pure test-infrastructure feature.** NO production code changes. This document describes a
> CUMULATIVE extension to the `infra-001` harness (`product/test/infra-001/`) that stands up the real
> cloud path (HTTPS multi-project server, self-signed TLS with leaf-fingerprint pinning, the vnc-039
> stdio→HTTPS MCP bridge, live `/observe` POSTs) and drives a full `context_cycle` end-to-end over it,
> then proves HTTPS-vs-UDS `MetricVector` parity from a single byte-identical workload.
>
> Designed within LOCKED Resolved Decisions D-1..D-6 and AC-01..AC-07. Addresses SR-01..SR-06.

---

## System Overview

The personal-cloud marquee promise C0 (#5191) is *"for a remote slug, retrieval AND behavioral signals
AND analytics/learning all function at parity with a local-UDS deployment of the same workload —
measured, not asserted."* Today no automated test drives the live HTTPS transport: the Python harness
spawns `serve --stdio` only, and cycle-review tests SQL-seed the `topic_signal` attribution join instead
of deriving it. nan-021 is the named evidence artifact that ADVANCES C0 by driving the *shipped* cloud
bytes end-to-end and measuring parity. (#837 holds the `proven` flip; nan-021 produces the artifact #837
consumes — D-4.)

This feature touches the boundary between the **two distinct infra-001 instruments** and binds them with
one new comparator:

```
                        ┌──────────────────────────────────────────────────────────────────┐
                        │  nan-021 fixture — ONE workload driver, TWO live transports        │
                        └──────────────────────────────────────────────────────────────────┘
                                                   │
         ┌─────────────────────────────────────────┴──────────────────────────────────────────┐
         │  HTTPS leg (live cloud path)                       │  UDS leg (local baseline)        │
         │  ─────────────────────────────                     │  ──────────────────────────      │
         │  Docker smoke substrate (D-1a):                    │  Python harness substrate (D-1b):│
         │  shipped image, HTTP-on, slug registered,          │  serve --stdio + UDS daemon      │
         │  self-signed TLS leaf on volume                    │                                  │
         │                                                    │  UnimatrixUdsClient  (MCP/UDS)   │
         │  mcp-bridge.js  ← spawned, stdio JSON-RPC          │  UnimatrixHookClient (hook IPC)  │
         │       │  pinned HTTPS, Mcp-Session-Id, SSE         │                                  │
         │       ▼                                            │                                  │
         │  POST /v1/{slug}  (MCP)                            │  context_cycle / tool calls      │
         │  POST /v1/{slug}/observe (hooks, pinned)           │  + observes over UDS             │
         └─────────────────────────┬──────────────────────────┴───────────────┬─────────────────┘
                                   ▼                                          ▼
                        context_cycle_review(feature)                context_cycle_review(feature)
                                   │   MetricVector (HTTPS)                    │  MetricVector (UDS)
                                   └──────────────► MetricVector COMPARATOR ◄──┘
                                          field-for-field == modulo D-5 exclusion set
```

Both legs run **in the same test execution** against the **same parameterized workload object** with a
**single stable session identity** (D-6 live-vs-live, SR-05). The comparator is the new code; everything
else is reuse.

---

## Component Breakdown

The fixture decomposes into five components, each mapped to the existing infra-001 asset it EXTENDS
(SR-04: every new helper names its parent asset — net-new spawn/cert/credstore/bundle code is a fork
smell to be flagged, not written).

| # | Component | Responsibility | Extends (parent asset) | Net-new? |
|---|-----------|----------------|------------------------|----------|
| C1 | **HTTPS-leg standup** (shell) | Boot shipped image HTTP-on, register slug, restart, read leaf cert + bearer off volume, emit bundle, `init --bundle` into hermetic sandbox | `docker-http-posture-smoke.sh` Gates 1–7 (`emit_bundle`, `consume_bundle`, `vol`, store-delta) | extends |
| C2 | **Bridge-driven cycle** (shell→node) | Spawn `mcp-bridge.js <projectHash>`, drive `context_cycle(start) → Bash + tool calls → stop` over stdio JSON-RPC; fire live hooks → pinned `POST /v1/{slug}/observe` | NEW gate function in the smoke (`cloud_cycle_gates`), reuses `mcp-bridge.js`, `cert-pin.js`, `credstore.js`, `bundle.js`, `init.js` as-is | new fn, no new transport code |
| C3 | **UDS-leg baseline** (Python) | Drive the *identical* workload over `UnimatrixUdsClient` (MCP/UDS) + `UnimatrixHookClient` (hook IPC) against a `serve` UDS daemon | `harness/uds_client.py`, `harness/hook_client.py`, `conftest.py` fixtures | extends |
| C4 | **Workload driver** (Python) | The single parameterized workload (tool-call sequence + Bash content + stable session identity) fed to BOTH legs; the comparator owner | NEW module under `harness/` (e.g. `parity_workload.py`) — the only substantial net-new code | new (comparator + driver) |
| C5 | **Gate wiring** (shell/YAML) | Verify-by-name run-marker + exit-code discriminator; Docker acquisition; release-gate lane via `workflow_dispatch`/tag | `release-gate-lib.sh:run_smoke_gate`, the `docker pull \|\| inspect \|\| exit-4` acquisition, the release workflow | extends |

**Boundary rule (D-1 hybrid, SR-04):** the **Docker smoke owns the HTTPS standup + the bridge-driven
cloud cycle** (C1, C2 — they need the shipped image, the cert-on-volume, the busybox sidecar). The
**Python harness owns the UDS baseline + the `MetricVector` comparison/assertion code** (C3, C4). The two
legs hand their `MetricVector`s to one comparator. The comparator and the workload definition live in
Python (D-1: "the Python harness owns the comparison/assertion code"). See ADR-001 for how the legs
share one workload across the language boundary.

---

## Component Interactions

### HTTPS leg sequence (C1 → C2), grounded readiness gates (SR-01, no sleeps)

```
1. acquire image      docker pull "$IMAGE" || docker image inspect "$IMAGE" || exit 4   (ADR-006 / #5208)
2. boot HTTP-on       docker run ... ; READINESS GATE: poll daemon log for "HTTP transport active"
3. register slug      docker run ... --project-dir /data project register "$SLUG"
4. restart            docker restart "$CNAME" ; READINESS GATE: listener bound (poll, not sleep)
5. read cert+bearer   vol cat /data/.unimatrix/<hash>/tls/cert.pem ; vol cat .../token   (busybox sidecar)
                      READINESS GATE: cert.pem present + non-empty before any pinned client runs
6. emit bundle        emit_bundle()  (UNIMATRIX_PUBLIC_URL set; Gate-5 placeholder guard)
7. init --bundle      consume_bundle() → writes ~/.unimatrix/<projectHash>/remote.json + .mcp.json
                      READINESS GATE: remote.json present (mode 0600) before bridge spawn
8. spawn bridge       node mcp-bridge.js <projectHash>   (stdio JSON-RPC ↔ pinned HTTPS)
                      READINESS GATE: Mcp-Session-Id captured (initialize reply observed)
                      then DRIVE IMMEDIATELY — no wait between capture and first tool call (R-05/ADR-002)
9. drive cycle        context_cycle(start) → Bash + tool calls (hooks→pinned /observe) → context_cycle(stop)
9b. DURABILITY BARRIER  deadline-poll (≤~10s, sleep 1) until expected observe count durable (DIR incl -wal)
                      — SYMMETRIC with the UDS leg; HARD-fail on timeout, never review an empty stream (R-06/ADR-006)
10. cycle_review      context_cycle_review(feature) → MetricVector(HTTPS)
```

Every child (`mcp-bridge.js`, `init`, container) writes stderr to a file under the hermetic `$SANDBOX`,
tail-dumped on failure only, never swallowed (ADR-005 capture-first, lessons #5266/#5267). Readiness is
**event-driven** (log line present, file present, session-id captured) — never a fixed `sleep` (SR-01).
The bridge is spawned LAST (step 8) and driven IMMEDIATELY so the captured `Mcp-Session-Id` is never left
idle long enough for rmcp `keep_alive` to evict it; a mid-cycle eviction relies on the shipped single-flight
self-heal (#830), not a re-implementation (R-05, ADR-002).

### Bridge framing contract (D-2, SR-02)

The cycle MCP traffic flows **through `mcp-bridge.js`**, spawned and driven over stdio JSON-RPC — NOT by
POSTing `mcp_url` directly (bridge coverage must not be optimized away). The bridge reads the credential
from the credstore (`credstore.read(projectHash)`), opens a pinned HTTPS session, flushes the bearer
**only after** `verifyPeerFingerprint` matches, captures `Mcp-Session-Id` on `initialize` and replays it
byte-stable, and parses `text/event-stream` (rmcp forces SSE, #5129). The assertion proves the bridge
**carried** the traffic (session-id replay observed, SSE parsed, derived attribution present) — not just a
200/204.

### UDS leg sequence (C3), identical workload

```
1. serve UDS daemon   (conftest fixture — UnimatrixClient/serve over UDS)
2. drive cycle        SAME workload object: context_cycle(start) → Bash + tool calls
                      → observes via UnimatrixHookClient.post_tool_use(...) over hook IPC → stop
2b. DURABILITY BARRIER  the SAME deadline-poll helper, SAME predicate/deadline as the HTTPS leg (R-06/ADR-006)
3. cycle_review       context_cycle_review(feature) → MetricVector(UDS)
```

The durability barrier (step 9b / 2b) is a SINGLE shared helper parameterized by leg (C4-owned). Symmetry
is load-bearing: `/observe` writes are fire-and-forget WAL, not synced before the 204 (#5265), and the UDS
observe path is async too — an asymmetric barrier compares a settled vector against an un-settled one and
self-induces a parity mismatch (ADR-006).

### Parity assertion (C4)

```
assert MetricVector(HTTPS) == MetricVector(UDS)   field-for-field, modulo D-5 exclusion set
assert MetricVector(HTTPS) non-empty              total_tool_calls>0, session_count>0, phases populated
assert every driven observation topic_signal == feature   derived (no SQL/struct seed in path)
```

---

## Technology Decisions

| ADR | Decision | Drives |
|-----|----------|--------|
| **ADR-001** | Hybrid substrate with a single-driver, one-stable-identity dual-transport workload | D-1, D-6, SR-05; AC-01, AC-04, AC-07 |
| **ADR-002** | Drive the cycle THROUGH `mcp-bridge.js` over pinned HTTPS; event-driven readiness gates, minimized idle window (spawn-bridge-last, drive-immediately) + intentional coupling to the shipped #830 self-heal, capture-first stderr (no sleeps) | D-2, SR-01, SR-02, R-05; AC-02 |
| **ADR-003** | `MetricVector` comparison contract — field-for-field equality modulo the enumerated D-5 exclusion set; FIRST-LIVE-RUN field-by-field validation gate; divergence disposition is a PRODUCT/HUMAN call (file a bug OR product-signed exclusion amendment), never a silent widen | D-5, R-01, R-02; AC-04 |
| **ADR-004** | Derived-attribution assertion contract — `topic_signal == feature`, no seed anywhere in the path | SR-06; AC-03 |
| **ADR-005** | Docker acquisition + false-green discriminator — reuse nan-019's `pull \|\| inspect \|\| exit-4` and verify-by-name run-marker verbatim; release-gate lane via `workflow_dispatch`/tag; capture-first child stderr | D-3, SR-01, SR-03; AC-05 |
| **ADR-006** | Symmetric observe-durability barrier — bounded deadline-poll gating BOTH `context_cycle_review` calls identically, before the non-empty/parity assertions | R-06; AC-04 |

ADRs are stored in Unimatrix (topic `nan-021`, category `decision`) AND as files in this directory.

---

## Integration Points (exercised as-is, NOT modified — AC-06)

| Feature | Surface exercised | How nan-021 uses it |
|---------|-------------------|---------------------|
| **vnc-038** | v:2 bundle (`compose_route_urls` → `mcp_url`, `observe_url`), leaf-fp pin contract | Emits + consumes a v:2 bundle; pins the leaf fp; register-pre-serve (entry #5098 trap) |
| **vnc-039** | `mcp-bridge.js` stdio→HTTPS bridge, credstore, pinned-flush, SSE/session-id replay | Spawns the bridge; drives the cycle through it (ADR-001/ADR-002 vnc-039 reused verbatim) |
| **#830/#5280** | rmcp `keep_alive` idle eviction + shipped single-flight self-heal | Exercised as-is; fixture minimizes the idle window and relies on the self-heal, never re-implements it (ADR-002, R-05) |
| **#5265 / observe WAL** | fire-and-forget `/observe` WAL write (async, not synced before 204) | Exercised as-is; fixture gates the review behind a symmetric durability barrier rather than changing the write (ADR-006, R-06) |
| **vnc-034** | cert-fingerprint format `sha256:<lowercase-hex>` over leaf DER (#4948) | `cert-pin.js:computeFingerprint`/`verifyPeerFingerprint` reused; harness reads cert off volume for the pin |
| **crt-055 / attribution** | `extract_topic_signal` → `enrich_topic_signal_with_source` derivation chain | The driven Bash content carries a parseable feature-ID; the column is DERIVED, never seeded (ADR-004) |
| **#832** | stable CC session id across declaration + observes (cycle-join) | The single stable session identity in the workload driver IS the #832 regression guard (SR-05) |

No production code, route, bridge, TLS provisioning, bundle schema, or attribution code is changed
(Non-Goals; AC-06). If the fixture surfaces a real cloud-path defect, fixing it is a separate bugfix.

---

## Integration Surface

Exact names/signatures so downstream agents do not invent them. Sourced from the live infra-001 tree.

### Docker smoke (`product/test/infra-001/scripts/docker-http-posture-smoke.sh`)

| Integration Point | Type / Signature | Source |
|-------------------|------------------|--------|
| Existing gates | Gate 1 (HTTP listener binds, log `"HTTP transport active"`), Gate 2 (pinned POST `/v1/<slug>/observe` → 204), Gate 3 (per-slug store file exists), Gate 4/AC-05 (store grew), Gate 5 `emit_bundle()`, Gate 6 `consume_bundle()`, Gate 7 `fire_hook()` + `gate7_store_size()` | smoke L356–439 |
| Docker acquisition | `docker pull "$IMAGE" \|\| docker image inspect "$IMAGE" >/dev/null 2>&1 \|\| { exit 4; }` (IMAGE= branch only); else `docker build` | smoke (IMAGE= branch) |
| HTTP-on env assert | `docker image inspect "$IMAGE" --format '{{json .Config.Env}}' \| grep -q 'UNIMATRIX_HTTP_ENABLED=true'` | smoke L336–338 |
| Busybox sidecar | `vol() { docker run --rm -v "$VOL:/data:ro" busybox "$@"; }`; `vol cat .../tls/cert.pem`, `vol cat .../token` | smoke L47, L397–399 |
| Slug register | `docker run --rm -v "$VOL:/data" "$IMAGE" --project-dir /data project register "$SLUG"` then `docker restart "$CNAME"` | smoke |
| Pinned curl | `curl -sS --cacert "$TMP/cert.pem" -X POST "https://localhost:18443/v1/${SLUG}/observe" -H "Authorization: Bearer ${TOKEN}"` | smoke L405–413 |
| HTTPS port | `18443` (hardcoded) | smoke L30, L349 |
| Per-slug store | `/data/.unimatrix/${SLUG}/unimatrix.db` (+ `-wal`, `-shm`); `store_size()` via `du -s` over the dir | smoke L49–55, L417 |
| Sourceable guard | `if [ "${BASH_SOURCE[0]}" != "${0}" ]; then return 0 2>/dev/null \|\| true; fi` (after gate-fn defs, before preflight) | smoke L302–304 |
| Stub seams | `SMOKE_EMIT_CMD`, `SMOKE_INIT_CMD`, `SMOKE_HOOK_CMD`, `SMOKE_STORE_SIZE_CMD` (default = real cmd; override = whitespace-split argv) | smoke L73–135 |
| Terminal marker | `[<name>-smoke] ALL GATES PASSED ...` (anchored, last statement) | smoke L439 |

### Release gate lib (`product/test/infra-001/scripts/release-gate-lib.sh`)

| Integration Point | Type / Signature | Source |
|-------------------|------------------|--------|
| Gate runner | `run_smoke_gate IMAGE SMOKE_CMD...` — invokes smoke once, captures rc (no pipe), discriminates, asserts marker; returns 0 iff rc==0 AND marker captured | lib L44–62 |
| Exit-code truth table | `0`=passed, `3`=skipped (Docker absent), `4`=image unacquirable, `1`=shipped-image-path broken, `*`=unexpected | lib L52–57 |
| Run-marker regex | `grep -qxE '\[[a-z0-9-]+-smoke\] ALL GATES PASSED.*'` (anchored, no substring forge) | lib L59–60 |

### Python harness

| Integration Point | Type / Signature | Source |
|-------------------|------------------|--------|
| Stdio MCP client | `UnimatrixClient(binary_path, project_dir=None, timeout=..., extra_env=None)`; spawns `[binary, "--project-dir", dir, "serve", "--stdio"]` | `harness/client.py:76`, L104 |
| `context_cycle` | `context_cycle(cycle_type, topic, *, keywords=None, phase=None, outcome=None, next_phase=None, goal=None, agent_id=None, format=None, timeout=None) -> MCPResponse` | `client.py:679` |
| `context_cycle_review` | `context_cycle_review(feature_cycle, *, agent_id=None, format=None, force=None, auto_close=None, timeout=None) -> MCPResponse` | `client.py:658` |
| UDS MCP client | `UnimatrixUdsClient(socket_path, timeout=...)`; `connect()`/`disconnect()`; `context_cycle()`, `context_cycle_review()`, etc. (AF_UNIX, newline-delimited JSON-RPC) | `harness/uds_client.py:85` |
| Hook IPC client | `UnimatrixHookClient(socket_path, timeout=...)`; `post_tool_use(session_id, tool, response_size, response_snippet, ...)`, `pre_tool_use(...)`, `session_start(...)`, `session_stop(...)`, `ping()`; 4-byte BE length-prefix frames | `harness/hook_client.py:108`, L236–306 |
| Fixtures | `server`, `shared_server`, `fast_tick_server`, `populated_server`, `admin_server` — yield `UnimatrixClient` | `harness/conftest.py:130–269` |
| Assertions | `parse_tool_result(response) -> ToolResult` (`.content`, `.is_error`, `.text`, `.parsed`) | `harness/assertions.py` |
| Generators | `make_entries(num, seed, topic_distribution, category_mix)` | `harness/generators.py` |
| Seed helpers (FORBIDDEN in nan-021 path) | `_seed_observation_sql_lifecycle(db_path, feature_ids, num_records=20)`, `_seed_attributed_observations_832(...)`, Rust `make_stamped_event(..., topic_signal)` | `suites/test_lifecycle.py:1253`, L4428; `uds/listener/tests/stamp_read.rs:28` |

### JS edge (`packages/unimatrix/lib/hook-client/`)

| Integration Point | Type / Signature | Source |
|-------------------|------------------|--------|
| Bridge | `node mcp-bridge.js <projectHash>` → `buildSession(projectHash)` reads credstore, validates `mcp_url`/`token`/`fingerprint`, opens pinned HTTPS; `StdioFramer` + `HttpSession` + `Lifecycle` | `lib/hook-client/mcp-bridge.js:86` |
| Cert pin | `computeFingerprint(derBuffer) -> "sha256:"+hex`; `verifyPeerFingerprint(socket, pinnedFp) -> null \| Error` (on `secureConnect`, `rejectUnauthorized:false`) | `lib/hook-client/cert-pin.js:26,67` |
| Credstore | `pathFor(projectHash) -> os.homedir()/".unimatrix"/projectHash/"remote.json"`; `read(projectHash) -> {schema_version:1, mcp_url, observe_url, token, fingerprint, timeouts?}` (mode 0600) | `lib/hook-client/credstore.js:48,77` |
| Bundle | `decodeBundle(raw) -> {v:2, mcp_url, observe_url, token, fp}` (5-guard strict schema, v:2 only) | `lib/hook-client/bundle.js:67` |
| Init | `init --bundle <blob> --project-dir <dir>` → writes credstore + token-free `.mcp.json` stdio entry | `lib/init.js` |
| Hook client | `node lib/hook-client/index.js <EVENT>` (stdin JSON; fail-open exit 0) | `lib/hook-client/index.js:341` |

### MetricVector (`crates/unimatrix-store/src/metrics.rs`) — the comparison contract

| Integration Point | Type / Signature | Source |
|-------------------|------------------|--------|
| `MetricVector` | `{ computed_at: u64, universal: UniversalMetrics, phases: BTreeMap<String, PhaseMetrics>, domain_metrics: HashMap<String, f64> }` | `metrics.rs:102` |
| `UniversalMetrics` (21 fields) | `total_tool_calls:u64`, `total_duration_secs:u64`, `session_count:u64`, `search_miss_rate:f64`, `edit_bloat_total_kb:f64`, `edit_bloat_ratio:f64`, `permission_friction_events:u64`, `bash_for_search_count:u64`, `cold_restart_events:u64`, `coordinator_respawn_count:u64`, `parallel_call_rate:f64`, `context_load_before_first_write_kb:f64`, `total_context_loaded_kb:f64`, `post_completion_work_pct:f64`, `follow_up_issues_created:u64`, `knowledge_entries_stored:u64`, `sleep_workaround_count:u64`, `agent_hotspot_count:u64`, `friction_hotspot_count:u64`, `session_hotspot_count:u64`, `scope_hotspot_count:u64` | `metrics.rs:45–89` |
| `PhaseMetrics` | `{ duration_secs: u64, tool_call_count: u64 }` | `metrics.rs:92` |
| `RetrospectiveReport.metrics` | carries `metrics: MetricVector` (the comparable surface) | `unimatrix-observe/src/types.rs:381` |
| **D-5 EXCLUSION SET** (wall-clock) | `MetricVector.computed_at`; `UniversalMetrics.total_duration_secs`; `PhaseMetrics.duration_secs` (per phase) | enumerated in ADR-003 |

The `MetricVector` is read from the JSON text of `context_cycle_review(feature)` (parsed via
`parse_tool_result`). The comparator operates on the parsed dict, NOT on the Rust struct directly.

---

## Cross-Cutting Concerns

- **Error boundaries / diagnosability (SR-01):** every child stderr → hermetic `$SANDBOX` file,
  tail-dumped on failure only, never `2>/dev/null` on a token-free child (ADR-005, #5266). The
  `emit_bundle` child stays suppressed (its blob carries the bearer).
- **Determinism (D-5):** the comparator excludes exactly the three enumerated wall-clock fields; an
  unexpected non-equal field outside the set is a REAL failure, never a tolerance to widen (ADR-003).
- **First-live-run parity validation + disposition authority (R-01/R-02):** the 3-field exclusion is an
  ASSUMPTION until proven. The first live dual-transport run is examined FIELD-BY-FIELD across all
  non-excluded `UniversalMetrics` fields + `phases` key set/`tool_call_count` and must match once before
  the gate is trusted. Session-lifecycle fields (`cold_restart_events`, `coordinator_respawn_count`,
  `context_load_before_first_write_kb`, `total_context_loaded_kb`, `permission_friction_events`) are the
  prime transport-inherent suspects. ANY non-wall-clock divergence is a PRODUCT/HUMAN call — either file a
  GitHub bug (real parity defect, the fixture working) OR add to the exclusion set with product sign-off +
  recorded rationale in ADR-003. The implementer/tester NEVER silently widens the set (ADR-003).
- **Observe durability (R-06):** `/observe` is fire-and-forget WAL, not synced before the 204 (#5265). A
  SYMMETRIC bounded deadline-poll barrier gates BOTH `context_cycle_review` calls before the non-empty /
  parity assertions; the same helper, same predicate/deadline on both legs (ADR-006). Timeout = hard
  fail, never an empty compare.
- **Idle-session eviction (R-05):** rmcp `keep_alive` evicts idle MCP sessions (#5280/#830). The bridge is
  spawned LAST and driven IMMEDIATELY (no wait between session capture and first call); a mid-cycle
  eviction relies on the shipped single-flight self-heal (#830), not a re-implementation (ADR-002).
- **False-green (SR-03):** Docker-absent → exit 3 (hard fail in the gate); image unacquirable → exit 4;
  positive terminal run-marker required (ADR-005). Skip never masquerades as parity-proven.
- **Hermeticity:** `init --bundle` writes the credstore ONLY under the isolated `$HOME=$SANDBOX/home`;
  the bridge resolves the credential from there (nan-020 negative-control precedent, #5258).

---

## Open Questions

1. **`projectHash` derivation in the harness.** The bridge is invoked `node mcp-bridge.js <projectHash>`
   and the credstore lives at `~/.unimatrix/<projectHash>/remote.json`. The exact hash that `init
   --bundle` writes under must be captured at consume time (read back from the credstore dir, or echoed
   by `init`) rather than recomputed in the fixture — recomputing it is a fork smell (SR-04). **For
   spec/pseudocode:** confirm `init.js` surfaces the `projectHash` (stdout/log) or the fixture lists the
   single dir under `$HOME/.unimatrix/` after consume. Net-new hashing is forbidden.

2. **Cross-language workload sharing (C4).** The single workload object is defined in Python (D-1 owns
   the comparator there), but the HTTPS leg drives `mcp-bridge.js` from the **shell** smoke. The
   workload must be expressed once and consumed by both. ADR-001 resolves this as a **declarative
   workload manifest** (an ordered tool-call list + Bash content + session identity) that the Python
   driver executes directly for UDS and that the shell C2 gate reads/replays for HTTPS. **For spec:**
   pin the manifest format and the single source of truth (a JSON/py file under `harness/`), so neither
   leg hand-writes a parallel script (SR-05).

3. **Single-execution coupling of the two legs (D-6).** Both `MetricVector`s must come from the *same*
   test run to be live-vs-live. The Docker smoke (HTTPS leg) and pytest (UDS leg) are separate
   processes. **For spec:** decide the orchestration seam — either pytest shells out to the smoke's C1/C2
   gate and reads back `MetricVector(HTTPS)`, or the smoke emits `MetricVector(HTTPS)` to a sandbox file
   that the pytest comparator ingests in the same invocation. ADR-001 recommends pytest-as-orchestrator
   (the comparator owner drives both legs); confirm the release-gate lane invokes pytest, which invokes
   the smoke gate.

4. **`feature_cycle` value for the driven cycle.** AC-03 requires `topic_signal == feature`. The Bash
   content must carry a feature-ID token that the attribution scanner parses to exactly that value. **For
   spec:** pin the literal feature-ID used in the workload (and ensure it is a *valid* registry feature
   so `enrich_topic_signal_with_source` resolves `declared`, not `unattributed` — SR-06).

5. **Durability-barrier predicate (R-06/ADR-006).** The barrier polls until the *expected* observe count
   is durable. **For spec:** pin where that expected count comes from (the manifest's count of
   observe-firing tool calls) and the durability read used (store-DIR size incl. `-wal` reaching a stable
   point, or the review's own observe count once non-zero and stable) — sampled at DIR granularity, never
   `unimatrix.db` alone (#5265). Confirm the bound (~10s, sleep 1) and that timeout hard-fails with the
   observed-vs-expected count.
