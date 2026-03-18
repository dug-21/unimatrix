# dsn-001 Implementation Brief — Config Externalization (W0-3)

> Revised: 2026-03-18 (post-preset-system design re-run — all prior WARNs closed)
> Synthesized from: 6 ADRs + SCOPE + SPECIFICATION + ARCHITECTURE + RISK-TEST-STRATEGY + ALIGNMENT-REPORT

---

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/dsn-001/SCOPE.md |
| Scope Risk Assessment | product/features/dsn-001/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/dsn-001/architecture/ARCHITECTURE.md |
| Specification | product/features/dsn-001/specification/SPECIFICATION.md |
| Risk/Test Strategy | product/features/dsn-001/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/dsn-001/ALIGNMENT-REPORT.md |
| ADR-001 | product/features/dsn-001/architecture/ADR-001-confidence-params-struct.md |
| ADR-002 | product/features/dsn-001/architecture/ADR-002-config-type-placement.md |
| ADR-003 | product/features/dsn-001/architecture/ADR-003-two-level-config-merge.md |
| ADR-004 | product/features/dsn-001/architecture/ADR-004-forward-compat-stubs.md |
| ADR-005 | product/features/dsn-001/architecture/ADR-005-preset-enum-and-weights.md |
| ADR-006 | product/features/dsn-001/architecture/ADR-006-preset-resolution-pipeline.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| config loader (`infra/config.rs`) | pseudocode/config-loader.md | test-plan/config-loader.md |
| ConfidenceParams extension (`unimatrix-engine`) | pseudocode/confidence-params.md | test-plan/confidence-params.md |
| CategoryAllowlist extension (`infra/categories.rs`) | pseudocode/category-allowlist.md | test-plan/category-allowlist.md |
| SearchService boosted_categories (`services/search.rs`) | pseudocode/search-service.md | test-plan/search-service.md |
| AgentRegistry externalization (`infra/registry.rs`) | pseudocode/agent-registry.md | test-plan/agent-registry.md |
| UnimatrixServer instructions (`server.rs`) | pseudocode/server-instructions.md | test-plan/server-instructions.md |
| context_cycle_review rename (`mcp/tools.rs`) | pseudocode/tool-rename.md | test-plan/tool-rename.md |
| Startup wiring (`main.rs`) | pseudocode/startup-wiring.md | test-plan/startup-wiring.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

Stage 3a complete. All pseudocode and test-plan files produced. Two implementation notes from pseudocode agent:
1. `ServiceLayer::new` and `with_rate_config` must grow a `boosted_categories: HashSet<String>` parameter — delivery agent for search-service must also update `services/mod.rs` and the UDS listener path.
2. UDS listener's internal `ServiceLayer` construction defaults to `HashSet::from(["lesson-learned"])` for dsn-001 scope — documented known limitation.

---

## Goal

