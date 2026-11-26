# ADR Index — nan-024

ADR content lives in Unimatrix (category `decision`), not in this file. This file is the index.

**List every ADR for this feature:**
`context_lookup({"category": "decision", "topic": "nan-024", "agent_id": "<your agent type>"})`

**Read one ADR in full:**
`context_get({"id": <Unimatrix ID, integer>, "format": "markdown", "agent_id": "<your agent type>"})`
`context_get` resolves a superseded ID to its current version, so an older ID still returns the live decision.

Only uni-architect creates, corrects, or deprecates ADRs, or edits this file. If you find a conflict, raise it to the scrum master.

| ADR | Unimatrix ID | Title | Decision (one line) | Status |
|-----|--------------|-------|---------------------|--------|
| ADR-001 | 5888 | Wiring form is a user-level shim invoked by absolute resolved path | Hooks and MCP invoke a per-machine shim in `~/.unimatrix/bin/` by absolute path (no shell expansion in hook destinations), backed by a versioned package copy and recorded node path; MCP written per-machine only, owned entries removed from project `.mcp.json`, `.codex/config.toml`, `opencode.json` (D-027, D-030, D-031, D-046, D-047, D-060, D-064, D-066) | active |
| ADR-002 | 5875 | Single runtime resolver, fixed override precedence, trust check | One resolver: binary `UNIMATRIX_BINARY` > redirect > install, client redirect > install, binary resolved only for local transport or Linux pass-through; trust check on shim/install/redirect/trust.json, refuse loudly, Windows ACL + identity recheck (D-024, D-029, D-032, D-036) | active |
| ADR-003 | 5887 | One ownership predicate module and cross-scope dedupe | One ownership module (`lib/hook-ownership.js`) decides owned entries; at most one owned entry per event across scopes; foreign entries at the `unimatrix` key left in place with a warning; unparseable `~/.claude.json` aborts the MCP leg with exit 1 (D-010, D-026, D-062, D-064, D-065) | active |
| ADR-004 | 5886 | Launcher routing, shared project predicate, exit outcomes, status | `unimatrix hook` routes to in-process Node; not a project → 0 silent, not installed → 0 + hint (Linux) / silent (macOS/Windows), resolution failure → 1; macOS/Windows cloud-only; `status` reports legacy project MCP entries (D-023, D-032, D-042, D-043, D-045, D-050–D-054, D-068) | active |
| ADR-005 | 5878 | Hook latency budget relative to Node baseline, incl. not-a-project path | p95 ≤ max(`node -e 0` p95 + 15 ms, 50 ms) in the same CI job, covering the not-a-project path, with a pre-Node shell check (path-component root match) if that path breaches (D-025) | active |
| ADR-006 | 5882 | init writes no OpenCode provisioning; wire keeps MCP | init writes no OpenCode plugin provisioning; `wire --harness opencode` writes `mcp.unimatrix` to `~/.config/opencode/opencode.json` and removes an owned project entry (D-004, D-060, D-063, D-064) | active |
