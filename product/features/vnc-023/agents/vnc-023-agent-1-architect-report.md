# Agent Report: vnc-023-agent-1-architect

## Task
Architecture for rmcp 0.16 to 1.7 migration with CVE-2026-42559 resolution, Implementation description enrichment, and allowed_origins config wiring.

## Outputs

| File | Path |
|------|------|
| ARCHITECTURE.md | `product/features/vnc-023/architecture/ARCHITECTURE.md` |
| ADR-001 | `product/features/vnc-023/architecture/ADR-001-initialize-signature-strategy.md` |
| ADR-002 | `product/features/vnc-023/architecture/ADR-002-allowed-origins-config.md` |
| ADR-003 | `product/features/vnc-023/architecture/ADR-003-extension-propagation-test.md` |

## ADR Summary

| ADR | Title | Unimatrix ID |
|-----|-------|--------------|
| ADR-001 | Compile-First Strategy for ServerHandler::initialize Signature | #4700 |
| ADR-002 | allowed_origins as Additive HttpConfig Field | #4701 |
| ADR-003 | Extension Propagation Integration Test for ResolvedIdentity | #4702 |

## Key Decisions

1. **Compile-first for SR-03**: Do not preemptively rewrite `initialize()`. Let compiler reveal if trait signature changed. Fix is mechanical either way.
2. **Additive config for allowed_origins**: Add `Vec<String>` field to `HttpConfig`, wire through constructor chain. Empty default = backward compatible.
3. **Extension propagation via test suite**: Existing capability-gated tool tests implicitly validate ResolvedIdentity propagation. Add explicit test only if gap found.

## Scope Risk Resolutions

| Risk | Status | Finding |
|------|--------|---------|
| SR-01 (feature flags) | RESOLVED | All 6 features verified present in rmcp 1.7.0 |
| SR-02 (MSRV) | RESOLVED | rmcp 1.7 does not declare rust-version; workspace MSRV 1.89 unaffected |
| SR-03 (initialize signature) | DESIGN DECISION | Compile-first; mechanical fix if changed (ADR-001) |
| SR-04 (http crate) | RESOLVED | rmcp 1.7 uses `http ^1`, compatible with our `http = "1"` |
| SR-07 (schemars) | RESOLVED | rmcp 1.7 uses `schemars ^1`, compatible with our `schemars = "1"` |
| SR-08 (extension propagation) | DESIGN DECISION | Integration test strategy (ADR-003) |

## Open Questions

1. Does `..Default::default()` work with `#[non_exhaustive]` + `Default` on `ServerInfo` from external crate? If not, use builder/constructor. Compiler will tell.
2. `allowed_origins` vs `allowed_hosts` interaction semantics — independent checks per rmcp source. Document in field comment.
3. `serve_client` location in 1.7 — compilation will reveal if moved.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- retrieved 17 entries; key relevant entries: #77 (ADR-001 rmcp pin), #4699 (rmcp migration pattern), #4355 (client_type_map ADR), #4356 (initialize override ADR)
- Stored: entry #4700 "ADR-001 vnc-023: Compile-First Strategy for ServerHandler::initialize Signature" via /uni-store-adr
- Stored: entry #4701 "ADR-002 vnc-023: allowed_origins as Additive HttpConfig Field" via /uni-store-adr
- Stored: entry #4702 "ADR-003 vnc-023: Extension Propagation Integration Test for ResolvedIdentity" via /uni-store-adr
