# Agent Report: dsn-001-agent-1-pseudocode

**Agent ID**: dsn-001-agent-1-pseudocode
**Role**: Pseudocode Specialist
**Session**: Session 2 Stage 3a

---

## Task

Produce per-component pseudocode files for all components in dsn-001 (Config
Externalization, W0-3).

---

## Source Documents Read

- `product/features/dsn-001/architecture/ARCHITECTURE.md`
- `product/features/dsn-001/specification/SPECIFICATION.md`
- `product/features/dsn-001/RISK-TEST-STRATEGY.md`
- `product/features/dsn-001/architecture/ADR-001-confidence-params-struct.md`
- `product/features/dsn-001/architecture/ADR-002-config-type-placement.md`
- `product/features/dsn-001/architecture/ADR-003-two-level-config-merge.md`
- `product/features/dsn-001/architecture/ADR-004-forward-compat-stubs.md`
- `product/features/dsn-001/architecture/ADR-005-preset-enum-and-weights.md`
- `product/features/dsn-001/architecture/ADR-006-preset-resolution-pipeline.md`

---

## Source Files Read

| File | Purpose |
|------|---------|
| `crates/unimatrix-engine/src/confidence.rs` | Pre-dsn-001 ConfidenceParams state |
| `crates/unimatrix-server/src/infra/categories.rs` | CategoryAllowlist current state |
| `crates/unimatrix-server/src/infra/registry.rs` | AgentRegistry current state |
| `crates/unimatrix-store/src/registry.rs` | agent_resolve_or_enroll current signature |
| `crates/unimatrix-server/src/services/search.rs` | Four hardcoded comparisons |
| `crates/unimatrix-server/src/server.rs` | SERVER_INSTRUCTIONS const + UnimatrixServer::new |
| `crates/unimatrix-server/src/background.rs` | spawn_background_tick signature |
| `crates/unimatrix-server/src/mcp/tools.rs` | context_retrospective tool |
| `crates/unimatrix-server/src/main.rs` | tokio_main_daemon + tokio_main_stdio |
| `crates/unimatrix-server/src/infra/scanning.rs` | ContentScanner::global() |
| `crates/unimatrix-server/src/services/mod.rs` | ServiceLayer::new + SearchService wiring |

---

## Output Files Produced

All files in `product/features/dsn-001/pseudocode/`:

| File | Component | Primary File |
|------|-----------|-------------|
| `OVERVIEW.md` | Cross-component | — |
| `config-loader.md` | Config loader | `infra/config.rs` (new) |
| `confidence-params.md` | ConfidenceParams | `unimatrix-engine/src/confidence.rs` |
| `category-allowlist.md` | CategoryAllowlist | `infra/categories.rs` |
| `search-service.md` | SearchService | `services/search.rs` |
| `agent-registry.md` | AgentRegistry + Store | `infra/registry.rs` + `unimatrix-store/src/registry.rs` |
| `server-instructions.md` | UnimatrixServer | `server.rs` |
| `tool-rename.md` | Tool rename blast radius | `mcp/tools.rs` + 13 other files |
| `startup-wiring.md` | Startup wiring | `main.rs` + `background.rs` |

---

## Key Design Decisions Recorded in Pseudocode

### Option A chosen for AgentRegistry.session_caps
`session_caps: Vec<Capability>` is stored on the `AgentRegistry` struct (Option A),
not passed per-call (Option B). Rationale: simpler at call sites; session caps are
fixed at startup and do not vary per enrollment call. Aligned with architecture
integration surface.

### ServiceLayer.new requires boosted_categories parameter
`services/mod.rs` constructs `SearchService` inside `ServiceLayer::with_rate_config`.
The delivery agent must add `boosted_categories: HashSet<String>` to both
`ServiceLayer::new` and `ServiceLayer::with_rate_config`, and pass it to
`SearchService::new`. This was not explicit in the architecture but is required
by the component boundary.

### UDS listener's internal ServiceLayer uses default boosted_categories
`uds_listener.rs` constructs its own `ServiceLayer` internally and has no access
to operator config at call time. It will use
`HashSet::from(["lesson-learned".to_string()])` as the default. This is acceptable
for dsn-001 scope; hook-path search with custom boosted categories is a follow-up.

### load_config failures are non-fatal
SPECIFICATION.md and RISK-TEST-STRATEGY.md specify graceful degradation (R-15).
Config load errors and `dirs::home_dir() = None` both fall back to
`UnimatrixConfig::default()` with a `tracing::warn!`. No startup abort.

### Tool rename is a pure find-and-replace across 31 locations
Detailed in `tool-rename.md`. Excluded historical directories are listed explicitly.
The mandatory grep sweep (`grep -r "context_retrospective"`) must return zero
results outside excluded directories before PR merge.

---

## Gaps Flagged

None. All components are fully specified. The pseudocode references only interfaces
traced to architecture or codebase — no invented names.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server`, `unimatrix-store`,
  `unimatrix-engine` — no patterns found specific to config externalization, tool
  renaming, or ConfidenceParams extension. This is the first config externalization
  in the project.
- Deviations from established patterns:
  - None for `ConfidenceParams`, `CategoryAllowlist`, `AgentRegistry`, `SearchService`,
    `server.rs`, `background.rs`.
  - Tool rename (`context_retrospective` → `context_cycle_review`) is the first tool
    rename in the repository; no prior rename pattern existed to follow.
  - `ServiceLayer::new` must grow a `boosted_categories` parameter — this is a new
    pattern for threading config values into service layer construction.

---

## Self-Check

- [x] Architecture output was read before writing any pseudocode
- [x] No invented interface names — every name traced to architecture or codebase
- [x] Output is per-component (OVERVIEW.md + 8 component files), not monolithic
- [x] Each component file includes function signatures, error handling, and test scenarios
- [x] No TODO, placeholder functions, or TBD sections — gaps flagged explicitly
- [x] Shared types defined in OVERVIEW.md match usage in component files
- [x] All output files within `product/features/dsn-001/pseudocode/`
- [x] Knowledge Stewardship report block included
