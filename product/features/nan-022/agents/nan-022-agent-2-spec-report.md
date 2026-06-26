# Agent Report — nan-022-agent-2-spec

## Deliverable
`product/features/nan-022/specification/SPECIFICATION.md`

## Summary
Authored SPECIFICATION.md for the nan-022 cross-transport parity suite (#837), the C0
proof artifact. Generalizes nan-021's single-output MetricVector gate into a
dimension-keyed parity matrix over six dimensions × two transports, one workload, one
identity, test-only on `infra-001`.

## Key decisions / interpretations
- Defined **four outcome classes** per dimension (PARITY-PASS / PARITY-FAIL /
  INTRA-TRANSPORT-NONDETERMINISM / INFRA-ERROR) as the structural spine that encodes OQ-2
  (separate cross-transport divergence from intra-transport nondeterminism) and SR-02/SR-04.
- OQ-2 encoded as **double-capture-and-diff per leg** (FR-11) + C-8: HNSW/#4990/GH#746
  flip is INTRA-TRANSPORT-NONDETERMINISM (separate filed bug), retrieval intra-transport
  determinism is a PREREQUISITE not a red gate; D1/D4 parity asserted over a **stable ranked
  prefix** only (exact-order assertions over the unstable tail would flake — SR-01).
- OQ-3 encoded in D5: PreCompact stays in scope; "restored context identical" defined
  concretely (server-restored entry ids/content/order, byte-equal modulo wall-clock/order
  exclusion set); host-side-undrivable portion is a documented delivery-time measurability
  call-out, never a vacuous pass.
- SR-02/#839 silent half-open eviction → INFRA-ERROR via bounded transport-health preflight
  (FR-13, C-6); corrects nan-021 NFR-7's "self-heal covers eviction" assumption.
- NFR-3/NFR-4 single-source the comparator contract + ONE ranking-tolerance policy shared by
  retrieval (D1) and proactive delivery (D4) — the #5302/SR-05 drift defense.
- All AC-01..AC-12 mapped to FRs + verification methods (§6 table).

## Open questions handed forward
- OQ-A architect: ranked-prefix depth N + the single D1/D4 tie/score tolerance.
- OQ-B architect: how the seed corpus/query phase folds into the single manifest (SR-06).
- OQ-C architect: PreCompact symmetric capturability vs host-side gap (OQ-3/SR-07).
- OQ-D architect: `Informs` edges + phase determinism — barrier or tolerance? (OQ-4).
- OQ-E human: do all six dimensions gate the C0 flip, or the three `done_when` pillars? (OQ-6).

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` + `context_get` (#5298) — surfaced the
  canonical 11-frame RecordEvent contract, nan-021 ADR-001, false-green discriminator,
  #5302 single-source-the-contract lesson, C0 capability (#5191). No storage (read-only tier).
