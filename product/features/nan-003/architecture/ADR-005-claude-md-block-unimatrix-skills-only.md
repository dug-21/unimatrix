## ADR-005: CLAUDE.md Block Lists Only unimatrix-* Skills

### Context

The CLAUDE.md block appended by `/unimatrix-init` must include a skills listing. There are currently 11 existing skills in Unimatrix (`store-adr`, `review-pr`, `retro`, `query-patterns`, `store-pattern`, `store-procedure`, `store-lesson`, `record-outcome`, `knowledge-lookup`, `knowledge-search`, `uni-git`) plus the two new `unimatrix-*` skills from nan-003.

Two options for the skills section:
1. **All skills**: list all 13 skills with descriptions
2. **unimatrix-* only**: list only the two new `unimatrix-init` and `unimatrix-seed` skills

### Decision

The CLAUDE.md block lists **only `unimatrix-*` prefixed skills** (`/unimatrix-init` and `/unimatrix-seed`).

**Rationale**:
- The existing 11 skills (`store-adr`, `retro`, etc.) are designed for agents already inside the Unimatrix workflow — they appear in agent definition files, system prompts, and hook integrations. They assume familiarity with Unimatrix concepts (ADRs, retrospectives, pattern categories).
- The two `unimatrix-*` skills are the **entry-point API** for users who have just installed Unimatrix. They are designed for developers with no prior Unimatrix knowledge.
- Listing all 13 skills in the CLAUDE.md block creates an overwhelming inventory that buries the two skills that matter at initialization time.
- The block must be **self-contained for a developer with no prior Unimatrix knowledge** (AC-11). Adding `store-adr`, `query-patterns`, etc. requires explaining ADRs, retrospectives, and the full knowledge lifecycle — content that belongs in documentation (nan-005), not a CLAUDE.md block.
- The `unimatrix-` prefix naming convention is explicitly established by nan-003 (SCOPE.md Goal 4) as the production skill namespace. The CLAUDE.md block reinforces this convention by surfacing only the canonical entry-point skills.

**Skills listed in block**:
| Skill | Description |
|-------|-------------|
| `/unimatrix-init` | First-time setup: wire CLAUDE.md and get agent orientation recommendations |
| `/unimatrix-seed` | Populate Unimatrix with foundational repo knowledge (requires MCP) |

The block also includes a category guide and usage triggers (when to store knowledge) so developers know the system is bigger than these two skills — they just don't need to learn all 13 skills on day one.

### Consequences

- Onboarding developers see a minimal, focused skills table — not an overwhelming inventory
- The existing 11 skills remain accessible via agent definitions, skill files, and documentation (nan-005)
- Future `unimatrix-*` skills added by subsequent features are candidates for inclusion in the block (via `/unimatrix-init --update`)
- The category guide in the block points developers toward the full knowledge lifecycle without requiring them to know all skill names up front
