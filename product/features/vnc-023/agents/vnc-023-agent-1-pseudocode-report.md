# Agent Report: vnc-023-agent-1-pseudocode

## Task

Produce per-component pseudocode for rmcp 0.16 to 1.7 migration (7 components).

## Status

Complete.

## Files Produced

- `product/features/vnc-023/pseudocode/OVERVIEW.md`
- `product/features/vnc-023/pseudocode/cargo-version-bump.md`
- `product/features/vnc-023/pseudocode/server-struct-migration.md`
- `product/features/vnc-023/pseudocode/server-test-migration.md`
- `product/features/vnc-023/pseudocode/config-allowed-origins.md`
- `product/features/vnc-023/pseudocode/router-origin-wiring.md`
- `product/features/vnc-023/pseudocode/main-call-site.md`
- `product/features/vnc-023/pseudocode/initialize-signature.md`

## Components Covered

All 7 from the brief's Component Map:
1. cargo-version-bump
2. server-struct-migration
3. server-test-migration
4. config-allowed-origins
5. router-origin-wiring
6. main-call-site
7. initialize-signature

## Open Questions

1. **ServerInfo construction API in rmcp 1.7**: The pseudocode provides three strategies (struct literal with `..Default::default()`, constructor/builder, Default + field mutation). The `#[non_exhaustive]` attribute blocks struct literal construction from external crates, so Strategy A will likely fail. The implementer must check rmcp 1.7 docs/source for the correct constructor. This is compile-driven per ADR-001.

2. **ClientInfo construction API in rmcp 1.7**: Same `#[non_exhaustive]` challenge. Multiple strategies provided; compiler will guide.

3. **StreamableHttpServerConfig.allowed_origins field access**: Need to verify whether this is a pub field (direct assignment) or requires a builder method. Compile-driven.

4. **serve_client location**: May have been renamed or moved in rmcp 1.7. Compile-driven fix.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing -- 19 entries returned; relevant: #4699 (rmcp migration scope pattern confirming McpAdapter isolation boundary), #4367 (ServerHandler traps in rmcp 0.16.0), #4368 (RequestContext as tool handler parameter), #646 (serde default config pattern)
- Queried: mcp__unimatrix__context_search (pattern: rmcp migration) -- #4699 confirmed ADR-003 transport isolation limits breakage to ~3 files
- Queried: mcp__unimatrix__context_search (decision: vnc-023) -- found all 3 ADRs (#4700, #4701, #4702)
- Deviations from established patterns: none