Unimatrix is a domain-agnostic knowledge engine whose runtime behavior is currently hard-coupled to software delivery vocabulary through compiled constants. This feature externalizes five groups of constants to a two-level TOML configuration system (`~/.unimatrix/config.toml` global, `~/.unimatrix/{hash}/config.toml` per-project), adds a `[profile]` preset system with four named knowledge-lifecycle archetypes (`authoritative`, `operational`, `empirical`, `collaborative`) plus a `custom` escape hatch, and validates all security-critical config fields at startup with abort-on-violation semantics. Two hardcoded vocabulary fixes accompany the config work: `context_retrospective` is renamed to `context_cycle_review` across the entire codebase, and the `CycleParams.topic` field doc is neutralized to domain-agnostic language. After this feature, Unimatrix can be deployed for legal, SRE, environmental monitoring, and scientific domains by supplying a `~/.unimatrix/config.toml` without recompiling.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| `ConfidenceParams` struct scope — 3 fields or 9 fields | Extended from 3 fields to 9 fields: six `w_*` weight fields + `freshness_half_life_hours`, `alpha0`, `beta0`. `Default` reproduces compiled constants exactly (= collaborative preset). `compute_confidence` and `freshness_score` use `params.*` instead of compiled constants. SR-02 eliminated: preset weight selection now flows directly into the formula. | ADR-001, SR-02 resolved | product/features/dsn-001/architecture/ADR-001-confidence-params-struct.md |
| Config type placement — server crate vs. core vs. new crate | `UnimatrixConfig` and all sub-structs live in `unimatrix-server/src/infra/config.rs`. `toml` crate added to `unimatrix-server` only. No `Arc<UnimatrixConfig>` crosses any crate boundary — only plain primitives (`bool`, `Vec<String>`, `HashSet<String>`, `Vec<Capability>`, `ConfidenceParams`). | ADR-002, SR-08 resolved | product/features/dsn-001/architecture/ADR-002-config-type-placement.md |
| Two-level config merge semantics | Replace semantics field-by-field: per-project field present overrides global; absent falls through. List fields replace entirely — no append. Per-project `preset = "custom"` with no per-project `[confidence] weights` aborts even if global has weights; cross-level weight inheritance is prohibited. | ADR-003, SR-06 resolved | product/features/dsn-001/architecture/ADR-003-two-level-config-merge.md |
| `[confidence]` section role | Promoted from empty forward-compat stub to live section active only when `preset = "custom"`. Named presets ignore `[confidence]` entirely even if present (warn-and-continue). `CycleConfig` stub removed from `UnimatrixConfig`. | ADR-004, SR-12 resolved | product/features/dsn-001/architecture/ADR-004-forward-compat-stubs.md |
| Preset enum design and weight table | Five variants: `Authoritative`, `Operational`, `Empirical`, `Collaborative` (default), `Custom`. `#[serde(rename_all = "lowercase")]` — unknown strings fail at deserialization before `validate_config`. Exact weight table locked; all rows sum to 0.92. `collaborative` row equals `ConfidenceParams::default()` exactly (SR-10 mandatory test). | ADR-005, SR-09/SR-10 resolved | product/features/dsn-001/architecture/ADR-005-preset-enum-and-weights.md |
| Preset resolution pipeline | Single resolution site: `resolve_confidence_params(&config) -> Result<ConfidenceParams, ConfigError>` in `config.rs`. Named presets use ADR-005 table + optional `[knowledge]` half-life override. `custom` requires both `[confidence] weights` AND `[knowledge] freshness_half_life_hours`; absence of either aborts startup. | ADR-006, SR-11/SR-13 resolved | product/features/dsn-001/architecture/ADR-006-preset-resolution-pipeline.md |

---

## Files to Create / Modify

### New Files

| File | Summary |
|------|---------|
| `crates/unimatrix-server/src/infra/config.rs` | Entire config system: `UnimatrixConfig`, five sub-structs, `Preset` enum, `ConfigError`, `load_config`, `validate_config`, `resolve_confidence_params`, `confidence_params_from_preset`, `merge_configs`, `check_permissions` |

### Modified Files

