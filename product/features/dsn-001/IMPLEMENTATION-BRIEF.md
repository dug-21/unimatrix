# dsn-001 Implementation Brief — Config Externalization (W0-3)

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/dsn-001/SCOPE.md |
| Scope Risk Assessment | product/features/dsn-001/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/dsn-001/architecture/ARCHITECTURE.md |
| Specification | product/features/dsn-001/specification/SPECIFICATION.md |
| Risk/Test Strategy | product/features/dsn-001/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/dsn-001/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| config loader (`infra/config.rs`) | pseudocode/config-loader.md | test-plan/config-loader.md |
| config distribution (`main.rs`) | pseudocode/config-distribution.md | test-plan/config-distribution.md |
| CategoryAllowlist (`infra/categories.rs`) | pseudocode/category-allowlist.md | test-plan/category-allowlist.md |
| ConfidenceParams (`unimatrix-engine`) | pseudocode/confidence-params.md | test-plan/confidence-params.md |
| SearchService (`services/search.rs`) | pseudocode/search-service.md | test-plan/search-service.md |
| AgentRegistry (`infra/registry.rs`) | pseudocode/agent-registry.md | test-plan/agent-registry.md |
| UnimatrixServer (`server.rs`) | pseudocode/server-instructions.md | test-plan/server-instructions.md |
| Tool vocabulary fixes (`mcp/tools.rs`) | pseudocode/tool-rename.md | test-plan/tool-rename.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

Note: pseudocode and test-plan files are produced in Session 2 Stage 3a. The Component Map
lists expected components from the architecture. The Cross-Cutting Artifacts section tracks
files consumed by specific stages.

---

## Goal

Replace four categories of hardcoded constants in the `unimatrix-server` binary with values
loaded from a two-level TOML config system (`~/.unimatrix/config.toml` global,
`~/.unimatrix/{hash}/config.toml` per-project), validated at startup with security-critical
checks that abort on violation. Additionally perform a hardcoded rename of the
`context_retrospective` MCP tool to `context_cycle_review` and neutralise the
`CycleParams.topic` field documentation to remove domain-specific vocabulary. After this
feature, Unimatrix can be deployed for non-software-delivery domains by supplying a config
file without recompiling.

---

## OWNER DECISION REQUIRED — Vision Variances

The following three variances were identified by the vision guardian. They do not block
delivery but require explicit owner acknowledgement before the PR is opened.

### VARIANCE-1 (WARN): Confidence dimension weights dropped from W0-3

The PRODUCT-VISION W0-3 section lists `[confidence] weights` (lambda weights:
`confidence_freshness`, `graph_quality`, `embedding_consistency`, `contradiction_density`)
as a first-class deliverable and marks "Lambda dimension weights hardcoded" as CRITICAL in
the Critical Gaps table. The vision states these weights are "the interim fix that bridges
the gap until W3-1's GNN learns them automatically" and that W3-1 cold-start degrades to
"config-defined defaults (W0-3 `[confidence] weights`)".

SCOPE.md drops them as a non-goal with the rationale that operators cannot meaningfully tune
ML weights and the GNN cold-starts from internal defaults.

An empty `ConfidenceConfig` stub is included (ADR-004) so no TOML format break occurs, but
the semantic gap remains: W3-1 will need to add `weights` to `ConfidenceConfig` before
operator-configured cold-start defaults are possible.

**Owner must choose**:
1. Accept deferral — W3-1 adds `weights` to `ConfidenceConfig` when that feature is scoped.
   Update the vision's W0-3 description to note the deferral. No code change to dsn-001.
2. Add minimal stub fields — Add `Option` weight fields to `ConfidenceConfig` now so the
   W3-1 design has a concrete hook point without a later format change.
3. Restore scope — Add the lambda weights to W0-3. Aligns with the vision but contradicts
   the SCOPE.md rationale.

### VARIANCE-2 (WARN): `[cycle]` label configurability dropped — replaced with doc-fix

The PRODUCT-VISION W0-3 section includes a `[cycle]` section with `work_context_label` and
`cycle_label` as runtime-configurable fields. SCOPE.md replaces this with a hardcoded
doc-fix on `CycleParams.topic` (FR-019) on the grounds that the tool concept is already
domain-neutral. An empty `CycleConfig` stub is included.

