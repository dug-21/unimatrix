# Component 3: CLAUDE.md Block Template — Pseudocode

## Exact Block Content

The block below is appended verbatim to CLAUDE.md. No dynamic content. No substitution.

```markdown
<!-- unimatrix-init v1: DO NOT REMOVE THIS LINE -->
## Unimatrix

Knowledge engine (MCP server). Makes agent expertise searchable, trustworthy, and self-improving.

### Available Skills

| Skill | When to Use |
|-------|-------------|
| `/unimatrix-init` | First-time setup: wire CLAUDE.md and get agent recommendations |
| `/unimatrix-seed` | Populate Unimatrix with foundational repo knowledge |

### Knowledge Categories

| Category | What Goes Here |
|----------|---------------|
| `decision` | Architectural decisions (ADRs) — use `/store-adr` |
| `pattern` | Reusable implementation patterns — use `/store-pattern` |
| `procedure` | Step-by-step workflows — use `/store-procedure` |
| `convention` | Project-wide coding/process standards |
| `lesson-learned` | Post-failure takeaways — use `/store-lesson` |

### When to Invoke

- Before implementing anything new → search knowledge base
- After each architectural decision → store ADR
- After each shipped feature → run retrospective
- When a technique evolves → update procedure
<!-- end unimatrix-init v1 -->
```

## Design Rules

- Block is static — no repo-specific content injected
- Five categories listed (no `outcome` — per SCOPE.md and ARCHITECTURE.md)
- Only `unimatrix-*` prefixed skills in the table (ADR-005)
- Sentinel open/close pair enables future `--update` block replacement
- Self-contained: reader needs no external docs to understand skills and categories
