# dsn-001 Researcher Report

## Summary

SCOPE.md written to `product/features/dsn-001/SCOPE.md`.

## Key Findings

### Constants Located (file:line)

| Config Section | Constant | File | Line |
|---|---|---|---|
| `[knowledge] categories` | `INITIAL_CATEGORIES` (8-element array) | `crates/unimatrix-server/src/infra/categories.rs` | 8–17 |
| `[knowledge] boosted_categories` | hardcoded `"lesson-learned"` string comparison | `crates/unimatrix-server/src/services/search.rs` | 413, 418, 484, 489 |
| `[knowledge] freshness_half_life_hours` | `FRESHNESS_HALF_LIFE_HOURS = 168.0` | `crates/unimatrix-engine/src/confidence.rs` | 37 |
| `[confidence] weights` | `DEFAULT_WEIGHTS` struct (freshness=0.35, graph=0.30, contradiction=0.20, embedding=0.15) | `crates/unimatrix-server/src/infra/coherence.rs` | 31–36 |
| `[server] instructions` | `SERVER_INSTRUCTIONS` const | `crates/unimatrix-server/src/server.rs` | 179 |
| `[agents] default_trust` | `PERMISSIVE_AUTO_ENROLL = true` | `crates/unimatrix-server/src/infra/registry.rs` | 25 |
| `[agents] bootstrap` | hardcoded SQL bootstrap (system, human, cortical-implant) | `crates/unimatrix-store/src/registry.rs` | 16–82 |
| `[agents] session_capabilities` | permissive/strict caps branches | `crates/unimatrix-store/src/registry.rs` | 113–119 |
| `[cycle] work_context_label/cycle_label` | `#[tool(description = "...")]` attribute string | `crates/unimatrix-server/src/mcp/tools.rs` | 1501–1505 |

### Critical Constraints Discovered

1. **`FRESHNESS_HALF_LIFE_HOURS` is in `unimatrix-engine`** (pure compute crate). Plumbing config to it requires an API change: add a parameter to `compute_confidence()` or expose it as a server-layer override. The engine crate intentionally has no server coupling.

2. **`[cycle]` labels in rmcp macro attributes** — the tool description is a compile-time `&'static str` in the `#[tool(description = "...")]` macro invocation. rmcp 0.16.0 is pinned. Whether runtime-variable descriptions are supported needs investigation; this is the riskiest scope item.

3. **No TOML dependency exists** anywhere in the workspace. The `toml` crate must be added.

4. **`agent_bootstrap_defaults()` is in `unimatrix-store`** — making it configurable requires passing config values across the crate boundary or moving bootstrap logic to the server layer. The latter is cleaner.

5. **ContentScanner** (`infra/scanning.rs`) is already the right tool for `[server] instructions` injection validation. It has 26 injection + 6 PII patterns; `scan_title()` (injection-only) is the correct method.

6. **File permission check is Unix-only** — needs `#[cfg(unix)]` guard.

### Startup Path

Config must be loaded in `tokio_main_daemon` (main.rs:315) and `tokio_main_stdio` (main.rs:560), after `ensure_data_directory()` returns `paths`, before any subsystem construction. Bridge mode (`tokio_main_bridge`) does not need config. Hook, export, import paths must not load config.

### No Schema Changes

Config is purely runtime. No DB schema version bump.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for configuration, config loading, server startup, confidence weights, categories allowlist -- MCP tools unavailable in this session; searched codebase directly.
- Stored: nothing novel to store -- findings are feature-specific constant locations and constraint analysis; no generalizable pattern discovered that isn't already evident from the codebase structure.