The practical impact is lower than VARIANCE-1 because `feature_cycle` and `topic` are
free-form string fields — operators can supply their domain's identifiers without a config
change. However, operators deploying for SRE or legal cannot change the tool description
vocabulary without recompiling.

**Owner must choose**:
1. Accept deferral — Doc-fix (FR-019) is sufficient; `[cycle]` stub reserves the namespace.
   Label configurability added if a real deployment requires it. No change to dsn-001.
2. Restore scope — Add `work_context_label` and `cycle_label` to `CycleConfig` and wire to
   tool description generation. Aligns with vision but adds complexity not yet justified by
   operator demand.

### VARIANCE-3 (WARN): `default_trust` default is `"permissive"` — vision shows `"restricted"`

The PRODUCT-VISION W0-3 config example shows `default_trust = "restricted"`. All three source
documents use `"permissive"` as the compiled default (preserving the current
`PERMISSIVE_AUTO_ENROLL = true` constant). This is internally consistent and correct given
the W0-2 deferral rationale (no security value before OAuth), but diverges from the vision's
stated example value. If operators read the vision and set nothing, they may expect
`"restricted"` behavior but receive `"permissive"`.

**Owner must confirm**: `"permissive"` is the intended W0-3 compiled default. No code change
needed; the vision's W0-3 example config should be updated to show `"permissive"` (or add a
clarifying note about the W0-2 deferral).

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Bare `freshness_half_life_hours: f64` parameter vs. `ConfidenceParams` struct for engine API | Introduce `ConfidenceParams` struct (`freshness_half_life_hours`, `alpha0`, `beta0`); absorbs W3-1 additions without further API churn; all existing call sites migrate to `&ConfidenceParams::default()` | SR-02, SPEC C-02, OQ-01 | architecture/ADR-001-confidence-params-struct.md |
| Where `UnimatrixConfig` lives — `unimatrix-server`, `unimatrix-core`, or new crate | `unimatrix-server/src/infra/config.rs` only; `toml` dependency contained to server crate; no `Arc<UnimatrixConfig>` crosses any crate boundary; plain values passed as parameters | SR-08, SPEC C-04, OQ-03 | architecture/ADR-002-config-type-placement.md |
| Merge semantics for two-level config — replace vs. extend for list fields | Replace semantics: per-project field overrides global field; list fields replace entirely; `Option<T>` intermediate deserialization distinguishes "explicitly set" from "absent" to avoid R-03 false-negative | SR-06, SPEC FR-003, OQ-04 | architecture/ADR-003-two-level-config-merge.md |
| Forward-compatibility stubs for `[confidence]` and `[cycle]` sections | Empty `ConfidenceConfig {}` and `CycleConfig {}` structs with `#[serde(default)]` reserve TOML namespace for W3-1; no active fields in W0-3 | SR-04, SPEC FR-008/FR-009 | architecture/ADR-004-forward-compat-stubs.md |

---

## Files to Create / Modify

### New files

| File | Summary |
|------|---------|
| `crates/unimatrix-server/src/infra/config.rs` | `UnimatrixConfig` and five sub-structs; `load_config()`; `validate_config()`; file permission check; 64 KB size cap; two-level merge |

### Modified files

