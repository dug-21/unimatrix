## ADR-003: No Confidence Floor (Deviation from Product Vision)

### Context

The product vision (PRODUCT-VISION.md, crt-002 row) specifies "floor at 0.1" for confidence. The SCOPE.md for crt-002 (Decision 8) deviates from this: "Confidence can be 0.0. This only happens for entries with no usage, no votes, and the lowest trust source -- which accurately reflects zero confidence in the entry's quality."

Scope risk SR-06 flagged this as a deliberate deviation that should be traceable.

### Decision

No confidence floor. The `compute_confidence()` function clamps to [0.0, 1.0] without a lower bound. An entry with zero usage, no votes, unknown trust source, and deprecated status will have confidence near 0.0.

**Rationale:**
- A floor of 0.1 creates false confidence signals. An entry that has never been accessed, never voted on, from an unknown source, is genuinely zero-confidence knowledge. Showing 0.10 implies it has been evaluated and found to have some minimal value.
- The additive formula with a 0.5 base_score (for active entries) means no active entry will ever reach 0.0 in practice. `w_base * 0.5 = 0.10` is the minimum for an active entry with zero usage, zero votes, unknown trust source, and zero corrections.
- Only deprecated entries with zero usage and lowest trust source approach 0.0: `0.20 * 0.2 + 0.15 * 0.0 + 0.20 * 0.0 + 0.15 * 0.5 + 0.15 * 0.5 + 0.15 * 0.3 = 0.04 + 0.075 + 0.075 + 0.045 = 0.235` minimum. Even the worst-case entry has meaningful positive confidence.
- This means the floor is emergent from the formula structure (minimum ~0.04 for worst-case deprecated, ~0.19 for worst-case active), not an artificial clamp.

### Consequences

**Easier:**
- No artificial floor to maintain or explain
- The formula is self-documenting: the minimum confidence is derivable from the weights and component minimums
- No need for a separate constant that could drift from the formula

**Harder:**
- Deviates from product vision text -- requires acknowledgment in ALIGNMENT-REPORT.md
- If the formula weights change, the emergent floor changes too (must be recalculated)
