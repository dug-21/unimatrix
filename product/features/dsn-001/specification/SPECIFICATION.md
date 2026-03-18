# SPECIFICATION: dsn-001 — Config Externalization (W0-3)

## Objective

Unimatrix is a domain-agnostic knowledge engine whose behavior is currently hard-coupled to software delivery vocabulary through compiled constants. This feature externalizes four groups of runtime constants to a two-level TOML configuration system (`~/.unimatrix/config.toml` global, `~/.unimatrix/{hash}/config.toml` per-project), adds a `[profile]` preset system that maps knowledge-lifecycle archetypes to calibrated confidence weight vectors, and validates all security-critical config fields at startup with abort-on-violation semantics. Two hardcoded vocabulary fixes accompany the config work: `context_retrospective` is renamed to `context_cycle_review` across the entire codebase, and the `CycleParams.topic` field doc is neutralised to remove Agile/SDLC vocabulary.

---

## Functional Requirements

### FR-01: Config File Loading

The server loads `~/.unimatrix/config.toml` (global) and `~/.unimatrix/{hash}/config.toml` (per-project) at startup, after `ensure_data_directory()` resolves `ProjectPaths`, before any subsystem is constructed. When a file is absent, the compiled defaults apply with no error and no warning. When a file is present but malformed (TOML parse error or validation failure), startup aborts with a descriptive error identifying the file path and the violation.

### FR-02: Two-Level Config Merge

Per-project config values shadow global values, which shadow compiled defaults. Merge semantics are replace (not append): a per-project field that is present overrides the entire corresponding global field; a per-project field that is absent falls through to the global value. For list fields (`categories`, `boosted_categories`, `session_capabilities`), a per-project list replaces the global list entirely — there is no append behavior. Both files are validated independently before merging; a violation in either aborts startup.

### FR-03: Profile Preset System

A `[profile]` TOML section with a single `preset` key selects one of five named behaviors: `authoritative`, `operational`, `empirical`, `collaborative`, or `custom`. Named presets expand to a fully-populated `ConfidenceParams` carrying all nine fields (six weights, `freshness_half_life_hours`, `alpha0`, `beta0`) without requiring any operator knowledge of the weight values. The default preset when `[profile]` is absent is `collaborative`. The `custom` preset activates the `[confidence]` section; named presets ignore `[confidence]` even if present.

### FR-04: ConfidenceParams Struct Extension

`ConfidenceParams` in `unimatrix-engine/src/confidence.rs` is extended from three fields to nine fields: `w_base`, `w_usage`, `w_fresh`, `w_help`, `w_corr`, `w_trust` (all `f64`), plus the existing `freshness_half_life_hours`, `alpha0`, `beta0`. `Default` reproduces the compiled constants exactly (the `collaborative` preset). `compute_confidence` uses `params.w_*` instead of compiled weight constants. `freshness_score` uses `params.freshness_half_life_hours`. All callers of these functions update to pass `&ConfidenceParams`.

### FR-05: Knowledge Category Externalization

When `[knowledge] categories` is set, the configured list replaces the compiled `INITIAL_CATEGORIES` (8 values) as the seed for `CategoryAllowlist`. A new `CategoryAllowlist::from_categories(Vec<String>)` constructor is added; the existing `CategoryAllowlist::new()` delegates to it using the compiled defaults. When no config is present, `new()` behavior is unchanged.

### FR-06: Boosted Categories Externalization

When `[knowledge] boosted_categories` is set, `SearchService` builds its provenance-boost lookup from that list instead of the hardcoded `entry.category == "lesson-learned"` comparisons in `search.rs`. The field is stored as `HashSet<String>` at `SearchService` construction time. All four hardcoded string comparisons in `search.rs` are replaced with a `HashSet` lookup.

### FR-07: Freshness Half-Life Externalization

When `[knowledge] freshness_half_life_hours` is present, `freshness_score()` uses that value instead of the compiled `FRESHNESS_HALF_LIFE_HOURS` constant (168.0). The field uses `Option<f64>` — `None` means "absent from TOML, use preset's built-in value"; `Some(v)` means the operator explicitly supplied `v`. The compiled constant is retained as the backing value for `ConfidenceParams::default()` and the `collaborative` preset.

### FR-08: Server Instructions Externalization

When `[server] instructions` is set, that string is used as `ServerInfo.instructions` in the MCP `initialize` handshake instead of the compiled `SERVER_INSTRUCTIONS` constant. The compiled constant is removed from `server.rs`.

### FR-09: Agent Enrollment Externalization