| File | Change |
|------|--------|
| `crates/unimatrix-server/Cargo.toml` | Add `toml = "0.8"` (exact pin) |
| `crates/unimatrix-server/src/main.rs` | Call `load_config()` after `ensure_data_directory()`; distribute plain values to subsystem constructors in `tokio_main_daemon` and `tokio_main_stdio` |
| `crates/unimatrix-server/src/infra/categories.rs` | Add `CategoryAllowlist::from_categories(Vec<String>) -> Self`; make `new()` delegate to `from_categories(INITIAL_CATEGORIES.to_vec())` |
| `crates/unimatrix-server/src/infra/registry.rs` | Replace `const PERMISSIVE_AUTO_ENROLL` with a `permissive: bool` constructor parameter on `AgentRegistry::new()` |
| `crates/unimatrix-server/src/server.rs` | Replace `SERVER_INSTRUCTIONS` const with the value passed into `UnimatrixServer::new()`; update three comments referencing `context_retrospective` |
| `crates/unimatrix-server/src/services/search.rs` | Replace four `entry.category == "lesson-learned"` comparisons with `boosted_categories: HashSet<String>` field lookup |
| `crates/unimatrix-server/src/mcp/tools.rs` | Rename `context_retrospective` → `context_cycle_review` in `#[tool(name)]`, function name, audit log strings, cross-references, and doc comments; neutralise `CycleParams.topic` field doc |
| `crates/unimatrix-engine/src/confidence.rs` | Introduce `ConfidenceParams` struct; change `freshness_score()` and `compute_confidence()` to accept `&ConfidenceParams`; keep `FRESHNESS_HALF_LIFE_HOURS` as the default-backing constant only |
| `crates/unimatrix-store/src/registry.rs` | Add third parameter `session_caps: Option<&[Capability]>` to `SqlxStore::agent_resolve_or_enroll()`; existing callers pass `None` |
| `crates/unimatrix-observe/src/types.rs` | Update comment referencing `context_retrospective` |
| `crates/unimatrix-observe/src/session_metrics.rs` | Update `classify_tool("context_retrospective")` test string |
| `product/test/infra-001/harness/client.py` | Rename `context_retrospective()` method; update call-site tool name string |
| `product/test/infra-001/suites/test_tools.py` | Update 11 `server.context_retrospective(...)` call sites and 3 section comments |
| `product/test/infra-001/suites/test_protocol.py` | Change `"context_retrospective"` to `"context_cycle_review"` in expected tool list |
| `.claude/protocols/uni/uni-agent-routing.md` | Update one `context_retrospective` reference |
| `.claude/skills/uni-retro/SKILL.md` | Update `mcp__unimatrix__context_retrospective` call |
| `packages/unimatrix/skills/retro/SKILL.md` | Update `mcp__unimatrix__context_retrospective` call |
| `README.md` | Update `context_retrospective` table row to `context_cycle_review` |

All engine test files that call `compute_confidence` or `freshness_score` with positional
args must be migrated to `&ConfidenceParams`. The RISK-TEST-STRATEGY identifies 13 call-site
files:
`pipeline_regression.rs`, `pipeline_calibration.rs`, `test_scenarios_unit.rs`,
`test_scenarios.rs`, `coherence.rs`, `response/mod.rs`, `response/status.rs`, `tools.rs`,
`server.rs`, `services/confidence.rs`, `services/usage.rs`, `services/status.rs`,
`confidence.rs`.

---

## Data Structures

```rust
// unimatrix-server/src/infra/config.rs

pub struct UnimatrixConfig {
    pub knowledge:  KnowledgeConfig,   // [knowledge] section
    pub server:     ServerConfig,      // [server] section
    pub agents:     AgentsConfig,      // [agents] section
    pub confidence: ConfidenceConfig,  // [confidence] — reserved, no fields (W3-1)
    pub cycle:      CycleConfig,       // [cycle] — reserved, no fields (future)
}

pub struct KnowledgeConfig {
    pub categories:               Vec<String>,  // default: INITIAL_CATEGORIES (8 values)
    pub boosted_categories:       Vec<String>,  // default: ["lesson-learned"]
    pub freshness_half_life_hours: f64,         // default: 168.0
}

pub struct ServerConfig {
    pub instructions: String,  // default: SERVER_INSTRUCTIONS const
}

pub struct AgentsConfig {
    pub default_trust:         String,       // default: "permissive"
    pub session_capabilities:  Vec<String>,  // default: ["Read","Write","Search"]
}

pub struct ConfidenceConfig {}  // empty stub, reserved for W3-1
pub struct CycleConfig {}       // empty stub, reserved for future use

pub enum ConfigError {
    Io      { path: PathBuf, source: io::Error },
    TooLarge{ path: PathBuf, size: usize },
    Permission { path: PathBuf, reason: &'static str },
    Parse   { path: PathBuf, detail: String },
    Validation { field: String, value: String, constraint: String },
}
```

