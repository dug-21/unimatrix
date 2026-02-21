# ASS-006: Claude Configuration Surface & Behavioral Driving (Track 2C)

**Phase**: Assimilate (Research Spike)
**Source**: Pre-Roadmap Spike, Track 2C
**Date**: 2026-02-20
**Status**: Complete
**Depends On**: ASS-002 (D4: MCP Integration Guide), ASS-004 (D5: Context Injection Playbook, D5b: Deep Dive, D5c: Design Constraints)

---

## Objective

Map every Claude Code configuration mechanism, determine which ones reliably drive behavior, and design the minimal configuration surface that makes Unimatrix integration trivial for users.

## Research Questions

### Config Hierarchy
1. What are the exact precedence rules when CLAUDE.md, rules, agent defs, and MCP tool descriptions conflict?
2. Which mechanisms persist across conversation turns vs. are one-shot?
3. Do .claude/rules/ fire reliably? What are the glob semantics?
4. Can agent definitions include MCP tool usage instructions that reliably drive subagent behavior?
5. How do hooks interact with MCP tool calls?

### Behavioral Driving
6. Which config mechanism most reliably drives "always search memory before starting work"?
7. Can end-of-session store behavior be driven by config, or does it require hooks?
8. Can correction detection be driven by instructions alone?
9. Is convention checking best driven by CLAUDE.md or rules/ files?
10. Do Task-spawned subagents inherit parent MCP connections?

### Config Surface Design
11. What is the MINIMUM config for reliable Unimatrix integration?
12. Can MCP tool descriptions alone drive sufficient behavior (no CLAUDE.md changes)?
13. Can a single CLAUDE.md append (5-10 lines) cover 90% of cases?
14. Which behaviors require server-side logic vs. config-side instructions?

## Deliverables

- **D6a**: Claude config mechanism audit — complete map of every mechanism, scope, precedence, interactions
- **D6b**: Behavioral driving playbook — for each target behavior, recommended mechanism and reliability assessment
- **D6c**: Unimatrix config surface design — minimal user config, what Unimatrix generates, what users write
- **NOTABLE-FINDINGS.md** — insights for later tracks
