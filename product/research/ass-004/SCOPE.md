# ASS-004: Context Injection Patterns (Track 2B)

**Phase**: Assimilate (Research Spike)
**Source**: Pre-Roadmap Spike, Track 2B
**Date**: 2026-02-20
**Status**: Complete
**Depends On**: ASS-002 (MCP Integration Guide / D4)

---

## Objective

Determine how MCP tool responses influence Claude's subsequent generation. Produce a context injection playbook documenting what response formats work, what gets ignored, and optimal result count and length.

## Research Questions

1. When Claude calls `memory_search`, how does the response content influence subsequent generation?
2. Does response length affect utilization? (Do long responses get ignored?)
3. How do multiple tool calls in sequence interact? (search -> get -> use)
4. What response format does Claude utilize best? (plain text? markdown? JSON with explanation?)
5. How does MCP tool context interact with CLAUDE.md instructions?
6. Can we use tool responses to inject "system-like" instructions? (e.g., "Based on project conventions, you should...")

## Deliverable

**D5: Context Injection Playbook** — `CONTEXT-INJECTION-PLAYBOOK.md`
