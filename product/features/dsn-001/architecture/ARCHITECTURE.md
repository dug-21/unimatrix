# Architecture: dsn-001 — Config Externalization (W0-3) — Revised (Preset System)

## Decisions

| ADR | Title | File | Unimatrix ID |
|-----|-------|------|--------------|
| ADR-001 | ConfidenceParams Struct — Extended with Six Weight Fields | `ADR-001-confidence-params-struct.md` | #2284 (updated) |
| ADR-002 | Config Type Placement — unimatrix-server owns UnimatrixConfig | `ADR-002-config-type-placement.md` | #2285 (updated) |
| ADR-003 | Two-Level Config Merge — Replace Semantics | `ADR-003-two-level-config-merge.md` | #2286 (updated) |
| ADR-004 | [confidence] Section Promoted from Stub to Live | `ADR-004-forward-compat-stubs.md` | #2287 (updated) |
| ADR-005 | Preset Enum Design and Weight Table | `ADR-005-preset-enum-and-weights.md` | TBD (new) |
| ADR-006 | Preset Resolution Pipeline | `ADR-006-preset-resolution-pipeline.md` | TBD (new) |

## System Overview

dsn-001 replaces five categories of hardcoded constants in the `unimatrix-server`
binary with values loaded from a TOML config file at startup. The preset system —
added after the first design pass — is the primary interface for confidence
configuration: operators identify their knowledge lifecycle type, not ML weights.

The feature adds a `[profile]` section with four named presets (`authoritative`,
`operational`, `empirical`, `collaborative`) and a `custom` escape hatch. The
`collaborative` preset reproduces the compiled defaults exactly, ensuring no
behavioral change when no config file is present.

Config is loaded once at startup, validated immediately, and resolved into concrete
values distributed to subsystems. No request-handling path changes; no DB schema
changes; no new MCP tools. Two hardcoded vocabulary fixes accompany the config work:
`context_retrospective` → `context_cycle_review`, and the `CycleParams.topic` field
doc is neutralised.

After dsn-001, Unimatrix can be deployed for non-software-delivery domains — legal,
SRE, environmental monitoring, scientific research — by supplying a
`~/.unimatrix/config.toml` without recompiling.

## Component Breakdown

### 1. Config loader (`unimatrix-server/src/infra/config.rs`) — new file

Owns the entire config lifecycle:

- `UnimatrixConfig` struct with five sub-structs:
  `ProfileConfig`, `KnowledgeConfig`, `ServerConfig`, `AgentsConfig`,
  `ConfidenceConfig`
- `Preset` enum: `Authoritative | Operational | Empirical | Collaborative | Custom`
- `load_config(home_dir, data_dir) -> Result<UnimatrixConfig, ConfigError>`
  — reads, size-caps, validates, and merges global + per-project configs
- `validate_config(config, path) -> Result<(), ConfigError>`
  — post-parse field validation; includes `custom` preset weight sum check
- `resolve_confidence_params(config) -> Result<ConfidenceParams, ConfigError>`
  — single site that converts preset selection into a populated `ConfidenceParams`
  with all six weight fields + freshness_half_life_hours (see ADR-006)
- `confidence_params_from_preset(preset) -> ConfidenceParams`
  — helper that looks up the ADR-005 weight table for named presets
- File permission check (`#[cfg(unix)]`); 64 KB size cap before TOML parse

This component has no runtime state: it produces values and is done.

### 2. ConfidenceParams (changes to `unimatrix-engine/src/confidence.rs`)

Extended from 3 fields to 9 fields (see ADR-001):

```rust
pub struct ConfidenceParams {
    pub w_base:  f64,
    pub w_usage: f64,
    pub w_fresh: f64,
    pub w_help:  f64,
    pub w_corr:  f64,
    pub w_trust: f64,
    pub freshness_half_life_hours: f64,
    pub alpha0: f64,
    pub beta0:  f64,
}
```

`Default` returns the compiled constants — reproduces pre-dsn-001 behavior exactly.
`compute_confidence` uses `params.w_*` instead of the compiled constants.
`freshness_score` uses `params.freshness_half_life_hours`.

