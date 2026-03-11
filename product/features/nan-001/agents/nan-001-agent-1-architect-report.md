# Agent Report: nan-001-agent-1-architect

## Task

Design the architecture for nan-001 (Knowledge Export) — CLI subcommand that exports the Unimatrix knowledge base to a portable JSONL file.

## Artifacts Produced

### ARCHITECTURE.md
- **Path**: `/workspaces/unimatrix-nan-001/product/features/nan-001/architecture/ARCHITECTURE.md`
- System overview, component breakdown (CLI extension, export module, row serialization)
- Component interactions with data flow diagram
- Full JSONL format contract (v1): header schema, entries column mapping (all 26 columns), type encoding rules, table emission order, excluded tables
- Integration surface table with all function signatures
- Error boundary analysis

### ADR Files
1. **ADR-001**: `/workspaces/unimatrix-nan-001/product/features/nan-001/architecture/ADR-001-snapshot-isolation.md` — BEGIN DEFERRED transaction for consistent multi-table reads under concurrent MCP server writes (addresses SR-07)
2. **ADR-002**: `/workspaces/unimatrix-nan-001/product/features/nan-001/architecture/ADR-002-explicit-column-mapping.md` — Explicit column-to-JSON mapping via serde_json::Value, no Rust struct intermediary (addresses SR-01, SR-03)
3. **ADR-003**: `/workspaces/unimatrix-nan-001/product/features/nan-001/architecture/ADR-003-deterministic-key-ordering.md` — serde_json `preserve_order` feature for insertion-order key serialization (addresses SR-05)

## Key Design Decisions

1. **Snapshot isolation (ADR-001)**: Entire export wrapped in a single `BEGIN DEFERRED` transaction. Provides consistent snapshot across all 8 tables without blocking MCP server writes. Directly addresses SR-07 (highest-priority risk).

2. **Explicit column mapping (ADR-002)**: Each table has a hardcoded SQL column list and explicit `serde_json::Value` construction. Decouples export format from Rust type representations. SQL NULL always becomes JSON null (never omitted). f64 round-trips exactly via serde_json/ryu. JSON-in-TEXT columns emitted as raw strings (no double-encoding). Directly addresses SR-01 and SR-03.

3. **Deterministic key ordering (ADR-003)**: Enable `preserve_order` feature on serde_json for insertion-order key serialization. Keys appear in SQL column order, making output human-readable. Implementation agent should verify no test regressions from this feature flag. Directly addresses SR-05.

4. **Module structure**: Single new file `crates/unimatrix-server/src/export.rs` with `run_export()` entry point. Follows the `uds/hook.rs` pattern — self-contained, minimal shared code paths. Sync-only, no tokio (addresses SR-09).

5. **No new dependencies**: All required crates already present in unimatrix-server.

## Scope Risks Addressed

| Risk | Mitigation |
|------|-----------|
| SR-01 (format contract stability) | ADR-002: explicit column-to-JSON mapping table in ARCHITECTURE.md |
| SR-03 (f64 precision) | ADR-002: serde_json/ryu guarantees shortest round-trip representation |
| SR-05 (deterministic key ordering) | ADR-003: preserve_order feature or BTreeMap fallback |
| SR-07 (transaction isolation) | ADR-001: BEGIN DEFERRED for snapshot consistency |
| SR-09 (CLI regression risk) | Self-contained export module, minimal shared code |

## Open Questions

1. **Schema column list drift (SR-02, SR-04)**: Hardcoded column lists are acceptable for v1 given schema v11 stability assumption. Shared definition (const array or macro) is a future enhancement.
2. **preserve_order feature interaction**: Implementation agent should verify that enabling `serde_json`'s `preserve_order` feature does not break existing tests in unimatrix-server. If it does, fall back to default BTreeMap (lexicographic) ordering — still deterministic, just less readable.

## Unimatrix Knowledge Storage

ADR storage in Unimatrix via `/store-adr` was not completed because MCP tools (`mcp__unimatrix__context_search`, `mcp__unimatrix__context_store`) were not available as callable tools in this agent session. The three ADR files exist on disk and should be stored in Unimatrix in a subsequent session.
