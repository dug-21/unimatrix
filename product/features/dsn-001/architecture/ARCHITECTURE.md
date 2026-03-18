# Architecture: dsn-001 — Config Externalization (W0-3)

## Decisions

| ADR | Title | File | Unimatrix ID |
|-----|-------|------|--------------|
| ADR-001 | ConfidenceParams Struct for Engine API | `ADR-001-confidence-params-struct.md` | #2284 |
| ADR-002 | Config Type Placement — unimatrix-server owns UnimatrixConfig | `ADR-002-config-type-placement.md` | #2285 |
| ADR-003 | Two-Level Config Merge — Replace Semantics | `ADR-003-two-level-config-merge.md` | #2286 |
| ADR-004 | Forward-Compatibility Stubs for [confidence] and [cycle] | `ADR-004-forward-compat-stubs.md` | #2287 |

## System Overview

dsn-001 replaces four categories of hardcoded constants in the `unimatrix-server`
binary with values loaded from a TOML config file at startup. It is a pure startup-path
change: config is loaded once, validated, and distributed as plain values to the
subsystems that need them. No request-handling path changes; no DB schema changes;
no new MCP tools.

The feature also performs two hardcoded vocabulary fixes: renaming the
`context_retrospective` MCP tool to `context_cycle_review`, and neutralising the
`CycleParams.topic` field doc.

After dsn-001, Unimatrix can be deployed for non-software-delivery domains by
supplying a `~/.unimatrix/config.toml` without recompiling.

## Component Breakdown

### 1. Config loader (`unimatrix-server/src/infra/config.rs`) — new file

Owns the entire config lifecycle:
- `UnimatrixConfig` struct and five sub-structs (`KnowledgeConfig`, `ServerConfig`,
  `AgentsConfig`, `ConfidenceConfig` stub, `CycleConfig` stub)
- `load_config(home_dir: &Path, data_dir: &Path) -> Result<UnimatrixConfig, ConfigError>`
  — reads, validates, and merges global + per-project configs
- `validate_config(config: &UnimatrixConfig, path: &Path) -> Result<(), ConfigError>`
  — post-parse field validation (categories, freshness bounds, instructions scan,
  agent trust allowlist)
- File permission check (`#[cfg(unix)]`)
- 64 KB file size cap before TOML parse

This component has no runtime state: it produces a value and is done.

### 2. Config distribution (changes to `tokio_main_daemon` and `tokio_main_stdio` in `main.rs`)

Inserts config load immediately after `ensure_data_directory()` returns `paths`.
Extracts concrete values from the loaded `UnimatrixConfig` and passes them to
subsystem constructors as plain parameters. Does not store `Arc<UnimatrixConfig>`
on any struct — config values are distributed at construction time and each
subsystem owns its own copy of the values it needs.

### 3. CategoryAllowlist (changes to `unimatrix-server/src/infra/categories.rs`)

New constructor: `CategoryAllowlist::from_categories(cats: Vec<String>) -> Self`.
Existing `CategoryAllowlist::new()` delegates to
`from_categories(INITIAL_CATEGORIES.iter().map(|s| s.to_string()).collect())`.
This preserves all existing tests (SR-07 resolved).

The loaded config's `knowledge.categories` is passed to `from_categories` at startup.
The `boosted_categories` `HashSet<String>` is held by `SearchService` (see §5).

### 4. ConfidenceParams (changes to `unimatrix-engine/src/confidence.rs`)

New struct `ConfidenceParams` with fields:
- `freshness_half_life_hours: f64`
- `alpha0: f64`
- `beta0: f64`

`Default` impl returns the current compiled constants. Replaces the three positional
parameters `(freshness_half_life_hours via const, alpha0, beta0)` in
`compute_confidence` and `freshness_score`. See ADR-001.

### 5. SearchService (changes to `unimatrix-server/src/services/search.rs`)

Replaces the four hardcoded `entry.category == "lesson-learned"` comparisons with a
`HashSet<String>` lookup against a `boosted_categories` field. The `HashSet` is
constructed from `config.knowledge.boosted_categories` at `SearchService` construction
and never mutated at runtime.

### 6. AgentRegistry (changes to `unimatrix-server/src/infra/registry.rs`)

Replaces `const PERMISSIVE_AUTO_ENROLL: bool = true` with a value passed into
`AgentRegistry::new(store, permissive: bool)`. The server extracts the bool from
`config.agents.default_trust == "permissive"` and passes it at construction.

The `session_capabilities: Vec<Capability>` from config is passed as a plain
`Vec<Capability>` to `agent_resolve_or_enroll` when a new agent is auto-enrolled.
`unimatrix-store`'s `agent_resolve_or_enroll(agent_id, permissive)` already accepts
a bool and derives capabilities internally; W0-3 extends this to
`agent_resolve_or_enroll(agent_id, permissive, session_caps: &[Capability])` so the
server can override the capability set from config. See Integration Surface below.

### 7. UnimatrixServer (changes to `unimatrix-server/src/server.rs`)

