## ADR-003: context_status Pre-flight Check for /unimatrix-seed

### Context

`/unimatrix-seed` calls `context_store` to persist knowledge entries. If the Unimatrix MCP server is not running or not wired in the target repo's Claude settings, the skill will fail mid-conversation — potentially after exploring significant repo content — with no entries saved. This wastes tokens and leaves the user confused.

SR-06 identifies this as a Medium/Medium risk and recommends a pre-flight check at skill entry before any exploration begins.

### Decision

`/unimatrix-seed` opens with a `context_status()` call as its first action, before any file reads, explorations, or proposals.

**Pre-flight sequence** (first thing in SKILL.md):
```
1. Call: context_status()
2. If the call succeeds: print "Unimatrix MCP server: online ✓" and continue
3. If the call fails or returns an error: print the failure message + instructions below; STOP
```

**Error message when pre-flight fails**:
```
Unimatrix MCP server is not available.

Before running /unimatrix-seed, ensure:
1. The Unimatrix binary is running (check ~/.claude/mcp-settings.json or settings.json)
2. The MCP server is wired in your Claude Code project settings
3. The Unimatrix database has been initialized (nan-003 prerequisite)

No entries were stored. Re-run /unimatrix-seed after MCP is available.
```

**context_status selection rationale**: `context_status()` is a lightweight read call that validates connectivity, server health, and database accessibility in one call. It is the correct pre-flight probe — not a write or search operation that could have side effects.

`/unimatrix-init` does NOT require a pre-flight MCP check because Phase 3 (CLAUDE.md append) uses only file operations, and Phase 2 (agent scan) uses only file reads. Neither requires MCP connectivity. Only `/unimatrix-seed` requires MCP.

### Consequences

- Fail-fast before wasted exploration: if MCP is unavailable, the skill terminates immediately with clear instructions rather than failing mid-conversation
- Users learn about the prerequisite at the earliest possible moment in the skill flow
- The error message bridges the nan-003/nan-004 gap (SR-04): it explicitly points to MCP wiring setup which nan-004 will automate
- `context_status` call adds ~1 turn overhead to every `/unimatrix-seed` invocation — acceptable given it prevents far more expensive mid-conversation failures