### 3. Config distribution (changes to `tokio_main_daemon` and `tokio_main_stdio`)

Inserts config load and confidence param resolution immediately after
`ensure_data_directory()` returns `paths`:

```rust
let config = load_config(home_dir, &paths.data_dir)?;
let confidence_params = Arc::new(resolve_confidence_params(&config)?);
```

Extracts concrete values and passes to subsystem constructors as plain parameters.
Does not store `Arc<UnimatrixConfig>` on any struct.

### 4. CategoryAllowlist (changes to `unimatrix-server/src/infra/categories.rs`)

New constructor: `CategoryAllowlist::from_categories(cats: Vec<String>) -> Self`.
Existing `CategoryAllowlist::new()` delegates to `from_categories(INITIAL_CATEGORIES.iter()...)`.
All existing tests remain valid (SR-07 resolved).

### 5. SearchService (changes to `unimatrix-server/src/services/search.rs`)

Replaces the four hardcoded `entry.category == "lesson-learned"` comparisons with a
`HashSet<String>` lookup against a `boosted_categories` field. The `HashSet` is
constructed from `config.knowledge.boosted_categories` at `SearchService` construction.

### 6. AgentRegistry (changes to `unimatrix-server/src/infra/registry.rs`)

Replaces `const PERMISSIVE_AUTO_ENROLL: bool = true` with a value passed into
`AgentRegistry::new(store, permissive: bool)`.

The `session_capabilities: Vec<Capability>` from config is passed as a plain
`Vec<Capability>` parameter to `agent_resolve_or_enroll` when a new agent is
auto-enrolled (see Integration Surface for the updated signature).

### 7. UnimatrixServer (changes to `unimatrix-server/src/server.rs`)

Replaces `const SERVER_INSTRUCTIONS: &str = "..."` with the value from
`config.server.instructions`.

### 8. Background tick (changes to `unimatrix-server/src/background.rs`)

Receives `Arc<ConfidenceParams>` at spawn time. Uses it in all
`compute_confidence(entry, now, &params)` calls. The params are fixed at startup —
not reloaded per tick.

### 9. Tool vocabulary fixes (`unimatrix-server/src/mcp/tools.rs`)

- Rename `context_retrospective` → `context_cycle_review` in `#[tool(name = "...")]`
  and handler function name.
- Update `CycleParams.topic` field doc to domain-agnostic language.
- Blast-radius fix: all non-Rust references (protocols, skills, tests, CLAUDE.md)
  must be updated in the same PR.

## Component Interactions

```
main.rs
  └─ ensure_data_directory() → ProjectPaths
  └─ load_config(home_dir, data_dir) → UnimatrixConfig           [NEW]
       ├─ reads ~/.unimatrix/config.toml
       ├─ reads ~/.unimatrix/{hash}/config.toml
       ├─ validates both (including preset/weights combination)
       └─ merges (per-project fields win over global)
  └─ resolve_confidence_params(&config) → ConfidenceParams       [NEW]
       ├─ Collaborative → ConfidenceParams::default()
       ├─ Authoritative|Operational|Empirical → weight table + optional half_life
       └─ Custom → [confidence].weights + [knowledge].freshness_half_life_hours
  └─ extracts values:
       ├─ categories: Vec<String>           → CategoryAllowlist::from_categories()
       ├─ boosted_categories: HashSet       → SearchService field
       ├─ Arc<ConfidenceParams>             → background tick
       ├─ instructions: String             → UnimatrixServer::new()
       ├─ permissive: bool                 → AgentRegistry::new()
       └─ session_caps: Vec<Capability>    → agent_resolve_or_enroll()
```

Config values do not flow back from subsystems to the config loader. The flow is
one-directional at startup.

## Preset Weight Table

The authoritative weight values. All rows sum to exactly 0.92.

