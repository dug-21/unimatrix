# nan-021 — HTTPS-Bridge Integration Fixture: Drive Cycles End-to-End Over the Cloud Transport to Prove Parity

> **Test-infrastructure feature (CI/Nanoprobes deliverable).** A CUMULATIVE extension to the
> `infra-001` harness (`product/test/infra-001/`) that stands up the **real cloud path** — HTTPS /
> multi-project server, self-signed TLS with fingerprint pinning, the vnc-039 stdio→HTTPS MCP bridge,
> live hook `/observe` POSTs, project register + slug routing — and drives a full
> `context_cycle(start) → tool calls (incl. a Bash command) → stop` over the bridge. It **ADVANCES**
> capability **C0** (Unimatrix #5191 — "Full intelligence-pipeline fidelity over HTTPS == local") and
> is its **named evidence artifact** — it does NOT, on its own, prove C0 or flip it to `proven`; the
> `proven` flip is held by **#837**. NO production code changes; pure test infrastructure. Tracked by
> GH **#836**.
> Scoped in a uni-zero session 2026-06-24.

## Problem Statement

The personal-cloud marquee promise (**C0**, #5191) is *"for a remote slug, retrieval AND behavioral
signals AND analytics/learning all function at parity with a local-UDS deployment of the same
workload — **measured, not asserted**."* Today it is asserted. No automated test drives the real cloud
transport end-to-end, so every cloud-path behavioral claim is unprovable in-harness — proven only by
manual dogfood or left owed.

The gap is concrete and grounded in two facts:

1. **The Python `infra-001` MCP harness spawns the server in stdio-MCP mode only.** `UnimatrixClient`
   (`product/test/infra-001/harness/client.py:104`) does
   `Popen([binary, "--project-dir", dir, "serve", "--stdio"], stdin/stdout=PIPE)` — a single-project
   stdio daemon. It never binds a TCP/TLS listener, never registers a slug, never runs the bridge,
   never POSTs `/observe`. The companion `project_routing_integration.rs` exercises the `SlugRouter`
   in-process at the tower layer and its own header states it *"cannot reach the `/v1/{slug}/` HTTP
   edge (it spawns single-project stdio)"* and explicitly excludes `/observe`. So **no test in the
   suite drives the live HTTPS transport**.

2. **Because the wire is never driven, cycle-review tests SEED the attribution join instead of
   deriving it.** `_seed_observation_sql_lifecycle()`
   (`product/test/infra-001/suites/test_lifecycle.py:1253`, *"Seed minimal observation data for
   context_cycle_review"*) injects observation rows **directly via SQL** with hand-chosen
   `feature_ids` that become the `topic_signal` column. The real derivation path
   (`extract_topic_signal` → `enrich_topic_signal_with_source`) is bypassed entirely. This is exactly
   what #832's BA-1 had to do: its own commit message states *"Harness is stdio-MCP only; literal
   HTTPS-bridge end-to-end is not exercisable here (drove closest achievable parity at the
   `topic_signal=cycle_id` join)."*

**Who is affected:** every downstream feature that depends on a proven cloud path. **Why now:** C0 is
now a *measurement* feature with no blocking constituent — its prerequisite capabilities C5/C10/C11
are all `proven` (#5191 note); the only remaining gap is C0's own `done_when` (a measured
remote-vs-local parity run), which has no writable proof artifact until this fixture exists.

## Goals

1. **Stand up the real cloud path in `infra-001`, cumulatively.** Extend the existing `infra-001`
   fixtures/helpers so a test can run the server in **HTTPS / multi-project mode** (`serve` with
   `http.enabled` + a registered `[[projects]]` slug, NOT `serve --stdio`), terminate self-signed
   **TLS with leaf-fingerprint pinning**, run the **vnc-039 stdio→HTTPS MCP bridge**
   (`lib/hook-client/mcp-bridge.js`), spawn **live hooks → `/v1/{slug}/observe` POSTs**, and route via
   **`/v1/{slug}/...`**.
2. **Drive a full cycle over the bridge.** Execute `context_cycle(start) → tool calls (including a
   real `Bash` command) → context_cycle(stop)` so observations cross the HTTPS bridge and the
   `topic_signal` attribution column is **derived** (`extract_topic_signal` →
   `enrich_topic_signal_with_source`), never SQL-seeded.
3. **Assert cloud attribution.** Every observation produced by the driven cycle has
   `topic_signal == feature` — derived from real observed content over the wire, with **NO seeded
   `topic_signal`** anywhere in the assertion path.
4. **Assert measured parity.** `context_cycle_review` over the HTTPS path returns the **same non-empty
   metrics** (the `MetricVector` — `UniversalMetrics`'s 21 fields + per-phase `PhaseMetrics`) as a
   **local-UDS** run of the **identical workload**. This is the named evidence artifact that ADVANCES
   C0 (the `proven` flip is held by #837 — see Tracking).
5. **CI-runnable, false-green-proof.** The fixture runs in a Docker-capable lane (the shipped HTTPS
   image), honors the existing skip-is-failure / verify-by-name run-marker contract (nan-019), and a
   green result provably means "the cycle ran over the live bridge and parity held."

## Non-Goals

- **NO production code changes; no new server behavior.** Pure test infrastructure. If the fixture
  reveals a real cloud-path defect, fixing it is a separate bugfix — this feature's job is to *catch*
  it, not fix it. (Matches the nan-019 "wire, don't change behavior" posture.)
- **NOT forking scaffolding.** Test infra is cumulative — this EXTENDS `infra-001`'s existing
  fixtures, helpers, the Docker smoke (`docker-http-posture-smoke.sh`), and the cert-pin/credstore JS
  machinery. It does not create an isolated new harness.
- **NOT a Claude-Code-driven integration.** The cycle is driven by the test harness acting as the MCP
  client / hook source over the real transports — it does not require a live Claude Code session.
- **NOT broadening C0's parity surface beyond the cycle/observe/review pipeline.** Retrieval
  (`context_search`/`get`) over the bridge is already covered by vnc-039's AC-03; this feature is the
  **behavioral-signal + analytics** half (observe → topic_signal → cycle_review). It asserts those,
  not a re-test of every `context_*` tool over HTTPS.
- **NOT wiring this into the JS-only `ci.yml` `pull_request` matrix.** Per the standing rule (GH CI is
  JS-client-only; Rust/container validation lives in the release workflow + protocol gates), the home
  is the release-gate Docker lane via `workflow_dispatch`/tag (mirroring nan-019) — NOT `pull_request`.
  (RESOLVED — see Resolved Decisions D-3; promotable to a PR lane later.)
- **NOT changing the bundle schema, the bridge, TLS provisioning, slug routing, or the attribution
  chain.** All of these (vnc-038/vnc-039/vnc-034/crt-055) are reused as-is; this feature only
  *exercises* them.
- **NOT a soak/load test.** SL1 (#703, declared-session attribution soak) and #818/#819 regression
  coverage are *unblocked substrates* this fixture enables, not in-scope deliverables here.

## Background Research (grounded in code)

### The two "infra-001" surfaces (read this first — they are distinct)
`product/test/infra-001/` contains **two separate test instruments**, and this feature touches the
boundary between them:
- **(a) Python pytest MCP harness** — `harness/{client.py,conftest.py,assertions.py,generators.py}` +
  `suites/`. Spawns `serve --stdio` (subprocess, stdio JSON-RPC), drives all 14 `context_*` tools
  including `context_cycle` and `context_cycle_review` via typed client methods
  (`client.py:679` `context_cycle`, the `context_cycle_review` method). Reusable fixtures: `server`,
  `shared_server`, `fast_tick_server`, `populated_server`, `admin_server`. **This is the harness the
  issue means by "stdio-MCP mode only."** It also already has a **`UnimatrixHookClient`**
  (`harness/hook_client.py`, posts synthetic hook events to the local UDS hook socket) and a
  **`UnimatrixUdsClient`** (MCP over UDS) — these give the **local-UDS parity baseline** for Goal 4.
- **(b) Docker HTTPS smoke** — `Dockerfile`, `docker-compose.yml`,
  `scripts/docker-http-posture-smoke.sh`, `scripts/release-gate-lib.sh`. ALREADY stands up the
  shipped HTTPS image, asserts `UNIMATRIX_HTTP_ENABLED=true`, boots HTTP-on, `project register
  arch-research`, restarts, and does a **cert-pinned POST to
  `https://localhost:18443/v1/<slug>/observe`** with the bearer (read from the data volume via a
  read-only `busybox` sidecar), then asserts the per-slug store grew. **This is the cumulative
  extension point for the live cloud transport.** It currently exercises only a single
  `SessionRegister` observe POST + bundle-attach (Gates 5–7, nan-020) — NOT a full cycle, NOT the MCP
  bridge, NOT behavioral observe (PostToolUse), NOT cycle_review.

### The full cloud path (what the fixture must stand up), grounded
- **HTTPS / multi-project server:** `serve` binds TCP via
  `http/listener.rs:start_http_listener()` when config enables HTTP; per-slug servers built by
  `http_provision.rs:build_project_server()`; router chain `StaticTokenAuth → PathRouter →
  SlugRouter`. HTTP is the image default (`UNIMATRIX_HTTP_ENABLED=true` baked into the Dockerfile,
  #783).
- **Self-signed TLS + pinning:** server generates an idempotent self-signed leaf
  (`http/cert_provisioner.rs:load_or_generate_cert`) at `{data_dir}/tls/{cert.pem,key.pem}`; acceptor
  via `http/tls.rs:build_tls_acceptor`. Client pins the leaf fingerprint
  (`lib/hook-client/cert-pin.js:computeFingerprint/verifyPeerFingerprint`,
  `fp = "sha256:"+hex(sha256(leaf DER))`). The smoke already reads the cert off the volume for
  `curl --cacert`; the harness analogue reads it for the pin (precedent: entry #5098).
- **stdio→HTTPS MCP bridge (vnc-039):** `lib/hook-client/mcp-bridge.js`, invoked
  `node mcp-bridge.js <projectHash>`; reads credential from the out-of-tree credstore
  (`lib/hook-client/credstore.js`, `~/.unimatrix/<projectHash>/remote.json`), opens a pinned HTTPS
  session, captures/replays `Mcp-Session-Id`, parses SSE (rmcp **forces** `text/event-stream`, entry
  #5129), flushes the bearer ONLY after the pin matches.
- **Bundle → init → wiring:** `client_bundle.rs:compose_route_urls` →
  `mcp_url={base}/v1/{slug}`, `observe_url={base}/v1/{slug}/observe`. `init --bundle` (`lib/init.js`)
  writes the credstore + a token-free `.mcp.json` stdio entry. The smoke already does
  emit-bundle → `init --bundle` (Gate 5).
- **Slug routing + observe:** `http/router/seam.rs:parse_project_key` (`^[a-z0-9][a-z0-9-]{0,62}$`);
  `http/router/handlers.rs:route_observe` (`POST /v1/{slug}/observe`).

### topic_signal attribution (Goal 3) — derived, not seeded
- **Hook-side extraction:** `uds/hook.rs:extract_event_topic_signal` (≈373) sets
  `ImplantEvent.topic_signal` per event type via `extract_topic_signal(text)`
  (`unimatrix-observe/src/attribution.rs`), scanning observed content (file paths under
  `product/features/<id>/`, `feature/<id>` git checkout, free-text feature-ID tokens).
- **Server-side enrichment:** `uds/listener.rs:enrich_topic_signal_with_source` (≈196) resolves the
  final column via a priority chain: **declared** registry feature → **extracted** valid feature-ID →
  registry-fill → vote → unattributed. `topic_signal == feature` when a declared cycle is active.
- **What "seeded" means (the gap):** tests today inject the column directly — Python
  `_seed_observation_sql_lifecycle` (SQL rows with chosen `feature_ids`) and the Rust unit fixture
  `make_stamped_event(..., topic_signal: Option<String>)`
  (`uds/listener/tests/stamp_read.rs:28`). nan-021's assertion path must contain **no such injection**
  — the `topic_signal` is produced only by the real cycle crossing the wire.
- **#832 relevance:** #832 fixed the cloud-cycle attribution bug (declaration hook spawn vs per-tool
  observe spawns keyed on divergent CC session ids, breaking the cycle-join) by anchoring on a single
  stable CC session id. Its regression guard (BA-1/BA-2 in `test_lifecycle.py`) had to seed the join
  because the bridge was un-drivable in-harness. nan-021 converts that to a real HTTPS reproduction —
  a permanent regression guard for #832 and the #818/#819 silent-observe family.

### context_cycle_review parity (Goal 4) — the comparable surface
- **Metric struct:** `RetrospectiveReport` (`unimatrix-observe/src/types.rs:381`) carries
  `metrics: MetricVector` (`unimatrix-store/src/metrics.rs:102`): `computed_at`,
  `universal: UniversalMetrics` (21 typed `i64`/ratio fields — `total_tool_calls`, `session_count`,
  per-phase counts, hotspots, knowledge-reuse, etc.), `phases: BTreeMap<String, PhaseMetrics>`
  (`{duration_secs, tool_call_count}`), `domain_metrics`. Aggregates are derived from **content-opaque
  durable streams** (`cycle_events`, `SessionRecord.outcome`, `query_log ∪ injection_log`;
  `cycle_aggregates.rs`), never the transcript — so they are transport-agnostic by construction, which
  is exactly why parity is the right assertion.
- **"Parity" concretely:** run the identical workload twice — once driving observes/cycle over the
  **HTTPS bridge + `/observe`**, once over **local UDS** (the existing `UnimatrixUdsClient` /
  `UnimatrixHookClient` baseline) — and assert the resulting `MetricVector`s are equal field-for-field
  (all 21 `UniversalMetrics`, the `phases` map, `domain_metrics`), and **non-empty** (`total_tool_calls > 0`,
  `session_count > 0`, `phases` populated). "Same non-empty metrics" = both transports produce the same
  real numbers, not a believable `0`.

### Reusable assets (extend, never fork)
- Docker smoke + gate lib (`docker-http-posture-smoke.sh`, `release-gate-lib.sh`): exit-code +
  run-marker contract, busybox volume inspection, cert/token extraction, per-slug store delta.
- Python harness: `UnimatrixClient` (stdio), `UnimatrixUdsClient` (UDS MCP), `UnimatrixHookClient`
  (UDS hook/observe), fixtures, generators, assertions.
- JS edge: `mcp-bridge.js`, `cert-pin.js`, `credstore.js`, `bundle.js`, `init.js`.
- Prior art for "harness must register a slug + speak pinned HTTPS, not plain HTTP `/observe`":
  entry #5098 (the vnc-038 Layer-2 trap — register-pre-serve, HTTPS-only listener, leaf-fp pin).

## Proposed Approach

High-level, for the design phase to refine (substrate/wiring decisions are LOCKED in Resolved
Decisions below; the rest is design-phase detail):

1. **Standing posture is a HYBRID (D-1):** a containerized HTTPS fixture (extend
   `docker-http-posture-smoke.sh` — `docker compose up` of the shipped cloud image, cert-pinned),
   mirroring nan-019's release-gate smoke, stands up the HTTPS path; the Python pytest harness
   (`harness/client.py`) drives the local-UDS baseline run and OWNS the `MetricVector`
   comparison/assertion code. Docker is **verified available** in this dev env (`docker version` →
   Engine 29.5.2, Compose v2.40.3, responsive). The container gives a real shipped-image HTTPS surface,
   cert on a volume, and the existing skip-is-failure gate contract for free.
2. **Stand up the cloud path:** boot the image HTTP-on, `project register <slug>`, restart, read the
   leaf cert + bearer off the volume (busybox sidecar, existing helper). Emit a bundle and
   `init --bundle` into a hermetic sandbox so the credstore + token-free `.mcp.json` bridge entry
   exist (Gate-5 precedent).
3. **Drive a full cycle over the live transports:** harness acts as MCP client *through the bridge*
   (`node mcp-bridge.js <projectHash>` → pinned HTTPS `mcp_url`) — `context_cycle(start)`, a `Bash`
   tool call + other tool calls (firing real PostToolUse hook events → pinned `POST /v1/{slug}/observe`),
   `context_cycle(stop)`. The Bash command is load-bearing: it is the canonical content the attribution
   scanner must turn into a derived `topic_signal` (and the #832 BA-2 fragment-demotion guard).
4. **Run the identical workload over local UDS** (existing `UnimatrixUdsClient` + `UnimatrixHookClient`
   against a `serve --stdio`/UDS daemon) to produce the parity baseline.
5. **Assert:** (a) every driven observation's `topic_signal == feature`, derived, no SQL/struct seed in
   the assertion path; (b) `context_cycle_review(feature)` over HTTPS returns the same non-empty
   `MetricVector` as the UDS baseline, field-for-field.
6. **Wire as a standing, verify-by-name, skip-is-failure gate** in a Docker-capable lane (reuse
   `release-gate-lib.sh:run_smoke_gate`; emit a terminal `ALL GATES PASSED` run-marker).

Rationale: the cumulative-extension constraint and the already-built Docker HTTPS smoke make a
container fixture the lowest-risk way to exercise the *shipped* cloud bytes end-to-end. The
content-opaque, transport-agnostic `MetricVector` makes "same metrics over HTTPS vs UDS" a clean,
deterministic parity assertion that proves C0's `done_when` without inspecting transcripts.

## Acceptance Criteria

- **AC-01:** A CI-runnable test stands up the cloud path on the `infra-001` substrate (CUMULATIVE
  extension, not a fork): HTTPS / multi-project server (`serve` HTTP-on with a registered `[[projects]]`
  slug — NOT `serve --stdio`), self-signed TLS terminated with leaf-**fingerprint pinning**, the
  vnc-039 stdio→HTTPS MCP bridge, live hook spawns → `POST /v1/{slug}/observe`, and `/v1/{slug}/...`
  slug routing.
- **AC-02:** The test drives a full `context_cycle(start) → tool calls including a real `Bash` command →
  context_cycle(stop)` with the MCP traffic flowing **through the vnc-039 stdio→HTTPS bridge**
  (`mcp-bridge.js`, spawned and driven over stdio JSON-RPC) over pinned HTTPS, and the hook
  observations flowing over pinned HTTPS `/observe` — not over UDS, not stdio, and **NOT by POSTing
  `mcp_url` directly** (bridge coverage is in-scope and must not be optimized away — D-2).
- **AC-03:** Every observation produced by the driven cycle has `topic_signal == feature`, **derived**
  by the real attribution chain (`extract_topic_signal` → `enrich_topic_signal_with_source`) from the
  observed content over the wire. **No `topic_signal` is seeded** — neither SQL-injected
  (`_seed_observation_sql_lifecycle`) nor struct-injected (`make_stamped_event`) — anywhere in this
  test's setup or assertion path.
- **AC-04:** `context_cycle_review(feature)` over the HTTPS path returns a **non-empty** `MetricVector`
  (`total_tool_calls > 0`, `session_count > 0`, `phases` populated) that is **equal field-for-field —
  modulo the documented exclusion set (D-5)** of non-deterministic wall-clock fields (`computed_at`,
  the enumerated duration-derived fields) — across all `UniversalMetrics` fields, the `phases`
  BTreeMap key set + `tool_call_count`, and `domain_metrics`, to the `MetricVector` returned by
  `context_cycle_review(feature)` for the **identical workload driven over local UDS in the same test
  execution** (live-vs-live, NOT a captured golden vector — D-6).
- **AC-05:** The fixture is wired as a standing gate in the **release-gate Docker lane via
  `workflow_dispatch`/tag (D-3)**, mirroring nan-019 — NOT a per-PR Docker lane (promotable later). A
  green run provably means the fixture **actually ran end-to-end** over the live bridge:
  skip-when-Docker-absent is a hard failure (exit-code discriminator) and a positive terminal
  run-marker is asserted (reuse the nan-019 `release-gate-lib.sh` verify-by-name contract). No
  early-exit-0 or environment skip can masquerade as parity-proven.
- **AC-06:** NO production code changed — the diff is test infrastructure only (harness/fixtures/
  scripts/CI lane). The shipped image, routes, bridge, TLS provisioning, and attribution chain are
  exercised as-is, not modified.
- **AC-07:** The fixture EXTENDS existing `infra-001` assets (the Docker smoke + gate lib and/or the
  Python harness clients/fixtures) — it introduces no parallel scaffolding that duplicates server
  spawn, cert-pinning, credstore, or bundle handling already present.

## Constraints

- **Test infrastructure is cumulative.** Extend `infra-001` (`product/test/infra-001/`) — the Docker
  smoke + `release-gate-lib.sh`, the Python harness clients/fixtures, the JS cert-pin/credstore/bridge
  machinery. Do NOT scaffold isolated infrastructure.
- **No production code changes.** Pure test infra; no new server behavior (the C0 done_when is
  *measurement*, not new function).
- **Cloud MCP is bundle-only over a pinned, self-signed leaf (vnc-038/vnc-039).** The fixture must
  register a slug pre-serve, speak HTTPS with a leaf-fingerprint pin, and use the bridge — never plain
  HTTP `/observe`, never an unpinned connection (entry #5098, #5129). The bearer flushes only after the
  pin matches.
- **rmcp forces SSE.** The bridge/MCP path must send `Accept: application/json, text/event-stream` and
  parse `text/event-stream` responses (entry #5129) — the fixture exercises this real framing, not a
  JSON-only shortcut.
- **Stable session/attribution identity.** #832's root cause was divergent CC session ids between the
  cycle-declaration hook spawn and per-tool observe spawns. The fixture must present a stable session
  identity across declaration + observes so the cycle-join holds (the very behavior under test).
- **CI home is a Docker-capable lane, NOT `pull_request`.** GH `ci.yml` is JS-client-only; Rust/
  container validation lives in the release workflow + protocol gates (standing rule). Docker is
  verified present in the dev env; CI runners (`ubuntu-22.04`/`-arm`) ship Docker.
- **Determinism for parity (D-5).** The HTTPS and UDS runs must execute the *identical* workload so the
  `MetricVector` comparison is exact field-for-field, **modulo a documented exclusion set of
  non-deterministic fields**: `computed_at` (wall-clock stamp) and any duration-derived field with
  sub-second wall-clock jitter — at minimum `phases[*].duration_secs` (and any `UniversalMetrics`
  duration/latency field the design enumerates). The comparator compares structural metrics
  (counts, num/den pairs, the `phases` key set + `tool_call_count`, `domain_metrics`) and excludes the
  enumerated wall-clock fields; the exclusion set is named in the test so the gate is non-flaky.
- **Rust/JS workspace rules** apply to any helper code (≤500 lines/file, no stubs, no `.unwrap()` in
  non-test Rust, `tracing` for logs, zero new JS runtime deps).

## Resolved Decisions

All scope-shaping open questions are resolved (human-concurred via the coordinator, 2026-06-24 — locked
pre-design). No open questions remain.

- **D-1 — Fixture substrate: HYBRID (RESOLVED).** The containerized Docker fixture stands up the
  shipped HTTPS image (cert-pinned) — cumulatively extend `docker-http-posture-smoke.sh`. The Python
  pytest harness drives the **local-UDS baseline** run and **owns the `MetricVector` comparison /
  assertion code** — cumulatively extend `harness/client.py` (+ `UnimatrixUdsClient` /
  `UnimatrixHookClient`). Do NOT fork scaffolding. (Drives AC-01, AC-04, AC-07; Proposed Approach
  step 1.)
- **D-2 — Bridge in path (RESOLVED).** Drive the full `context_cycle(start) → tool calls (incl. a
  `Bash` command) → stop` **THROUGH `mcp-bridge.js`** (the vnc-039 stdio→HTTPS bridge): spawn it and
  speak stdio JSON-RPC to it as the local MCP server. Do NOT POST `mcp_url` directly — bridge coverage
  is in-scope and must not be optimized away. (Drives AC-02; Proposed Approach step 3.)
- **D-3 — CI lane: release-gate via `workflow_dispatch`/tag (RESOLVED).** Wire as a standing gate in
  the release-gate Docker lane (`workflow_dispatch`/tag), mirroring nan-019 — NOT a per-PR Docker lane.
  Promotable to a PR lane later. (Drives AC-05; the `pull_request` Non-Goal.)
- **D-4 — Naming / ownership: nan-021 owns the fixture (RESOLVED).** nan-021 (Nanoprobes
  test-infra/gate) owns the fixture, cumulative on `infra-001`/nan-019 — matching nan-019's "wire a
  gate, change no behavior" posture. The capability credit flows to C0 (#5191, personal-cloud): nan-021
  ADVANCES C0 as its named evidence artifact; it does not flip C0 to `proven` (that is held by #837).
- **D-5 — Parity tolerance: exact field-for-field equality MODULO a documented exclusion set
  (RESOLVED).** The comparator asserts exact `MetricVector` equality except for enumerated
  non-deterministic fields: `computed_at` (wall-clock stamp) and any duration-derived field with
  sub-second wall-clock jitter — at minimum `phases[*].duration_secs` (plus any `UniversalMetrics`
  duration/latency field the design enumerates). The exclusion set is named in the test so the gate is
  non-flaky. (Drives AC-04; the Determinism constraint.)
- **D-6 — UDS baseline: LIVE-vs-LIVE (RESOLVED).** Run the identical workload over local UDS **in the
  same test execution** (two transports, one workload) — NOT a captured golden `MetricVector`. The
  stronger parity proof; no golden-drift risk. (Drives AC-04.)

## Tracking

GitHub Issue: **#836** (`feat(personal-cloud): HTTPS-bridge integration fixture`). **ADVANCES**
capability **C0** (Unimatrix #5191) as its **named end-to-end measured-parity evidence artifact**
(children C5/C10/C11 already proven). nan-021 does **NOT** flip C0 to `proven` on its own — the
`proven` flip is held by **#837** (which consumes this fixture's parity artifact). Builds cumulatively
on **infra-001** (`product/test/infra-001/`) and **nan-019** (the release-gate Docker smoke +
verify-by-name gate lib). Converts **#832**'s BA-1 from a seeded join-level guard to a real HTTPS
reproduction. Produces the owed measured HTTPS-vs-UDS parity artifact that **C0 #5191** needs (the flip
itself is #837); unblocks **SL1 #703** (declared-session attribution soak substrate) and regression
coverage for the **#818/#819** silent-observe family. Exercises vnc-038/vnc-039/vnc-034/crt-055 as-is.
This SCOPE.md feeds the design session (architecture + spec).