```rust
// unimatrix-engine/src/confidence.rs

pub struct ConfidenceParams {
    pub freshness_half_life_hours: f64,  // default: FRESHNESS_HALF_LIFE_HOURS (168.0)
    pub alpha0:                    f64,  // default: COLD_START_ALPHA
    pub beta0:                     f64,  // default: COLD_START_BETA
}

impl Default for ConfidenceParams { ... }  // returns current compiled constants
```

### Merge intermediate (internal to config.rs)

The two-level merge uses `Option<T>` intermediate structs during deserialization (not
exposed in the public API). The final `UnimatrixConfig` contains only `T`, never `Option<T>`.
This correctly distinguishes "absent from file" from "explicitly set to the default value"
and avoids the R-03 false-negative where a per-project value equal to the compiled default
would be silently dropped in favour of the global value.

---

## Function Signatures

```rust
// config.rs
pub fn load_config(
    home_dir: Option<&Path>,
    data_dir: &Path,
) -> Result<UnimatrixConfig, ConfigError>;

fn try_load_file(path: &Path) -> Result<Option<UnimatrixConfig>, ConfigError>;
// reads ≤64 KB, checks permissions (#[cfg(unix)]), deserializes, validates

fn validate_config(config: &UnimatrixConfig, path: &Path) -> Result<(), ConfigError>;
// enforces all field constraints; called for each file before merge

fn merge_configs(global: UnimatrixConfig, project: UnimatrixConfig) -> UnimatrixConfig;
// field-by-field replace: per-project non-default values win over global

// categories.rs
impl CategoryAllowlist {
    pub fn new() -> Self;                              // delegates to from_categories(INITIAL_CATEGORIES)
    pub fn from_categories(cats: Vec<String>) -> Self; // production path
}

// confidence.rs (engine)
pub fn freshness_score(
    last_accessed_at: u64, created_at: u64, now: u64,
    params: &ConfidenceParams,
) -> f64;

pub fn compute_confidence(
    entry: &EntryRecord, now: u64,
    params: &ConfidenceParams,
) -> f64;

// registry.rs (store)
pub async fn agent_resolve_or_enroll(
    &self,
    agent_id: &str,
    permissive: bool,
    session_caps: Option<&[Capability]>,
) -> Result<AgentRecord>;
```

---

## Constraints

1. `toml = "0.8"` added to `unimatrix-server/Cargo.toml` only (exact pin, not caret). Run
   `cargo tree` after adding to surface transitive conflicts before implementation begins.
2. No `Arc<UnimatrixConfig>` crosses any crate boundary. Config values cross as plain
   parameters (`bool`, `Vec<String>`, `Vec<Capability>`, `String`).
3. `ContentScanner::global()` must be called (and thus initialized via `OnceLock`) at the
   top of `load_config` before `validate_config` runs `scan_title()`. Place an explicit
   `let _scanner = ContentScanner::global();` with a comment explaining the ordering
   invariant (SR-03).
4. File permission check is `#[cfg(unix)]` only. No behavior change on Windows.
5. Config is loaded once at startup — `tokio_main_daemon` and `tokio_main_stdio` only.
   `tokio_main_bridge`, `Command::Hook`, and export/import subcommands do not load config.
6. `dirs::home_dir()` returning `None` must not abort startup — degrade to defaults with
   `tracing::warn!`.
7. `agent_bootstrap_defaults()` (system/human/cortical-implant) stays hardcoded. Only
   `PERMISSIVE_AUTO_ENROLL` and session capabilities are externalised.
8. No DB schema migration. Config is purely runtime state.
9. `rmcp` pin stays at `=0.16.0`. No version change.
10. `[confidence]` and `[cycle]` stubs must remain empty in W0-3's PR — no active fields.
11. The rename from `context_retrospective` to `context_cycle_review` is total — the
    zero-match grep (`grep -r "context_retrospective" . --include="*.rs" --include="*.py"
    --include="*.md" --include="*.toml"`) is a mandatory gate before the PR opens. Build
    passing is necessary but not sufficient.
12. `validate_config()` must be independently unit-testable (no tokio, no store required).

---