| Preset | w_base | w_usage | w_fresh | w_help | w_corr | w_trust | SUM  | half_life |
|--------|--------|---------|---------|--------|--------|---------|------|-----------|
| `collaborative` | 0.16 | 0.16 | 0.18 | 0.12 | 0.14 | 0.16 | 0.92 | 168.0h |
| `authoritative` | 0.14 | 0.14 | 0.10 | 0.14 | 0.18 | 0.22 | 0.92 | 8760.0h |
| `operational`   | 0.14 | 0.18 | 0.24 | 0.08 | 0.18 | 0.10 | 0.92 | 720.0h |
| `empirical`     | 0.12 | 0.16 | 0.34 | 0.04 | 0.06 | 0.20 | 0.92 | 24.0h |

`collaborative` = current compiled defaults. The `collaborative` preset and
`ConfidenceParams::default()` must be equal (SR-10 test, mandatory in delivery).

`custom` has no built-in weights; all six are supplied by `[confidence] weights`.

## `freshness_half_life_hours` Precedence Chain

| Preset | `[knowledge]` override | Effective value |
|--------|----------------------|-----------------|
| named (non-custom) | absent | Preset's built-in value (from table above) |
| named (non-custom) | present | `[knowledge]` value (operator override) |
| `custom` | absent | **Startup abort** — required for custom |
| `custom` | present | `[knowledge]` value |

Single resolution site: `resolve_confidence_params()` in `config.rs`. No other
code decides which `freshness_half_life_hours` to use.

## Technology Decisions

### TOML parsing: `toml = "0.8"`

Standard Rust TOML parser with serde integration. Added to `unimatrix-server/Cargo.toml`
only. Pinned as `toml = "0.8"` (not `^`) to surface version conflicts early.

### `Preset` enum with `#[serde(rename_all = "lowercase")]`

TOML string values (`"authoritative"`, etc.) deserialize directly to enum variants.
Invalid strings abort deserialization before `validate_config` runs. The `Default`
impl returns `Preset::Collaborative` so an absent `[profile]` section produces the
`collaborative` preset.

### `Option<f64>` for `freshness_half_life_hours`

