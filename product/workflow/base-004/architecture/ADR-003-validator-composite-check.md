## ADR-003: One Composite Stewardship Check Per Gate

### Context

The validator currently has 4 checks in Gate 3a, 6 checks in Gate 3b, and 4 checks in Gate 3c. Adding a per-agent stewardship check to each gate would add 2-3 checks per gate, nearly doubling the check count for Gate 3a and significantly increasing Gate 3c. More checks means more validator context consumption, longer gate runs, and higher probability of spurious failures.

The alternative -- one composite check per gate that covers all agents in that gate's scope -- keeps the check count flat while still enforcing stewardship.

### Decision

Add exactly one stewardship check to each gate:

**Gate 3a check #5**: "Stewardship compliance (design agents)" -- Verify that architect, risk-strategist, and pseudocode agent reports each contain a `## Knowledge Stewardship` section with content appropriate to their tier.

**Gate 3b check #7**: "Stewardship compliance (implementation agents)" -- Verify that each rust-dev agent report and the vision-guardian report (if spawned) contain a `## Knowledge Stewardship` section with `Stored:` or "nothing novel" entries.

**Gate 3c check #5**: "Stewardship compliance (test agents)" -- Verify that the tester agent report contains a `## Knowledge Stewardship` section with `Queried:` and `Stored:` or "nothing novel" entries.

Enforcement level:
- Missing `## Knowledge Stewardship` heading entirely: REWORKABLE FAIL
- Heading present but `Stored:` line missing for active-storage agent: REWORKABLE FAIL
- Heading present but reason missing after "nothing novel": WARN (not blocking)

### Consequences

- Check count increases by exactly 3 (one per gate). Gate 3a goes from 4 to 5, Gate 3b from 6 to 7, Gate 3c from 4 to 5. Marginal, not doubling.
- The composite check means the validator reads all relevant agent reports for stewardship in a single pass, rather than N separate checks that each read one report.
- Enforcement is graduated: FAIL for missing, WARN for thin. This gives the system a warm-up period where agents learn the format without blocking delivery (addresses SR-07).
- If a new agent type is added to a gate's scope, the composite check description must be updated. This is a single edit, not a new check item.