Replaces `const SERVER_INSTRUCTIONS: &str = "..."` with the value from
`config.server.instructions`. Passed into `UnimatrixServer::new()` as a parameter
(or stored on the server struct and used in `server_info` construction).

### 8. Tool vocabulary fixes (`unimatrix-server/src/mcp/tools.rs`)

- Rename `context_retrospective` → `context_cycle_review` in `#[tool(name = "...")]`
  and handler function name.
- Update `CycleParams.topic` field doc to domain-agnostic language.
- This is a blast-radius fix: all non-Rust references (protocols, skills, tests,
  CLAUDE.md) must be updated in the same PR.

## Component Interactions

```
main.rs
  └─ ensure_data_directory() → ProjectPaths
  └─ load_config(home_dir, data_dir) → UnimatrixConfig  [NEW]
       ├─ reads ~/.unimatrix/config.toml
       ├─ reads ~/.unimatrix/{hash}/config.toml
       ├─ validates both
       └─ merges (per-project fields win over global)
  └─ extracts values:
       ├─ categories: Vec<String>        → CategoryAllowlist::from_categories()
       ├─ boosted_categories: HashSet    → SearchService field
       ├─ freshness_half_life_hours: f64 → ConfidenceParams (built per tick call)
       ├─ instructions: String           → UnimatrixServer::new()
       ├─ permissive: bool               → AgentRegistry::new()
       └─ session_caps: Vec<Capability>  → agent_resolve_or_enroll()
```

Config values do not flow back from subsystems to the config loader. The flow is
one-directional at startup.

## Technology Decisions

### TOML parsing: `toml = "0.8"`

`toml = "0.8"` is the standard Rust TOML parser with serde integration. No
alternatives considered — it is the de facto standard and has no significant
transitive dependencies. Added to `unimatrix-server/Cargo.toml` only. Pinned
as `toml = "0.8"` (not `^`) per SR-01 to surface conflict before it manifests.

### serde deserialization with `#[serde(default)]`

