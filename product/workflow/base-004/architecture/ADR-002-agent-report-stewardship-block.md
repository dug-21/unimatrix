## ADR-002: Structured Agent Report Stewardship Block

### Context

The validator needs to verify stewardship compliance from agent reports. SR-02 identifies the risk: relying on grep heuristics over free-form prose is brittle. The validator currently parses structured sections (gate reports use tables and headings), but agent reports have no standardized stewardship format.

Two options considered:
1. **Free-form text**: Agent mentions stewardship in prose. Validator greps for keywords ("stored", "entry #", "nothing novel"). Fragile -- synonyms, phrasing variations, and reformatting break the check.
2. **Structured block**: Agent writes a `## Knowledge Stewardship` section with fixed bullet prefixes (`Queried:`, `Stored:`, `Declined:`). Validator checks heading presence and bullet prefix existence.

### Decision

Use a structured block with the heading `## Knowledge Stewardship` and three fixed bullet prefixes:

```markdown
## Knowledge Stewardship

- Queried: /query-patterns for {area} -- {findings summary or "no results"}
- Stored: entry #{id} "{title}" via /store-pattern (or "nothing novel to store -- {reason}")
- Declined: {category} -- {reason}
```

Rules:
- `## Knowledge Stewardship` heading is the parsing anchor. Validator checks for this exact heading.
- `Queried:` is required for all non-exempt agents. Lists what was searched and key findings.
- `Stored:` is required for active-storage tier agents. Must contain either an entry ID or "nothing novel to store" with a reason in the same line.
- `Declined:` is optional. Used when agent considered storing but decided against it. Provides audit trail.
- The validator checks: (1) heading exists, (2) appropriate bullet prefixes exist per agent tier.

### Consequences

- Validator parsing is reliable: heading match + bullet prefix match. No NLP, no synonym handling, no fragile regex.
- Agents have a clear contract: include this block or get REWORKABLE FAIL. The format is simple enough that even if an agent paraphrases slightly, the heading and `Stored:`/`Queried:` prefixes are unambiguous.
- Future tooling (automated retro, dashboard) can extract stewardship data programmatically from agent reports.
- Agents must learn a new report section. Cost is low -- the section is 3-4 lines and the format is self-documenting.
- The "nothing novel to store -- {reason}" pattern prevents gaming: an agent cannot just write "nothing novel" without explaining why. The reason is a WARN-level check, not a FAIL, to avoid over-enforcement during adoption.