When `[agents] default_trust` and `session_capabilities` are set, unknown agents auto-enroll with the configured trust mode and capability set. `AgentRegistry::new(store, permissive: bool)` receives the `permissive` flag instead of reading `PERMISSIVE_AUTO_ENROLL`. `agent_resolve_or_enroll` in `unimatrix-store` gains a third parameter `session_caps: Option<&[Capability]>` — when `Some`, uses the provided capabilities; when `None`, uses the existing permissive/strict branch. All existing call sites pass `None`.

### FR-10: Preset Resolution Pipeline

A single function `resolve_confidence_params(config: &UnimatrixConfig) -> Result<ConfidenceParams, ConfigError>` in `unimatrix-server/src/infra/config.rs` is the one location where preset selection converts to a populated `ConfidenceParams`. No other code determines which weight values or `freshness_half_life_hours` to use. Named presets use the ADR-005 weight table; `custom` uses `[confidence] weights` and `[knowledge] freshness_half_life_hours`. A helper `confidence_params_from_preset(preset: Preset) -> ConfidenceParams` (also in `config.rs`) constructs named-preset params for testing; calling it with `Preset::Custom` is a logic error and panics.

### FR-11: `context_retrospective` → `context_cycle_review` Rename

The MCP tool formerly named `context_retrospective` is renamed to `context_cycle_review`. This is a hardcoded, non-configurable rename. The `#[tool(name = "...")]` attribute, the handler function name, and every reference across all crate types, test files, protocol files, skill files, research documents, and operational documents are updated in the same PR. No reference to `context_retrospective` remains anywhere in the repository after this change.

### FR-12: CycleParams.topic Field Doc Update

The `CycleParams.topic` field doc comment in `tools.rs` no longer references "feature" as the canonical example. It conveys the domain-agnostic concept: a bounded unit of work tracked by any domain — feature, incident, campaign, case, sprint, or experiment.

### FR-13: Security Validation at Load Time

All security-critical config fields are validated in `validate_config()` immediately after TOML deserialization. Any violation aborts startup with a descriptive error. Validated fields: category name character set and length, category count, `boosted_categories` subset constraint, `freshness_half_life_hours` range, `instructions` length and injection scan, `preset` enum membership, custom weight presence and sum, `default_trust` allowlist, `session_capabilities` allowlist. There is no warn-and-continue path for security-critical fields.

### FR-14: File Permission Enforcement

On Unix (`#[cfg(unix)]`), both config files are checked for world-writable permissions (`mode & 0o002 != 0`). A world-writable file aborts startup with an error identifying the file path and the violation. A group-writable file (`mode & 0o020 != 0`) logs a tracing warning and continues. This check runs before TOML parsing for each file. On non-Unix platforms the check is omitted.

### FR-15: File Size Cap

Each config file is read with a 64 KB cap before being passed to the TOML parser. A file exceeding 64 KB causes startup to abort with an error before parsing begins.

### FR-16: No-Config Backward Compatibility

When neither config file is present, all subsystems receive values identical to the pre-dsn-001 compiled defaults. No existing test is modified as a result of this feature. The `collaborative` preset equals `ConfidenceParams::default()` exactly — this invariant is enforced by a mandatory unit test (see AC-21).

---

## Non-Functional Requirements

### NFR-01: Performance — Startup Only

Config loading and validation runs exactly once per server start, in the main startup path before any request is handled. No config value is re-read, re-parsed, or re-validated during request handling or background ticks. The startup overhead budget for config load is under 5 ms (file reads + TOML parse + validation for typical config files under 4 KB).

### NFR-02: Memory

The resolved `ConfidenceParams` is passed as `Arc<ConfidenceParams>` to the background tick task. No other subsystem retains a reference to `UnimatrixConfig` after startup completes. The config struct itself is not stored on any long-lived component.

### NFR-03: Crate Boundary

No `Arc<UnimatrixConfig>` crosses any crate boundary. Config values that cross into `unimatrix-store` or `unimatrix-engine` are passed as plain primitive values: `bool`, `Vec<String>`, `HashSet<String>`, `Vec<Capability>`, or `ConfidenceParams`. The `toml` crate is added to `unimatrix-server/Cargo.toml` only.

### NFR-04: Testability

`validate_config()` is independently testable without a running tokio runtime, store, or embedded server. The only external dependency it requires is `ContentScanner::global()`, which is a lazy singleton warmable before test calls. New tests extend the existing test infrastructure (fixtures, helpers) in `unimatrix-server`; they do not create isolated scaffolding.

### NFR-05: No Schema Migration

Config is purely runtime state. No new DB tables, no schema version bump, no migration path. Existing DB entries with `category = "lesson-learned"` continue to receive the provenance boost under the default config; behavior changes after config-driven restart only.

### NFR-06: rmcp Version

`rmcp = "=0.16.0"` is not changed. No rmcp API is modified by this feature.

### NFR-07: Windows Compatibility

File permission checking is gated `#[cfg(unix)]`. The server compiles and runs on Windows without the permission check. All other config behaviors are platform-independent.

