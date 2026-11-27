# Test Plan — C15 status (`lib/status.js`) — extend status / `launcher-status.test.js` suites

Risks: R-04, R-10, R-18, R-16. S-1, AC-1(a) status part.

## Parity (SR-09)
- For overrides {none, each var, all}: `status.collect({env,home,platform,cwd}).configDirs` deep-equals init's `resolved:` paths.
- Spawned `status` with `CLAUDE_CONFIG_DIR=D` prints `D/.claude.json`.

## claudeMcp (text + JSON, exit 0 always)
- `wired`, `missing` (after deleting `projects[R]`; line `mcp claude-code: MISSING ... in <file>; run 'npx unimatrix init'`), `foreign`, `unreadable`, `n/a` (outside project, file/key null).
- `CLAUDE_CONFIG_DIR=D`, owned entry only in `H/.claude.json` → `missing`, never `wired`; appears as `other profile:` and in `alternateProfiles`, not `staleLegacy` (D-028).
- Linked worktree root; win32-normalized key.

## Other fields
- `staleLegacy`: owned twin at legacy location → `stale:` line; unreadable legacy → `key:null, unreadable:true`.
- `opencodeConfig`: `ok|absent|unreadable`; unparseable user `opencode.json` named as leaving OpenCode unwired, exit 0.
- §6a exact JSON field names and value sets (R-10).
- Read-only: tree hash unchanged after status; no private `codexHome`/`home/.claude.json` (grep).
