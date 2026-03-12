## ADR-001: Absolute Paths for Hook Command Resolution

### Context

Claude Code shell hooks execute commands configured in `.claude/settings.json`. These hooks fire in a shell context that does NOT inherit the npm/npx environment — `node_modules/.bin/` is not on PATH. SR-09 identifies this as the top risk for nan-004.

Three approaches were considered:

1. **Bare name (`unimatrix hook ...`)**: Relies on the binary being on PATH. Fails because hooks execute outside npm context. Would require shell profile modification (`~/.bashrc` PATH addition), which is invasive and fragile.

2. **PATH shimming via wrapper script**: Create a wrapper in `/usr/local/bin/` or `~/.local/bin/`. Requires elevated permissions or user shell configuration. Breaks in containers and CI environments with read-only system paths.

3. **Absolute paths**: Write the full path to the binary in each hook command. The path resolves to `node_modules/@dug-21/unimatrix-linux-x64/bin/unimatrix`. Breaks if the project is moved or `node_modules` is rebuilt, but re-running `npx unimatrix init` repairs it.

### Decision

Use absolute paths to the platform binary in all hook commands written to `.claude/settings.json`.

The `npx unimatrix init` command resolves the binary's absolute path at init time and writes it directly into each hook command:

```json
{
  "type": "command",
  "command": "/home/user/project/node_modules/@dug-21/unimatrix-linux-x64/bin/unimatrix hook SessionStart"
}
```

The path points to the platform binary directly (inside the platform package), NOT to the JS shim in `node_modules/.bin/`. This avoids a Node.js process spawn on every hook invocation, which is critical for the <50ms hook latency budget.

The `.mcp.json` command field also uses the absolute path to the platform binary for the same reason — the MCP server should not go through a JS shim for startup.

When the project is moved or `node_modules` is rebuilt, the user must re-run `npx unimatrix init`. This is documented in the init command's output summary.

### Consequences

**Easier:**
- Hook execution is zero-dependency: no PATH, no Node.js, no npm context required.
- Hook latency stays within the <50ms budget (direct binary execution).
- MCP server starts without Node.js overhead.
- No shell profile modification needed.

**Harder:**
- Moving a project or reinstalling `node_modules` invalidates the paths. User must re-run `npx unimatrix init`.
- The init command must resolve the real path (follow symlinks) to avoid breaking if npm reorganizes `node_modules`.
- Hooks are less portable between machines (absolute paths are machine-specific). This is acceptable because Unimatrix is a local development tool.