---

## Acceptance Criteria

### Default / No-Config Behavior

**AC-01** — When no `~/.unimatrix/config.toml` and no per-project `config.toml` exist, the server starts with all existing default values. All existing unit and integration tests pass without modification.
- Verification: run the full test suite with no config files present; assert zero test changes.

**AC-22** — When no `[profile]` section is present, the server uses the `collaborative` preset (current compiled defaults). The effective `ConfidenceParams` is equal to `ConfidenceParams::default()`.
- Verification: unit test asserting `confidence_params_from_preset(Preset::Collaborative) == ConfidenceParams::default()` (the mandatory SR-10 test).

### Knowledge Config

**AC-02** — When `[knowledge] categories` is set to a valid list, `CategoryAllowlist` reflects that list instead of the compiled 8-category default.
- Verification: construct `CategoryAllowlist::from_categories(vec![...])` with a custom list; assert that `is_allowed("outcome")` returns true only when "outcome" is in the custom list.

**AC-03** — When `[knowledge] boosted_categories` is set, those categories receive the provenance boost in search re-ranking. The hardcoded string comparison `entry.category == "lesson-learned"` no longer exists in `search.rs`.
- Verification: grep for the literal string `"lesson-learned"` in `search.rs` — must not be found. Integration test with a custom `boosted_categories` showing boost applied to the configured category and not to "lesson-learned" when it is absent from the list.

**AC-04** — When `[knowledge] freshness_half_life_hours` is set to a valid value, `freshness_score()` uses that value instead of 168.0.
- Verification: unit test calling `freshness_score()` with a `ConfidenceParams` carrying the override value; assert the result differs from the default at a known age.

### Server Config

**AC-05** — When `[server] instructions` is set, that string appears in `ServerInfo.instructions` returned during the MCP `initialize` handshake.
- Verification: integration test with a config file setting `instructions = "Test instructions"` and asserting the field appears in the `initialize` response.

### Agents Config

**AC-06** — When `[agents] default_trust = "strict"` and `session_capabilities` are set, auto-enrolled unknown agents receive `[Read, Search]` (not `[Read, Write, Search]`).
- Verification: integration test enrolling a new agent with strict config; assert the enrolled agent's capability set matches the configured value.

### Two-Level Merge

**AC-07** — A per-project config at `{data_dir}/config.toml` overrides the global `~/.unimatrix/config.toml` for all fields it specifies. Unspecified fields fall through to the global value or compiled defaults.
- Verification: integration test with global config setting `categories = ["a", "b"]` and per-project setting `categories = ["c"]`; assert effective categories are `["c"]` only.

### File Permissions

**AC-08** — A config file with world-writable permissions (`mode & 0o002 != 0`) causes server startup to abort with an error message identifying the file path and the violation.
- Verification: unit test creating a temp file with mode 0o666, calling `check_permissions(path)`, asserting `Err(ConfigError::WorldWritable(...))`.

**AC-09** — A config file with group-writable permissions (`mode & 0o020 != 0`) logs a tracing warning and does not abort startup.
- Verification: unit test with mode 0o664; assert `Ok(())` return and warning emission.

### Validation — Knowledge

**AC-10** — A `[knowledge] categories` entry containing characters outside `[a-z0-9_-]`, length > 64, or a total category count > 64 causes startup to abort with a descriptive error.
- Verification: unit tests for each violation: invalid chars (e.g., "Cat!"), length 65, count 65.

**AC-11** — A `[knowledge] boosted_categories` value not present in the validated `categories` set causes startup to abort with an error naming the invalid value.
- Verification: unit test with `categories = ["a"]` and `boosted_categories = ["b"]`; assert error names "b".

**AC-16** — A `[knowledge] freshness_half_life_hours` value of `0.0`, negative, `NaN`, or `Infinity` causes startup to abort with a descriptive error.
- Verification: unit tests for each: `0.0`, `-1.0`, `f64::NAN`, `f64::INFINITY`.

**AC-17** — A `[knowledge] freshness_half_life_hours` value greater than `87600.0` (10 years) causes startup to abort with a descriptive error.
- Verification: unit test with `87600.001`; assert error.

### Validation — Server

**AC-12** — A `[server] instructions` value matching any injection pattern in `ContentScanner` causes startup to abort with an error identifying the triggering pattern category.
- Verification: unit test with a known injection string; assert `Err(ConfigError::InstructionsInjection(...))`.

**AC-20** — A `[server] instructions` value exceeding 8 KB (8192 bytes) causes startup to abort with an error before `ContentScanner` runs.
- Verification: unit test with a 8193-byte string; assert error before scan.

### Validation — Preset