## Dependencies

| Dependency | Type | Notes |
|-----------|------|-------|
| `toml = "0.8"` | New crate | Add to `unimatrix-server/Cargo.toml` as exact pin |
| `serde` with `derive` feature | Existing | Already in `unimatrix-server`; needed for `Deserialize` on config structs |
| `dirs` | Existing | Already in `unimatrix-server`; provides `dirs::home_dir()` |
| `ContentScanner` | Existing internal | `unimatrix-server/src/infra/scanning.rs`; `scan_title()` for instructions validation |
| `CategoryAllowlist` | Existing internal | `unimatrix-server/src/infra/categories.rs`; seeded from config |
| `PERMISSIVE_AUTO_ENROLL` + `agent_resolve_or_enroll` | Existing internal | `unimatrix-server/src/infra/registry.rs` + `unimatrix-store/src/registry.rs` |
| `SERVER_INSTRUCTIONS` | Existing internal | `unimatrix-server/src/server.rs:179` |
| `FRESHNESS_HALF_LIFE_HOURS` + `freshness_score()` + `compute_confidence()` | Existing internal | `unimatrix-engine/src/confidence.rs:37,148` |
| `project::ensure_data_directory()` | Existing internal | `main.rs`; provides `paths.data_dir` for per-project config path |

---

## NOT In Scope

- Confidence dimension weights (`W_BASE`, `W_USAGE`, `W_FRESH`, `W_HELP`, `W_CORR`, `W_TRUST`)
- Coherence gate lambda weights (`freshness`, `graph`, `contradiction`, `embedding`) in `coherence.rs`
- `PROVENANCE_BOOST` magnitude (`0.02`) — only which categories receive it is configurable
- Adaptive blend weight parameters (`observed_spread * 1.25`, clamp bounds `[0.15, 0.25]`)
- `agent_bootstrap_defaults()` configurability (system/human/cortical-implant full bootstrap list)
- Active fields in `[confidence]` or `[cycle]` TOML sections
- Renaming `context_cycle` (already domain-neutral)
- Runtime config reload (config is loaded once at startup; restart required for changes)
- `UNIMATRIX_CONFIG` environment variable for overriding global config path
- Config tooling (`validate` subcommand, `unimatrix config show`)
- Domain packs (W0-3 provides hook points; loading is a separate feature)
- OAuth / authentication config (deferred per W0-2)
- Per-session or per-agent config overrides
- DB schema migration
- `toml` crate upgrade beyond `0.8`

---

## Alignment Status

**Overall: WARN — three vision variances require owner acknowledgement.**

The feature direction (moving hardcoded constants to TOML config for domain-agnostic
deployment) is fully aligned with the product vision's core principle. All five items listed
in the vision's Critical Gaps table for W0-3 are addressed **except** the lambda dimension
weights (VARIANCE-1).

| Check | Status |
|-------|--------|
| Vision Alignment | WARN — see VARIANCE-1, VARIANCE-2, VARIANCE-3 above |
| Milestone Fit | PASS — correctly Wave 0; no W1/W2/W3 capabilities introduced prematurely |
| Scope Gaps | PASS — all SCOPE.md goals addressed in spec/arch/risk docs |
| Scope Additions | PASS — FR-008/FR-009 stubs and FR-020/FR-021 robustness requirements are justified additions, not scope creep |
| Architecture Consistency | PASS — all four SCOPE-RISK-ASSESSMENT risks resolved by ADRs |
| Risk Completeness | PASS — 13 risks with prioritised scenarios; security risks fully analysed |

One specification gap noted by the vision guardian: `categories = []` (empty list) is not
explicitly addressed in the validation table. The spec says `≤ 64` with no minimum. The
delivery team must define and test the boundary: either reject (no categories accepted after
restart is a degenerate but valid concern) or accept (consistent with `≤ 64` reading). The
chosen behavior must be documented in a code comment and tested.

The `ContentScanner::global()` warm-up ordering constraint (SR-03) is resolved via a
documented invariant (`let _scanner = ContentScanner::global()` at top of `load_config`)
rather than a type-system guarantee. This must be verified in code review — build passing
alone is not sufficient.
