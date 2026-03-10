## ADR-002: Deterministic Example Selection via Timestamp Ordering

### Context

SCOPE.md specifies "k=3 random examples per collapsed finding, drawn from the combined evidence pools of all findings in the group." The Scope Risk Assessment (SR-02) flags non-deterministic output as a testing and debugging concern: the same report produces different markdown on each call.

Three approaches:
1. **Random selection**: As SCOPE states. Simple but non-deterministic. Complicates snapshot testing and debugging.
2. **Seeded RNG**: Use a deterministic seed (e.g., hash of feature_cycle). Reproducible for the same report, but adds complexity and the seed choice is arbitrary.
3. **Deterministic by timestamp**: Sort all evidence across grouped findings by `ts` ascending, take first 3. Always reproducible. Biased toward earliest events, which are typically the most diagnostic (first occurrence of a pattern).

### Decision

Deterministic selection by timestamp. Sort the combined evidence pool of all findings in a `rule_name` group by `ts` ascending, take the first 3 (or fewer if pool < 3).

Rationale:
- Earliest events in a pattern are typically the most diagnostic -- they show when the issue first appeared.
- Deterministic output enables snapshot testing: same report always produces same markdown.
- No additional dependencies (no RNG, no hashing).
- The SCOPE's use of "random" expressed intent for diversity (don't show all examples from one sub-finding), not statistical randomness. Timestamp ordering across a merged pool achieves similar diversity since different sub-findings typically have different timestamps.

### Consequences

- Same report always produces identical markdown. Enables `assert_eq!` in tests.
- Examples are biased toward earliest events. Late-occurring high-signal events may be missed. Acceptable for MVP; a "pick heaviest cluster" heuristic can be layered on later.
- If all sub-findings in a group have the same timestamp (degenerate case), selection falls back to input order. This is stable and deterministic.