**AC-23** — When `preset` is `authoritative`, `operational`, `empirical`, or `collaborative`, the corresponding weight vector and `freshness_half_life_hours` from the table below are used. The `[confidence]` section is ignored even if present. A warning is logged if `[confidence] weights` is present with a named preset.

| Preset | w_base | w_usage | w_fresh | w_help | w_corr | w_trust | SUM  | half_life_h |
|--------|--------|---------|---------|--------|--------|---------|------|-------------|
| `collaborative` | 0.16 | 0.16 | 0.18 | 0.12 | 0.14 | 0.16 | 0.92 | 168.0 |
| `authoritative` | 0.14 | 0.14 | 0.10 | 0.14 | 0.18 | 0.22 | 0.92 | 8760.0 |
| `operational`   | 0.14 | 0.18 | 0.24 | 0.08 | 0.18 | 0.10 | 0.92 | 720.0 |
| `empirical`     | 0.12 | 0.16 | 0.34 | 0.04 | 0.06 | 0.20 | 0.92 | 24.0 |

- Verification: unit tests asserting `confidence_params_from_preset(Preset::Authoritative)` returns the exact values from this table. Confirm `[confidence] weights` values are not reflected in the resolved params when a named preset is active.

**AC-24** — When `preset = "custom"`, all six `[confidence] weights` keys are required. Startup aborts with a descriptive error naming the missing field if any are absent.
- Verification: unit test with `preset = custom` and `weights` missing entirely; assert error. Unit test with five of six keys; assert error naming the missing key.

**AC-25** — `freshness_half_life_hours` precedence for all combinations:

| `[profile] preset` | `[knowledge] freshness_half_life_hours` | Effective value |
|--------------------|----------------------------------------|-----------------|
| named (non-custom) | absent | Preset's built-in value from AC-23 table |
| named (non-custom) | present | `[knowledge]` value (operator override) |
| `custom` | absent | **Startup abort** — required for `custom` |
| `custom` | present | `[knowledge]` value |

- Verification: four unit tests, one per row. The `custom`+absent case asserts `Err(ConfigError::CustomPresetMissingHalfLife)`.

**AC-26** — An unrecognised `preset` value causes startup to abort with an error listing the five valid values.
- Verification: TOML deserialization of `preset = "unknown"` returns an error from serde before `validate_config` runs (because `Preset` uses `#[serde(rename_all = "lowercase")]` with no catch-all variant).

### Validation — Agents

**AC-18** — A `[agents] default_trust` value other than `"permissive"` or `"strict"` causes startup to abort with an error listing valid values.
- Verification: unit test with `default_trust = "admin"`.

**AC-19** — A `[agents] session_capabilities` list containing any value other than `"Read"`, `"Write"`, or `"Search"` (including `"Admin"`) causes startup to abort with an error.
- Verification: unit test with `session_capabilities = ["Admin"]`.

### Validation — File Size

**AC-15** — A config file exceeding 64 KB causes startup to abort with an error before TOML parsing begins.
- Verification: unit test writing a 65537-byte temp file; assert `Err(ConfigError::FileTooLarge(...))`.

### Tool Rename

**AC-13** — The MCP tool formerly named `context_retrospective` is now named `context_cycle_review`. No reference to `context_retrospective` remains in the repository (Rust source, Python tests, protocol files, skill files, research documents, or operational documents).
- Verification: `grep -r "context_retrospective" .` in the repository root returns zero results. See SR-05 Rename Checklist below for the exhaustive file list.

### CycleParams Doc

**AC-14** — The `CycleParams.topic` field doc no longer references "feature" as the canonical example. The doc communicates the domain-agnostic concept of a bounded work unit tracked by any domain.
- Verification: read `tools.rs`; assert the word "feature" does not appear as the primary example in the `topic` field doc.

### Preset Cold-Start

**AC-27** — The `ConfidenceParams` struct carries the effective weight vector (from the active preset or `custom` values) at startup. W3-1 reads this struct for GNN cold-start without any additional config parsing.
- Verification: assert `ConfidenceParams` has exactly nine public fields (`w_base`, `w_usage`, `w_fresh`, `w_help`, `w_corr`, `w_trust`, `freshness_half_life_hours`, `alpha0`, `beta0`) and that `resolve_confidence_params` populates all of them regardless of which preset is active.

### Test Coverage

**AC-21** — All new validation paths have unit tests. The mandatory SR-10 test is present in `unimatrix-server`:
```rust
assert_eq!(
    confidence_params_from_preset(Preset::Collaborative),
    ConfidenceParams::default()
);
```
All existing unit and integration tests continue to pass.
- Verification: CI must be green. The SR-10 test must be present as a named test with the comment "SR-10: If this test fails, fix the weight table, not the test."

---

## Domain Models

### UnimatrixConfig

