## ADR-006: init writes no OpenCode provisioning; wire keeps MCP

### Context
D-004: OpenCode plugin package unpublished. `lib/opencode-install.js` provisions `.opencode/package.json`, plugins and `plugin[]`.

### Decision
- `init` no longer calls OpenCode provisioning. Plugin-provisioning functions are deleted; the MCP writer stays.
- `wire --harness opencode` writes to the OpenCode user config `~/.config/opencode/opencode.json` (per-machine, D-060), not the project `opencode.json`, and removes an owned `mcp.unimatrix` from the project file. It writes `mcp.unimatrix = {"type":"local","command":<argv>}` with the ADR-001 MCP argv for the OS family: Unix/macOS `["sh","-c","exec \"$HOME/.unimatrix/bin/unimatrix\" mcp-stdio"]`; Windows `["cmd","/d","/s","/c","\"\"%USERPROFILE%\\.unimatrix\\bin\\unimatrix.cmd\" mcp-stdio\""]` (subject to D-034 proof; D-047 fallback `["<recorded node>","<abs home>/.unimatrix/bin/shim-entry.js","mcp-stdio"]`).
- Existing `plugin[]` entries are left unchanged (AC-09).

### Consequences
- OpenCode = retrieval only until #998.
