# Scope Risk Assessment: nan-021

Mode: scope-risk. Pure test-infra (no production code). Risks weighted toward fixture brittleness, CI determinism, and false-green over runtime/user failure. Historical evidence cited by Unimatrix entry ID.

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | **Live HTTPS-bridge + TLS-pin + credstore fixture is brittle.** The path chains image boot → slug register → restart → busybox cert/bearer read → bundle emit → `init --bundle` → `mcp-bridge.js` spawn → pinned-HTTPS SSE session. Each link (cert-on-volume timing, credstore path `~/.unimatrix/<projectHash>`, SSE `text/event-stream` framing #5129, bearer-after-pin) is a flake/ordering hazard. | High | High | Architect: define explicit readiness gates between links (cert present, listener bound, session-id captured) rather than fixed sleeps; reuse existing smoke helpers, don't reinvent. Capture every child's stderr to a sandbox file (entry #5266) — never swallow. |
| SR-02 | **Bridge spawn/SSE handling differs from the smoke's `curl --cacert` path.** The Docker smoke proves cert-pinned `curl`, but nan-021 newly drives `mcp-bridge.js` over stdio JSON-RPC + SSE parse — an un-smoked surface. rmcp forces SSE (#5129); a JSON-only assumption silently mis-frames. | High | Med | Spec: make AC-02 assert the bridge actually carried the MCP traffic (session-id replay observed, SSE parsed), not just a 200. Bridge coverage must not be optimized to a direct `mcp_url` POST (D-2). |
| SR-03 | **Docker IMAGE acquisition / cross-runner cache miss false-fails (or skip false-passes).** nan-019's exact trap: `docker image inspect` does not pull; a cross-runner cache miss false-failed the gate (#5208). Inverse risk: Docker absent → early-exit-0 masquerades as parity-proven. | High | Med | Architect: reuse nan-019's conditional `docker pull \|\| inspect \|\| exit-4` acquisition and the exit-code discriminator verbatim — extend, do not re-author. Honor skip-is-failure (AC-05). |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | **Accidentally forking infra-001 instead of extending it (violates AC-07/Constraints).** Two distinct infra-001 surfaces (Python pytest harness vs Docker smoke) + a HYBRID mandate (D-1) make it tempting to scaffold a third parallel server-spawn / cert-pin / bundle path. | High | Med | Architect: explicitly map each new helper to the existing asset it extends (smoke gate-lib OR `UnimatrixUdsClient`/`HookClient`); flag any net-new spawn/cert/credstore/bundle code as a fork smell in design. |
| SR-05 | **Dual-transport workload drift — the HTTPS run and UDS run are not truly identical.** D-6 live-vs-live parity only holds if both transports execute byte-identical tool calls in identical order with identical declared session identity (#832 root cause was divergent CC session ids). Divergence makes parity vacuously pass or flakily fail. | High | Med | Spec: factor ONE workload driver fed to both transports — not two parallel scripts. Pin a stable session identity across declaration + observes (Constraint). |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | **topic_signal derived path silently degrades to unattributed → AC-03 passes on wrong value or the cycle-join breaks.** The derivation chain (`extract_topic_signal` → `enrich_topic_signal_with_source`) depends on the Bash command's observed content carrying a parseable feature-ID and on the cycle-join holding. A near-miss yields `unattributed`, not `feature`. | High | Med | Spec: assert `topic_signal == feature` exactly AND assert it was derived (no seed in setup/assertion path — neither `_seed_observation_sql_lifecycle` nor `make_stamped_event`). Make the Bash content's feature-ID token explicit and load-bearing. |

## Assumptions

- **(SCOPE Resolved Decisions D-1, Proposed Approach §1)** Docker is available and responsive in CI runners (Engine 29.5.2 verified in dev). If a runner lacks Docker, AC-05's skip-is-failure must HARD-fail, not green — but a release-gate lane reached for the first time has no green baseline (#5266/#5267): budget multiple tag rounds to first-green, surfacing failures in sequence.
- **(SCOPE §"context_cycle_review parity")** `MetricVector` aggregates are content-opaque and transport-agnostic *by construction*, so HTTPS==UDS field-for-field modulo D-5 exclusions. If any field carries hidden wall-clock/latency jitter NOT in the enumerated exclusion set, the gate flakes. The exclusion set being *complete* is the load-bearing assumption.
- **(SCOPE Constraints, D-2)** The vnc-039 bridge, TLS provisioning, slug routing, and attribution chain are correct as-shipped — nan-021 only exercises them. If the fixture surfaces a real cloud-path defect, that is a separate bugfix (Non-Goals), but the fixture being *green* must not depend on the defect being absent in a way that masks it.

## Design Recommendations

1. **Make false-green structurally impossible (SR-03, SR-06).** Per #4977: a green run must prove NON-SKIP — assert run-time delta + absence of skip/"skipping" log lines + a positive terminal run-marker (AC-05), not just exit 0. Assert the cycle observations exist AND are derived, not merely that the call returned.
2. **One workload, two transports (SR-05).** Drive both HTTPS and UDS from a single parameterized workload + stable session identity so D-6 parity is identical-by-construction.
3. **Capture-first, extend-don't-fork (SR-01, SR-02, SR-04).** Redirect every child (`mcp-bridge.js`, `init`, container) stderr to a sandbox file dumped tail-bounded on failure (#5266); reuse nan-019's acquisition + exit-code discriminator and infra-001's existing clients; map each helper to the asset it extends.
4. **Enumerate the exclusion set explicitly in the comparator (D-5).** Name every non-deterministic field in the test; treat an unexpected non-equal field as a real failure, not a tolerance to widen.

## Knowledge Stewardship
- Queried: context_search for false-green / parity-flakiness / test-infra risk patterns — strong hits #4977 (vacuous skip-guard), #5208 (IMAGE= pull ordering), #5266/#5267 (release-only never-green + stderr swallow), #3526 (infra-001 round-trip strategy).
- Stored: nothing novel — the recurring patterns (false-green skip-guards, release-only never-green tax, IMAGE acquisition ordering) are already captured as lessons #4977/#5208/#5266/#5267; nan-021-specific risks live in this assessment, not Unimatrix.
