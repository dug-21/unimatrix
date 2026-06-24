# Agent Report — nan-021-agent-1-architect

## Task
Architecture for nan-021: the HTTPS-bridge integration fixture (pure test-infra, CUMULATIVE on infra-001).
Produce ARCHITECTURE.md + ADRs within LOCKED D-1..D-6 / AC-01..AC-07, addressing SR-01..SR-06.

## Artifacts produced
- `product/features/nan-021/architecture/ARCHITECTURE.md`
- `product/features/nan-021/architecture/ADR-001-hybrid-substrate-single-driver-dual-transport.md` (Unimatrix #5286)
- `product/features/nan-021/architecture/ADR-002-bridge-in-path-readiness-gates.md` (Unimatrix #5287 → #5292 → #5294; R-05 idle-eviction + NFR-7/#830 intentional-coupling amendments)
- `product/features/nan-021/architecture/ADR-003-metricvector-comparison-contract.md` (Unimatrix #5288 → #5293; first-live-run validation gate + product disposition authority)
- `product/features/nan-021/architecture/ADR-004-derived-attribution-no-seed.md` (Unimatrix #5289)
- `product/features/nan-021/architecture/ADR-005-docker-acquisition-false-green-discriminator.md` (Unimatrix #5290)
- `product/features/nan-021/architecture/ADR-006-symmetric-durability-barrier.md` (Unimatrix #5291; R-06)

## Key decisions
- **ADR-001 (D-1/D-6/SR-05):** Single declarative workload manifest + one stable session identity, driven
  to BOTH transports; pytest-as-orchestrator owns the comparator and shells out to the smoke's new
  `cloud_cycle_gates` for the HTTPS leg, reading `MetricVector(HTTPS)` back in the same execution. Extend-
  don't-fork map closes SR-04/AC-07.
- **ADR-002 (D-2/SR-01/SR-02):** Drive the cycle THROUGH `mcp-bridge.js` over pinned HTTPS; five explicit
  event-driven readiness gates (cert present, listener bound, HTTP transport active, credstore present,
  session-id captured) — NO sleeps; capture-first child stderr to `$SANDBOX`.
- **ADR-003 (D-5):** Field-for-field `MetricVector` equality modulo a CLOSED 3-field exclusion set —
  `MetricVector.computed_at`, `UniversalMetrics.total_duration_secs`, `PhaseMetrics.duration_secs`. The
  other 20 UniversalMetrics fields + phases key set/`tool_call_count` + `domain_metrics` compared exactly.
  Unexpected non-equal field = REAL failure, never widen.
- **ADR-004 (SR-06):** Assert `topic_signal == feature` exactly AND derived — structurally seed-free path
  (no `_seed_observation_sql_lifecycle`, `_seed_attributed_observations_832`, or `make_stamped_event`
  topic_signal); load-bearing valid feature-ID in the Bash content; declared cycle for the `declared` arm.
- **ADR-005 (D-3/SR-03):** Reuse nan-019's `pull || inspect || exit-4` acquisition and verify-by-name
  run-marker VERBATIM; new gates behind the sourceable guard with `SMOKE_*_CMD` seams (pre-merge
  stub-drivable); release-gate lane via `workflow_dispatch`/tag; non-skip structurally proven.

## Open questions (for spec/pseudocode)
1. `projectHash` derivation — capture from `init --bundle` output / credstore dir, never recompute (fork smell).
2. Cross-language workload manifest format + single source of truth under `harness/`.
3. Single-execution orchestration seam (pytest drives smoke, ingests `MetricVector(HTTPS)` from sandbox file).
4. The literal `feature` ID for the workload — must be a valid registry feature so attribution resolves `declared`.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_get -- strong hits #5115 (vnc-039 bridge ADR, stable session id), #5183 (nan-019 verify-by-name gate ADR), #5208 (pull-or-inspect-or-exit-4 acquisition), #5258 (stub-drive sourceable-gate pattern), #4948 (vnc-034 cert-fp format), #5266/#5267 (capture-first stderr / never-green-first-run), #5119 (vnc-039 bundle-only). Applied all to the ADRs.
- Stored: entries #5286-#5290 "ADR-001..ADR-005 nan-021" via context_store (category decision, topic nan-021). No prior nan-021 ADRs existed -> no supersession. No typed edges asserted: all predecessor relations are reuse (not Contradicts), prose-expressed, and fail the traversal-necessity bar at authoring; intra-feature Prerequisite spine + Supports links left for retro per convention.
