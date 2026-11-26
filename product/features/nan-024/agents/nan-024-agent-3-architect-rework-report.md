# nan-024-agent-3-architect-rework report

Unimatrix entries consulted: #5889, #5890, #5891, #5892, #5893, #5894 (ADR-001..006 current), #3598, #3611 (stale-surface lessons)

## Changes
- ADRs corrected in Unimatrix: ADR-001 #5889->#5896 (D-078, D-081, D-082, D-072 text); ADR-003 #5891->#5897 (D-081); ADR-004 #5892->#5899 (D-077, D-080, D-079 note); ADR-005 #5893->#5898 (D-079). ADR-002 #5890 and ADR-006 #5894 unchanged.
- ADR-INDEX.md: IDs refreshed (prior index IDs were stale), rows extended.
- ARCHITECTURE.md: components, surface (allowSelf, home, {env,home}, pre-Node check row, package copy), tests, resolved decisions, new D-083 section, Codex paths.
- D-080 `effectiveEnv`: not blessed; marked pending H1 in the Integration Surface.

## D-083 form
See ARCHITECTURE.md section "Reference configs and docs".

## Open / flags
- ADR-002 does not carry `allowSelf` (left unchanged per instruction); contract text is in ARCHITECTURE.md only.
- D-081: Codex MCP write follows `$CODEX_HOME/config.toml` (consistent with D-060/D-062); whether the MCP write is gated by the same detection rule as hooks is not stated by D-081; left to existing behaviour. Relative/invalid `$CODEX_HOME` handling unspecified.
- Gemini form uses `$HOME` (Unix reference only); Windows Gemini shell semantics unverified.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing, context_lookup (nan-024 decisions) -- ADRs 001-006
- Stored: corrected #5896, #5897, #5898, #5899 via context_correct
