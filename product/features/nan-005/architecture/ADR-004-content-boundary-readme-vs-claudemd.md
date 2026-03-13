## ADR-004: Content Boundary — README vs CLAUDE.md vs nan-003 Skills

### Context

Three documentation surfaces exist for Unimatrix-related content:

1. **README.md** — the external-facing product document (this feature)
2. **CLAUDE.md** — project instructions for contributors and agents working on Unimatrix itself
3. **nan-003 skills** (`/unimatrix-init`, `/unimatrix-seed`) — per-repo onboarding for projects adopting Unimatrix

Without explicit boundaries, content drifts between these surfaces. The risk assessment identified:
- SR-04: Operational guidance in README could overlap with `/unimatrix-init` CLAUDE.md block content
- SCOPE.md Non-Goals: "Documenting Unimatrix's internal development workflow" is explicitly out of scope for README

### Decision

**README.md owns**:
- What Unimatrix is (product description, value proposition)
- What users can do (capabilities, tools, skills, CLI)
- How to install and configure (getting started, prerequisites)
- Operational guidance users need at runtime (session boundaries, naming conventions, category discipline)
- Reference tables (tools, skills, categories, CLI)
- High-level architecture (storage backend, transport, data layout)
- Security model (trust hierarchy, content scanning, audit trail — user-facing summary only)

**README.md does NOT own**:
- Internal development workflow (protocols, agent spawning, swarm orchestration) — stays in `.claude/`
- Per-repo onboarding configuration (CLAUDE.md block content, agent recommendation instructions) — owned by `/unimatrix-init`
- Step-by-step onboarding walkthrough — owned by `/unimatrix-seed`
- Contributing guidelines, codebase conventions — stays in CLAUDE.md
- Feature-by-feature changelog — owned by CHANGELOG.md (nan-004)

**Cross-reference, don't duplicate** (SR-04 mitigation):
- The README "Getting Started" section mentions `/unimatrix-init` and `/unimatrix-seed` by name with a one-line description and reference. It does NOT restate what those skills do or replicate their content.
- Example: "Use `/unimatrix-seed` to populate foundational knowledge entries after installation." Links to skill, does not explain the skill's Level 0 / Level 1 scan model.

**CLAUDE.md owns**:
- Behavioral rules for contributors and agents working on Unimatrix (the current CLAUDE.md content)
- Non-negotiable development rules
- Phase prefix tables, feature naming conventions — these are internal development conventions, not user-facing

### Consequences

- A user evaluating Unimatrix reads only README.md and gets everything they need to install, configure, and operate it.
- A contributor working on Unimatrix reads CLAUDE.md for internal dev rules and feature naming conventions.
- A new project adopting Unimatrix runs `/unimatrix-init` for per-repo configuration and `/unimatrix-seed` for knowledge population.
- The documentation agent (uni-docs) updates only README.md — it never touches CLAUDE.md or skill files.
- Future features that add user-facing constraints must update README.md through the documentation agent, not CLAUDE.md.
