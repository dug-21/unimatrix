# nan-003 Pseudocode Overview

## Component Interaction

```
/unimatrix-init                          /unimatrix-seed
      |                                        |
      v                                        v
[Pre-flight: sentinel check]           [Pre-flight: context_status]
      |                                        |
      v                                        v
[Agent scan algorithm]                  [Existing-check: context_search]
      |                                        |
      v                                        v
[CLAUDE.md block template]              [Seed state machine]
      |  (append)                              |  (Level 0 -> Gate 0 -> ...)
      v                                        v
[CLAUDE.md file]                        [Entry quality gate] --> [context_store]
```

## Data Flow

### `/unimatrix-init`
1. Read CLAUDE.md -> check sentinel -> decision: skip or continue
2. Glob `.claude/agents/**/*.md` -> read each -> agent scan algorithm -> terminal report
3. Compose block from CLAUDE.md block template -> append to CLAUDE.md (or create)
4. `--dry-run` intercepts at step 3: print instead of write

### `/unimatrix-seed`
1. `context_status()` -> validate MCP available
2. `context_search(category: "convention")` + `context_search(category: "pattern")` + `context_search(category: "procedure")` -> count existing entries -> warn if >= 3
3. Level 0: read README, manifests, CLAUDE.md, .claude/ structure -> generate entries
4. Each entry passes entry quality gate (What/Why/Scope) before human presentation
5. Approved entries -> `context_store()` calls
6. STOP gate -> human decides: go deeper or done
7. Level 1/2: category-selected exploration -> per-entry approval -> store

## Shared Concepts

- **Sentinel**: `<!-- unimatrix-init v1: DO NOT REMOVE THIS LINE -->` / `<!-- end unimatrix-init v1 -->`
- **Quality gate**: What (<=200 chars), Why (>=10 chars), Scope (present)
- **Allowed seed categories**: convention, pattern, procedure
- **STOP gate phrasing**: "**STOP. Wait for human response before proceeding.**"
- **Skill file format**: YAML frontmatter (name, description) + markdown body

## Component List

| # | Component | Pseudocode File | Purpose |
|---|-----------|----------------|---------|
| 1 | `/unimatrix-init` skill | unimatrix-init.md | Overall init flow |
| 2 | `/unimatrix-seed` skill | unimatrix-seed.md | Overall seed flow |
| 3 | CLAUDE.md block template | claude-md-template.md | Exact block content |
| 4 | Agent scan algorithm | agent-scan.md | Read-only agent analysis |
| 5 | Seed state machine | seed-state-machine.md | State transitions + gates |
| 6 | Entry quality gate | entry-quality-gate.md | What/Why/Scope validation |
