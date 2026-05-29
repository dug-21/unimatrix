# vnc-021-docs Agent Report

## Task
Update README.md for vnc-021 HTTPS transport feature and create client setup documentation.

## Artifacts Read
- `product/features/vnc-021/SCOPE.md`
- `product/features/vnc-021/specification/SPECIFICATION.md`
- `product/features/vnc-021/IMPLEMENTATION-BRIEF.md`
- `README.md`

## Files Created
- `docs/client-setup.md` — Client connection docs for Claude Code, Codex CLI, Gemini CLI (AC-23, AC-24, AC-25)

## README Sections Modified
1. **MCP Transport** (Architecture Overview) — Added HTTPS transport description alongside UDS, including content port, bearer token auth, path-dispatching routes (/health, /observe, /*)
2. **Data Layout** (Architecture Overview) — Added `token` file entry
3. **Configuration** — Added `[http]` and `[tls]` config section documentation with defaults
4. **CLI Reference** — Updated `serve --daemon` and `serve --foreground` descriptions to mention HTTP listener activation
5. **Security Model** — Renamed from "Security Model (Mostly Future Use)" to "Security Model"; added HTTP Authentication subsection documenting bearer token auth with constant-time validation

## Commit
`f543ab8c` — `docs: client setup + README update for vnc-021 (#658)`

## Verification
- All edits trace to SCOPE.md AC-01 through AC-25, SPECIFICATION.md FR-01 through FR-30
- No source code read
- No aspirational language added
- Terminology consistent (Unimatrix, config.toml, content port)
- MCP tool count (14) unchanged — no new tools added by vnc-021
- Skills count (10) unchanged
