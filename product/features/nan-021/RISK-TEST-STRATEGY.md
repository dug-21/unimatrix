# Risk-Based Test Strategy: nan-021

> Architecture-risk mode. Pure test-infrastructure feature (zero production code) — a CUMULATIVE
> extension to `infra-001` that stands up the live HTTPS cloud path, drives a full `context_cycle`
> through the vnc-039 `mcp-bridge.js`, and proves HTTPS-vs-UDS `MetricVector` parity from one
> byte-identical workload. Risks are weighted toward **what makes this fixture fail in CI**: parity
> flakiness, the closed D-5 exclusion set being wrong, the two-process single-execution orchestration
> seam (ADR-001 OQ3), the bridge being silently bypassed (FR-9), readiness-gate races, Docker
> false-green. "Failure" here means a wrong verdict — a green that didn't prove parity, or a red that
> proves only a fixture defect. Historical evidence cited by Unimatrix entry ID.

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | **Parity comparator flakes on an INCOMPLETE D-5 exclusion set** — a `UniversalMetrics` ratio/duration field carries hidden sub-second wall-clock jitter not enumerated; HTTPS≠UDS on a non-deterministic field → intermittent red | High | High | Critical |
| R-02 | **Parity passes VACUOUSLY on an OVER-BROAD exclusion set** — a field that legitimately diverges (real cloud-path defect) is excluded as "non-deterministic"; green hides divergence | High | Med | Critical |
| R-03 | **Single-execution orchestration seam (OQ3 / ADR-001) is fragile** — HTTPS leg (shell smoke) and UDS leg (pytest) are separate processes; if `MetricVector(HTTPS)` is read back from a stale/partial sandbox file or a prior run, the two vectors aren't live-vs-live | High | Med | Critical |
| R-04 | **Bridge silently bypassed — D-2/FR-9 not actually proven** — fixture POSTs `mcp_url` directly, or asserts only a 200/204, so `mcp-bridge.js` (SSE parse, `Mcp-Session-Id` replay) is never exercised; AC-02 green without bridge coverage | High | Med | Critical |
| R-05 | **rmcp keep_alive evicts the idle MCP session mid-cycle → 404** — between bridge spawn (gate 8) and the first tool call, or across the readiness-gate wait, the captured `Mcp-Session-Id` goes stale; the cycle aborts or silently loses observes | High | Med | Critical |
| R-06 | **`/observe` is fire-and-forget WAL — observes not durable before the review call** — server Acks 204 then writes async to `-wal`, not synced (#5265); `context_cycle_review` runs before the rows land → empty/short `MetricVector`, flaky non-empty precondition | High | High | Critical |
| R-07 | **Derived `topic_signal` degrades to `unattributed` — AC-03 passes on the wrong value or hard-fails a real near-miss** — Bash content's feature-ID token not parseable, or slug ≠ a *valid registry feature* so `enrich_topic_signal_with_source` resolves `unattributed`/`vote` not `declared` | High | Med | High |
| R-08 | **Docker false-green / false-fail** — Docker-absent early-exit-0 masquerades as parity-proven, OR `inspect`-without-`pull` cross-runner cache miss false-fails (the exact nan-019 trap #5208) | High | Med | High |
| R-09 | **Divergent CC session identity across the two legs (#832 regression class)** — declaration-hook spawn and per-tool observe spawns key on different session ids; the cycle-join breaks on one transport but not the other → parity diverges or both go `unattributed` | High | Med | High |
| R-10 | **Accidental fork of infra-001 (AC-07/NFR-2 violation)** — a third server-spawn / cert-pin / credstore / bundle path is scaffolded; passes functionally but violates the cumulative constraint and re-introduces drift | Med | Med | Medium |
| R-11 | **`projectHash` recomputed instead of read-back (OQ1)** — fixture derives the credstore dir hash itself rather than reading it back from `init --bundle`; a hash-algo drift in production silently points the bridge at the wrong (or empty) credstore | Med | Med | Medium |
| R-12 | **First-green tax — release-only gate never exercised pre-tag** — gate-spine bytes (exit-code discriminator, run-marker grep, orchestration shell) are unit-tested only via stubs; the live path fails in sequence across N tag rounds (#5267) | Med | High | High |
| R-13 | **Child stderr swallowed → undiagnosable red** — `mcp-bridge.js` / `init` / container failures emit nothing to the sandbox; a release-only red has no green baseline and no captured cause (#5266) | Med | Med | Medium |
| R-14 | **Hermeticity leak — credstore/`$HOME` bleed between legs or across runs** — `init --bundle` writes outside `$SANDBOX/home`, or a stale `~/.unimatrix/<hash>/remote.json` from a prior run is read; bridge attaches to the wrong server | Med | Low | Low |

---

## Risk-to-Scenario Mapping

### R-01: Incomplete D-5 exclusion set → parity flakes
**Severity**: High · **Likelihood**: High · **Impact**: Intermittent CI red on a green-on-rerun field; erodes trust in the gate, gets the exclusion set widened reactively until it hides real divergence (becomes R-02).
**Test Scenarios**:
1. Run HTTPS-vs-UDS parity ≥20x back-to-back in one session; assert ZERO field outside the enumerated D-5 set ever differs. Any field that differs even once is either added to D-5 *with a documented reason* or is a real defect — never silently tolerated.
2. Enumerate every `UniversalMetrics` field that is duration/latency/ratio-derived (`total_duration_secs` is already named; audit `search_miss_rate`, `edit_bloat_ratio`, `parallel_call_rate`, `post_completion_work_pct`, and every num/den ratio) and assert each is EITHER deterministic across transports OR in the exclusion set — no field is unclassified.
3. Negative: inject a 1-second artificial delay into one leg's workload; assert the comparator still passes (proves it truly excludes wall-clock, not just that the runs happened to match).
**Coverage Requirement**: Every one of the 21 `UniversalMetrics` fields + `PhaseMetrics.duration_secs`/`tool_call_count` is explicitly classified `deterministic` or `excluded` in the comparator, with the excluded set named as a literal. Zero out-of-set divergence across a repeated-run burst.

### R-02: Over-broad exclusion set → parity passes vacuously
**Severity**: High · **Likelihood**: Med · **Impact**: A real HTTPS-path divergence (the exact thing this fixture exists to catch — C0 done_when) is silently masked; C0 advances on a false artifact.
**Test Scenarios**:
1. Mutation test the comparator: force one *structural* (non-excluded) field to differ between the two vectors (e.g. drop one observe from the HTTPS leg) and assert the comparator FAILS. Proves the comparator has teeth on counts.
2. Assert the exclusion set is *minimal*: each excluded field has an inline justification tied to wall-clock/jitter; `total_tool_calls`, `session_count`, `knowledge_entries_stored`, hotspot counts, and the `phases` key set are NEVER excludable (count semantics are transport-invariant).
3. Assert non-empty is checked on the structural fields, not the excluded ones (`total_tool_calls > 0`, `session_count > 0`, `phases` populated) — a believable `0` cannot satisfy parity (#5265 gaze-width).
**Coverage Requirement**: A comparator mutation harness proves divergence on any non-excluded field is caught; the exclusion set contains ONLY wall-clock/jitter fields, each justified in-code.

### R-03: Fragile single-execution orchestration seam (OQ3)
**Severity**: High · **Likelihood**: Med · **Impact**: The two `MetricVector`s come from different runs → not live-vs-live (violates D-6); worst case a stale HTTPS vector from a prior tag silently satisfies parity.
**Test Scenarios**:
1. Confirm pytest-as-orchestrator (ADR-001 recommendation): a single pytest invocation drives the UDS leg AND shells out to the smoke's C1/C2 gate, then ingests `MetricVector(HTTPS)` from a sandbox file written *by this invocation*. Assert the HTTPS-vector file is created fresh under `$SANDBOX` (not a fixed path) and is deleted/absent at test start.
2. Assert the HTTPS vector carries a run-correlation token (the workload's stable session identity / run id) and the comparator rejects a vector whose token ≠ this run's — a stale file cannot be ingested.
3. Failure-mode: if the smoke shell-out exits non-zero or the HTTPS-vector file is missing, the pytest test ERRORS (not skips, not compares against empty) — assert this explicitly.
**Coverage Requirement**: One pytest process owns both legs; `MetricVector(HTTPS)` is provably from the same invocation (correlation token checked); a missing/stale HTTPS vector is a hard error.

### R-04: Bridge silently bypassed (D-2 / FR-9)
**Severity**: High · **Likelihood**: Med · **Impact**: The headline coverage (vnc-039 bridge: SSE framing, pinned-flush, session-id replay) is never exercised; AC-02 is green but the fixture proves only a direct HTTPS POST — the un-smoked surface SR-02 flagged.
**Test Scenarios**:
1. Assert `mcp-bridge.js` is actually spawned (process exists) and the cycle MCP traffic is driven over *its* stdio JSON-RPC — assert NO direct `mcp_url` POST is issued by the fixture for cycle tool calls.
2. Assert the bridge CARRIED the traffic per FR-9: `Mcp-Session-Id` was captured on `initialize` and replayed byte-stable on a later call (observable in bridge stderr/log), and an SSE (`text/event-stream`) response was parsed — not just a 200.
3. Negative control: a JSON-only `Accept` (no `text/event-stream`) must FAIL the framing (rmcp forces SSE, #5129) — proves the fixture exercises real SSE, not a JSON shortcut.
**Coverage Requirement**: Bridge process spawned and driven over stdio; session-id replay + SSE parse asserted from observable bridge output; zero direct cycle-`mcp_url` POSTs.

### R-05: rmcp keep_alive idle session eviction → 404 mid-cycle
**Severity**: High · **Likelihood**: Med · **Impact**: The very behavior fixed in #830/#5280 — an idle cloud session is evicted; the bridge replays a stale `Mcp-Session-Id` and the next POST 404s. In a fixture with readiness-gate waits between spawn and drive, the idle window is real. Cycle aborts (red, but a fixture-timing defect not a parity finding) or silently drops observes (vacuous parity).
**Test Scenarios**:
1. Minimize the idle window: assert the first tool call follows `initialize` without an interposed fixed wait; readiness gate 8 (session-id captured) is event-driven, then drive immediately.
2. Assert the bridge's self-heal (single-flight re-init on `SESSION_NOT_FOUND` -32099, #5280) is the SHIPPED bridge being exercised — if a mid-cycle eviction occurs, the bridge re-inits once and the cycle completes; the fixture must not depend on eviction never happening.
3. Failure-mode: a 404 that exhausts re-heal surfaces as a hard cycle failure with captured bridge stderr — never a silent dropped observe that shows up only as a short `MetricVector`.
**Coverage Requirement**: Idle window between session-capture and first call is event-gated (no sleep); a mid-cycle eviction is survived by the shipped self-heal OR fails loud with captured cause — never silently truncates the observe stream.

### R-06: Fire-and-forget WAL observes not durable before review
**Severity**: High · **Likelihood**: High · **Impact**: Server `/observe` writes are `tokio::spawn` fire-and-forget to `-wal`, NOT synced before the 204 (#5265, #5191 pool_config). If `context_cycle_review` runs immediately after `cycle(stop)`, observes may not have landed → `total_tool_calls`/`phases` short or empty → flaky non-empty precondition AND a parity mismatch that is purely a timing artifact.
**Test Scenarios**:
1. Bound the review call with a deadline-poll on observe durability (cap ~10s, sleep 1, per #5265) — NOT a flat sleep, NOT an immediate single read; assert the expected observe count is present before `cycle_review`.
2. Apply the SAME durability barrier to BOTH legs — UDS observes are also async; an asymmetric barrier itself causes parity divergence.
3. Sample the per-slug store at the DIR granularity (includes `-wal`), never just `unimatrix.db`, when asserting observes persisted (#5265 takeaway 3).
**Coverage Requirement**: A deadline-poll durability barrier gates `cycle_review` on both legs symmetrically; non-empty `MetricVector` is asserted only after observes are provably durable.

### R-07: topic_signal degrades to unattributed
**Severity**: High · **Likelihood**: Med · **Impact**: AC-03 either passes on a wrong value (if the assertion is loose) or the cycle attributes `unattributed`/`vote` instead of `feature` — the #832/#818/#819 silent-observe regression class this fixture is meant to guard.
**Test Scenarios**:
1. Pin the workload's `feature_cycle` to a literal feature-ID that IS a valid registry feature, so `enrich_topic_signal_with_source` resolves the **declared** branch (not registry-fill/vote/unattributed). Assert the slug/feature is registered before the cycle drives.
2. Assert `topic_signal == feature` EXACTLY (string equal) for every driven observation — `unattributed` is a hard fail, asserted explicitly as a near-miss guard.
3. Static guard: the test's setup + assertion path invokes NEITHER `_seed_observation_sql_lifecycle` (SQL) NOR `make_stamped_event` (struct) — assert by construction (grep/import audit) that no seed site is reachable from this test.
4. Assert the Bash command's observed content carries the parseable feature-ID token (load-bearing per FR-3) — the derivation has real input, not an accidental match.
**Coverage Requirement**: Declared-feature resolution proven; exact `topic_signal == feature`; zero seed-site reachability; the derived value comes from real Bash content.

### R-08: Docker false-green / false-fail
**Severity**: High · **Likelihood**: Med · **Impact**: Either a Docker-absent runner early-exits 0 and the gate reports parity-proven (the worst false-green), or `inspect` without `pull` cross-runner cache-misses and false-fails (#5208).
**Test Scenarios**:
1. Reuse nan-019's acquisition verbatim: `docker pull || docker image inspect || exit-<code>`; assert image-unacquirable is a DISTINCT exit code from Docker-absent (OQ3 — confirm exit-4 vs the pull-miss code).
2. Docker-absent → exit 3, and `run_smoke_gate` treats exit 3 as a HARD failure (skip-is-failure, AC-05) — assert a Docker-absent run does NOT report passed.
3. Assert the anchored whole-line terminal run-marker via `grep -qxE` (no substring forge); a run that exits 0 WITHOUT the marker fails the gate.
**Coverage Requirement**: Distinct exit codes for absent/unacquirable/passed; skip-is-failure enforced; anchored run-marker required for green. Acquisition path is nan-019's, not re-authored.

### R-09: Divergent CC session identity (#832 class)
**Severity**: High · **Likelihood**: Med · **Impact**: The #832 root cause — declaration spawn and observe spawns key on divergent session ids → cycle-join breaks. If it breaks on one transport only, parity diverges; if on both, both go `unattributed` (vacuous AC-03).
**Test Scenarios**:
1. Assert ONE stable session identity is threaded through declaration + all observes on EACH leg, and the SAME identity value is used on both legs (the workload driver owns it — FR-4/SR-05).
2. Regression assertion: this fixture IS the #832 guard — assert the cycle-join produces attributed observations (`topic_signal == feature`), which only holds if the session id is stable. A divergent-id regression resurfaces as a hard AC-03 fail.
3. Assert the workload is ONE driver fed to both legs (not two parallel scripts) — divergent identity is structurally impossible if there is a single source of truth.
**Coverage Requirement**: Single stable session identity, single workload driver, both legs; cycle-join attribution asserted as the #832 regression guard.

### R-10: Accidental fork of infra-001 (AC-07)
**Severity**: Med · **Likelihood**: Med · **Impact**: Passes functionally but violates the cumulative constraint; re-introduces a parallel spawn/cert/credstore/bundle path that drifts from the shipped one.
**Test Scenarios**:
1. Diff review: every net-new helper names the existing asset it extends (smoke gate-lib / `UnimatrixUdsClient` / `UnimatrixHookClient` / JS bridge machinery). Any net-new server-spawn, cert-pin, credstore, or bundle code is flagged as a fork smell.
2. Assert C4 (`parity_workload.py` + comparator) is the ONLY substantial net-new module; C1/C2/C3/C5 are extensions of named parents (Architecture Component Breakdown).
**Coverage Requirement**: New code maps 1:1 to extended assets; the comparator + workload driver is the sole new substantial module; no duplicated spawn/cert/credstore/bundle path.

### R-11: projectHash recomputed instead of read-back (OQ1)
**Severity**: Med · **Likelihood**: Med · **Impact**: A production hash-algo drift silently points the bridge at the wrong/empty credstore dir; the fixture computes the "right" hash and never notices.
**Test Scenarios**:
1. Assert the `projectHash` passed to `node mcp-bridge.js <projectHash>` is READ BACK from `init --bundle` (stdout/log or by listing the single dir under `$SANDBOX/home/.unimatrix/`) — NOT recomputed in the fixture.
2. Assert no hashing primitive is imported/invoked in the fixture's bridge-spawn path (net-new hashing forbidden, SR-04).
**Coverage Requirement**: `projectHash` is consumed from `init`'s output, never derived; zero hashing code in the fixture.

### R-12: First-green tax / release-only gate (OQ4)
**Severity**: Med · **Likelihood**: High · **Impact**: The gate-spine bytes only ever run live on a tag; failures surface in sequence across N tag rounds (#5267), each round a slow round-trip.
**Test Scenarios**:
1. Mirror nan-019: a pre-merge stub-drive test (`SMOKE_*_CMD` seams) unit-tests the gate-spine arithmetic (exit-code discrimination, run-marker grep, orchestration control flow) BEFORE the live tag run.
2. Budget multiple tag rounds to first-green; sequence-surface failures (cert read → bridge spawn → cycle → review → parity) so each round advances one link.
**Coverage Requirement**: Gate-spine logic stub-tested pre-merge per nan-019; first-green budgeted as multiple tag rounds, not assumed one-shot.

### R-13: Child stderr swallowed → undiagnosable red (#5266)
**Severity**: Med · **Likelihood**: Med · **Impact**: A release-only red with no captured cause and no green baseline — the #5266 trap; the gate is undiagnosable by construction.
**Test Scenarios**:
1. Assert every child (`mcp-bridge.js`, `init`, container) writes stderr to a `$SANDBOX` file, tail-dumped on failure only (ADR-005), never `2>/dev/null` on a token-free child.
2. Assert the `emit_bundle` child stays suppressed (its blob carries the bearer) — the ONE deliberate exception, asserted as intentional.
**Coverage Requirement**: Capture-first stderr for all children except the bearer-bearing `emit_bundle`; failure dumps the captured tail.

### R-14: Hermeticity leak across legs/runs
**Severity**: Med · **Likelihood**: Low · **Impact**: A stale `~/.unimatrix/<hash>/remote.json` or a `$HOME` bleed attaches the bridge to the wrong server; parity compares against the wrong cloud.
**Test Scenarios**:
1. Assert `init --bundle` writes the credstore ONLY under `$HOME=$SANDBOX/home` (nan-020 negative-control precedent #5258); assert the sandbox home is fresh per run.
2. Assert no real `~/.unimatrix` outside the sandbox is read or written.
**Coverage Requirement**: Credstore confined to a per-run hermetic `$HOME`; no global `~/.unimatrix` access.

---

## Integration Risks

The fixture's risk concentrates entirely at component boundaries — it writes almost no logic of its own.

- **Shell↔Python cross-language workload seam (C2↔C4, OQ2/OQ3):** the workload is defined once in Python but the HTTPS leg drives `mcp-bridge.js` from the shell. A declarative manifest (ordered tool-call list + Bash content + session identity) must be the single source both legs replay; any hand-written parallel shell script reintroduces R-09 divergence. **Highest-leverage integration risk** (R-03, R-09).
- **Bridge↔server MCP seam:** SSE framing (#5129), `Mcp-Session-Id` capture/replay, pinned-flush ordering, and idle eviction/self-heal (#5280) all live here — exercised live for the first time in-harness (R-04, R-05).
- **Hook↔store observe seam:** async fire-and-forget WAL writes mean a durability barrier is mandatory before any aggregate read (R-06); the same barrier must be symmetric across legs.
- **Two-process orchestration seam:** smoke (HTTPS) and pytest (UDS) are separate OS processes joined only by a sandbox file + correlation token (R-03).
- **Attribution chain seam:** `extract_topic_signal → enrich_topic_signal_with_source` resolves `declared` only if the slug is a *valid registry feature* and the session id is stable (R-07, R-09).

## Edge Cases

- **Empty/short MetricVector** from an observe race (R-06) — looks like a parity mismatch but is a timing artifact; the non-empty precondition must gate on durability.
- **Mid-cycle session eviction** when the idle window crosses rmcp keep_alive (R-05).
- **`unattributed` near-miss** — derivation runs but resolves the wrong branch; a loose assertion passes (R-07).
- **Stale HTTPS vector** from a prior tag run satisfying parity (R-03).
- **Docker-absent runner** early-exit-0 (R-08).
- **Cross-runner image cache miss** with `inspect`-no-`pull` (R-08, #5208).
- **Asymmetric durability barrier** between the two legs self-inducing divergence (R-06).
- **`emit_bundle` stderr** — the one child whose output must NOT be captured (carries the bearer) (R-13).

## Security Risks

This fixture changes no production code and accepts no external/untrusted input — the actor is the test
harness itself driving the shipped transports. Security-relevant surfaces are about NOT weakening the
shipped trust boundary while exercising it:

- **Untrusted input this fixture accepts:** none from outside; the only sensitive data are the
  fixture-minted self-signed leaf cert and the bearer token read off the data volume.
- **Bearer-token exposure (blast radius):** the bearer is read off the volume (busybox sidecar) and
  flows through the bundle blob into the credstore. **The `emit_bundle` child stderr must stay
  suppressed** (R-13) — capturing it would log the bearer to a sandbox file. The credstore must be
  written mode 0600 under the hermetic `$HOME` only (R-14). Blast radius is confined to the ephemeral
  CI sandbox/container; nothing persists to a real `~/.unimatrix`.
- **TLS trust boundary must be EXERCISED, not asserted by shape (#4970):** the fixture must drive the
  real pinned-flush — bearer flushed ONLY after `verifyPeerFingerprint` matches the leaf DER fingerprint
  (`sha256:<hex>`). A shape/options assertion (e.g. "pin was configured") is false green for a
  trust-boundary; the bridge must complete a real pinned HTTPS handshake. An unpinned or plain-HTTP
  `/observe` path is forbidden (NFR-5).
- **No new attack surface:** no new route, no new deserialization, no path-traversal sink — the fixture
  only consumes the slug-validated `/v1/{slug}/...` routes as-is.

## Failure Modes (expected behavior when a risk materializes)

- **Docker absent** → exit 3, gate FAILS hard (never reports passed) (R-08).
- **Image unacquirable** → distinct exit (4) FAILS, distinguishable from absent (R-08).
- **Bridge spawn / cert read / init failure** → child stderr tail-dumped from `$SANDBOX`, hard fail with cause (R-13).
- **Mid-cycle session eviction** → shipped self-heal re-inits once and completes; if heal exhausts, hard fail with bridge stderr — never a silent dropped observe (R-05).
- **Observes not yet durable** → deadline-poll waits (≤~10s), then proceeds; on timeout, hard fail "observes not durable" — never compares an empty vector (R-06).
- **`topic_signal == unattributed`** → hard fail (near-miss guard), never a pass (R-07).
- **Out-of-D-5-set field divergence** → REAL failure surfaced with the field name; never auto-widened into the exclusion set (R-01/R-02).
- **Missing/stale HTTPS vector** → pytest ERRORS; never compares against empty or a prior run (R-03).
- **Run exits 0 without the anchored run-marker** → gate FAILS (false-green guard) (R-08).

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (brittle live-HTTPS/TLS/credstore chain, flake/ordering) | R-03, R-05, R-06, R-13, R-14 | Architecture mandates event-driven readiness gates (ARCH §HTTPS-leg sequence, ADR-002) + capture-first stderr (ADR-005). Refined here: WAL-durability barrier (R-06) and idle-eviction (R-05) are the specific ordering hazards beyond cert/listener readiness. |
| SR-02 (bridge spawn/SSE differs from `curl --cacert`; un-smoked) | R-04 | Resolved by FR-9 + ADR-002 — AC-02 asserts session-id replay + SSE parse (bridge carried it), not a 200. Scenario adds a JSON-only negative control. |
| SR-03 (Docker IMAGE acquisition false-fail / skip false-pass) | R-08, R-12 | Resolved by ADR-005 — reuse nan-019 `pull \|\| inspect \|\| exit-<code>` + exit-code discriminator + anchored run-marker verbatim. R-12 (first-green tax) is the residual operational cost. |
| SR-04 (forking infra-001 instead of extending) | R-10, R-11 | Resolved by ARCH Component Breakdown (each helper names its parent) + boundary rule. R-11 (projectHash recompute) is the concrete fork-smell to guard (OQ1). |
| SR-05 (dual-transport workload drift — not truly identical) | R-03, R-09 | Resolved by ADR-001 single-driver/one-stable-identity + declarative manifest (OQ2). R-09 is the #832 session-id regression form of this risk. |
| SR-06 (topic_signal degrades to unattributed silently) | R-07, R-09 | Resolved by ADR-004 — exact `== feature` + no-seed static guard. Refined: must resolve the *declared* branch (valid registry feature) and hold the cycle-join (R-09). |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 6 (R-01..R-06) | 18 |
| High | 3 (R-07, R-08, R-09) | 10 |
| Medium | 4 (R-10, R-11, R-12, R-13) | 8 |
| Low | 1 (R-14) | 2 |

## Knowledge Stewardship
- Queried: context_search for false-green/release-gate/parity-flakiness and cloud-bridge/SSE/readiness-race risk patterns -- strong hits #5265 (store-delta gaze-width + fire-and-forget WAL not synced before 204), #5280 (rmcp keep_alive idle eviction → stale Mcp-Session-Id 404 + single-flight self-heal), #5266/#5267 (undiagnosable swallowed-stderr + never-green-on-tag first-green tax), #5208 (IMAGE inspect-no-pull cache-miss false-fail), #4970 (trust-boundary must be exercised not shape-asserted), #5129 (rmcp forces SSE).
- Stored: nothing novel -- the recurring patterns (false-green skip-guards, release-only first-green tax, IMAGE acquisition ordering, fire-and-forget WAL durability barrier, idle-eviction self-heal) are already captured as #5265/#5266/#5267/#5208/#5280; nan-021-specific risks live in this strategy, not Unimatrix. The closest candidate for a NEW pattern -- "live-vs-live parity gates need a symmetric durability barrier + a complete-not-over-broad exclusion set" -- is not yet visible across 2+ features (only nan-021), so it is not stored per the 2-feature threshold.
