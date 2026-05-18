# Agent Report: vnc-017-agent-2-spec

## Task

Write the specification for vnc-017: auto-redirect incoming graph edges to the
correction target when context_correct supersedes an entry.

## Output

`product/features/vnc-017/specification/SPECIFICATION.md`

## Key Decisions Made

1. **SR-06 resolution (High risk)** — Source-validation posture is skip-with-warn:
   before calling `redirect_graph_edge`, the loop checks whether the source entry is
   Quarantined or Deprecated. If so, skip and log `tracing::warn!`. This is captured in
   FR-06 and AC-08. Consistent with ADR-003.

2. **SR-02 resolution (Med risk)** — `Ok(false)` from `redirect_graph_edge` (UNIQUE
   conflict) is treated as success, not failure or warning. The three-case return
   contract is explicitly tabled in FR-07. Referenced lesson #4041.

3. **SR-01 (Med risk: transaction fan-in ceiling)** — No cardinality cap is enforced in
   this spec. The constraint is addressed by designating inline-synchronous execution
   (NFR-01), one transaction per edge (NFR-03), and documenting that no enforcement
   mechanism exists. The architect is responsible for deciding whether to add an explicit
   cap. This is not flagged as an open question — the scope and SCOPE.md explicitly
   accept unbounded cardinality with the assumption of "handful" fan-in in practice.

4. **SR-05 resolution (Low risk)** — Zero-edge case: no response text append, no summary
   log. The append is conditional on `total_found > 0` (non-Supersedes edges only).
   Captured in FR-10, FR-13, AC-11.

5. **SR-04 (Low risk: Supersedes exclusion level)** — Left as OQ-01 for architect:
   loop-level filter (current plan) vs. SQL-level exclusion. No correctness impact;
   spec defines loop-level as the implementation, architect may override.

6. **AC-09 edge case (Ok(false) counter)** — Left as OQ-02 for architect: whether
   `Ok(false)` increments the redirected counter or not affects response text accuracy.
   The spec prescribes "no failure counted, treated as success" but defers exact counter
   semantics to architect.

7. **AC-16 added** — SR-07 recommendation incorporated: integration test must assert
   `DependencyOnDeprecated` does not fire after a successful full redirect + graph tick.

8. **AC-02 confirmed aligned with OQ-05 resolution** — `find_terminal_active` is NOT
   called. `new_entry.id` is always the target. AC-03 verifies this behaviorally.

## Gaps / Conflicts With Scope

- No conflicts identified. All SCOPE.md ACs (AC-01 through AC-11) are present and
  expanded. Five new ACs (AC-12 through AC-16) were added to cover SR-05, SR-07, and
  response text variants.
- OQ-01 and OQ-02 are specification-level ambiguities that need architect resolution
  before pseudocode is written.
- The `find_terminal_active` reference in SCOPE.md Goals §3 is superseded by OQ-05
  resolution; the spec reflects the resolved decision (always `new_entry.id`).

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — 9 entries returned. Key entries applied:
  #4459 (source-validation pattern for Contradicts redirect, pre-staged for vnc-017),
  #4420 (ADR-003 partial-write posture), #92 (ADR-002 correction chain atomicity),
  #4439 (ADR-001 edge input validation caller contract). No new patterns stored
  (specification decisions are feature-specific).