| File | What Changes |
|------|-------------|
| `crates/unimatrix-server/Cargo.toml` | Add `toml = "0.8"` (exact pin) |
| `crates/unimatrix-server/src/infra/mod.rs` | Export `config` module |
| `crates/unimatrix-engine/src/confidence.rs` | `ConfidenceParams` extended to 9 fields; `compute_confidence` and `freshness_score` use `params.*` instead of compiled constants; compiled constants become backing values for `Default` only |
| `crates/unimatrix-server/src/main.rs` | Insert `load_config` + `resolve_confidence_params` after `ensure_data_directory()`; pass extracted values to subsystem constructors; `tokio_main_daemon` and `tokio_main_stdio` only |
| `crates/unimatrix-server/src/infra/categories.rs` | Add `CategoryAllowlist::from_categories(Vec<String>) -> Self`; `new()` delegates to `from_categories(INITIAL_CATEGORIES.to_vec())` |
| `crates/unimatrix-server/src/infra/registry.rs` | Remove `const PERMISSIVE_AUTO_ENROLL`; `AgentRegistry::new(store, permissive: bool)` receives flag from config; pass `session_caps` Vec through to store call |
| `crates/unimatrix-server/src/services/search.rs` | Replace all four `entry.category == "lesson-learned"` comparisons with `boosted_categories: HashSet<String>` lookup; field added to `SearchService` |
| `crates/unimatrix-server/src/server.rs` | Remove `SERVER_INSTRUCTIONS` const; use `config.server.instructions`; update doc comments referencing `context_retrospective` (3 comments) |
| `crates/unimatrix-server/src/background.rs` | Accept `Arc<ConfidenceParams>` at spawn; use it in all `compute_confidence` calls |
| `crates/unimatrix-server/src/mcp/tools.rs` | Rename `context_retrospective` → `context_cycle_review` in `#[tool(name)]`, handler fn, audit log strings (2 locations), doc strings; neutralize `CycleParams.topic` field doc |
| `crates/unimatrix-store/src/registry.rs` | `agent_resolve_or_enroll` gains third param `session_caps: Option<&[Capability]>`; all existing call sites pass `None` |
| `crates/unimatrix-observe/src/types.rs` | Update doc comment referencing `context_retrospective` |
| `crates/unimatrix-observe/src/session_metrics.rs` | Update test assertion: `classify_tool("context_cycle_review")` |
| `product/test/infra-001/harness/client.py` | Rename `context_retrospective` method + update tool name string in `call_tool()` |
| `product/test/infra-001/suites/test_protocol.py` | Update tool name in expected tool list (line 55) |
| `product/test/infra-001/suites/test_tools.py` | Update ~14 `context_retrospective` call sites and section-header comments |
| `.claude/skills/uni-retro/SKILL.md` | Update `mcp__unimatrix__context_retrospective` → `context_cycle_review` |
| `.claude/protocols/uni/uni-agent-routing.md` | Update one reference |
| `packages/unimatrix/skills/retro/SKILL.md` | Update reference |
| `product/workflow/base-001/protocol-evolved/uni-agent-routing.md` | Update reference |
| `product/PRODUCT-VISION.md` | Update `context_retrospective` references (lines 32, 43, 282, 819) |
| `README.md` | Update tool table row |
| `product/ALPHA_UNIMATRIX_COMPLETED_VISION.md` | Update references |
| `CLAUDE.md` | Update tool name in tool list if present |

All `unimatrix-engine` test files calling `compute_confidence` or `freshness_score` (~15 call sites) must be migrated to accept `&ConfidenceParams`. Use `..Default::default()` struct update syntax for tests that override a single field.

---

## Data Structures

### UnimatrixConfig (new — `unimatrix-server/src/infra/config.rs`)

```rust
pub struct UnimatrixConfig {
    pub profile:    ProfileConfig,     // [profile] — preset selection
    pub knowledge:  KnowledgeConfig,   // [knowledge] — categories, freshness
    pub server:     ServerConfig,      // [server] — instructions string
    pub agents:     AgentsConfig,      // [agents] — trust, capabilities
    pub confidence: ConfidenceConfig,  // [confidence] — weights (custom preset only)
}
// CycleConfig is removed — never active; vocabulary fix is a hardcoded rename.

pub struct ProfileConfig {
    pub preset: Preset,  // default: Preset::Collaborative
}

pub struct KnowledgeConfig {
    pub categories:                Vec<String>,   // default: INITIAL_CATEGORIES (8 values)
    pub boosted_categories:        Vec<String>,   // default: ["lesson-learned"]
    pub freshness_half_life_hours: Option<f64>,   // None = use preset's built-in value
}

pub struct ServerConfig {
    pub instructions: Option<String>,  // None = use SERVER_INSTRUCTIONS compiled default
}

pub struct AgentsConfig {
    pub default_trust:        String,        // "permissive" | "strict", default: "permissive"
    pub session_capabilities: Vec<String>,   // ["Read", "Write", "Search"] default
}

pub struct ConfidenceConfig {
    pub weights: Option<ConfidenceWeights>,  // Required when preset = "custom"; ignored otherwise
}

pub struct ConfidenceWeights {
    pub base:  f64,
    pub usage: f64,
    pub fresh: f64,
    pub help:  f64,
    pub corr:  f64,
    pub trust: f64,
}
```

### Preset (new — `unimatrix-server/src/infra/config.rs`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Preset {
    Authoritative,
    Operational,
    Empirical,
    Collaborative,  // default — equals ConfidenceParams::default() exactly (SR-10 invariant)
    Custom,
}

