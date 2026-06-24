# Agent Report — nan-021-agent-1-pseudocode (Stage 3a pseudocode)

## Deliverables
Per-component pseudocode under `product/features/nan-021/pseudocode/`:
- OVERVIEW.md — pytest-orchestrator seam, WORKLOAD manifest shared type, MetricVector comparator
  contract (named 3-field D-5 exclusion set), durability barrier, sequencing, cross-cutting.
- c1-https-standup.md (C1) — verbatim reuse of smoke Gates 1–7; C1→C2 handoff.
- c2-bridge-cycle.md (C2) — NEW `cloud_cycle_gates` fn; bridge-in-path; projectHash read-back;
  NFR-7 idle-window; FR-9 bridge-carried assertion; barrier; emit MetricVector(HTTPS)+token.
- c3-uds-baseline.md (C3) — UDS leg + the single orchestrator pytest entrypoint; no-seed; AC-03.
- c4-workload-comparator.md (C4) — sole net-new module `harness/parity_workload.py`: manifest,
  symmetric barrier helper, comparator, no-seed static guard.
- c5-gate-wiring.md (C5) — reuse run_smoke_gate + exit-code/marker contract; release-gate lane;
  nan-019 acquisition; stub-drive.

## Components covered
C1, C2, C3, C4, C5 — all five, one file each + OVERVIEW.

## Open questions / gaps flagged
1. **Barrier predicate single-sourcing (C2):** the durability barrier must be IDENTICAL on both legs,
   but C2 is shell and C4 is Python. Flagged for Stage 3b to pick ONE single-sourced mechanism (shell
   calls a `python -m harness.parity_workload barrier` entrypoint, OR the smoke imports the SAME
   bound/predicate constants C4 exports) — must not hand-duplicate the predicate (SR-05).
2. **First-live-run validation gate (ADR-003):** the comparator is wired but the 3-field exclusion set
   is NOT TRUSTED until the tester's first dual-transport run is examined field-by-field across all 18
   non-excluded UniversalMetrics fields. Not codeable — a delivery-gate obligation; surfaced in C4 test
   scenarios and OVERVIEW for the tester/leader.
3. **observe_count() durability read (C4):** two viable predicates documented (review's own stable count
   vs store-DIR delta incl -wal); Stage 3b picks one, used by both legs.

## Verification of non-invention
Every signature traced to ARCHITECTURE §Integration Surface / live infra-001 tree (verified the smoke's
`vol`/`emit_bundle`/`consume_bundle`/`SMOKE_*_CMD` seams, `[783-smoke]` marker, sourceable guard, and the
harness client files exist). C4 is named as the sole substantial net-new module; C1/C2/C3/C5 each name the
parent asset they extend. No new spawn/cert/credstore/bundle path proposed (AC-07/NFR-2).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing / context_search were unavailable as direct tools in this
  session (ToolSearch returned no deferred match for the context tools); fell back to the SubagentStart
  hook context (ADR-005 surfaced) + read ALL six ADR files + the three source documents directly.
  Findings: #5285 (cloud-transport parity / derive-don't-seed), #5192 (nan-019 verify-by-name gate),
  #5129 (rmcp-forces-SSE), #5265 (fire-and-forget WAL barrier), #5280/#830 (idle-eviction self-heal),
  #5208 (inspect-no-pull false-fail) — all incorporated via the ADRs/spec.
- Deviations from established patterns: none. Pseudocode honors ADR-001 (pytest-orchestrator, single
  manifest), ADR-002 (bridge-in-path, idle-window, #830 coupling), ADR-003 (closed 3-field exclusion +
  first-live-run gate + product disposition), ADR-004 (no-seed), ADR-005 (acquisition + false-green),
  ADR-006 (symmetric barrier). Reuses nan-019 gate contract verbatim.
