# Test Plan — C7 Claude MCP (`lib/claude-json.js`) — extend `test/wire.test.js` / claude-json tests

Risks: R-02, R-15, R-16, R-18, R-19.

## Unit (pure mutates)
- Destination write + G2 (user-scope `mcpServers.unimatrix`) removal in one mutate → two legs (`created|updated`, `removed`), one rename; mutate(output) → `null`.
- Foreign value at `projects[R].mcpServers.unimatrix` → `skipped-foreign`, warning; G2 not removed.
- `.mcp.json` owned removal; foreign siblings deep-equal; empty `mcpServers` handling per existing behavior.
- Fidelity: `__proto__` key preserved as own property; integer `9007199254740993` text preserved; key order; indent reuse.
- Unparseable resolved `.claude.json` → throw, exit 1, bytes unchanged.
- `readClaudeMcpState(file,R,platform)` → `wired|missing|foreign|unreadable|n/a`; win32 key via `resolveProjectKey` case-normalized; worktree root key.
- Under override, no code path opens `H/.claude.json` for write (spy).
- `atomicWriteText` re-export and mtime/size CAS removed (grep).
