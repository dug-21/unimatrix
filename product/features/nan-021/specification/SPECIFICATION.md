# SPECIFICATION — nan-021

**Feature:** nan-021 — HTTPS-Bridge Integration Fixture: Drive Cycles End-to-End Over the Cloud Transport to Prove Parity
**Issue:** #836
**Type:** Test-infrastructure (CI / Nanoprobes). CUMULATIVE extension to `infra-001` (`product/test/infra-001/`). **Zero production code changes.**
**Source:** `product/features/nan-021/SCOPE.md` (Resolved Decisions D-1..D-6 and AC-01..AC-07 are LOCKED). Risk: `product/features/nan-021/SCOPE-RISK-ASSESSMENT.md` (SR-01..SR-06).

---

## Objective

Stand up the **real cloud path** in the `infra-001` harness — HTTPS / multi-project server, self-signed TLS with leaf-fingerprint pinning, the vnc-039 stdio→HTTPS MCP bridge, live hook `/observe` POSTs, and slug routing — and drive a full `context_cycle(start) → tool calls (including a real `Bash` command) → context_cycle(stop)` over the bridge so the `topic_signal` attribution column is **derived** (never seeded). Assert that every driven observation attributes to `topic_signal == feature` and that `context_cycle_review` over HTTPS returns the **same non-empty `MetricVector`** as a local-UDS run of the identical workload (live-vs-live). This is the named evidence artifact that **ADVANCES** capability C0 (#5191); it does not flip C0 to `proven` (held by #837).

---

## Domain Models / Ubiquitous Language

- **MetricVector** (`unimatrix-store/src/metrics.rs`): the comparable surface returned inside `RetrospectiveReport.metrics` (`unimatrix-observe/src/types.rs`). Three components:
  - `universal: UniversalMetrics` — 21 typed `i64`/ratio fields (`total_tool_calls`, `session_count`, per-phase counts, hotspots, knowledge-reuse, etc.).
  - `phases: BTreeMap<String, PhaseMetrics>` — each `PhaseMetrics` = `{duration_secs, tool_call_count}`.
  - `domain_metrics` — domain-keyed aggregates.
  - `computed_at` — wall-clock stamp (non-deterministic; in the D-5 exclusion set).
  Aggregates derive from **content-opaque durable streams** (`cycle_events`, `SessionRecord.outcome`, `query_log ∪ injection_log`; `cycle_aggregates.rs`), never the transcript — therefore transport-agnostic by construction.
- **topic_signal derivation** (the chain under test): `extract_topic_signal` (`unimatrix-observe/src/attribution.rs`, scans observed content — `product/features/<id>/` paths, `feature/<id>` git checkout, free-text feature-ID tokens) feeding `enrich_topic_signal_with_source` (`uds/listener.rs`, priority chain: **declared registry feature → extracted valid feature-ID → registry-fill → vote → unattributed**). `topic_signal == feature` holds when a declared cycle is active.
- **"Seeded" vs "derived"**:
  - *Seeded* = the `topic_signal` column is injected directly, bypassing the derivation chain. Two forbidden sites: Python `_seed_observation_sql_lifecycle` (`suites/test_lifecycle.py:1253`, SQL rows with chosen `feature_ids`) and Rust `make_stamped_event(..., topic_signal: Option<String>)` (`uds/listener/tests/stamp_read.rs:28`, struct injection).
  - *Derived* = `topic_signal` is produced solely by the driven cycle's observed content crossing the wire through the real chain.
- **D-5 exclusion set**: the enumerated non-deterministic fields excluded from field-for-field equality — `computed_at` (wall-clock stamp) plus any duration-derived field with sub-second wall-clock jitter, **at minimum** `phases[*].duration_secs` and any `UniversalMetrics` duration/latency field the design enumerates. The set is named explicitly in the comparator; an unexpected non-equal field outside the set is a real failure, not a tolerance to widen.
- **Live-vs-live parity (D-6)**: the HTTPS run and the local-UDS run execute the *identical workload* **in the same test execution** (two transports, one workload), and their resulting `MetricVector`s are compared field-for-field modulo D-5. NOT a captured golden vector.
- **The bridge (vnc-039)**: `lib/hook-client/mcp-bridge.js`, spawned `node mcp-bridge.js <projectHash>`; reads credential from the credstore (`lib/hook-client/credstore.js`, `~/.unimatrix/<projectHash>/remote.json`), opens a pinned HTTPS session, captures/replays `Mcp-Session-Id`, parses SSE (rmcp forces `text/event-stream`), flushes bearer only after the pin matches.
- **Cloud path**: HTTPS / multi-project server (`serve` HTTP-on with a registered `[[projects]]` slug — NOT `serve --stdio`), self-signed leaf TLS (`http/cert_provisioner.rs`), leaf-fingerprint pin (`cert-pin.js`, `fp = "sha256:"+hex(sha256(leaf DER))`), bundle → `init --bundle` (credstore + token-free `.mcp.json`), slug routing (`/v1/{slug}/...`, `route_observe`).

---

## Functional Requirements

Each is testable; verification methods appear in Acceptance Criteria.

- **FR-1 — Stand up the cloud path (cumulative).** The fixture boots the shipped HTTPS image HTTP-on, registers a `[[projects]]` slug pre-serve, restarts, reads the leaf cert + bearer off the data volume (busybox sidecar), emits a bundle, and runs `init --bundle` into a hermetic sandbox so the credstore + token-free `.mcp.json` bridge entry exist. It binds a real TCP/TLS listener, terminates self-signed TLS with **leaf-fingerprint pinning**, and routes via `/v1/{slug}/...`. It EXTENDS `docker-http-posture-smoke.sh` (NOT `serve --stdio`, NOT a forked spawn path). [D-1]

- **FR-2 — Drive a full cycle through the bridge.** The fixture spawns `mcp-bridge.js <projectHash>` and speaks **stdio JSON-RPC to the bridge as the local MCP server**, driving `context_cycle(start) → tool calls (including a real `Bash` command) → context_cycle(stop)`. MCP traffic flows through the bridge over **pinned HTTPS** (SSE-framed); hook observations flow over **pinned HTTPS `POST /v1/{slug}/observe`**. The bridge must NOT be bypassed by POSTing `mcp_url` directly. [D-2]

- **FR-3 — Load-bearing real `Bash` tool call.** The cycle issues a real `Bash` tool call whose observed content carries an **explicit, load-bearing feature-ID token** (parseable by `extract_topic_signal`). This is the canonical content the attribution scanner turns into a derived `topic_signal`, and the #832 BA-2 fragment-demotion guard.

- **FR-4 — Stable session/attribution identity.** The fixture presents a **single stable CC session identity** across the cycle-declaration hook spawn and all per-tool observe spawns, so the cycle-join holds (the #832 root cause was divergent session ids). The HTTPS and UDS runs use the same declared session identity.

- **FR-5 — Run the identical workload over local UDS.** Using the existing `UnimatrixUdsClient` (MCP/UDS) + `UnimatrixHookClient` (UDS observe) against a local-UDS daemon, the fixture runs the **byte-identical workload** (same tool calls, same order, same declared session identity) in the same test execution to produce the parity baseline. ONE workload driver feeds both transports; no two parallel scripts. [D-6, SR-05]

- **FR-6 — Derived-attribution assertion.** Assert every observation produced by the driven cycle has `topic_signal == feature`, derived by the real chain. Assert (statically/by construction) that **no `topic_signal` is seeded** anywhere in this test's setup or assertion path — neither `_seed_observation_sql_lifecycle` (SQL) nor `make_stamped_event` (struct). A near-miss yielding `unattributed` is a hard failure, not a pass. [SR-06]

- **FR-7 — MetricVector parity assertion.** Call `context_cycle_review(feature)` over the HTTPS path and over the UDS baseline. Assert both return a **non-empty** `MetricVector` (`total_tool_calls > 0`, `session_count > 0`, `phases` populated) and that they are **equal field-for-field** across all `UniversalMetrics` fields, the `phases` BTreeMap key set + `tool_call_count`, and `domain_metrics` — **modulo the named D-5 exclusion set**. The comparator enumerates the exclusion set in the test. [D-5, D-6]

- **FR-8 — False-green gate contract.** The fixture is wired as a standing gate in the release-gate Docker lane via `workflow_dispatch`/tag (mirroring nan-019), reusing `release-gate-lib.sh` (`resolve_image`, `run_smoke_gate`). A green run provably means the fixture ran end-to-end over the live bridge: Docker-absent / skip is a **hard failure** by distinct exit code (exit-code discriminator), and a positive **anchored terminal run-marker** (`grep -qx`) is asserted. No early-exit-0 or environment skip can masquerade as parity-proven. Image acquisition reuses nan-019's conditional `docker pull || inspect || exit-<code>` path verbatim. [D-3, SR-03]

- **FR-9 — Bridge-carried-the-traffic assertion.** AC-02 verification asserts the bridge actually carried the MCP traffic — `Mcp-Session-Id` captured/replayed and SSE (`text/event-stream`) parsed — not merely that a 200 returned. [SR-02]

- **FR-10 — Symmetric durability barrier before review.** Before either `context_cycle_review` call runs, a bounded **deadline-poll** (cap ~10s, sleep ~1s — NOT a flat sleep, NOT an immediate single read) blocks until the driven observations are durable and visible to the aggregation read, asserting the expected observe count is present before review. The barrier is applied **identically on BOTH the HTTPS and UDS legs** — the symmetry is load-bearing, since `/observe` is fire-and-forget WAL (server Acks 204 then writes async to `-wal`, not synced; #5265) on both transports and an asymmetric barrier itself induces parity divergence. Durability is sampled at the per-slug store DIR granularity (includes `-wal`), never just `unimatrix.db`. On timeout the test hard-fails ("observes not durable") — it never compares an empty/short vector. AC-04's non-empty + parity assertion is valid ONLY after the barrier passes on both legs. This is fixture orchestration only — no production code change. [R-06, D-5/D-6, SR-01; preserves NFR-1]

---

## Non-Functional Requirements

- **NFR-1 — Zero production-code diff.** The change set is test infrastructure only (harness / fixtures / scripts / CI lane). The shipped image, routes, bridge, TLS provisioning, slug routing, and attribution chain are exercised **as-is**. Verification: `git diff` touches only `product/test/infra-001/**`, the release-gate CI lane, and nan-021 docs — no `crates/**` or `lib/**` runtime changes. [AC-06]

- **NFR-2 — Cumulative extension, no fork.** No parallel scaffolding duplicating server spawn, cert-pinning, credstore, or bundle handling. Each net-new helper maps to the existing asset it extends (the Docker smoke + `release-gate-lib.sh`, OR `UnimatrixUdsClient` / `UnimatrixHookClient`, OR the JS `mcp-bridge.js`/`cert-pin.js`/`credstore.js`/`bundle.js`). Any net-new spawn/cert/credstore/bundle path is a fork smell to flag. [AC-07, SR-04]

- **NFR-3 — CI determinism / non-flakiness.** Readiness gates, NOT fixed sleeps, between every link of the chain (cert present, listener bound, `Mcp-Session-Id` captured). Every child process (`mcp-bridge.js`, `init`, container) has its stderr captured to a sandbox file, dumped tail-bounded on failure — never swallowed (entry #5266). The parity comparator is exact modulo the *complete* enumerated D-5 exclusion set; any field carrying hidden wall-clock/latency jitter not in the set is a real defect to enumerate, not a tolerance to widen. [SR-01, D-5]

- **NFR-4 — Dual-transport workload identity.** Live-vs-live parity holds only if both transports run byte-identical tool calls in identical order with identical declared session identity. The workload is factored as ONE driver consumed by both transports. [SR-05]

- **NFR-5 — Pinned-HTTPS / bundle-only transport fidelity.** The fixture registers a slug pre-serve, speaks HTTPS with a leaf-fingerprint pin, and uses the bridge — never plain HTTP `/observe`, never an unpinned connection. The bearer flushes only after the pin matches. The MCP path sends `Accept: application/json, text/event-stream` and parses SSE (rmcp forces it, entry #5129) — no JSON-only shortcut. [Constraints; entries #5098, #5129]

- **NFR-6 — Workspace rules for helper code.** Any Rust/JS helper obeys workspace rules: ≤500 lines/file, no stubs / `todo!()` / `unimplemented!()`, no `.unwrap()` in non-test Rust, `tracing` for logs, zero new JS runtime deps.

- **NFR-8 — Exclusion-set amendment disposition authority (process).** Any non-wall-clock `UniversalMetrics`/`phases` field that **diverges on the first live dual-transport run** (AC-04 first-live-run gate) is escalated to a **HUMAN / PRODUCT call** and dispositioned as exactly one of: **(a) a real parity defect** → file a GitHub bug (the C0 done_when failure this fixture exists to catch); or **(b) a transport-inherent field** → added to the D-5 exclusion set **ONLY with product sign-off and a recorded rationale** (named in the comparator alongside the field). The implementer/tester **MUST NOT silently widen the exclusion set** — that is the R-01/R-02 failure mode (a reactively-widened set that hides real divergence). Likely candidates flagged by name as the at-risk **session-lifecycle-derived** fields requiring this disposition: `cold_restart_events`, `coordinator_respawn_count`, `context_load_before_first_write_kb`, `total_context_loaded_kb`, `permission_friction_events`. This is a process/disposition requirement; it changes no production code. [R-01, R-02, D-5; preserves NFR-1]

- **NFR-7 — Idle-window minimization + shipped self-heal.** The fixture minimizes the idle window between `Mcp-Session-Id` capture and the first tool call: readiness gate 8 (session-id captured) is event-driven, then the first call drives immediately with no interposed fixed wait, so the captured session is not evicted by rmcp keep_alive. The fixture relies on the **shipped single-flight keep_alive self-heal** (single-flight re-init on `SESSION_NOT_FOUND` -32099; #5280/#830) rather than re-implementing reconnection — it must not depend on eviction never happening, and must not contain its own retry/reconnect logic (that would be re-authoring shipped behavior, violating NFR-2). A stale-session 404 mid-cycle is treated as a **fixture defect**, not a product bug; if the shipped self-heal exhausts, the cycle hard-fails with captured bridge stderr — never a silently dropped observe surfacing as a short `MetricVector`. [R-05, SR-01; preserves NFR-1/NFR-2]

---

## Acceptance Criteria

Traceable to SCOPE AC-01..AC-07 and D-1..D-6. Each lists a concrete verification method.

- **AC-01 — Cloud path stood up cumulatively.** [SCOPE AC-01 · D-1 · FR-1]
  *Verify:* The test brings up HTTPS / multi-project server (`serve` HTTP-on with a registered `[[projects]]` slug — assert NOT `serve --stdio`), self-signed TLS terminated with leaf-fingerprint pinning, the vnc-039 bridge, live hook → `POST /v1/{slug}/observe`, and `/v1/{slug}/...` routing. Inspect the change set: the path extends `docker-http-posture-smoke.sh` + the Python harness clients; no third server-spawn/cert/bundle path exists.

- **AC-02 — Full cycle through the bridge over pinned HTTPS.** [SCOPE AC-02 · D-2 · FR-2/FR-9/NFR-7 · R-05]
  *Verify:* `context_cycle(start) → tool calls incl. real `Bash` → context_cycle(stop)` runs with MCP traffic flowing through `mcp-bridge.js` (spawned, driven over stdio JSON-RPC) over pinned HTTPS, and hook observations over pinned HTTPS `/observe`. Assert NOT UDS, NOT stdio-direct, and NOT a direct `mcp_url` POST. Assert the bridge carried it: `Mcp-Session-Id` captured/replayed and SSE parsed (not just a 200). The first tool call follows session-id capture with no interposed fixed wait (NFR-7 idle-window minimization); a mid-cycle eviction is survived by the shipped self-heal (#5280/#830) or hard-fails with captured bridge stderr — never silently truncates the observe stream.

- **AC-03 — Derived topic_signal == feature, no seed.** [SCOPE AC-03 · FR-3/FR-6 · SR-06]
  *Verify:* Every observation produced by the driven cycle has `topic_signal == feature`, derived by `extract_topic_signal → enrich_topic_signal_with_source` from observed content over the wire. Static check: the test's setup + assertion path invokes neither `_seed_observation_sql_lifecycle` (SQL) nor `make_stamped_event` (struct) — no `topic_signal` injection anywhere. A derived `unattributed` (near-miss) hard-fails.

- **AC-04 — Live-vs-live MetricVector parity.** [SCOPE AC-04 · D-5/D-6 · FR-5/FR-7/FR-10 · R-06]
  *Verify:* The **symmetric durability barrier (FR-10) passes on BOTH legs first** — the expected observe count is provably durable (per-slug DIR granularity, incl. `-wal`) on the HTTPS leg and the UDS leg before either review call runs; a barrier timeout hard-fails "observes not durable" rather than comparing an empty vector. Only then: `context_cycle_review(feature)` over HTTPS returns a non-empty `MetricVector` (`total_tool_calls > 0`, `session_count > 0`, `phases` populated) equal **field-for-field — modulo the documented D-5 exclusion set** — to the `MetricVector` for the **identical workload driven over local UDS in the same test execution** (live-vs-live, NOT a captured golden). Equality spans all `UniversalMetrics` fields, the `phases` key set + `tool_call_count`, and `domain_metrics`. The exclusion set is named in the comparator.
  **First-live-run validation gate:** the parity assertion is NOT trusted until the first live dual-transport run is examined **field-by-field across all 18 non-excluded `UniversalMetrics` fields** (plus the `phases` key set / `tool_call_count`) and confirmed equal. The D-5 exclusion-set completeness — the **3 wall-clock fields** (`computed_at`, `phases[*].duration_secs`, and the enumerated `UniversalMetrics` duration field) — is a **load-bearing ASSUMPTION pending that first run**, flagged as such. Any non-wall-clock field that diverges on the first run is dispositioned per **NFR-8** (never silently excluded).

- **AC-05 — Standing false-green-proof gate (release-gate Docker lane).** [SCOPE AC-05 · D-3 · FR-8 · SR-03]
  *Verify:* Wired as a standing gate in the release-gate Docker lane via `workflow_dispatch`/tag (NOT per-PR), mirroring nan-019 with `release-gate-lib.sh`. A green run provably ran end-to-end: skip-when-Docker-absent hard-fails by distinct exit code (exit-code discriminator), and a positive **anchored whole-line** terminal run-marker is asserted via `grep -qx`. No early-exit-0 / environment skip can pass as parity-proven. Image acquisition reuses nan-019's `pull || inspect || exit-<code>` verbatim.

- **AC-06 — Zero production-code change.** [SCOPE AC-06 · NFR-1]
  *Verify:* The diff is test infrastructure only (harness / fixtures / scripts / CI lane). Image, routes, bridge, TLS provisioning, attribution chain exercised as-is. `git diff` shows no `crates/**` or `lib/**` runtime modification.

- **AC-07 — Extends infra-001, no parallel scaffolding.** [SCOPE AC-07 · NFR-2 · SR-04]
  *Verify:* New helpers map to existing assets (Docker smoke + gate lib AND/OR Python harness clients/fixtures). No net-new code duplicating server spawn, cert-pinning, credstore, or bundle handling already present.

---

## User / Agent Workflows

The actor is the **test harness itself** acting as MCP client + hook source over the real transports (NOT a live Claude Code session).

1. **Setup (HTTPS):** boot image HTTP-on → `project register <slug>` → restart → busybox read of leaf cert + bearer off the data volume → emit bundle → `init --bundle` into hermetic sandbox (credstore + token-free `.mcp.json`). Readiness-gated at each step.
2. **Drive cycle over bridge:** spawn `node mcp-bridge.js <projectHash>` → stdio JSON-RPC → `context_cycle(start)` → real `Bash` call (load-bearing feature-ID token) + other tool calls firing real PostToolUse hooks → pinned `POST /v1/{slug}/observe` → `context_cycle(stop)`. Stable session identity throughout.
3. **Drive identical workload over UDS:** same ONE workload driver against `UnimatrixUdsClient` + `UnimatrixHookClient` on a local-UDS daemon.
4. **Durability barrier + assert:** apply the symmetric durability barrier (FR-10) on both legs — deadline-poll until the driven observes are durable (DIR granularity incl. `-wal`) before either review — then assert (a) derived `topic_signal == feature`, no seed; (b) `context_cycle_review(feature)` HTTPS `MetricVector` == UDS `MetricVector` field-for-field modulo D-5, both non-empty.
5. **Gate:** emit the anchored terminal run-marker; the release-gate lane runs `run_smoke_gate` with the exit-code + verify-by-name contract.

---

## Constraints

- **Cumulative — extend `infra-001`, never fork.** Docker smoke + `release-gate-lib.sh`; Python harness clients/fixtures; JS cert-pin/credstore/bridge machinery.
- **No production code changes; no new server behavior** — C0's `done_when` is *measurement*, not new function.
- **Cloud MCP is bundle-only over a pinned self-signed leaf (vnc-038/vnc-039).** Register slug pre-serve, leaf-fingerprint pin, use the bridge — never plain HTTP `/observe`, never unpinned (entries #5098, #5129). Bearer flushes only after pin match.
- **rmcp forces SSE** — bridge/MCP path sends `Accept: application/json, text/event-stream` and parses `text/event-stream` (entry #5129).
- **Stable session/attribution identity** across declaration + observes (#832 root cause).
- **CI home is a Docker-capable lane, NOT `pull_request`** — GH `ci.yml` is JS-client-only; Rust/container validation lives in the release workflow + protocol gates.
- **Determinism for parity (D-5)** — identical workload both transports; comparator excludes only the enumerated non-deterministic fields; the set is named so the gate is non-flaky.
- **Workspace rules** apply to helper code (≤500 lines/file, no stubs, no non-test `.unwrap()`, `tracing` logs, zero new JS runtime deps).

---

## Dependencies

- **infra-001** (`product/test/infra-001/`): Docker smoke (`docker-http-posture-smoke.sh`), `release-gate-lib.sh` (`resolve_image`, `run_smoke_gate`); Python harness (`harness/client.py`, `conftest.py`, `assertions.py`, `generators.py`, `UnimatrixUdsClient`, `UnimatrixHookClient`); fixtures (`server`, `shared_server`, `fast_tick_server`, `populated_server`, `admin_server`).
- **nan-019** gate contract — verify-by-name exit-code discriminator + anchored run-marker (`release-gate-lib.sh`), reused as-is (entries #5192, #5180, #5183).
- **JS edge** — `lib/hook-client/{mcp-bridge.js, cert-pin.js, credstore.js, bundle.js}`, `lib/init.js`.
- **Exercised as-is (not modified):** vnc-038 (TLS provisioning / leaf-fp pin), vnc-039 (stdio→HTTPS bridge), vnc-034 (slug routing / `resolve_store` seam), crt-055 (attribution chain), the shipped keep_alive single-flight self-heal (#5280/#830). Capability C0 (#5191); patterns #5285, #5129, #5098; behaviors #5265 (fire-and-forget WAL not synced before 204), #5280 (idle-eviction self-heal).
- **Risk strategy:** `product/features/nan-021/RISK-TEST-STRATEGY.md` (R-05 → NFR-7, R-06 → FR-10; architect's ADR-006 pending in parallel).
- **External:** Docker Engine + Compose (verified Engine 29.5.2 / Compose v2.40.3 in dev; CI runners `ubuntu-22.04`/`-arm` ship Docker).

---

## NOT in Scope

- NO production code changes / no new server behavior. A revealed cloud-path defect → separate bugfix; this fixture *catches*, does not fix.
- NOT forking scaffolding — extends `infra-001`, no isolated new harness.
- NOT a Claude-Code-driven integration — the harness is the MCP client / hook source.
- NOT broadening C0's parity surface beyond cycle/observe/review — retrieval (`context_search`/`get`) over the bridge is vnc-039 AC-03's job; this is the behavioral-signal + analytics half.
- NOT wiring into the JS-only `ci.yml` `pull_request` matrix — home is the release-gate Docker lane via `workflow_dispatch`/tag (promotable later).
- NOT changing the bundle schema, bridge, TLS provisioning, slug routing, or the attribution chain.
- NOT a soak/load test — SL1 (#703) and #818/#819 regression coverage are *enabled substrates*, not deliverables here.
- This fixture does NOT flip C0 to `proven` — it ADVANCES C0 as the named evidence artifact; the flip is held by #837.

---

## Open Questions (for architect / tester)

- **OQ-1 (architect):** Where does the `MetricVector` comparator live — in the Python harness (it owns the comparison per D-1) consuming both transports' `RetrospectiveReport`, and how does it obtain the HTTPS-side review (Python drives UDS; the container drives HTTPS)? The harness must read the HTTPS `MetricVector` back out (review call over the bridge vs. store inspection). Design must pin the seam without forking a new client.
- **OQ-2 (architect/tester):** The **complete** D-5 exclusion set — beyond `computed_at` and `phases[*].duration_secs`, enumerate every `UniversalMetrics` duration/latency/ratio field that can carry sub-second wall-clock jitter. An incomplete set flakes; an over-broad set hides real divergence (NFR-3 load-bearing assumption). Completeness is unconfirmed until the AC-04 **first-live-run validation gate** examines all 18 non-excluded fields; first-run divergence of any session-lifecycle-derived candidate (`cold_restart_events`, `coordinator_respawn_count`, `context_load_before_first_write_kb`, `total_context_loaded_kb`, `permission_friction_events`) is dispositioned per **NFR-8** (human/product call — defect vs. justified exclusion), never silently excluded.
- **OQ-3 (architect):** Exact distinct exit codes for the Docker-absent / image-cache-miss discriminators (reuse nan-019's numbering — confirm `exit-4` for absent vs. the pull-miss code) so AC-05 skip-is-failure is unambiguous.
- **OQ-4 (tester):** First-green tax — a release-only gate has no green baseline (#5266/#5267); budget multiple tag rounds to first-green surfacing failures in sequence. Confirm the pre-merge stub-drive coverage for the gate logic mirrors nan-019 so the gate-spine bytes are unit-tested before the live run.

---

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_get — strong hits: #5285 (cloud-transport parity: derive-don't-seed topic_signal, transport-agnostic MetricVector, cumulative extension points), #5192 (nan-019 verify-by-name gate spine reused as-is), #5129 (rmcp-forces-SSE bridge framing), #5191 (C0 capability ADVANCED not flipped), #5098 (register-pre-serve / HTTPS-only / leaf-fp pin precedent).
- Incorporated from RISK-TEST-STRATEGY.md (R-05/R-06): #5265 (fire-and-forget WAL not synced before 204 → symmetric durability barrier, FR-10), #5280/#830 (rmcp keep_alive idle-eviction + single-flight self-heal → idle-window minimization, NFR-7).
- Human design-gate refinement (R-01/R-02): AC-04 first-live-run validation gate (18 non-excluded fields; D-5 completeness as load-bearing assumption pending first run) + NFR-8 exclusion-set amendment disposition authority (human/product call, no silent widening).
- No novel knowledge stored — spec decisions are feature-specific; the generalizable patterns already exist as #5285/#5192/#5129/#5265/#5280.