impl Default for Preset {
    fn default() -> Self { Preset::Collaborative }
}
```

### ConfidenceParams (extended — `unimatrix-engine/src/confidence.rs`)

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct ConfidenceParams {
    // Six weight fields — sum must equal 0.92 exactly (ADR-005 invariant).
    pub w_base:  f64,  // Weight: base quality. Default: 0.16
    pub w_usage: f64,  // Weight: usage frequency. Default: 0.16
    pub w_fresh: f64,  // Weight: freshness (recency). Default: 0.18
    pub w_help:  f64,  // Weight: helpfulness (Bayesian posterior). Default: 0.12
    pub w_corr:  f64,  // Weight: correction chain quality. Default: 0.14
    pub w_trust: f64,  // Weight: creator trust level. Default: 0.16
    // Freshness and Bayesian prior parameters.
    pub freshness_half_life_hours: f64,  // Default: 168.0
    pub alpha0: f64,                     // Default: COLD_START_ALPHA (3.0)
    pub beta0:  f64,                     // Default: COLD_START_BETA  (3.0)
}
// Default reproduces compiled constants exactly — no behavioral change when no config present.
// W3-1 will add Option<LearnedWeights> without changing any call site using Default.
```

### ConfigError (new — `unimatrix-server/src/infra/config.rs`)

Required variants (exact names used in tests and error messages):
`FileTooLarge`, `WorldWritable`, `MalformedToml`, `InvalidCategoryChar`, `TooManyCategories`,
`InvalidCategoryLength`, `BoostedCategoryNotInAllowlist`, `InvalidHalfLifeValue`,
`HalfLifeOutOfRange`, `InstructionsTooLong`, `InstructionsInjection`, `InvalidDefaultTrust`,
`InvalidSessionCapability`, `CustomPresetMissingWeights`, `CustomPresetMissingHalfLife`,
`CustomWeightOutOfRange`, `CustomWeightSumInvariant`.

All `Display` implementations must include: (a) the file path, (b) the specific field or constraint violated, (c) valid values or range where applicable.

---

## Preset Weight Table (authoritative — ADR-005)

| Preset | w_base | w_usage | w_fresh | w_help | w_corr | w_trust | SUM  | half_life_h |
|--------|--------|---------|---------|--------|--------|---------|------|-------------|
| `collaborative` | 0.16 | 0.16 | 0.18 | 0.12 | 0.14 | 0.16 | 0.92 | 168.0 |
| `authoritative` | 0.14 | 0.14 | 0.10 | 0.14 | 0.18 | 0.22 | 0.92 | 8760.0 |
| `operational`   | 0.14 | 0.18 | 0.24 | 0.08 | 0.18 | 0.10 | 0.92 | 720.0 |
| `empirical`     | 0.12 | 0.16 | 0.34 | 0.04 | 0.06 | 0.20 | 0.92 | 24.0 |

Weight sum invariant: `(sum - 0.92).abs() < 1e-9` — NOT `sum <= 1.0`. The SCOPE.md config schema comment (`sum must be <= 1.0`) is incorrect; ADR-005 governs all validation code and tests.

`collaborative` row must equal `ConfidenceParams::default()` exactly — enforced by the mandatory SR-10 test.

---

## Function Signatures

### New in `unimatrix-server/src/infra/config.rs`

```rust
// Reads, size-caps, permission-checks, validates, and two-level-merges config.
// ContentScanner::global() MUST be called at the top (ordering invariant for scan_title()).
pub fn load_config(home_dir: &Path, data_dir: &Path) -> Result<UnimatrixConfig, ConfigError>

// Post-parse field validation for a single config file. Independently testable
// (no tokio, no store, no scanner dependency beyond ContentScanner::global()).
pub fn validate_config(config: &UnimatrixConfig, path: &Path) -> Result<(), ConfigError>

// Single resolution site: converts preset + config into a fully-populated ConfidenceParams.
// Named preset: ADR-005 table + optional [knowledge] half_life override.
// Custom: [confidence] weights + required [knowledge] freshness_half_life_hours.
// Returns Err if custom preset is missing either required field.
pub fn resolve_confidence_params(config: &UnimatrixConfig) -> Result<ConfidenceParams, ConfigError>

// Constructs ConfidenceParams for a named preset from the ADR-005 weight table.
// Panics on Preset::Custom — that is a logic error; use resolve_confidence_params instead.
// Used by resolve_confidence_params internally and by the SR-10 mandatory test.
pub fn confidence_params_from_preset(preset: Preset) -> ConfidenceParams

// Merges global and per-project configs field-by-field (replace semantics per ADR-003).
fn merge_configs(global: UnimatrixConfig, project: UnimatrixConfig) -> UnimatrixConfig

// Unix-only: checks file permissions. World-writable returns Err; group-writable logs warn.
#[cfg(unix)]
fn check_permissions(path: &Path) -> Result<(), ConfigError>
```

