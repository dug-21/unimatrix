# nan-005 Pseudocode Overview

## Components

| Component | Artifact Type | Output |
|-----------|--------------|--------|
| readme-rewrite | README.md full rewrite | `/README.md` |
| uni-docs-agent | Agent definition file | `.claude/agents/uni/uni-docs.md` |
| delivery-protocol-mod | Protocol section edit | `.claude/protocols/uni/uni-delivery-protocol.md` |

## Data Flow

```
readme-rewrite
  Inputs:  tools.rs (11 tools), .claude/skills/ (14 skills), categories.rs (8 categories),
           main.rs (5 CLI subcommands), project.rs (data layout), package.json (npm),
           Cargo.toml (rust-version, crate count), migration.rs (schema version)
  Output:  README.md — 11 sections, capability-first, all facts verified from codebase

uni-docs-agent
  Inputs:  Existing agent patterns (uni-vision-guardian, uni-synthesizer)
  Output:  .claude/agents/uni/uni-docs.md — frontmatter + role + inputs/outputs + rules + self-check
  Dependency: README.md section structure from readme-rewrite (agent must know heading names)

delivery-protocol-mod
  Inputs:  Current uni-delivery-protocol.md Phase 4 section
  Output:  Modified Phase 4 with documentation step inserted after `gh pr create`, before `/review-pr`
  Dependency: uni-docs agent name from uni-docs-agent component
```

## Sequencing

All three components can be implemented in parallel. They share no write dependencies. The uni-docs agent references README section names but does not need to wait for the README to be written — it uses the section structure defined in ADR-001 and ARCHITECTURE.md.

## Shared Context: Verified Facts

Every component shares these verified values (from codebase, not estimates):

| Fact | Verified Value | Source |
|------|---------------|--------|
| MCP tool count | 11 | `grep -c '#[tool(' tools.rs` |
| Skill count | 14 | `ls .claude/skills/ | wc -l` |
| Crate count | 9 | `ls crates/` |
| Schema version | 11 | `CURRENT_SCHEMA_VERSION` in migration.rs |
| SQLite table count | 19 | `grep -c 'CREATE TABLE IF NOT EXISTS' db.rs` |
| Rust version | 1.89 | `Cargo.toml` rust-version |
| npm package | @dug-21/unimatrix | package.json |
| npm version | 0.5.0 | package.json |
| Test count | 2131+ | `grep -r '#[test]' crates/` |
| Storage backend | SQLite (rusqlite 0.34 bundled) | unimatrix-store/Cargo.toml |
| Database filename | unimatrix.db | project.rs |
| Hook events | UserPromptSubmit, PreCompact, PreToolUse, PostToolUse, Stop | hook.rs |
| `maintain` param | Silently ignored (col-013); background tick handles maintenance | tools.rs comment |
| Node.js requirement | >=18 | package.json engines |

## Open Questions Resolved

- **OQ-01 (Tool Count)**: Verified as 11 tools. SCOPE.md claim of 12 is incorrect.
- **OQ-02 (MicroLoRA Detail)**: User-facing framing only: "adaptive embeddings that tune to project-specific usage patterns." No InfoNCE/EWC++ details.
- **OQ-03 (unimatrix-learn)**: Crate provides shared ML infrastructure — training reservoirs, EWC++ regularization state, model versioning, neural models (SignalClassifier, ConventionScorer). Description for architecture section: "Shared ML infrastructure and neural models."
- **OQ-04 (/uni-git)**: Developer/contributor-focused skill (git conventions). Include in skills table with scope note.
