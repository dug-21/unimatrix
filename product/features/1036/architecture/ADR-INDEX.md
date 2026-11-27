# ADR Index — 1036

ADR content lives in Unimatrix (category `decision`), not in this file. This file is the index.

**List every ADR for this feature:**
`context_lookup({"category": "decision", "topic": "1036", "agent_id": "<your agent type>"})`

**Read one ADR in full:**
`context_get({"id": <Unimatrix ID, integer>, "format": "markdown", "agent_id": "<your agent type>"})`
`context_get` resolves a superseded ID to its current version, so an older ID still returns the live decision.

Only uni-architect creates, corrects, or deprecates ADRs, or edits this file. If you find a conflict, raise it to the scrum master.

| ADR | Unimatrix ID | Title | Decision (one line) | Status |
|-----|--------------|-------|---------------------|--------|
| ADR-001 | 5937 | Placement rule D-046 and named global-file exceptions | Project-scoped per-machine surface first; global files only for Claude MCP (`${CLAUDE_CONFIG_DIR:-~}/.claude.json`), Codex (`$CODEX_HOME`), OpenCode MCP (resolved dir); measured Claude override layout; intent gate is store/credential (D-001, D-003, D-046) | active |
| ADR-002 | 5960 (supersedes 5949, 5938) | One config-dir resolver `lib/config-dirs.js` | Sole path builder for wire, status, mcpNode; precedence per harness (OpenCode measured); relative values ignored with a warning; `source` attribution; `legacy` = `~/.claude/settings.json`, `~/.config/opencode`; `alternates` incl. default `~/.claude.json` (D-019, D-028) | active |
| ADR-003 | 5962 (supersedes 5950, 5940, 5939) | One hardened synchronous writer | `lib/safe-write.js`: realpath-follow, `wx` random temp, fsync, sha256 CAS, fixpoint verify, win32 retry, 3 attempts then exit 1; `enforceMode` for remote.json/install.json/trust.json/root (0600) and bin/* (0700) (D-025); `containWithin` project root for project files, code `outside-project` (D-026); coverage per D-020; atomic package-dir rename; `atomic-write.js` deleted | active |
| ADR-004 | 5959 (supersedes 5954, 5951, 5941) | Wiring plan table drives writes, removals, --dry-run | `lib/wire-plan.js` ordered W/R/X steps identical in both modes; write-before-remove gating; legacy removal only `~/.claude/settings.json` hooks and `~/.config/opencode` MCP; default `~/.claude.json` kept (D-028); same-real-file rule (D-007); D-015; D-022; D-029; `isLegacyOpencodePlugin` | active |
| ADR-005 | 5955 (supersedes 5952, 5942) | Writer core inside trust.js self-contained boundary | Core in `lib/trust.js` (built-ins only) so hook-time trust.json writes use it; `safe-write.js` is the API; no second copy; symlinked trust.json exits 1 with canonical fix message (ARCHITECTURE §6a); trust check and trust set unchanged (D-021) | active |
| ADR-006 | 5943 | Git-ignore ensure via per-repo exclude file | `lib/git-exclude.js`: git at main root; no-git/not-repo/tracked/error branches reported; check-ignore first; `--git-path info/exclude`; append via hardened writer; re-check after append (D-011) | active |
| ADR-007 | 5961 (supersedes 5953, 5944) | Resolved-file reporting in status and init | status prints resolved files and sources; Claude local-scope check on resolved `.claude.json` only; `staleLegacy`; `alternateProfiles` incl. default `~/.claude.json` (D-028); `opencodeConfig` unreadable (D-024); init `[VAR]` + D-013 note | active |

Amended prior ADRs (placement rationale and paths only): nan-024 ADR-001 5896→5945, ADR-003 5897→5946→5956, ADR-006 5894→5947, ADR-004 5901→5948.