Top-level config struct in `unimatrix-server/src/infra/config.rs`. Five sub-structs, all optional in TOML (absent sections use compiled defaults via `#[serde(default)]`). `CycleConfig` is absent — removed; the vocabulary fix it reserved for is a hardcoded rename.

```
UnimatrixConfig
  └── profile:    ProfileConfig     [profile]    — preset selection
  └── knowledge:  KnowledgeConfig   [knowledge]  — categories, freshness
  └── server:     ServerConfig      [server]     — instructions string
  └── agents:     AgentsConfig      [agents]     — trust, capabilities
  └── confidence: ConfidenceConfig  [confidence] — weights (custom only)
```

### Preset

Enum in `unimatrix-server/src/infra/config.rs`. Five variants:

| Variant | TOML string | Weight source |
|---------|-------------|---------------|
| `Collaborative` | `"collaborative"` | Compiled defaults (= `ConfidenceParams::default()`) |
| `Authoritative` | `"authoritative"` | ADR-005 table |
| `Operational` | `"operational"` | ADR-005 table |
| `Empirical` | `"empirical"` | ADR-005 table |
| `Custom` | `"custom"` | `[confidence] weights` + `[knowledge] freshness_half_life_hours` |

Default: `Preset::Collaborative`. `#[serde(rename_all = "lowercase")]` maps TOML strings to enum variants; an unrecognised string is a serde error (fails before validation).

### ConfidenceParams

Struct in `unimatrix-engine/src/confidence.rs`. Nine fields. The single carrier for all confidence tuning parameters. `Default` reproduces compiled constants. W3-1 will add `Option<LearnedWeights>` without changing any call site using `Default`.

| Field | Type | Default | Role |
|-------|------|---------|------|
| `w_base` | f64 | 0.16 | Weight: base quality (status + trust_source) |
| `w_usage` | f64 | 0.16 | Weight: usage frequency |
| `w_fresh` | f64 | 0.18 | Weight: freshness (recency) |
| `w_help` | f64 | 0.12 | Weight: helpfulness (Bayesian posterior) |
| `w_corr` | f64 | 0.14 | Weight: correction chain quality |
| `w_trust` | f64 | 0.16 | Weight: creator trust level |
| `freshness_half_life_hours` | f64 | 168.0 | Exponential decay parameter for `freshness_score()` |
| `alpha0` | f64 | 3.0 | Bayesian prior — helpfulness alpha |
| `beta0` | f64 | 3.0 | Bayesian prior — helpfulness beta |

Sum invariant: `w_base + w_usage + w_fresh + w_help + w_corr + w_trust == 0.92` (tolerance `(sum - 0.92).abs() < 1e-9`). Not `<= 1.0` — the SCOPE.md comment is incorrect on this point; the ADR-005 invariant governs.

### CategoryAllowlist

Struct in `unimatrix-server/src/infra/categories.rs`. Two constructors:
- `new()` — delegates to `from_categories(INITIAL_CATEGORIES.to_vec())`. Preserves all existing call sites.
- `from_categories(cats: Vec<String>) -> Self` — seeds the allowlist from the supplied list. Called from `main.rs` after config load.

### ConfigError

Error type returned by `load_config`, `validate_config`, and `resolve_confidence_params`. Variants must include (at minimum): `FileTooLarge`, `WorldWritable`, `MalformedToml`, `InvalidCategoryChar`, `TooManyCategories`, `InvalidCategoryLength`, `BoostedCategoryNotInAllowlist`, `InvalidHalfLifeValue`, `HalfLifeOutOfRange`, `InstructionsTooLong`, `InstructionsInjection`, `InvalidDefaultTrust`, `InvalidSessionCapability`, `CustomPresetMissingWeights`, `CustomPresetMissingHalfLife`, `CustomWeightOutOfRange`, `CustomWeightSumInvariant`.

### context_cycle_review (formerly context_retrospective)

MCP tool that analyzes observation data for a named work cycle. The tool concept is domain-neutral. The rename removes the only remaining Agile-vocabulary MCP tool name. Internal operation (draining the cycle accumulator, running detection rules, generating the structured report) is unchanged by this feature.

---

## User Workflows

### Workflow 1: Deploy Unimatrix for a New Domain (e.g., Legal)

1. Operator creates `~/.unimatrix/config.toml`.
2. Sets `[profile] preset = "authoritative"` — selects the weight vector calibrated for long-lived authoritative documents (W_TRUST dominant, W_FRESH minimal, half_life 8760h).
3. Sets `[knowledge] categories` to legal-domain terms (e.g., `["ruling", "statute", "brief", "precedent", "memo"]`).
4. Sets `[knowledge] boosted_categories = ["ruling"]` to elevate primary sources in search.
5. Sets `[server] instructions` to domain-appropriate agent guidance.
6. Starts the server. Validation runs; startup succeeds or aborts with a descriptive error.
7. All confidence scoring uses the `authoritative` weight vector from the first knowledge entry onward.

