# Pseudocode: config-extension

**Wave**: 2 (parallel with domain-pack-registry and rule-dsl-evaluator)
**Crate**: `unimatrix-server`
**File**: `crates/unimatrix-server/src/infra/config.rs`

## Purpose

Add `ObservationConfig` struct and `DomainPackConfig` struct to `config.rs`, and add
an `observation: ObservationConfig` field to `UnimatrixConfig`. This follows the
exact `#[serde(default)]` pattern already established by `KnowledgeConfig`.

No changes to config loading logic, validation pipeline, or preset system.

## New Structs

### ObservationConfig

```
/// `[observation]` section — domain pack registration.
///
/// Absent section defaults to empty `domain_packs` (built-in claude-code pack
/// is always loaded regardless via DomainPackRegistry::new()).
#[derive(Debug, Default, Clone, PartialEq, serde::Deserialize)]
#[serde(default)]
pub struct ObservationConfig:
    /// Additional domain packs to register at startup.
    /// The built-in "claude-code" pack is always registered regardless of this list.
    pub domain_packs: Vec<DomainPackConfig>
```

### DomainPackConfig

```
/// Configuration for one domain pack, from `[[observation.domain_packs]]`.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct DomainPackConfig:
    /// Domain identifier. Must match `^[a-z0-9_-]{1,64}$`; "unknown" is reserved.
    pub source_domain: String
    /// Known event type strings for this domain.
    pub event_types: Vec<String>
    /// Knowledge categories this domain's agents may store entries under.
    pub categories: Vec<String>
    /// Path to a TOML file containing `[[rules]]` stanzas (RuleDescriptor).
    /// If absent, the pack registers no DSL rules (built-in Rust rules only).
    #[serde(default)]
    pub rule_file: Option<PathBuf>
```

Note: `DomainPackConfig` does NOT have `#[serde(default)]` at the struct level because
all fields are required in a valid config (except `rule_file` which has its own
`#[serde(default)]`). If `source_domain` or `event_types` are absent, serde returns a
parse error, which propagates as a server startup failure.

## UnimatrixConfig Modification

Add field to existing `UnimatrixConfig`:

```
#[derive(Debug, Default, Clone, PartialEq, serde::Deserialize)]
pub struct UnimatrixConfig:
    #[serde(default)]
    pub profile: ProfileConfig
    #[serde(default)]
    pub knowledge: KnowledgeConfig
    #[serde(default)]
    pub server: ServerConfig
    #[serde(default)]
    pub agents: AgentsConfig
    #[serde(default)]
    pub confidence: ConfidenceConfig
    #[serde(default)]
    pub inference: InferenceConfig
    #[serde(default)]
    pub observation: ObservationConfig    -- ADD THIS FIELD
```

The field must derive `Default` via `#[serde(default)]` so that existing config files
without an `[observation]` section continue to work (FR-02.4, AC-03).

## rule_file Loading (at server startup, not in config.rs)

`DomainPackConfig.rule_file` is a filesystem path. The file contains TOML rule
descriptors. Loading is NOT done in `config.rs` (which has no runtime state).
Loading is done in the server startup wiring in `lib.rs` when converting
`DomainPackConfig` into `DomainPack`.

The conversion function (in `lib.rs` or a startup helper):

```
fn domain_pack_from_config(cfg: DomainPackConfig) -> Result<DomainPack, ObserveError>:
    let rules: Vec<RuleDescriptor> = if let Some(rule_file_path) = cfg.rule_file:
        -- Read and parse the rule file
        let contents = std::fs::read_to_string(&rule_file_path)
            .map_err(|e| ObserveError::InvalidRuleDescriptor {
                rule_name: format!("<file:{}>", rule_file_path.display()),
                reason: format!("cannot read rule file: {e}"),
            })?
        -- Parse as TOML array of rule descriptors
        -- The file format is: [[rules]] stanzas, each with kind, name, source_domain, etc.
        #[derive(Deserialize)]
        struct RuleFile { rules: Vec<RuleDescriptor> }
        let parsed: RuleFile = toml::from_str(&contents)
            .map_err(|e| ObserveError::InvalidRuleDescriptor {
                rule_name: format!("<file:{}>", rule_file_path.display()),
                reason: format!("malformed rule file: {e}"),
            })?
        parsed.rules
    else:
        vec![]

    Ok(DomainPack {
        source_domain: cfg.source_domain,
        event_types: cfg.event_types,
        categories: cfg.categories,
        rules,
    })
```

This function lives in server startup code (`lib.rs`), not in `config.rs`.
The `toml` crate is already a dependency of `unimatrix-server`.

## Initialization Sequence (server startup in lib.rs)

```
startup:
    -- 1. Load TOML config (existing path, unchanged)
    let config: UnimatrixConfig = load_config(...)

    -- 2. Convert DomainPackConfig -> DomainPack (including rule_file loading)
    let packs: Vec<DomainPack> = config.observation.domain_packs
        .into_iter()
        .map(domain_pack_from_config)
        .collect::<Result<_, _>>()?   -- startup failure on any invalid pack (R-09)

    -- 3. Build DomainPackRegistry (validates all rule descriptors, built-in pack always first)
    let registry = DomainPackRegistry::new(packs)?

    -- 4. Register domain pack categories into CategoryAllowlist (IR-02)
    --    Must happen BEFORE the server starts accepting requests
    for pack in registry.iter_packs():
        for category in &pack.categories:
            category_allowlist.add_category(category)
            -- or: use existing CategoryAllowlist::from_categories() path

    -- 5. Thread registry as Arc into SqlObservationSource
    let registry_arc = Arc::new(registry)
    let obs_source = SqlObservationSource::new(store.clone(), registry_arc.clone())
```

Step 4 ordering is critical for IR-02: categories must be in `CategoryAllowlist` before
the first `context_store` call from any domain agent.

## Error Handling

- Absent `[observation]` section: `#[serde(default)]` produces `ObservationConfig { domain_packs: vec![] }`. No error.
- Malformed `[[observation.domain_packs]]` entry: serde parse error → startup failure with TOML parse message.
- `rule_file` path absent: `domain_pack_from_config` returns `Err(InvalidRuleDescriptor)` → startup failure with the file path named (R-09, FM-01).
- `rule_file` malformed: same as above.
- `source_domain = "unknown"` in config: `DomainPackRegistry::new()` returns `Err(InvalidSourceDomain)` → startup failure.

## Key Test Scenarios

1. **No `[observation]` section**: `UnimatrixConfig` deserializes successfully;
   `observation.domain_packs` is empty (`vec![]`) — AC-03.

2. **`[[observation.domain_packs]]` stanza**: deserializes into `DomainPackConfig`
   with correct `source_domain`, `event_types`, `categories`, `rule_file: None`.

3. **`rule_file = "/path/to/rules.toml"`**: deserializes `rule_file` as `Some(PathBuf)`.

4. **Missing required field in domain pack config**: serde returns an error; startup fails.

5. **PartialEq on UnimatrixConfig**: adding the new `observation` field does not break
   any existing tests that compare `UnimatrixConfig` values.

6. **Default ObservationConfig**: `ObservationConfig::default()` has `domain_packs = vec![]`.
