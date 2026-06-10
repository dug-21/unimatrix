## ADR-002: Fixed Install Dir `~/.unimatrix/dogfood-client/`, Not the npm Global Prefix

### Context
The switchover writes an **absolute** hook command (`node <path>/lib/hook-client/index.js
<EVENT>`) into `.claude/settings.json`. That path must be stable across container rebuilds,
or the F6 soak silently breaks when the path moves. Candidate locations: the npm global
prefix (`npm install -g` / `--prefix $(npm prefix -g)`), or a fixed dedicated directory.

### Decision
Install to the **fixed** directory `~/.unimatrix/dogfood-client/` (overridable via
`--target` for the harness's test-scoped temp installs).

Rationale: under nvm the npm global prefix is node-version-pinned
(`.../node/vX.Y.Z/lib/node_modules/...`; node is currently v24). A container rebuild that
bumps node changes that path, invalidating the absolute hook command baked into settings. A
fixed dir under `$HOME/.unimatrix/` is rebuild-stable and lives beside the existing
`~/.unimatrix/{hash}/` state tree the client already uses (#4923), so it is conceptually
co-located with Unimatrix's runtime home. (OQ-1 resolved.)

### Consequences
Easier: a stable absolute hook command that survives container/node rebuilds; deterministic
F6 reset point; co-located with the state dir.
Harder: the install dir is not managed by npm (no `npm ls`/`npm uninstall`), so the install
script owns its lifecycle (clean-replace per ADR-001). The dir is global per-user, so the
test harness must install to a **test-scoped temp dir** (via `--target`) to avoid disturbing
a human-staged dogfood install.

Related: ADR-001 (install mechanism), ADR-003 (path is what the emitted command points at).
Cites #4923.