### Workflow 2: Per-Project Override

1. Operator has a global config with `preset = "authoritative"`.
2. For a specific fast-moving project (e.g., an incident response system), creates `~/.unimatrix/{hash}/config.toml`.
3. Sets `[profile] preset = "operational"` in the per-project file.
4. The global `categories` apply (not overridden); the per-project `preset` overrides the global preset.
5. Server uses `operational` weights (W_FRESH dominant, half_life 720h) for this project.

### Workflow 3: Expert Custom Weights

1. Operator has domain science justification for a specific weight distribution.
2. Sets `[profile] preset = "custom"` in config.
3. Sets all six weights in `[confidence] weights = { base = ..., usage = ..., fresh = ..., help = ..., corr = ..., trust = ... }`.
4. Sets `[knowledge] freshness_half_life_hours = ...` (required for `custom`).
5. Server validates: all six keys present, each in `[0.0, 1.0]`, sum equals 0.92 within tolerance, `freshness_half_life_hours` present and in range.

### Workflow 4: Operator Calling context_cycle_review

1. Agent invokes `context_cycle_review(feature_cycle: "col-999")`.
2. Server routes to the renamed handler (previously `context_retrospective`).
3. Output format and detection behavior are identical to the pre-dsn-001 tool.
4. No protocol or skill file references the old name.

---

## Constraints

### Technical Constraints

1. **`toml = "0.8"` pinned** — added to `unimatrix-server/Cargo.toml` only, not to `unimatrix-engine`, `unimatrix-store`, or `unimatrix-core`. Run `cargo tree` after adding to confirm no transitive version conflicts.

2. **Weight sum invariant is `(sum - 0.92).abs() < 1e-9`** — not `sum <= 1.0`. The SCOPE.md config schema comment is incorrect. The ADR-005 invariant governs. All validation code and tests must use the 0.92 form.

3. **`from_preset(Custom)` panics** — calling `confidence_params_from_preset(Preset::Custom)` is a logic error and panics by design. Only `resolve_confidence_params` handles the `Custom` path. Code review must ensure no direct call with `Custom` exists.

4. **`FRESHNESS_HALF_LIFE_HOURS` lives in `unimatrix-engine`** — plumbing requires adding `freshness_half_life_hours` as a `ConfidenceParams` field and passing `params` through `freshness_score()`. The compiled constant is retained as the `Default` backing value but no longer used directly in computation.

5. **`CategoryAllowlist::new()` must remain valid** — all existing test call sites use `new()`. This constructor must not be removed or changed in signature. It delegates to `from_categories(INITIAL_CATEGORIES.to_vec())`.

6. **`agent_resolve_or_enroll` third parameter defaults to `None`** — all existing call sites gain `None` as the third argument to preserve current behavior. No call site requires a store-level refactor beyond the parameter addition.

7. **File permission check is `#[cfg(unix)]` only** — `std::os::unix::fs::PermissionsExt` is not available on Windows. The feature compiles and runs on Windows without this check.

8. **`dirs::home_dir()` returning `None` must not panic** — if the home directory is unresolvable, degrade to compiled defaults with a `tracing::warn!` and continue. This handles CI and container environments.

9. **`ContentScanner::global()` must be called at the top of `load_config`** — the singleton must be warmed before `validate_config()` calls `scan_title()`. A comment in `load_config` must document this ordering invariant.

10. **Hook path and bridge mode are excluded** — `Command::Hook` is a sync path with a sub-50ms budget and must not load config. `tokio_main_bridge` does not run server subsystems. Export/import subcommands are offline tools. Config loading occurs only in `tokio_main_daemon` and `tokio_main_stdio`.

11. **No `Arc<UnimatrixConfig>` crosses crate boundaries** — only plain primitive values are passed: `bool`, `Vec<String>`, `HashSet<String>`, `Vec<Capability>`, `ConfidenceParams`.

12. **`CycleConfig` is removed from `UnimatrixConfig`** — it was never active; the vocabulary fix is a hardcoded rename. If prior spec or code referenced it, those references are removed.

13. **Per-project `custom` preset requires per-project `[confidence] weights`** — per ADR-003, `custom` weights are not inherited from the global config. If the per-project config uses `preset = "custom"` without weights, and the global config has weights, startup aborts. Cross-level weight inheritance is explicitly prohibited.

---

## Dependencies