### Changed in `unimatrix-engine/src/confidence.rs`

```rust
// params.w_* replace compiled weight constants.
pub fn compute_confidence(entry: &EntryRecord, now: u64, params: &ConfidenceParams) -> f64

// params.freshness_half_life_hours replaces FRESHNESS_HALF_LIFE_HOURS const.
pub fn freshness_score(last_accessed_at: u64, created_at: u64, now: u64,
                       params: &ConfidenceParams) -> f64
```

### Changed in `unimatrix-server/src/infra/categories.rs`

```rust
// New constructor — seeds allowlist from supplied list (called from main.rs after config load).
pub fn from_categories(cats: Vec<String>) -> Self

// Unchanged signature — delegates to from_categories(INITIAL_CATEGORIES.to_vec()).
// All existing test call sites remain valid.
pub fn new() -> Self
```

### Changed in `unimatrix-store/src/registry.rs`

```rust
// Added third parameter. All existing call sites pass None to preserve current behavior.
pub async fn agent_resolve_or_enroll(
    &self,
    agent_id: &str,
    permissive: bool,
    session_caps: Option<&[Capability]>,  // Some → use provided caps; None → permissive/strict branch
) -> Result<AgentRecord>
```

---

## `freshness_half_life_hours` Precedence Chain (ADR-006)

| `[profile] preset` | `[knowledge] freshness_half_life_hours` | Effective value |
|--------------------|----------------------------------------|-----------------|
| named (non-custom) | absent (`None`) | Preset's built-in value from table above |
| named (non-custom) | present (`Some(v)`) | `[knowledge]` value (operator override) |
| `custom` | absent (`None`) | **Startup abort** — `CustomPresetMissingHalfLife` |
| `custom` | present (`Some(v)`) | `[knowledge]` value |

Single resolution site: `resolve_confidence_params()` in `config.rs`. No other code determines which `freshness_half_life_hours` to use.

---

## Startup Sequence (After dsn-001)

```
tokio_main_daemon / tokio_main_stdio:
  1. initialize tracing
  2. ensure_data_directory() → paths
  3. load_config(home_dir, &paths.data_dir) → config              ← NEW
       a. ContentScanner::global() warm (ordering guard for scan_title)
       b. check permissions (unix only) for global config file
       c. read + size-cap (≤ 64 KB) global file
       d. deserialize + validate_config for global
       e. repeat b–d for per-project config
       f. merge_configs(global, project) → final UnimatrixConfig
  4. resolve_confidence_params(&config) → ConfidenceParams        ← NEW
  5. open_store_with_retry()
  6. CategoryAllowlist::from_categories(config.knowledge.categories)    ← CHANGED
  7. AgentRegistry::new(store, config.agents.permissive)                ← CHANGED
  8. UnimatrixServer::new(..., config.server.instructions)              ← CHANGED
  9. SearchService constructed with boosted_categories HashSet          ← CHANGED
 10. background tick spawned with Arc<ConfidenceParams>                ← CHANGED

NOT involved: Command::Hook (sub-50ms sync budget), tokio_main_bridge,
              export/import subcommands.
```

---

## Constraints

