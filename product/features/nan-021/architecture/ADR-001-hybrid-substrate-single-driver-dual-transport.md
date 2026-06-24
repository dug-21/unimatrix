## ADR-001 nan-021: Hybrid Substrate with a Single-Driver, One-Stable-Identity Dual-Transport Workload

### Context
D-1 LOCKS a HYBRID substrate: the containerized Docker fixture (`docker-http-posture-smoke.sh`) stands up
the shipped HTTPS image and the Python pytest harness drives the local-UDS baseline AND owns the
`MetricVector` comparison/assertion code. D-6 LOCKS live-vs-live: run the identical workload over both
transports **in the same test execution** (not a captured golden vector). AC-04 requires the two
`MetricVector`s be equal field-for-field.

SR-05 is the dominant correctness hazard: live-vs-live parity is only meaningful if both transports
execute byte-identical tool calls, in identical order, under an identical declared session identity.
#832's root cause was exactly divergent CC session ids between the cycle-declaration hook spawn and the
per-tool observe spawns, which broke the cycle-join. Two parallel scripts (one for HTTPS, one for UDS)
would drift — making parity vacuously pass or flakily fail. SR-04 compounds this: two distinct infra-001
surfaces plus a hybrid mandate make it tempting to scaffold a THIRD parallel server-spawn / cert-pin /
bundle path, violating AC-07.

The hard structural problem: the HTTPS leg drives the cycle from the **shell** smoke (it needs the
shipped image, cert-on-volume, busybox sidecar — Docker-resident), while the comparator and UDS leg live
in **Python**. The workload must be expressed once and consumed by both across that language boundary.

### Decision
**One workload, two transports, one stable identity — pytest-as-orchestrator.**

1. **Single declarative workload manifest.** The workload — the ordered tool-call list, the load-bearing
   `Bash` command content (carrying the feature-ID token, ADR-004), and one fixed session identity — is
   defined ONCE as a declarative manifest (a data structure under `harness/`, e.g.
   `parity_workload.py`/`workload.json`), not as imperative steps. Both legs consume the SAME manifest.
   Neither leg hand-writes a parallel script (SR-05). The manifest is the single source of truth for
   "what gets driven."

2. **Stable session identity across declaration + observes.** The manifest pins one CC session id used
   by BOTH the cycle declaration and every per-tool observe on BOTH legs (the #832 contract; vnc-039
   ADR-001's stable `Mcp-Session-Id` + `clientInfo.name` invariant). This is the very behavior under
   test — present it stably or the cycle-join breaks (SR-05, Constraint "Stable session/attribution
   identity").

3. **Pytest is the orchestrator and comparator owner (D-1).** The release-gate lane invokes pytest.
   Pytest: (a) drives the UDS leg directly via `UnimatrixUdsClient` + `UnimatrixHookClient` against a
   `serve` UDS daemon (extends `harness/uds_client.py`, `hook_client.py`, `conftest.py`); (b) shells out
   to the Docker smoke's new C2 gate to drive the HTTPS leg, passing the manifest, and reads back
   `MetricVector(HTTPS)` from a sandbox file; (c) runs the comparator (ADR-003) in the same execution.
   Both `MetricVector`s come from one test run — D-6 live-vs-live by construction.

4. **Extend, never fork (SR-04, AC-07).** Map: HTTPS standup → `emit_bundle`/`consume_bundle`/`vol`
   (smoke); cloud cycle → a NEW gate function `cloud_cycle_gates` in the smoke that REUSES `mcp-bridge.js`
   / `cert-pin.js` / `credstore.js` / `bundle.js` / `init.js` as-is (no new transport/cert/credstore
   code); UDS baseline → existing UDS+hook clients; comparator+driver → the only substantial net-new
   module (C4). Any net-new server-spawn, cert-pinning, credstore, or bundle code is a fork smell to be
   FLAGGED in design, not written.

### Consequences
- **Easier:** parity is identical-by-construction (one manifest, one identity) — SR-05 closed at the
  architecture level, not patched at assertion time. D-6 live-vs-live holds without golden-drift risk.
  The extend-don't-fork map gives the spec a checklist to reject parallel scaffolding (AC-07). The #832
  regression guard falls out for free (stable identity IS the workload contract).
- **Harder:** the workload must be expressible declaratively enough that BOTH a Python driver and a
  shell-driven `mcp-bridge.js` session can replay it — the manifest format is a real design artifact
  (Open Question 2). The pytest-orchestrates-smoke seam couples two processes in one test (Open Question
  3); the HTTPS `MetricVector` must cross the smoke→pytest boundary via a sandbox file. The shell C2 gate
  is still stub-drivable pre-merge (it lives behind the sourceable guard, #5258), but the
  cross-process orchestration adds a wiring surface the pure-shell smoke never had.

Related: D-1, D-6, SR-04, SR-05; AC-01, AC-04, AC-07. ADR-002 (bridge-in-path for the HTTPS leg), ADR-003
(the comparator this driver feeds), ADR-004 (the derived-attribution the Bash content produces). Reuses
vnc-039 ADR-001 (#5115, stable session id + clientInfo.name) and the nan-019/nan-020 stub-drive pattern
(#5258).
