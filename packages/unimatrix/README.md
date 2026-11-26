# Unimatrix

Self-learning context engine for multi-agent development orchestration.

```
npm install -g @dug-21/unimatrix
```

`npx unimatrix init` wires each machine separately: hooks call the user-level shim `~/.unimatrix/bin/unimatrix` (written to `.claude/settings.local.json`), and the MCP server is registered in user-level config, not the project `.mcp.json`. macOS and Windows are cloud-only; PowerShell is unsupported as a hook shell. Headless/CI use requires `UNIMATRIX_PROJECT_ROOT` (see `docs/client-setup.md`).

https://github.com/dug-21/unimatrix