All config sub-structs use `#[serde(default)]` so that absent TOML sections (or
absent fields within sections) silently inherit compiled defaults. This is the
same pattern used in `unimatrix-adapt` (crt-003, Unimatrix entry #651) for
`AdaptConfig` and `LearnConfig`. No new pattern.

### `ConfidenceParams` struct

See ADR-001. Chosen over bare parameter to absorb W3-1 API extension without
further engine churn.

### Config placement in `unimatrix-server`

See ADR-002. Single-crate placement; no circular dependencies; `toml` dependency
contained to server crate.

### Merge strategy: replace semantics

See ADR-003. Per-project section replaces global section field-by-field; list
fields replace entirely.

### Forward-compat stubs

See ADR-004. Empty `ConfidenceConfig` and `CycleConfig` reserve the TOML namespace
for W3-1.

## Integration Points

### `unimatrix-engine` crate (changed public API)

`ConfidenceParams` is a new public type in `unimatrix-engine/src/confidence.rs`.
`compute_confidence` and `freshness_score` signatures change. All callers inside
`unimatrix-engine` (tests) and outside (server background tick, explicit confidence
refresh) must be updated. The `FRESHNESS_HALF_LIFE_HOURS` constant remains public
as the default backing value.

### `unimatrix-store` crate (changed method signature)

`SqlxStore::agent_resolve_or_enroll(agent_id, permissive)` gains a third parameter:
`session_caps: Option<&[Capability]>`. When `Some`, the provided capability set is
used instead of the derived permissive/strict defaults. When `None`, existing
behavior is preserved. This preserves backward compatibility for all existing call
sites that pass `None`.

### `CategoryAllowlist` constructor

`CategoryAllowlist::new()` is preserved. `from_categories(Vec<String>)` is new.
All 15+ test call sites continue using `new()` unchanged.

### `ContentScanner` ordering (SR-03)

`ContentScanner::global()` is a startup singleton initialized via `once_cell`. It
must be initialized before `validate_config()` calls `scan_title()`. The natural
startup ordering in `tokio_main_daemon`/`tokio_main_stdio` has the tokio runtime
active before config load; `ContentScanner::global()` initializes on first call.
The implementation must call `ContentScanner::global()` before entering the
validation path (e.g., call it once at the top of `load_config` to warm the
singleton before the instructions field is validated). Document this ordering
constraint in code comments.

## Integration Surface

| Integration Point | Type / Signature | Source |
|---|---|---|
| `ConfidenceParams` | `pub struct ConfidenceParams { freshness_half_life_hours: f64, alpha0: f64, beta0: f64 }` | `unimatrix-engine/src/confidence.rs` (new) |
| `ConfidenceParams::default()` | `freshness_half_life_hours: 168.0, alpha0: 3.0, beta0: 3.0` | `unimatrix-engine/src/confidence.rs` |
| `compute_confidence(entry, now, params)` | `(&EntryRecord, u64, &ConfidenceParams) -> f64` | `unimatrix-engine/src/confidence.rs` (changed) |
| `freshness_score(last, created, now, params)` | `(u64, u64, u64, &ConfidenceParams) -> f64` | `unimatrix-engine/src/confidence.rs` (changed) |
| `CategoryAllowlist::from_categories(cats)` | `(Vec<String>) -> Self` | `unimatrix-server/src/infra/categories.rs` (new) |
| `CategoryAllowlist::new()` | `() -> Self` — delegates to `from_categories(INITIAL_CATEGORIES)` | `unimatrix-server/src/infra/categories.rs` (unchanged call signature) |
| `AgentRegistry::new(store, permissive)` | `(Arc<SqlxStore>, bool) -> Result<Self>` | `unimatrix-server/src/infra/registry.rs` (changed: adds `permissive` param) |
| `SqlxStore::agent_resolve_or_enroll(id, permissive, session_caps)` | `(&str, bool, Option<&[Capability]>) -> Result<AgentRecord>` | `unimatrix-store/src/registry.rs` (changed: adds `session_caps` param) |
| `UnimatrixConfig` | `pub struct` with `knowledge`, `server`, `agents`, `confidence`, `cycle` fields | `unimatrix-server/src/infra/config.rs` (new) |
| `load_config(home_dir, data_dir)` | `(&Path, &Path) -> Result<UnimatrixConfig, ConfigError>` | `unimatrix-server/src/infra/config.rs` (new) |
| `SearchService` `boosted_categories` field | `HashSet<String>` — replaces hardcoded `"lesson-learned"` comparisons | `unimatrix-server/src/services/search.rs` (changed) |
| `context_cycle_review` tool name | Renamed from `context_retrospective` | `unimatrix-server/src/mcp/tools.rs` |

## Startup Sequence (After dsn-001)

```
main() / tokio_main_daemon:
  1. Initialize tracing
  2. ensure_data_directory() → paths
  3. load_config(home_dir, paths.data_dir) → config    ← NEW
     a. Check global config file permissions (#[cfg(unix)])
     b. Read and size-cap global file (≤64 KB)
     c. Deserialize global → UnimatrixConfig (serde defaults fill gaps)
     d. validate_config(&global, global_path)
     e. Repeat a–d for per-project config
     f. merge(global, project) → final config
  4. open_store_with_retry()
  5. CategoryAllowlist::from_categories(config.knowledge.categories)   ← CHANGED
  6. AgentRegistry::new(store, config.agents.permissive)               ← CHANGED
  7. UnimatrixServer::new(..., config.server.instructions)             ← CHANGED
  8. SearchService constructed with boosted_categories HashSet         ← CHANGED
  9. Background tick uses ConfidenceParams from config                 ← CHANGED
```

## Open Questions

None — all risks from SCOPE-RISK-ASSESSMENT.md are resolved by the ADRs above:

- **SR-02** (ConfidenceParams vs bare param): resolved by ADR-001 — use struct.
- **SR-03** (ContentScanner ordering): resolved — call `ContentScanner::global()`
  at the top of `load_config` to force initialization before `scan_title()` is
  called during instructions validation. Document this in code comments.
- **SR-04** (forward-compat stubs): resolved by ADR-004 — empty stubs reserved.
- **SR-06** (merge semantics for lists): resolved by ADR-003 — replace semantics.
- **SR-07** (CategoryAllowlist constructor split): resolved by ADR-002 —
  `new()` delegates to `from_categories(INITIAL_CATEGORIES)`.
- **SR-08** (crate boundary / `Arc<UnimatrixConfig>` across store): resolved by
  ADR-002 — plain parameter crossing only; no Arc across crate boundary.

## Constraints for Delivery Team

1. `toml = "0.8"` added to `unimatrix-server/Cargo.toml` only. Run `cargo tree`
   after adding to confirm no version conflicts.

2. `ConfidenceParams` struct introduced in `unimatrix-engine`. The migration of
   existing call sites (`compute_confidence` with positional `alpha0, beta0`) is
   mechanical — use struct update syntax `{ ..Default::default() }` for any test
   that only overrides one field.

3. `agent_resolve_or_enroll` signature in `unimatrix-store` gains a third parameter.
   All existing call sites should pass `None` to preserve current behavior.

4. The `context_retrospective` rename has a blast radius across Rust source,
   `unimatrix-observe` types, protocol files, skill files, research docs, and
   CLAUDE.md. Build passing is necessary but not sufficient — all non-Rust files
   must be audited before the PR opens.

5. File permission check is `#[cfg(unix)]` only. No behavior change on Windows.

6. `dirs::home_dir()` returning `None` must not panic — degrade to compiled defaults
   with a tracing warning.

7. `ContentScanner::global()` must be called (and thus initialized) before any
   invocation of `scan_title()` in `validate_config`. Place an explicit
   `let _scanner = ContentScanner::global();` at the top of `load_config` with a
   comment explaining the ordering invariant (SR-03).

8. All new validation paths require unit tests. The `validate_config` function
   should be independently testable (no tokio, no store, no scanner singleton — use
   a test `ContentScanner` instance or mock the injection path).