Type-level distinction between "not specified" (None → use preset's built-in) and
"specified as zero" (Some(0.0) → startup abort via validation). Avoids false-positive
merge detection if the field used a numeric default.

### `ConfidenceParams` struct (ADR-001, extended)

Nine-field struct in `unimatrix-engine`. `Default` reproduces compiled constants.
W3-1 adds `Option<LearnedWeights>` to this struct without changing any call site
using `Default`. All six weights flow directly into `compute_confidence`.

### Config placement in `unimatrix-server` (ADR-002)

Single-crate placement; no circular dependencies; `toml` dependency contained to
server crate; `Preset` and resolution logic co-locate with `UnimatrixConfig`.

### Merge strategy: replace semantics (ADR-003)

Per-project section replaces global section field-by-field. `custom` preset with
no per-project `[confidence] weights` does not inherit global weights — each level
is self-contained to prevent implicit cross-level composition.

### `[confidence]` promoted to live section (ADR-004)

Only active when `preset = "custom"`. Ignored for all named presets even if present.
`CycleConfig` stub removed from `UnimatrixConfig` (never active; doc fix is hardcoded).

## Integration Points

### `unimatrix-engine` crate (changed public API)

`ConfidenceParams` gains six new weight fields. `compute_confidence` and
`freshness_score` signatures change. All callers inside `unimatrix-engine` (tests)
and outside (server background tick, explicit confidence refresh) must be updated.
The compiled weight constants remain public as `Default` backing values.

### `unimatrix-store` crate (changed method signature)

`SqlxStore::agent_resolve_or_enroll(agent_id, permissive)` gains a third parameter:
`session_caps: Option<&[Capability]>`. When `Some`, the provided capability set is
used instead of the derived permissive/strict defaults. When `None`, existing
behavior is preserved. All existing call sites pass `None`.

### `CategoryAllowlist` constructor

`CategoryAllowlist::new()` is preserved. `from_categories(Vec<String>)` is new.
All existing test call sites continue using `new()` unchanged.

### `ContentScanner` ordering

`ContentScanner::global()` must be initialized before `validate_config()` calls
`scan_title()`. Place an explicit `let _scanner = ContentScanner::global();` at
the top of `load_config` to warm the singleton. Document this ordering constraint
in a code comment.

## Integration Surface

| Integration Point | Type / Signature | Source |
|---|---|---|
| `ConfidenceParams` | `pub struct ConfidenceParams { w_base, w_usage, w_fresh, w_help, w_corr, w_trust: f64; freshness_half_life_hours, alpha0, beta0: f64 }` | `unimatrix-engine/src/confidence.rs` (extended) |
| `ConfidenceParams::default()` | `w_base:0.16, w_usage:0.16, w_fresh:0.18, w_help:0.12, w_corr:0.14, w_trust:0.16, freshness_half_life_hours:168.0, alpha0:3.0, beta0:3.0` | `unimatrix-engine/src/confidence.rs` |
| `compute_confidence(entry, now, params)` | `(&EntryRecord, u64, &ConfidenceParams) -> f64` | `unimatrix-engine/src/confidence.rs` (changed) |
| `freshness_score(last, created, now, params)` | `(u64, u64, u64, &ConfidenceParams) -> f64` | `unimatrix-engine/src/confidence.rs` (changed) |
| `Preset` | `pub enum Preset { Authoritative, Operational, Empirical, Collaborative, Custom }` | `unimatrix-server/src/infra/config.rs` (new) |
| `load_config(home_dir, data_dir)` | `(&Path, &Path) -> Result<UnimatrixConfig, ConfigError>` | `unimatrix-server/src/infra/config.rs` (new) |
| `resolve_confidence_params(config)` | `(&UnimatrixConfig) -> Result<ConfidenceParams, ConfigError>` | `unimatrix-server/src/infra/config.rs` (new) |
| `confidence_params_from_preset(preset)` | `(Preset) -> ConfidenceParams` | `unimatrix-server/src/infra/config.rs` (new) |
| `UnimatrixConfig` | `pub struct` with `profile, knowledge, server, agents, confidence` fields | `unimatrix-server/src/infra/config.rs` (new) |
| `CategoryAllowlist::from_categories(cats)` | `(Vec<String>) -> Self` | `unimatrix-server/src/infra/categories.rs` (new) |
| `CategoryAllowlist::new()` | `() -> Self` — delegates to `from_categories(INITIAL_CATEGORIES)` | `unimatrix-server/src/infra/categories.rs` (unchanged) |
| `AgentRegistry::new(store, permissive)` | `(Arc<SqlxStore>, bool) -> Result<Self>` | `unimatrix-server/src/infra/registry.rs` (changed: adds `permissive` param) |
| `SqlxStore::agent_resolve_or_enroll(id, permissive, session_caps)` | `(&str, bool, Option<&[Capability]>) -> Result<AgentRecord>` | `unimatrix-store/src/registry.rs` (changed: adds `session_caps` param) |
| `SearchService.boosted_categories` | `HashSet<String>` | `unimatrix-server/src/services/search.rs` (changed) |
| `context_cycle_review` tool name | Renamed from `context_retrospective` | `unimatrix-server/src/mcp/tools.rs` |

## Startup Sequence (After dsn-001)

```
main() / tokio_main_daemon:
  1. Initialize tracing
  2. ensure_data_directory() → paths
  3. load_config(home_dir, paths.data_dir) → config              ← NEW
     a. ContentScanner::global() warm (ordering guard for scan_title)
     b. Check global config file permissions (#[cfg(unix)])
     c. Read and size-cap global file (≤64 KB)
     d. Deserialize global → UnimatrixConfig (serde defaults fill gaps)
     e. validate_config(&global, global_path)
     f. Repeat b–e for per-project config
     g. merge(global, project) → final config
  4. resolve_confidence_params(&config) → confidence_params      ← NEW
     (Preset → ConfidenceParams with all 6 weights + half_life)
  5. open_store_with_retry()
  6. CategoryAllowlist::from_categories(config.knowledge.categories)   ← CHANGED
  7. AgentRegistry::new(store, config.agents.permissive)               ← CHANGED
  8. UnimatrixServer::new(..., config.server.instructions)             ← CHANGED
  9. SearchService constructed with boosted_categories HashSet         ← CHANGED
 10. Background tick spawned with Arc<ConfidenceParams>               ← CHANGED
```

## Open Questions

None. All risks from SCOPE-RISK-ASSESSMENT.md are resolved:

- **SR-02** (ConfidenceParams missing six weights): resolved by ADR-001 extended
  struct and ADR-006 resolution pipeline.
- **SR-09** (exact preset values): resolved by ADR-005 weight table.
- **SR-10** (collaborative = default): resolved by ADR-005 + mandatory SR-10 test.
- **SR-11** (freshness_half_life_hours precedence): resolved by ADR-006 single
  resolution site with explicit precedence chain.
- **SR-12** (`[confidence]` stub promoted to live): resolved by ADR-004 updated.
- **SR-13** (W3-1 unblocked by ConfidenceParams): resolved by ADR-001 + ADR-006.
- **SR-03** (ContentScanner ordering): call `ContentScanner::global()` at top of
  `load_config`; document the ordering invariant in code comment.
- **SR-04** (forward-compat): `ConfidenceConfig` now live; `CycleConfig` stub removed.
- **SR-06** (merge semantics for lists): resolved by ADR-003 replace semantics.
- **SR-07** (CategoryAllowlist constructor split): `new()` delegates to `from_categories`.
- **SR-08** (crate boundary): plain parameters only across crate boundaries (ADR-002).

## Constraints for Delivery Team

1. `toml = "0.8"` added to `unimatrix-server/Cargo.toml` only. Run `cargo tree`
   after adding to confirm no version conflicts.

2. `ConfidenceParams` gains six new fields. All existing call sites become
   `compute_confidence(entry, now, &ConfidenceParams::default())`. Use
   `{ ..Default::default() }` struct update syntax for tests overriding one field.

3. The weight sum invariant is `(sum - 0.92).abs() < 1e-9`, NOT `sum <= 1.0`.
   The SCOPE.md config schema comment says `≤ 1.0` — this is wrong. Use 0.92.

4. The SR-10 test is mandatory:
   `assert_eq!(confidence_params_from_preset(Preset::Collaborative), ConfidenceParams::default())`
   This must be added to `unimatrix-server` tests before the PR opens.

5. `from_preset(Custom)` panics — it is a logic error. Only `resolve_confidence_params`
   handles `Custom`. No direct call to `confidence_params_from_preset(Preset::Custom)`.

6. `agent_resolve_or_enroll` signature in `unimatrix-store` gains a third parameter.
   All existing call sites pass `None` to preserve current behavior.

7. The `context_retrospective` rename blast radius spans Rust source,
   `unimatrix-observe` types, protocol files, skill files, research docs, and
   CLAUDE.md. Build passing is necessary but not sufficient — all non-Rust files
   must be audited before the PR opens.

8. File permission check is `#[cfg(unix)]` only. No behavior change on Windows.

9. `dirs::home_dir()` returning `None` must not panic — degrade to compiled defaults
   with a tracing warning.

10. `ContentScanner::global()` must be called at the top of `load_config` before any
    `scan_title()` invocation in `validate_config`. Place an explicit warm call with
    a comment explaining the ordering invariant.

11. For `preset = "custom"`: both `[confidence] weights` AND `[knowledge]
    freshness_half_life_hours` are required. The error message must name the missing
    field explicitly.

12. When `preset != "custom"` but `[confidence] weights` is present, log a warning
    and continue (do not abort). The values are silently ignored.

13. All new validation paths require unit tests. `validate_config` must be
    independently testable — no tokio, no store, no scanner singleton dependency
    beyond what `ContentScanner::global()` provides.

14. `CycleConfig` is removed from `UnimatrixConfig`. If any prior code referenced it,
    remove those references.
