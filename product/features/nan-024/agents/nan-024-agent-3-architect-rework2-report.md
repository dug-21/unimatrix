# nan-024-agent-3-architect-rework2 report

Unimatrix entries consulted: #5890, #5899 (ADR-002, ADR-004), #5896, #5817

## Changes
- ADR-002 #5890 -> #5900 (D-090): sibling self-contained `~/.unimatrix/bin/trust.js`, atomic tmp+rename refresh on init/upgrade, shim path unchanged, loaded by shim-entry before anything from packageRoot; in trust set; byte-identical test.
- ADR-004 #5899 -> #5901 (D-091): `effectiveEnv` governs hooks and mcp-bridge; `status` gains `cloud env:` line and `cloudEnv` JSON field; exit 0 unchanged.
- ADR-INDEX.md IDs/rows refreshed; ARCHITECTURE.md components, interactions, integration surface updated; "pending H1" removed.

## Status surface (verbatim)
- Text line, after `transport:`: `cloud env: not set` | `in effect` | `IGNORED <NAME>[, <NAME>...]: UNIMATRIX_PROJECT_ROOT is unset; set it to the project root to use them` | `IGNORED <NAME>[, <NAME>...]: cwd is not inside UNIMATRIX_PROJECT_ROOT <root>; set it to the project root to use them`
- JSON, between `transport` and `trust`: `cloudEnv: {status: "unset" | "in-effect" | "ignored", vars: string[], reason: "project-root-unset" | "cwd-outside-root" | null, projectRoot: string | null}`

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- ADR-001/002, vnc-050
- Stored: corrected #5900, #5901 via context_correct
