# Agent Report: vnc-018-agent-2-spec

**Role**: Specification Writer
**Feature**: vnc-018 — context_graph (chain, current, neighbors)
**Output**: `product/features/vnc-018/specification/SPECIFICATION.md`

## Deliverable

SPECIFICATION.md written at `product/features/vnc-018/specification/SPECIFICATION.md`.

- 14 functional requirements (FR-01 through FR-14)
- 6 non-functional requirements (NFR-01 through NFR-07)
- 26 acceptance criteria (AC-01 through AC-20, with AC-03b, AC-10a, AC-15a, AC-15b, AC-15c added beyond SCOPE.md)
- Domain models: GraphParams, EdgeRecord, ChainResponse, TruncationStatus, CurrentResponse, NeighborsResponse
- Ubiquitous language: 11 terms defined
- 5 user workflows
- Explicit NOT-in-scope list (12 items)
- 2 open questions for architect

## Key Decisions Made

1. **SR-05 resolved — TruncationStatus struct, not flat bool**: Defined `TruncationStatus { forward: bool, backward: bool }` as the `truncated` field type on `ChainResponse`. A flat `bool` cannot satisfy AC-03b (agent must know which direction was capped in direction="both"). This is a spec authority decision per lesson #3953 — the FR prescribes the type model and the architect must implement it.

2. **SR-03 resolved — forward-compat fields are validated, not silently dropped**: FR-09 and AC-15b specify that `seed_ids`, `from_id`, `to_id`, and `max_nodes` passed to unsupported modes return descriptive errors with future-mode names. Each misuse produces a distinct error identifying the field and the correct future mode (#597 or #598).

3. **SR-08 noted as hard precondition**: vnc-017 merge to main is a gate-0 condition documented in the Dependencies section. Delivery must branch from post-vnc-017 state.

4. **OQ-06 (Supersedes exclusion) hardened**: AC-10a added to explicitly assert that silent Supersedes exclusion produces NO `excluded_types`, `warnings`, or similar field in the response. This makes the OQ-06 resolution testable.

5. **AC-15a added — exact error string for Supersedes rejection**: The exact error message `"Supersedes edges are not traversable via neighbors mode — use chain or current modes for supersession navigation"` is encoded as an AC verification requirement, making it contractual.

6. **AC-15c added — resolve_supersessions rejected on chain mode**: Explicit AC for the error when `resolve_supersessions=true` is passed to chain mode.

7. **depth upper bound**: Specified as `1..=10` in FR (SCOPE.md states default=1 but not an explicit upper bound). Flagged as OQ-01 for architect confirmation.

8. **Non-existent id in neighbors mode**: Not addressed explicitly in SCOPE.md. Flagged as OQ-02 for architect confirmation; recommendation is empty result (consistent with chain mode AC-04).

## ACs Added Beyond SCOPE.md

| AC | Reason for Addition |
|----|---------------------|
| AC-03b | SR-05: per-direction truncation distinguishability requires its own testable AC |
| AC-10a | SR-04: silent Supersedes exclusion must be explicitly tested (no warning in payload) |
| AC-15a | OQ-06 resolution: exact error string for Supersedes explicit rejection must be contractual |
| AC-15b | SR-03: forward-compat field error-on-misuse needs test coverage in vnc-018, not deferred to #597/#598 |
| AC-15c | Completeness: resolve_supersessions rejection on chain mode is described in prose but lacked an AC |

## Open Questions for Architect

**OQ-01**: Should `neighbors` mode return empty or an error when `id` does not exist? Recommend empty (consistent with chain mode AC-04).

**OQ-02**: Should `depth` be validated to `1..=10` (error if outside range) or allowed up to 50? Recommend validate to `1..=10`.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — pattern #4468 (SQL CTE for supersession, directly applied to chain/current constraints), ADR-001 vnc-017 #4460 (terminal-active resolution — confirms SQL-only mandate), ADR-006 vnc-015 #4429 (PPR deferral of Advances/Motivates — confirmed FR-12 scope), lesson #3953 (spec FR type model authority — applied to SR-05 truncation resolution).
