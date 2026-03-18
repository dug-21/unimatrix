# dsn-001 Test Plan — startup-wiring

Component: `crates/unimatrix-server/src/main.rs`
Also covers: `background.rs` (Arc<ConfidenceParams> at spawn)

Risks covered: R-13, R-15, R-20, IR-04, AC-01, AC-07 (integration path).

---

## Scope of Changes

`tokio_main_daemon` and `tokio_main_stdio` gain the following steps after
`ensure_data_directory()`:

```rust
let config = load_config(home_dir, &paths.data_dir)?;
let confidence_params = Arc::new(resolve_confidence_params(&config)?);
```

Extracted values passed to subsystem constructors:
- `config.knowledge.categories` → `CategoryAllowlist::from_categories(...)`
- `config.knowledge.boosted_categories` → `SearchService` (as HashSet)
- `Arc<ConfidenceParams>` → background tick
- `config.server.instructions` → `UnimatrixServer::new(...)`
- `config.agents.permissive` → `AgentRegistry::new(...)`
- `config.agents.session_capabilities` → `agent_resolve_or_enroll`

`Command::Hook` and `tokio_main_bridge` must NOT load config.

---

## `load_config` Called Before Subsystems (R-13, ordering)

### Code Review Gate: ContentScanner warm ordering

`load_config` must contain `let _scanner = ContentScanner::global();` at its top,
before any `validate_config` call, with a comment explaining the ordering invariant.

In Stage 3c, verify:
```bash
grep -A 5 "fn load_config" crates/unimatrix-server/src/infra/config.rs
```

The function body must start with (or include near the top):
```rust
// ContentScanner must be warmed before validate_config calls scan_title().
// This ordering is required — do not move this call below validate_config.
let _scanner = ContentScanner::global();
```

If this comment is absent, it is a code-level requirement violation (ARCHITECTURE.md
§ContentScanner ordering, SPECIFICATION.md Constraint #9). Document in the
RISK-COVERAGE-REPORT.md.

---

## Hook Path and Bridge Mode Excluded (R-20)

### Code Review Gate: Command::Hook does not load config

In Stage 3c, verify:
```bash
grep -n "load_config" crates/unimatrix-server/src/main.rs
```

All `load_config` calls must be inside `tokio_main_daemon` and `tokio_main_stdio`
only. Zero calls in `hook_main`, `bridge_main`, or export/import subcommand paths.

### test_hook_path_not_in_load_config_scope (static)

This is a grep audit, not an executable test. Include the grep result in the
RISK-COVERAGE-REPORT.md: "R-20 gate: load_config called in [N] locations: [list].
Locations: [function names]. All in tokio_main_daemon/stdio — PASS."

---

## `dirs::home_dir()` None Degrades Gracefully (R-15)

### test_load_config_with_no_home_dir_uses_defaults

`load_config` signature is `(&Path, &Path) -> Result<UnimatrixConfig, ConfigError>`.
If the caller passes home_dir from `dirs::home_dir()` which returns `None`, the
`main.rs` code must handle the `None` case gracefully before calling `load_config`.

The behavior: when `dirs::home_dir()` returns `None`, use `UnimatrixConfig::default()`
with a `tracing::warn!` — do not call `load_config` at all, or call it with a path
that results in no-config defaults.

```rust
fn test_main_startup_handles_no_home_dir() {
    // This is an integration test of the main.rs startup path.
    // A unit test approximation: verify that UnimatrixConfig::default() is
    // equivalent to what the server uses when home_dir is None.
    let config = UnimatrixConfig::default();
    let params = resolve_confidence_params(&config).unwrap();
    // Should equal ConfidenceParams::default() — no-config behavior.
    assert_eq!(params, ConfidenceParams::default());
    // No panic, no abort.
}
```

The actual `None` handling in `main.rs` must be verified by code review:
```bash
grep -A 10 "home_dir" crates/unimatrix-server/src/main.rs
```
Must show a `None` arm that logs a warning and continues with defaults.

---

## Subsystems Receive Correct Values (AC-01, IR-04)

### test_config_default_passes_correct_categories_to_allowlist

```rust
fn test_default_config_categories_match_initial_categories() {
    let config = UnimatrixConfig::default();
    // Default config must pass INITIAL_CATEGORIES to CategoryAllowlist.
    assert_eq!(
        config.knowledge.categories,
        INITIAL_CATEGORIES.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        "Default UnimatrixConfig must have INITIAL_CATEGORIES"
    );
}
```

### test_default_config_boosted_categories_is_lesson_learned

```rust
fn test_default_config_boosted_categories_is_lesson_learned() {
    let config = UnimatrixConfig::default();
    assert_eq!(
        config.knowledge.boosted_categories,
        vec!["lesson-learned".to_string()],
        "Default boosted_categories must be ['lesson-learned'] for backward compat"
    );
}
```

### test_default_config_agents_permissive_is_true

```rust
fn test_default_config_agents_permissive_is_true() {
    let config = UnimatrixConfig::default();
    // default_trust = "permissive" — must produce permissive=true.
    assert!(config.agents.permissive(),
        "Default AgentsConfig must produce permissive=true");
}
```

---

## Background Tick Receives Correct ConfidenceParams (IR-04)

The background tick receives `Arc<ConfidenceParams>` at spawn time. If config is
fully loaded and merged before `Arc::new(resolve_confidence_params(&config)?)`, the
tick operates on the correct params for the server's lifetime.

### test_background_tick_params_not_stale

This is tested through system behavior: with `preset = "empirical"` (w_fresh=0.34,
half_life=24h), a background confidence refresh on a known entry must produce a
score reflecting higher freshness weighting than collaborative defaults would.

Unit-level proxy test:
```rust
fn test_arc_confidence_params_from_empirical_preset() {
    let config = UnimatrixConfig {
        profile: ProfileConfig { preset: Preset::Empirical },
        ..Default::default()
    };
    let params = Arc::new(resolve_confidence_params(&config).unwrap());
    // The Arc<ConfidenceParams> passed to background tick must have empirical values.
    assert!((params.w_fresh - 0.34).abs() < 1e-9,
        "background tick params must carry empirical w_fresh=0.34");
    assert!((params.freshness_half_life_hours - 24.0).abs() < 1e-9,
        "background tick params must carry empirical half_life=24.0h");
}
```

The integration-level verification (tick actually uses these params) is covered by
IR-04 scenario: start server with `empirical` preset, trigger a background refresh,
assert the resulting confidence score reflects higher freshness weight.

---

## Two-Level Merge Integration (AC-07)

This test requires running a server with specific global and per-project configs.
If the harness config-injection fixture is available:

```python
def test_two_level_merge_replace_semantics(config_server):
    """AC-07: per-project categories replace global categories entirely."""
    # config_server: global categories=["a","b"], per-project categories=["c"]
    # After merge, effective categories must be ["c"] only.
    resp = config_server.context_store(
        title="test entry",
        content="content",
        category="c",
        topic="test"
    )
    assert resp["status"] == "ok", "category 'c' must be accepted (in per-project list)"

    resp = config_server.context_store(
        title="test entry 2",
        content="content",
        category="a",  # from global list, not per-project
        topic="test"
    )
    assert resp["status"] != "ok" or "error" in resp, \
        "category 'a' must be rejected (per-project list replaced global list)"
```

If harness fixture not available: document as gap. The unit test for `merge_configs`
in `config-loader.md` covers the merge logic.
