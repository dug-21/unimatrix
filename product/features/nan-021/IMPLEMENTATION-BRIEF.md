# nan-021 — Implementation Brief

> **HTTPS-Bridge Integration Fixture: Drive Cycles End-to-End Over the Cloud Transport to Prove Parity**
>
> Coordination artifact for Session 2 delivery. Pure **test-infrastructure** feature — a CUMULATIVE
> extension to `infra-001` (`product/test/infra-001/`). **Zero production-code changes.** Technical
> ground truth lives in the source documents below; this brief routes component work and summarizes the
> locked decisions, integration surface, and constraints. Issue **#836**.

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/nan-021/SCOPE.md |
| Scope Risk Assessment | product/features/nan-021/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/nan-021/architecture/ARCHITECTURE.md |
| Specification | product/features/nan-021/specification/SPECIFICATION.md |
| Risk / Test Strategy | product/features/nan-021/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/nan-021/ALIGNMENT-REPORT.md |
| Acceptance Map | product/features/nan-021/ACCEPTANCE-MAP.md |

### ADRs (files + Unimatrix, topic `nan-021`, category `decision`)

| ADR | File |
|-----|------|
| ADR-001 — Hybrid substrate, single-driver dual-transport workload | architecture/ADR-001-hybrid-substrate-single-driver-dual-transport.md |
| ADR-002 (#5294) — Bridge-in-path, readiness gates, capture-first stderr; **intentional #830 self-heal coupling** | architecture/ADR-002-bridge-in-path-readiness-gates.md |
| ADR-003 (#5293) — MetricVector comparison contract; **first-live-run validation gate + product disposition authority** | architecture/ADR-003-metricvector-comparison-contract.md |
| ADR-004 — Derived-attribution contract (no seed anywhere in path) | architecture/ADR-004-derived-attribution-no-seed.md |
| ADR-005 — Docker acquisition + false-green discriminator | architecture/ADR-005-docker-acquisition-false-green-discriminator.md |
| ADR-006 — Symmetric observe-durability barrier | architecture/ADR-006-symmetric-durability-barrier.md |

---

## Goal

Stand up the real cloud path in `infra-001` (HTTPS / multi-project server, self-signed TLS with
leaf-fingerprint pinning, the vnc-039 stdio→HTTPS MCP bridge, live hook `/observe` POSTs, slug routing)
and drive a full `context_cycle(start) → tool calls incl. a real Bash command → context_cycle(stop)` over
the bridge so the `topic_signal` attribution column is **derived**, never seeded. Assert every driven
observation attributes `topic_signal == feature` and that `context_cycle_review` over HTTPS returns the
**same non-empty `MetricVector`** as a local-UDS run of the identical workload (live-vs-live). This is the
named evidence artifact that **ADVANCES** capability C0 (#5191); it does **not** flip C0 to `proven` (held
by #837).

---

## ⚠ DELIVERY GATE — First-Live-Run Field-by-Field Validation + Product Disposition Authority

> **The single place this fixture touches the DEFINITION of C0 parity. The tester and the delivery
> leader MUST honor this gate — it is NOT optional, and it is NOT the implementer's call.** (ADR-003 / #5293;
> SPECIFICATION NFR-8 + amended AC-04; R-01/R-02.)

The 3-field D-5 exclusion set is an **UNPROVEN ASSUMPTION**, not a verified fact. The premise
"`MetricVector` is transport-agnostic, only the three wall-clock fields differ" is load-bearing but
unverified until a real dual-transport run.

1. **The parity gate is NOT TRUSTED** until the **first live dual-transport run** is examined
   **field-by-field across all 18 non-excluded `UniversalMetrics` fields** (plus the `phases` key set and
   per-phase `tool_call_count`) and confirmed to actually match. This first-run field-by-field confirmation
   must pass ONCE before the gate is relied upon as a parity proof.
2. **DISPOSITION AUTHORITY — a divergence is a PRODUCT/HUMAN call, never an implementer/tester edit.** Any
   non-wall-clock field that diverges (on the first run or any later run) is dispositioned as exactly ONE of:
   - **(a) Real parity defect** → file a **GitHub bug**. The fixture did its job; the gate stays RED until
     addressed. (This is *why* C0 is measured, not asserted.)
   - **(b) Transport-inherent field** → add to the exclusion set **ONLY with explicit product sign-off + a
     recorded rationale appended to ADR-003 (#5293) via `context_correct`** — naming the field, the
     transport-inherent reason, and the approver.
3. **NEVER silently widen the exclusion set to make a red go green** — that IS the R-01/R-02 failure mode
   (reactive widening hides real divergence). [SPECIFICATION NFR-8]
4. **At-risk session-lifecycle-derived fields (the prime divergence suspects)** — flagged by name so the
   tester examines them first and the leader knows where a product call may land:
   `cold_restart_events`, `coordinator_respawn_count`, `context_load_before_first_write_kb`,
   `total_context_loaded_kb`, `permission_friction_events`. A first-run divergence on any of these may force
   a product-signed exclusion-set amendment before first-green.

**Intentional #830 coupling (ADR-002 / #5294, NFR-7):** the fixture's reliability **intentionally depends
on** the shipped single-flight `keep_alive` self-heal (#830) holding. The coupling is desirable, not
incidental — if the self-heal regresses, the cloud cycle flakes HERE, so a flake in this gate correctly
SIGNALS a #830 regression. **This fixture therefore doubles as a standing #830 self-heal regression guard;**
the dependency is recorded as intended, not a hidden fragility. The fixture must NOT re-implement
reconnection (that re-authors shipped behavior, violating NFR-2).

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| D-1 — Fixture substrate | **HYBRID**: Docker smoke owns HTTPS standup + bridge cycle; Python harness owns UDS baseline + the MetricVector comparator. No fork. | SCOPE D-1 | ADR-001-hybrid-substrate-single-driver-dual-transport.md |
| D-2 — Bridge in path | Drive the cycle THROUGH `mcp-bridge.js` over pinned HTTPS; never POST `mcp_url` directly. | SCOPE D-2 | ADR-002-bridge-in-path-readiness-gates.md |
| D-3 — CI lane | Standing gate in the **release-gate Docker lane via `workflow_dispatch`/tag** (mirroring nan-019), NOT per-PR. | SCOPE D-3 | ADR-005-docker-acquisition-false-green-discriminator.md |
| D-4 — Naming / ownership | nan-021 owns the fixture; ADVANCES C0 (#5191) as named evidence artifact; the `proven` flip is held by #837. | SCOPE D-4 | (framing — see ARCHITECTURE System Overview) |
| D-5 — Parity tolerance | Exact field-for-field equality MODULO a documented, enumerated, closed exclusion set of wall-clock fields. | SCOPE D-5 | ADR-003-metricvector-comparison-contract.md |
| D-6 — UDS baseline | LIVE-vs-LIVE: identical workload over both transports in the same test execution; NOT a captured golden vector. | SCOPE D-6 | ADR-001-hybrid-substrate-single-driver-dual-transport.md |

**Provenance note (Alignment WARN, accepted — no human approval required):** two net-new
fixture-orchestration mechanisms beyond the literal SCOPE D-1..D-6 text, both pure test orchestration that
preserve NFR-1 (zero production diff) and NFR-2 (no fork): **ADR-006 / FR-10** (symmetric observe-durability
barrier, from R-06 / #5265 fire-and-forget WAL) and **ADR-002 / NFR-7** (idle-window minimization + reliance
on the shipped keep_alive self-heal, from R-05 / #5280/#830). They make AC-04 a non-vacuous, non-flaky parity
proof; they do not expand the parity surface.

**Human design-gate refinement (2026-06-24):** ADR-003 (#5293) adds the **first-live-run validation gate +
product disposition authority** and SPECIFICATION adds **NFR-8** (no silent exclusion-set widening — R-01/R-02);
ADR-002 (#5294) records the **intentional #830 coupling** (the fixture doubles as a #830 self-heal regression
guard). See the DELIVERY GATE section above — these are the delivery-time obligations the tester/leader must honor.

---

## Component Map

The five fixture components (ARCHITECTURE §Component Breakdown). Pseudocode and test-plan file paths are
filled during Session 2 Stage 3a; the components below are the expected decomposition.

| Component | Responsibility | Extends (parent asset) | Pseudocode | Test Plan |
|-----------|----------------|------------------------|-----------|-----------|
| C1 — HTTPS-leg standup (shell) | Boot image HTTP-on, register slug, restart, read leaf cert + bearer off volume, emit bundle, `init --bundle` into hermetic sandbox | `docker-http-posture-smoke.sh` Gates 1–7 | pseudocode/c1-https-standup.md | test-plan/c1-https-standup.md |
| C2 — Bridge-driven cycle (shell→node) | Spawn `mcp-bridge.js`, drive cycle over stdio JSON-RPC; fire live hooks → pinned `/observe` | NEW gate fn in the smoke; reuses bridge/cert-pin/credstore/bundle/init JS as-is | pseudocode/c2-bridge-cycle.md | test-plan/c2-bridge-cycle.md |
| C3 — UDS-leg baseline (Python) | Drive the identical workload over `UnimatrixUdsClient` + `UnimatrixHookClient` against a UDS daemon | `harness/uds_client.py`, `harness/hook_client.py`, `conftest.py` | pseudocode/c3-uds-baseline.md | test-plan/c3-uds-baseline.md |
| C4 — Workload driver + comparator (Python) | The single parameterized workload (manifest) fed to both legs; the MetricVector comparator + durability barrier helper | **NEW** module under `harness/` (`parity_workload.py`) — the only substantial net-new code | pseudocode/c4-workload-comparator.md | test-plan/c4-workload-comparator.md |
| C5 — Gate wiring (shell/YAML) | Verify-by-name run-marker + exit-code discriminator; Docker acquisition; release-gate lane | `release-gate-lib.sh:run_smoke_gate`, nan-019 acquisition, release workflow | pseudocode/c5-gate-wiring.md | test-plan/c5-gate-wiring.md |

**Boundary rule (D-1 hybrid, SR-04):** the Docker smoke owns C1/C2 (need the shipped image + cert-on-volume
+ busybox sidecar); the Python harness owns C3/C4 (the comparator + workload definition live in Python). The
two legs hand their `MetricVector`s to one comparator. ADR-001 recommends **pytest-as-orchestrator**: one
pytest invocation drives the UDS leg and shells out to the smoke's C1/C2 gate, then ingests
`MetricVector(HTTPS)` from a fresh sandbox file carrying a run-correlation token.

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Files to Create / Modify

All under `product/test/infra-001/` (+ the release-gate CI lane + nan-021 docs). Exact filenames are
pseudocode-phase detail; the seams below are LOCKED.

- `scripts/docker-http-posture-smoke.sh` — **extend** with a new `cloud_cycle_gates` gate function (C2):
  bridge spawn + stdio JSON-RPC cycle drive + pinned `/observe` hooks + durability barrier + HTTPS-side
  `context_cycle_review`, emitting `MetricVector(HTTPS)` to a `$SANDBOX` file with a run-correlation token.
  Reuse Gates 1–7 standup verbatim (C1). Do NOT add a third spawn/cert/bundle path.
- `scripts/release-gate-lib.sh` — reuse `run_smoke_gate` / exit-code truth table / run-marker regex as-is
  (C5); no new gate-runner logic.
- `harness/parity_workload.py` — **NEW** (C4): the declarative workload manifest (ordered tool-call list +
  load-bearing Bash content + single stable session identity + expected observe count), the single driver
  consumed by both legs, the symmetric durability-barrier helper, and the `MetricVector` comparator
  (field-for-field modulo the named D-5 exclusion set). Sole substantial net-new module.
- `harness/` UDS-leg test (C3) — **extend** existing `conftest.py` fixtures + `UnimatrixUdsClient` /
  `UnimatrixHookClient`; the pytest test that orchestrates both legs and runs the comparator.
- Release-gate CI lane (`workflow_dispatch`/tag) — **extend/add** a job mirroring nan-019 that invokes the
  pytest orchestrator (which invokes the smoke gate). NOT `pull_request`.

---

## Key Data Structures

- **Workload manifest** (C4, `harness/`): ordered tool-call list + load-bearing Bash content (carries a
  valid registry feature-ID token) + single stable CC session identity + expected observe count (drives the
  durability barrier). One source of truth replayed by both legs (ADR-001; OQ2).
- **`MetricVector`** (`crates/unimatrix-store/src/metrics.rs:102`) — the comparison surface, read from the
  JSON text of `context_cycle_review` via `parse_tool_result` (comparator operates on the parsed dict, not
  the Rust struct):
  - `computed_at: u64` — wall-clock; **in the D-5 exclusion set**.
  - `universal: UniversalMetrics` — 21 typed fields (`total_tool_calls`, `total_duration_secs`,
    `session_count`, `search_miss_rate`, `edit_bloat_total_kb`, `edit_bloat_ratio`,
    `permission_friction_events`, `bash_for_search_count`, `cold_restart_events`,
    `coordinator_respawn_count`, `parallel_call_rate`, `context_load_before_first_write_kb`,
    `total_context_loaded_kb`, `post_completion_work_pct`, `follow_up_issues_created`,
    `knowledge_entries_stored`, `sleep_workaround_count`, `agent_hotspot_count`, `friction_hotspot_count`,
    `session_hotspot_count`, `scope_hotspot_count`).
  - `phases: BTreeMap<String, PhaseMetrics>` — `PhaseMetrics = { duration_secs: u64, tool_call_count: u64 }`.
  - `domain_metrics: HashMap<String, f64>`.
- **D-5 exclusion set (enumerated, closed — ADR-003 / #5293):** the 3 wall-clock fields
  `MetricVector.computed_at`, `UniversalMetrics.total_duration_secs`, `PhaseMetrics.duration_secs` (per
  phase). Every other field is classified `deterministic` and compared exactly; the set is named as a literal
  in the comparator. **This 3-field set is an UNPROVEN ASSUMPTION** — its completeness is unverified until the
  first-live-run validation gate (see DELIVERY GATE) examines all 18 non-excluded `UniversalMetrics` fields.
  **An unexpected non-equal field OUTSIDE this set is a real failure dispositioned by product/human (defect
  vs. signed amendment), never silently widened** (R-01/R-02 / NFR-8).

## Key Function Signatures (integration surface — exact, do not invent)

Sourced from the live `infra-001` tree (ARCHITECTURE §Integration Surface).

- `UnimatrixClient(binary_path, project_dir=None, timeout=..., extra_env=None)` — spawns
  `[binary, "--project-dir", dir, "serve", "--stdio"]` (`harness/client.py:76`).
- `context_cycle(cycle_type, topic, *, keywords=None, phase=None, outcome=None, next_phase=None, goal=None, agent_id=None, format=None, timeout=None) -> MCPResponse` (`client.py:679`).
- `context_cycle_review(feature_cycle, *, agent_id=None, format=None, force=None, auto_close=None, timeout=None) -> MCPResponse` (`client.py:658`).
- `UnimatrixUdsClient(socket_path, timeout=...)` — `connect()`/`disconnect()`, `context_cycle()`, `context_cycle_review()` (`harness/uds_client.py:85`).
- `UnimatrixHookClient(socket_path, timeout=...)` — `post_tool_use(session_id, tool, response_size, response_snippet, ...)`, `pre_tool_use(...)`, `session_start(...)`, `session_stop(...)`, `ping()` (`harness/hook_client.py:108`).
- `parse_tool_result(response) -> ToolResult` (`.content`, `.is_error`, `.text`, `.parsed`) (`harness/assertions.py`).
- `run_smoke_gate IMAGE SMOKE_CMD...` — returns 0 iff rc==0 AND run-marker captured (`release-gate-lib.sh:44`). Exit truth table: `0`=passed, `3`=skipped/Docker-absent (HARD fail), `4`=image unacquirable, `1`=shipped-image-path broken, `*`=unexpected. Run-marker: `grep -qxE '\[[a-z0-9-]+-smoke\] ALL GATES PASSED.*'`.
- Bridge: `node mcp-bridge.js <projectHash>` → `buildSession(projectHash)` (`lib/hook-client/mcp-bridge.js:86`).
- `computeFingerprint(derBuffer) -> "sha256:"+hex`; `verifyPeerFingerprint(socket, pinnedFp) -> null | Error` (`lib/hook-client/cert-pin.js:26,67`).
- `credstore.read(projectHash) -> {schema_version:1, mcp_url, observe_url, token, fingerprint, timeouts?}` (mode 0600, `lib/hook-client/credstore.js:48,77`).
- `decodeBundle(raw) -> {v:2, mcp_url, observe_url, token, fp}` (`lib/hook-client/bundle.js:67`); `init --bundle <blob> --project-dir <dir>` writes credstore + token-free `.mcp.json` (`lib/init.js`).
- **FORBIDDEN in this test's path (seed sites):** `_seed_observation_sql_lifecycle(db_path, feature_ids, ...)` (`suites/test_lifecycle.py:1253`), `_seed_attributed_observations_832(...)`, Rust `make_stamped_event(..., topic_signal)` (`uds/listener/tests/stamp_read.rs:28`). No seed site may be reachable from this test (AC-03).

---

## Constraints

- **Cumulative — extend `infra-001`, never fork.** Every net-new helper names the existing asset it extends
  (SR-04). C4 (`parity_workload.py` + comparator) is the ONLY substantial net-new module; any net-new
  server-spawn / cert-pin / credstore / bundle path is a fork smell to flag (AC-07 / NFR-2).
- **Zero production-code diff.** `git diff` touches only `product/test/infra-001/**`, the release-gate CI
  lane, and nan-021 docs — no `crates/**` or `lib/**` runtime changes (AC-06 / NFR-1).
- **Cloud MCP is bundle-only over a pinned self-signed leaf (vnc-038/vnc-039).** Register the slug pre-serve,
  speak HTTPS with a leaf-fingerprint pin, use the bridge — never plain HTTP `/observe`, never unpinned. The
  bearer flushes only after `verifyPeerFingerprint` matches (entries #5098, #4970).
- **rmcp forces SSE (#5129).** The MCP path sends `Accept: application/json, text/event-stream` and parses
  `text/event-stream` — no JSON-only shortcut. AC-02 asserts the bridge CARRIED the traffic (`Mcp-Session-Id`
  capture/replay + SSE parsed), not merely a 200/204.
- **Stable session/attribution identity** (#832 root cause). ONE stable CC session identity threaded through
  declaration + all observes on each leg, and the SAME value on both legs (single workload driver).
- **Determinism for parity (D-5).** Identical workload on both transports; comparator excludes only the
  enumerated wall-clock fields; the set is named so the gate is non-flaky.
- **Symmetric durability barrier (FR-10 / ADR-006).** A bounded deadline-poll (cap ~10s, sleep ~1, DIR
  granularity incl. `-wal`, never `unimatrix.db` alone) gates BOTH `context_cycle_review` calls identically
  before the non-empty / parity assertions; timeout = HARD fail, never an empty compare.
- **Idle-window minimization + shipped self-heal (NFR-7 / ADR-002).** Spawn the bridge LAST and drive
  IMMEDIATELY (no wait between session-id capture and first call). Rely on the shipped single-flight
  self-heal (#5280/#830) for a mid-cycle eviction; do NOT re-implement reconnection. A heal-exhausting 404
  hard-fails with captured bridge stderr.
- **Capture-first child stderr (#5266/#5267).** Every child (`mcp-bridge.js`, `init`, container) writes
  stderr to a `$SANDBOX` file, tail-dumped on failure only — never `2>/dev/null` on a token-free child. The
  ONE exception: `emit_bundle` stays suppressed (its blob carries the bearer).
- **CI home is a Docker-capable lane, NOT `pull_request`** (D-3). Release workflow `workflow_dispatch`/tag.
- **Workspace rules for helper code:** ≤500 lines/file, no stubs / `todo!()` / `unimplemented!()`, no
  `.unwrap()` in non-test Rust, `tracing` for logs, zero new JS runtime deps.

## Dependencies

- **infra-001** (`product/test/infra-001/`): Docker smoke (`docker-http-posture-smoke.sh`),
  `release-gate-lib.sh` (`resolve_image`, `run_smoke_gate`); Python harness
  (`client.py`, `uds_client.py`, `hook_client.py`, `conftest.py`, `assertions.py`, `generators.py`);
  fixtures (`server`, `shared_server`, `fast_tick_server`, `populated_server`, `admin_server`).
- **nan-019** gate contract — verify-by-name exit-code discriminator + anchored run-marker, reused as-is
  (#5192, #5180, #5183).
- **JS edge** — `lib/hook-client/{mcp-bridge.js, cert-pin.js, credstore.js, bundle.js}`, `lib/init.js`.
- **Exercised as-is (NOT modified):** vnc-038 (TLS provisioning / leaf-fp pin), vnc-039 (stdio→HTTPS bridge),
  vnc-034 (slug routing / cert-fp format #4948), crt-055 (attribution chain), shipped keep_alive
  single-flight self-heal (#5280/#830). Behaviors #5265 (fire-and-forget WAL not synced before 204), #5280
  (idle-eviction self-heal). Patterns #5285, #5129, #5098, #4970.
- **External:** Docker Engine + Compose (verified Engine 29.5.2 / Compose v2.40.3 in dev; CI runners
  `ubuntu-22.04`/`-arm` ship Docker).

---

## NOT in Scope

- NO production code changes / no new server behavior. A revealed cloud-path defect → a separate bugfix; this
  fixture *catches*, does not fix.
- NOT forking scaffolding — extends `infra-001`, no isolated new harness.
- NOT a Claude-Code-driven integration — the harness is the MCP client / hook source.
- NOT broadening C0's parity surface beyond cycle/observe/review — retrieval (`context_search`/`get`) over
  the bridge is vnc-039 AC-03's job; this is the behavioral-signal + analytics half.
- NOT wiring into the JS-only `ci.yml` `pull_request` matrix — home is the release-gate Docker lane
  (promotable later).
- NOT changing the bundle schema, bridge, TLS provisioning, slug routing, or the attribution chain.
- NOT a soak/load test — SL1 (#703) and #818/#819 regression coverage are *enabled substrates*, not
  deliverables here.
- This fixture does NOT flip C0 to `proven` — it ADVANCES C0 as the named evidence artifact; the flip is
  held by #837.

---

## Alignment Status

Vision alignment: **PASS** (5 PASS, 1 WARN, 0 VARIANCE, 0 FAIL — ALIGNMENT-REPORT.md, reviewed 2026-06-24).
Directly advances goal #4946 (personal-cloud) marquee promise C0; pure measurement, builds no
future-milestone capability; the "ADVANCES vs proves" boundary with #837 is named identically across all
artifacts (no overstatement). The parity surface is held to observe → topic_signal → cycle_review (retrieval
excluded — vnc-039's job). Zero production-code diff asserted with concrete `git diff` verification.

**WARN (accepted; NO human approval required, NO blocking variance):** FR-10 / ADR-006 (symmetric durability
barrier) and NFR-7 / ADR-002 (idle-window minimization + shipped self-heal reliance) are net-new
fixture-orchestration mechanisms beyond the literal SCOPE D-1..D-6. Both are pure test orchestration that
preserve NFR-1 (zero production diff) and NFR-2 (no fork / no re-authoring shipped behavior), traceably
derived from RISK-TEST R-06 / R-05 (#5265 WAL, #5280/#830 eviction). They make AC-04 a non-vacuous, non-flaky
parity proof rather than expanding scope. Recommendation per the vision guardian: ACCEPT; optionally note in
SCOPE's Proposed Approach for provenance (captured here for downstream awareness).
