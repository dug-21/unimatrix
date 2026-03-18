# dsn-001 Pseudocode Overview — Config Externalization (W0-3)

## Components Covered

| Component | File | Scope |
|-----------|------|-------|
| config-loader | `pseudocode/config-loader.md` | New file: `infra/config.rs` |
| confidence-params | `pseudocode/confidence-params.md` | Modified: `unimatrix-engine/src/confidence.rs` |
| category-allowlist | `pseudocode/category-allowlist.md` | Modified: `infra/categories.rs` |
| search-service | `pseudocode/search-service.md` | Modified: `services/search.rs` |
| agent-registry | `pseudocode/agent-registry.md` | Modified: `infra/registry.rs` + `unimatrix-store/src/registry.rs` |
| server-instructions | `pseudocode/server-instructions.md` | Modified: `server.rs` |
| tool-rename | `pseudocode/tool-rename.md` | Modified: `mcp/tools.rs` + 31-location blast-radius |
| startup-wiring | `pseudocode/startup-wiring.md` | Modified: `main.rs` |

## Dependency Order (Wave Plan)

Wave 1 — No cross-component dependencies:
- **confidence-params**: extends `unimatrix-engine/src/confidence.rs`. No server dependency. Must be done first because config-loader and startup-wiring depend on `ConfidenceParams`.
- **category-allowlist**: adds one constructor to `infra/categories.rs`. No config dependency.
- **tool-rename**: pure text substitution across 31 locations. No API change.

Wave 2 — Depends on Wave 1:
- **config-loader**: new file. Depends on `ConfidenceParams` (W1), `CategoryAllowlist` (W1), `ContentScanner` (existing).
- **agent-registry**: store-layer signature change + server-infra constant removal. Depends on store-layer change being in place.
- **search-service**: field addition + four replacement comparisons. No hard dependency but references `boosted_categories`.
- **server-instructions**: removes const, wires `Option<String>`. Minor. No hard dependency.

Wave 3 — Depends on Wave 2:
- **startup-wiring**: inserts config load + resolution, wires all values to constructors. Depends on every other component.

## Data Flow: Config Load Through Subsystem Construction

```
main.rs (tokio_main_daemon / tokio_main_stdio)
  │
  ├─ Step 1: tracing init
  ├─ Step 2: ensure_data_directory() → ProjectPaths { home_dir, data_dir, ... }
  │
  ├─ Step 3: load_config(home_dir: &Path, data_dir: &Path) → UnimatrixConfig   [NEW]
  │     infra/config.rs:
  │       a. ContentScanner::global()  ← warm singleton (ordering invariant)
  │       b. for global_path + project_path:
  │            check_permissions(path)  ← #[cfg(unix)] only
  │            read file to Vec<u8>, assert len <= 65536
  │            toml::from_str(utf8) → UnimatrixConfig (serde defaults fill absent sections)
  │            validate_config(&config, path) → Ok or abort
  │       c. merge_configs(global, project) → UnimatrixConfig
  │
  ├─ Step 4: resolve_confidence_params(&config) → ConfidenceParams             [NEW]
  │     infra/config.rs:
  │       match config.profile.preset:
  │         Collaborative → ConfidenceParams::default()       (with optional half_life override)
  │         Authoritative | Operational | Empirical → table lookup + optional half_life override
  │         Custom → config.confidence.weights (required) + config.knowledge.freshness_half_life_hours (required)
  │
  ├─ Step 5: open_store_with_retry()  [unchanged]
  │
  ├─ Step 6: CategoryAllowlist::from_categories(config.knowledge.categories)   [CHANGED from new()]
  │
  ├─ Step 7: AgentRegistry::new(store, config.agents.permissive)               [CHANGED: adds permissive param]
  │     infra/registry.rs:
  │       stores permissive: bool on struct (replaces const PERMISSIVE_AUTO_ENROLL)
  │       resolve_or_enroll passes permissive + Some(config_session_caps) to store
  │
  ├─ Step 8: UnimatrixServer::new(..., config.server.instructions)             [CHANGED: adds instructions param]
  │     server.rs:
  │       instructions: config.server.instructions.unwrap_or_else(|| SERVER_INSTRUCTIONS_DEFAULT.to_string())
  │       SERVER_INSTRUCTIONS const removed; compiled default moved to a private const
  │
  ├─ Step 9: SearchService constructed with boosted_categories: HashSet<String>[CHANGED]
  │     services/search.rs:
  │       boosted_categories field on SearchService
  │       four entry.category == "lesson-learned" comparisons → boosted_categories.contains(&entry.category)
  │
  └─ Step 10: spawn_background_tick(..., Arc::new(confidence_params))          [CHANGED: adds params arg]
        background.rs:
          receives Arc<ConfidenceParams>; passes &params to compute_confidence(entry, now, &params)
```

## Shared Types

### Introduced by dsn-001 (new in `infra/config.rs`)

```
UnimatrixConfig { profile, knowledge, server, agents, confidence }
ProfileConfig   { preset: Preset }
KnowledgeConfig { categories: Vec<String>, boosted_categories: Vec<String>,
                  freshness_half_life_hours: Option<f64> }
ServerConfig    { instructions: Option<String> }
AgentsConfig    { default_trust: String, session_capabilities: Vec<String> }
ConfidenceConfig { weights: Option<ConfidenceWeights> }
ConfidenceWeights { base, usage, fresh, help, corr, trust: f64 }
Preset          { Authoritative, Operational, Empirical, Collaborative, Custom }
ConfigError     { 17 variants — see config-loader.md }
```

### Extended by dsn-001 (in `unimatrix-engine/src/confidence.rs`)

```
ConfidenceParams adds 6 weight fields:
  w_base: f64   (was implicit: W_BASE  = 0.16)
  w_usage: f64  (was implicit: W_USAGE = 0.16)
  w_fresh: f64  (was implicit: W_FRESH = 0.18)
  w_help: f64   (was implicit: W_HELP  = 0.12)
  w_corr: f64   (was implicit: W_CORR  = 0.14)
  w_trust: f64  (was implicit: W_TRUST = 0.16)
Existing fields retained: freshness_half_life_hours, alpha0, beta0
```

## Cross-Boundary Values (ADR-002: no Arc<UnimatrixConfig> crosses crate boundaries)

| Config value | Boundary | Mechanism |
|---|---|---|
| `knowledge.categories` | server → `CategoryAllowlist` | `Vec<String>` at construction |
| `knowledge.boosted_categories` | server → `SearchService` | `HashSet<String>` field |
| Resolved `ConfidenceParams` | server → background tick | `Arc<ConfidenceParams>` value |
| `agents.default_trust` | server → `AgentRegistry` | `bool permissive` |
| `agents.session_capabilities` | server → `SqlxStore` | `Option<&[Capability]>` param |
| `server.instructions` | server-internal only | `String` on `UnimatrixServer` |

## Sequencing Constraints

1. `ContentScanner::global()` must be called at the top of `load_config` before `validate_config` calls `scan_title()`.
2. `load_config` must complete before `resolve_confidence_params`.
3. `resolve_confidence_params` must complete before `spawn_background_tick`.
4. `CategoryAllowlist::from_categories` must be called with the merged config categories before `UnimatrixServer::new`.
5. `AgentRegistry::new(store, permissive)` must be called before `UnimatrixServer::new`.
6. Hook path (`Command::Hook`) and bridge mode (`tokio_main_bridge`) must NOT call `load_config`.
