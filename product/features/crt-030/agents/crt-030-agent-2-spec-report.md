# Agent Report: crt-030-agent-2-spec

## Output

- `/workspaces/unimatrix/product/features/crt-030/specification/SPECIFICATION.md`

## Status

Complete. All 18 acceptance criteria from SCOPE.md (AC-01 through AC-18) are present.
All scope risk items addressed.

## Key Decisions Made

### SR-03 (Critical): Step Order Fixed
The SCOPE.md Background Research section contained a stale description placing PPR "after
co-access boost prefetch (Step 6c)". The spec fixes this unambiguously: the authoritative order
is 6b → 6d (PPR) → 6c (co-access prefetch) → 7 (NLI). The Goals section and Proposed Approach
are authoritative. This is now the sole definition in the specification.

### SR-06: phase_affinity_score — No use_fallback Guard in PPR
The spec explicitly requires that Step 6d calls `phase_affinity_score` directly without a
`use_fallback` guard. The method's `1.0` cold-start return is the neutral PPR multiplier
contract (ADR-003 col-031, Unimatrix #3687). This is enforced by AC-06 and its verification
method.

### SR-04: ppr_blend_weight Dual Role — Intentional and Documented
Confirmed intentional per SCOPE.md Proposed Approach. One parameter serves both roles
(blend coefficient for existing candidates, floor coefficient for PPR-only entries). Both
roles express "fraction of score that PPR signal contributes." Documented in FR-08, the
`ppr_blend_weight` InferenceConfig field doc-comment requirement, and the Domain Models
section. A future `ppr_inject_weight` is deferred to a future feature if needed.

### SR-07: PPR-Only Entry Score Provenance
FR-09 and AC-07 require the fused scorer and NLI step treat `ppr_blend_weight × ppr_score`
identically to a real HNSW similarity. No special-casing, no origin flags. AC-17 verifies
this in the integration test path.

### SR-08: Phase Affinity Used When Available
AC-16 added explicitly to verify that when #414 data is present, the personalization vector
differs from a uniform-seeded baseline. This is the pre-release quality gate for the #414
dependency.

### SR-01 / SR-02: Latency Budget
NFR-01 specifies a three-scale-point latency table (1K/10K/100K nodes) with hard targets
(0.1ms / 1ms / 10ms). Rayon offload is a hard trigger at 100K+ nodes exceeding 10ms, not an
optional optimization. Sequential fetch accepted for v1 at ppr_max_expand=50.

### SR-05: Graph Traversal Memory
NFR-02 specifies no depth cap is required in v1. The `ppr_inclusion_threshold` + `ppr_max_expand`
bound the expansion work. Pre-launch validation of CoAccess edge density from crt-029 data is
a requirement before launch.

### AC-16 Added for SR-08
SCOPE.md had 18 ACs. AC-16 in SCOPE.md was a unit test requirement. The spec renumbers nothing;
the additional AC for SR-08 (#414 integration quality) is mapped as the new AC-16 in the spec,
matching the SCOPE.md AC-16 content (test coverage for CoAccess edges, which became AC-18).
Reconciliation: the spec preserves all 18 original SCOPE.md AC IDs exactly as written in SCOPE.md.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 12 entries. ADRs #3687, #3699, #3677,
  #3685 (col-031 phase affinity contracts), #3730 (pipeline step pattern), #3650 (TypedRelationGraph),
  #3627 (edges_of_type boundary) were directly applicable and reflected in the specification.
- Known stale entry: #3730 contains a step-ordering description inconsistent with SCOPE.md Goals.
  Flagged in spec Knowledge Stewardship section. Should be corrected in a follow-up `context_correct`
  call after this spec is reviewed.