| Dependency | Location | Version | Notes |
|------------|----------|---------|-------|
| `toml` | `unimatrix-server/Cargo.toml` | `"0.8"` (pinned, not `^`) | New dependency. Adds serde-based TOML parsing. |
| `serde` | already present | existing | `Deserialize` derive on config structs. |
| `dirs` | already present in `unimatrix-server` | existing | `dirs::home_dir()` for `~/.unimatrix/` resolution. |
| `ContentScanner` | `unimatrix-server/src/infra/scanning.rs` | internal | `scan_title()` used for `[server] instructions` validation. |
| `ConfidenceParams` | `unimatrix-engine/src/confidence.rs` | internal | Extended with six weight fields (ADR-001). |
| `CategoryAllowlist` | `unimatrix-server/src/infra/categories.rs` | internal | New `from_categories` constructor (ADR-002). |
| `AgentRegistry` | `unimatrix-server/src/infra/registry.rs` | internal | `new(store, permissive: bool)` — adds permissive param. |
| `SqlxStore::agent_resolve_or_enroll` | `unimatrix-store/src/registry.rs` | internal | Adds third param `session_caps: Option<&[Capability]>`. |
| `SearchService` | `unimatrix-server/src/services/search.rs` | internal | `boosted_categories: HashSet<String>` field replaces hardcoded comparison. |
| `rmcp` | `unimatrix-server/Cargo.toml` | `=0.16.0` (pinned) | Unchanged. No API change. |

---

## SR-05 Rename Checklist: context_retrospective → context_cycle_review

Build passing is not a sufficient gate for this rename. Protocol, skill, research, and documentation files are not compiled. Every file in this checklist must be updated in the same PR as the Rust source changes.

### Rust Source Files

| File | Reference type | What to change |
|------|----------------|----------------|
| `crates/unimatrix-server/src/mcp/tools.rs` | `#[tool(name = "context_retrospective")]` attribute | Change to `context_cycle_review` |
| `crates/unimatrix-server/src/mcp/tools.rs` | `async fn context_retrospective(` handler function | Rename to `context_cycle_review` |
| `crates/unimatrix-server/src/mcp/tools.rs` | `operation: "context_retrospective".to_string()` (audit log, line 1457) | Update to `context_cycle_review` |
| `crates/unimatrix-server/src/mcp/tools.rs` | `operation: "context_retrospective/lesson-learned".to_string()` (line 1734) | Update to `context_cycle_review/lesson-learned` |
| `crates/unimatrix-server/src/mcp/tools.rs` | `"Use context_retrospective to confirm..."` doc strings (lines 1505, 1560) | Update to `context_cycle_review` |
| `crates/unimatrix-server/src/mcp/tools.rs` | `/// Parameters for the context_retrospective tool.` (line 239) | Update doc |
| `crates/unimatrix-server/src/mcp/tools.rs` | `/// Called inside a tokio::spawn from context_retrospective.` (line 1617) | Update doc |
| `crates/unimatrix-server/src/server.rs` | `/// context_retrospective handler (drains on call).` (line 65) | Update doc |
| `crates/unimatrix-server/src/server.rs` | `/// features that complete without calling context_retrospective or context_cycle.` (line 147) | Update doc |
| `crates/unimatrix-server/src/server.rs` | `/// Shared with UDS listener; drained by context_retrospective handler.` (line 207) | Update doc |
| `crates/unimatrix-observe/src/types.rs` | `/// Complete analysis output returned by context_retrospective.` (line 221) | Update doc |
| `crates/unimatrix-observe/src/session_metrics.rs` | `assert_eq!(classify_tool("context_retrospective"), "other");` (line 601) | Update test assertion to `context_cycle_review` |

### Python Integration Test Files

| File | Reference type | What to change |
|------|----------------|----------------|
| `product/test/infra-001/harness/client.py` | `def context_retrospective(` method definition (line 629) | Rename method to `context_cycle_review` |
| `product/test/infra-001/harness/client.py` | `return self.call_tool("context_retrospective", args, ...)` (line 642) | Update tool name string |
| `product/test/infra-001/suites/test_protocol.py` | `"context_retrospective"` in tool list (line 55) | Update to `context_cycle_review` |
| `product/test/infra-001/suites/test_tools.py` | `# === context_retrospective (col-002) ===` section header (line 768) | Update comment |
| `product/test/infra-001/suites/test_tools.py` | `resp = server.context_retrospective(...)` calls (lines 773, 779, 785) | Update to `context_cycle_review` |
| `product/test/infra-001/suites/test_tools.py` | `# === context_retrospective baseline comparison (col-002b) ===` header (line 789) | Update comment |
| `product/test/infra-001/suites/test_tools.py` | `context_retrospective can find them via SqlObservationSource.` doc string (line 814) | Update |
| `product/test/infra-001/suites/test_tools.py` | `resp = server.context_retrospective(...)` calls (lines 893, 897, 935, 939, 966) | Update all to `context_cycle_review` |
| `product/test/infra-001/suites/test_tools.py` | `# === context_retrospective format dispatch (vnc-011) ===` header (line 983) | Update |
| `product/test/infra-001/suites/test_tools.py` | `resp = server.context_retrospective(...)` calls (lines 996, 1009, 1022) | Update all |

