# Test Plan: opencode-install (`lib/opencode-install.js`)

Risks: R-14, R-36, R-39, R-40, AC-09. Files: update `opencode-install.test.js`, `c14-opencode.test.js` (updated not deleted; provisioning assertions replaced by absence assertions).
- `init` with `.opencode/` fixture: no `.opencode/plugins/`, no `.opencode/package.json`, `plugin[]` byte-equal, no new files.
- `wire --harness opencode`: writes `mcp.unimatrix = {"type":"local","command":<MCP argv>}` to `~/.config/opencode/opencode.json` only (not project `opencode.json`); respects `XDG_CONFIG_HOME` fixture; outside a project -> silent (D-043).
- Owned project `opencode.json` `mcp.unimatrix` removed, foreign entries preserved, file kept; foreign `unimatrix` entry (user or project) unchanged + D-065 warning.
- Spawn the written argv from the fixture project: `initialize` + search succeed (Linux local, all-OS cloud); from outside a project exits 0 silent (R-39).
- Plugin provisioning code paths deleted: grep test asserts no `plugins/unimatrix.js` writer remains; `opencode-plugin/lib/emit.js` unchanged (hash assertion, D-075).