1. **`toml = "0.8"` pinned** — added to `unimatrix-server/Cargo.toml` only. Run `cargo tree` after adding to confirm no version conflicts.
2. **Weight sum invariant is `(sum - 0.92).abs() < 1e-9`** — not `sum <= 1.0`. SCOPE.md config schema comment is incorrect. ADR-005 governs all validation code and tests.
3. **`confidence_params_from_preset(Preset::Custom)` panics by design** — only `resolve_confidence_params` handles the `Custom` path. Code review must ensure no direct call with `Custom` exists outside `resolve_confidence_params`.
4. **`CategoryAllowlist::new()` must remain valid** — all existing test call sites use `new()`. Must not change signature; delegates to `from_categories(INITIAL_CATEGORIES.to_vec())`.
5. **`agent_resolve_or_enroll` third parameter defaults to `None`** — all existing call sites gain `None` as the third arg. No store-level refactor beyond the parameter addition.
6. **File permission check is `#[cfg(unix)]` only** — `std::os::unix::fs::PermissionsExt` not available on Windows. Feature compiles and runs on Windows without this check.
7. **`dirs::home_dir()` returning `None` must not panic** — degrade to compiled defaults with `tracing::warn!` and continue.
8. **`ContentScanner::global()` called at top of `load_config`** — explicit warm call before any `validate_config` calls `scan_title()`. Must include a code comment explaining the ordering invariant.
9. **Hook path and bridge mode excluded** — `Command::Hook` is sync sub-50ms; `tokio_main_bridge` does not run server subsystems.
10. **No `Arc<UnimatrixConfig>` crosses crate boundaries** — only plain primitive values: `bool`, `Vec<String>`, `HashSet<String>`, `Vec<Capability>`, `ConfidenceParams`.
11. **`CycleConfig` is removed from `UnimatrixConfig`** — never active; vocabulary fix is a hardcoded rename.
12. **Per-project `custom` preset requires per-project `[confidence] weights`** — cross-level weight inheritance prohibited per ADR-003. Global weights not inherited.
13. **`context_retrospective` rename blast radius** — build passing is necessary but not sufficient. Non-Rust files (protocols, skills, Python tests, product docs) must be audited. See SPECIFICATION.md §SR-05 for the exhaustive 31-location checklist across 14 files.
14. **`[knowledge] freshness_half_life_hours` uses `Option<f64>`** — `None` = absent from TOML (use preset's built-in); `Some(v)` = operator supplied value. Avoids false-positive merge detection.
15. **`rmcp = "=0.16.0"` is not changed** — no rmcp API modified.
16. **`validate_config()` must be independently testable** — no tokio, no store, no embedded server required. Only `ContentScanner::global()` is an external dependency (warmed before test calls).

---

## Dependencies

| Dependency | Location | Version | Notes |
|------------|----------|---------|-------|
| `toml` | `unimatrix-server/Cargo.toml` | `"0.8"` (pinned) | New. Adds serde-based TOML parsing. |
| `serde` | already present | existing | `Deserialize` derive on config structs. |
| `dirs` | already present in `unimatrix-server` | existing | `dirs::home_dir()` for `~/.unimatrix/` resolution. |
| `ContentScanner` | `unimatrix-server/src/infra/scanning.rs` | internal | `scan_title()` for `[server] instructions` validation. Must be warmed at top of `load_config`. |
| `ConfidenceParams` | `unimatrix-engine/src/confidence.rs` | internal | Extended with six weight fields (ADR-001). |
| `CategoryAllowlist` | `unimatrix-server/src/infra/categories.rs` | internal | New `from_categories` constructor (ADR-002). |
| `AgentRegistry` | `unimatrix-server/src/infra/registry.rs` | internal | `new(store, permissive: bool)` — adds permissive param from config. |
| `SqlxStore::agent_resolve_or_enroll` | `unimatrix-store/src/registry.rs` | internal | Adds third param `session_caps: Option<&[Capability]>`. |
| `SearchService` | `unimatrix-server/src/services/search.rs` | internal | `boosted_categories: HashSet<String>` replaces hardcoded comparison. |
| `rmcp` | `unimatrix-server/Cargo.toml` | `=0.16.0` (pinned) | Unchanged. No API change. |

---

## NOT in Scope

- Runtime config reload — config is loaded once at startup; changes require restart.
- `UNIMATRIX_CONFIG` env var — no env var override for global config path.
- Per-session or per-agent config — config is global to the server instance.
- Schema migration — no new DB tables, no schema version bump.
- OAuth / authentication config — deferred per W0-2 deferral.
- Config tooling — no `unimatrix config show`, no `--validate` flag.
- Domain packs — dsn-001 provides hook points; domain pack loading is a separate feature.
- Coherence gate lambda weights (`confidence_freshness`, `graph_quality`, `embedding_consistency`, `contradiction_density`) — these are KB-health metric weights for `compute_lambda()`. Remain hardcoded in `coherence.rs`.
- Full bootstrap agent list configurability (`system`, `human`, `cortical-implant`) — only `default_trust` and `session_capabilities` externalised.
- Renaming `context_cycle` — already domain-neutral.
- Externalizing `PROVENANCE_BOOST` magnitude (0.02) — only which categories receive the boost is configurable.
- Adaptive blend weight parameters (`observed_spread * 1.25`, clamp bounds `[0.15, 0.25]`) — part of the crt-019 adaptive system, not static config.
- Bridge mode config — `tokio_main_bridge` does not load config.
- Hook path config — `Command::Hook` is sync sub-50ms.

---

## Mandatory Pre-PR Gates

These gates are required in addition to `cargo build` and `cargo test --all` passing:

1. **SR-10 test present** with exact comment text `"SR-10: If this test fails, fix the weight table, not the test."` in `unimatrix-server`:
   ```rust
   assert_eq!(confidence_params_from_preset(Preset::Collaborative), ConfidenceParams::default());
   ```
2. **`grep -r "context_retrospective" .`** at repo root returns zero matches outside the historically-excluded directories listed in SPECIFICATION.md §SR-05.
3. **All four AC-25 freshness precedence cases** have named unit tests (named preset + absent, named preset + present, custom + absent, custom + present).
4. **Weight sum validation uses `(sum - 0.92).abs() < 1e-9`** — confirmed by a test asserting `custom` weights summing to `0.95` are rejected.
5. **Named preset immunity to `[confidence]`** — test confirms `[confidence] weights` values have no effect when `preset != "custom"`.

---

## Alignment Status

**Overall: WARN — one documentation-level namespace divergence; no delivery blockers.**

All three prior WARNs (VARIANCE-1, VARIANCE-2, VARIANCE-3) are CLOSED by the revised design:

| Prior Variance | Resolution | Status |
|---------------|------------|--------|
| VARIANCE-1: Confidence weights not in `ConfidenceParams` — W3-1 cold-start path broken | Preset system extends `ConfidenceParams` to 9 fields (ADR-001); `resolve_confidence_params()` populates all six `w_*` fields; AC-27 mandates W3-1 reads this struct directly. W3-1 cold-start prerequisite fully satisfied. | CLOSED |
| VARIANCE-2: `[cycle]` label doc-fix accepted as sufficient | Confirmed: `context_retrospective` → `context_cycle_review` hardcoded rename (FR-11) and `CycleParams.topic` doc neutralization (FR-12) delivered. `CycleConfig` stub removed per ADR-004 update. | CLOSED |
| VARIANCE-3: `default_trust = "permissive"` default correctness | Confirmed as correct default consistent with W0-2 deferral rationale; source documents are internally consistent. | CLOSED |

One remaining WARN (documentation only, not a delivery blocker):

**WARN-1: `[confidence]` TOML key semantic divergence from vision's W0-3 example.**
Vision's W0-3 config block shows lambda/coherence-gate weights (`freshness=0.35, graph=0.30, contradiction=0.20, embedding=0.15`). Source docs' `[confidence] weights` carries the six confidence scoring factor weights (`w_base`, `w_usage`, etc.). The source documents are more correct for W3-1 cold-start. Recommendation: correct the vision's W0-3 config block to match source docs; lambda weight externalization (if needed) targets `[coherence]` in a future feature. This is a documentation correction to the vision only.

Vision Critical Gaps addressed by this feature:
- Freshness half-life hardcoded at 168h — ADDRESSED (FR-07, AC-04)
- `lesson-learned` category name hardcoded in scoring — ADDRESSED (FR-06, AC-03)
- `SERVER_INSTRUCTIONS` const uses dev-workflow language — ADDRESSED (FR-08, AC-05)
- Initial category allowlist hardcoded — ADDRESSED (FR-05, AC-02)
- `context_retrospective` tool name is SDLC-specific — ADDRESSED (FR-11, AC-13)
- Confidence weights hardcoded, cannot adapt to domain — ADDRESSED via preset system (FR-03/FR-04, AC-22–27)

---

## Tracking

https://github.com/dug-21/unimatrix/issues/306