### Protocol and Skill Files

| File | Reference type | What to change |
|------|----------------|----------------|
| `.claude/skills/uni-retro/SKILL.md` | `mcp__unimatrix__context_retrospective(feature_cycle: ...)` (line 29) | Update to `context_cycle_review` |
| `.claude/protocols/uni/uni-agent-routing.md` | `Data gathering (context_retrospective + artifact review)` (line 151) | Update |
| `packages/unimatrix/skills/retro/SKILL.md` | `mcp__unimatrix__context_retrospective(feature_cycle: ...)` (line 29) | Update to `context_cycle_review` |
| `product/workflow/base-001/protocol-evolved/uni-agent-routing.md` | `context_retrospective` reference | Update |

### Product Vision and README

| File | Reference type | What to change |
|------|----------------|----------------|
| `product/PRODUCT-VISION.md` | Multiple references to `context_retrospective` (lines 32, 43, 282, 819) | Update all |
| `README.md` | Tool table row for `context_retrospective` (line 218) | Update tool name |
| `product/ALPHA_UNIMATRIX_COMPLETED_VISION.md` | References to `context_retrospective` | Update |

### Historical Feature and Research Documents

The following files contain references in historical/completed-feature context. They describe what the tool was called when those features were implemented — these references are historical records and updating them would rewrite history. They are **deliberately excluded** from the rename:

- All files under `product/features/col-002/`, `col-002b/`, `col-009/`, `col-010/`, `col-010b/`, `col-012/`, `col-014/`, `col-016/`, `col-017/`, `col-020/`, `col-020b/`, `col-022/` — these are completed feature artifacts.
- All files under `product/features/vnc-005/`, `vnc-008/`, `vnc-009/`, `vnc-011/` — completed feature artifacts.
- All files under `product/features/nxs-008/`, `nxs-009/` — completed feature artifacts.
- All files under `product/research/ass-007/`, `ass-014/`, `ass-015/`, `ass-016/`, `ass-018/`, `ass-020/`, `ass-022/` — research findings.
- All files under `product/features/crt-011/`, `crt-018/`, `crt-018b/` — completed/historical.
- All files under `product/features/bugfix-236/` — historical bugfix record.
- `product/research/optimizations/` — historical analysis.

**Delivery team note**: The grep sweep `grep -r "context_retrospective" .` before the PR opens should show results only in the excluded historical files listed above. Any match outside those directories is a missed update that must be fixed before merge.

---

## NOT In Scope

- **Runtime config reload** — config is loaded once at startup; changes require a server restart.
- **Environment variable override for config path** — no `UNIMATRIX_CONFIG` env var is introduced. Existing env vars (`UNIMATRIX_TICK_INTERVAL_SECS`, `UNIMATRIX_AUTO_QUARANTINE_CYCLES`) are not replaced or merged.
- **Per-session or per-agent config** — config is global to the server instance.
- **Schema migration** — no new DB tables; no schema version bump.
- **OAuth / authentication config** — deferred per ADR #1839 (W0-2 deferral).
- **Config tooling** — no `unimatrix config show` subcommand, no `--validate` flag.
- **Domain packs** — dsn-001 provides the hook points; domain pack loading is a separate feature.
- **Raw weight tuning as a primary interface** — operators select presets, not raw `W_TRUST = 0.22` values. `[confidence] weights` is the expert escape hatch only.
- **Coherence gate lambda weights** (`confidence_freshness`, `graph_quality`, `embedding_consistency`, `contradiction_density`) — these are KB-health metric weights for `compute_lambda()`. They remain hardcoded in `coherence.rs` and are not configurable.
- **`[cycle]` config section** — vocabulary fix for the cycle tool concept is a hardcoded rename, not runtime config. The `CycleConfig` stub is removed.
- **Renaming `context_cycle`** — the name is already domain-neutral.
- **Externalizing `PROVENANCE_BOOST` magnitude** (0.02) — only which categories receive the boost is configurable, not its magnitude.
- **Adaptive blend weight parameters** (`observed_spread * 1.25`, clamp bounds `[0.15, 0.25]`) — these are part of the crt-019 adaptive system, not static config.
- **Full bootstrap agent list configurability** (`system`, `human`, `cortical-implant`) — only `default_trust` and `session_capabilities` are externalised. Full bootstrap configurability requires significant store-layer refactoring and adds no domain-agnosticism value.
- **Bridge mode config** — `tokio_main_bridge` does not load config.
- **Hook path config** — `Command::Hook` is a sync sub-50ms path and must not load config.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for config externalization specification AC patterns — no results (this is the first feature of its kind in this codebase; no prior config or externalization patterns are stored).
