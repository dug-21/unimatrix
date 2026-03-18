# Risk-Based Test Strategy: dsn-001 — Config Externalization (W0-3)

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | `ConfidenceParams` call site migration incomplete — `compute_confidence` / `freshness_score` still use compiled constants (`W_BASE`, `FRESHNESS_HALF_LIFE_HOURS`) instead of `params.w_*`; non-collaborative presets produce identical scores to collaborative at runtime | High | High | Critical |
| R-02 | SR-10 regression: `confidence_params_from_preset(Preset::Collaborative) != ConfidenceParams::default()` — a single wrong digit in the collaborative row diverges production confidence scores from pre-dsn-001 behavior while all existing tests pass | High | Med | Critical |
| R-03 | SR-09 sum invariant violated: a named preset row sums to a value other than 0.92 (or validation uses `<= 1.0` instead of `(sum - 0.92).abs() < 1e-9`), corrupting the confidence scale for every entry under that preset | High | Med | Critical |
| R-04 | SR-05 rename partial: `context_retrospective` renamed in Rust source but not in Python integration tests, protocol files, skill files, or CLAUDE.md — build passes, runtime callers break | High | High | Critical |
| R-05 | `custom` preset missing-field: server does not abort on missing `[confidence] weights` or missing `[knowledge] freshness_half_life_hours` — instead panics at confidence computation time | High | Med | Critical |
| R-06 | `freshness_half_life_hours` precedence chain wrong: named preset + `[knowledge]` override silently ignored, or `custom` + absent half_life does not abort startup | High | Med | Critical |
| R-07 | `[server] instructions` injection bypass: a prompt-injection string passes `validate_config()` and is returned verbatim in the MCP `initialize` handshake to every connecting agent | High | Med | High |
| R-08 | `[confidence] weights` silently active for named presets: `resolve_confidence_params()` reads `[confidence]` even when `preset != "custom"`, overriding preset weights without warning | High | Low | High |
| R-09 | Weight sum validation uses `sum <= 1.0` (SCOPE.md config comment) instead of `(sum - 0.92).abs() < 1e-9` (ADR-005 invariant) — crafted custom weights at 0.95 pass validation and break the confidence scale | High | Med | High |
| R-10 | Cross-level custom preset weight inheritance: per-project `preset = "custom"` without per-project `[confidence] weights` silently inherits global weights instead of aborting (ADR-003 prohibits cross-level inheritance) | High | Med | High |
| R-11 | `[agents] session_capabilities` containing `"Admin"` passes validation — operators can grant Admin to all auto-enrolled agents via config | High | Low | High |
| R-12 | `freshness_half_life_hours` validation gap: `NaN`, `Infinity`, `0.0`, or values > 87600.0 reach `freshness_score()` causing division-by-zero or NaN propagation through all confidence scores | High | Low | High |
| R-13 | `ContentScanner::global()` not warmed before `validate_config()` calls `scan_title()` — scanner singleton not initialized, injection scan returns incorrect result | Med | Med | Med |
| R-14 | `AgentRegistry` session_caps not propagated through `resolve_or_enroll` wrapper — new `session_caps` parameter receives `None` at the server-infra call site, so configured capability sets are never applied | High | Med | High |
| R-15 | `dirs::home_dir()` returning `None` panics instead of degrading gracefully to compiled defaults — breaks CI and container deployments | Med | Med | Med |
| R-16 | File size cap bypassed: implementation passes a `File` handle to `toml::from_reader()` instead of reading to a bounded buffer first — 64 KB cap is not enforced | Med | Med | Med |
| R-17 | `CategoryAllowlist::new()` behavior changes: existing tests start receiving config-driven categories instead of compiled defaults, causing test pollution | Med | Med | Med |
| R-18 | `from_preset(Custom)` called directly outside `resolve_confidence_params()` — unguarded panic in production on a code path that passes code review without a type-system guard | Med | Low | Med |
| R-19 | `boosted_categories` subset validation absent: a boosted category not in `categories` is accepted, applying the provenance boost to a label that can never enter the knowledge base | Med | Med | Med |
| R-20 | Hook path or bridge mode accidentally loads config — a file read in `Command::Hook` violates the sub-50ms sync budget | Med | Low | Med |
| R-21 | World-writable abort error message does not identify which file (global vs per-project) triggered the abort — operator cannot determine which config is the problem | Med | Med | Med |
| R-22 | Merge false-negative: per-project field explicitly set to the compiled default value is treated as "absent" by the `PartialEq(Default)` detection strategy, allowing the global value to silently win | Med | Med | Med |

---

## Risk-to-Scenario Mapping

### R-01: ConfidenceParams call site migration incomplete

**Severity**: High
**Likelihood**: High
**Impact**: `compute_confidence` and `freshness_score` use compiled constants (`W_BASE` = 0.16, `FRESHNESS_HALF_LIFE_HOURS` = 168.0) regardless of which preset the operator configured. Selecting `preset = "empirical"` (w_fresh = 0.34, half_life = 24.0) produces identical scores to `collaborative`. The preset system is a no-op at runtime. No compile error — the constants are still exported; call sites that were not updated compile correctly.

**Test Scenarios**:
1. Unit test: call `compute_confidence(entry, now, &ConfidenceParams { w_fresh: 0.34, ..Default::default() })` and assert the result differs measurably from `compute_confidence(entry, now, &ConfidenceParams::default())` for an entry with a known age. A compiled-constant implementation returns the same value for both.
2. Unit test: call `freshness_score(last, created, now, &params)` with `params.freshness_half_life_hours = 24.0` vs `168.0`; assert the ratio matches the expected exponential decay ratio. A compiled-constant implementation returns the same value for both.
3. Static audit: grep for `W_BASE`, `W_USAGE`, `W_FRESH`, `W_HELP`, `W_CORR`, `W_TRUST`, `FRESHNESS_HALF_LIFE_HOURS` appearing outside of `Default::default()` implementations and constant definitions — must return zero matches in `compute_confidence` and `freshness_score` bodies.
4. Run full test suite with `ConfidenceParams::default()` at all existing call sites; assert zero test failures (confirming `Default` = prior behavior).

**Coverage Requirement**: The weight fields in `ConfidenceParams` must be demonstrably load-bearing. The `empirical` preset's w_fresh (0.34 vs default 0.18) provides the sharpest distinguishing signal. Tests must fail if the compiled constants are re-inserted.

---

### R-02: SR-10 regression — collaborative preset diverges from ConfidenceParams::default()

**Severity**: High
**Likelihood**: Med
**Impact**: Every pre-dsn-001 confidence score is reproducible from `ConfidenceParams::default()`. If the `collaborative` row in `confidence_params_from_preset` carries even one digit error (e.g., `w_corr = 0.15` instead of `0.14`), scores drift invisibly. No existing test catches this because existing tests use `Default::default()` directly, not `from_preset(Collaborative)`.

**Test Scenarios**:
1. Mandatory SR-10 test — must be present in `unimatrix-server` before the PR opens:
   ```rust
   #[test]
   fn collaborative_preset_equals_default_confidence_params() {
       // SR-10: If this test fails, fix the weight table, not the test.
       assert_eq!(
           confidence_params_from_preset(Preset::Collaborative),
           ConfidenceParams::default()
       );
   }
   ```
2. Field-by-field assertions against the locked preset table: `w_base == 0.16`, `w_usage == 0.16`, `w_fresh == 0.18`, `w_help == 0.12`, `w_corr == 0.14`, `w_trust == 0.16`, `freshness_half_life_hours == 168.0`.
3. AC-22 verification: server started with no config file resolves `ConfidenceParams` equal to `ConfidenceParams::default()` — confirm through `resolve_confidence_params(&UnimatrixConfig::default())`.

**Coverage Requirement**: The SR-10 test is non-negotiable. The comment "fix the weight table, not the test" must be present verbatim. Field-level assertions catch single-digit typos that a structural `PartialEq` comparison would also catch, but make the expected values explicit for code readers.

---

### R-03: Preset weight sum invariant violated

**Severity**: High
**Likelihood**: Med
**Impact**: A preset that sums to 0.91 or 0.93 shifts the maximum achievable confidence for every entry in that domain. The coherence gate thresholds and search re-ranking were calibrated at the 0.92 invariant. Off-sum presets silently corrupt domain scores without any runtime error.

**Locked weight table (all rows verified against ADR-005):**

| Preset | w_base | w_usage | w_fresh | w_help | w_corr | w_trust | SUM  | half_life_h |
|--------|--------|---------|---------|--------|--------|---------|------|-------------|
| `collaborative` | 0.16 | 0.16 | 0.18 | 0.12 | 0.14 | 0.16 | 0.92 | 168.0 |
| `authoritative` | 0.14 | 0.14 | 0.10 | 0.14 | 0.18 | 0.22 | 0.92 | 8760.0 |
| `operational`   | 0.14 | 0.18 | 0.24 | 0.08 | 0.18 | 0.10 | 0.92 | 720.0 |
| `empirical`     | 0.12 | 0.16 | 0.34 | 0.04 | 0.06 | 0.20 | 0.92 | 24.0 |

**Test Scenarios**:
1. Unit test — for each named preset, compute `w_base + w_usage + w_fresh + w_help + w_corr + w_trust` and assert `(sum - 0.92).abs() < 1e-9`.
2. Unit test — assert each field value matches the locked table above exactly (e.g., `empirical.w_fresh == 0.34`, `authoritative.w_trust == 0.22`).
3. Unit test — `custom` preset validation: weights summing to `0.92` pass; weights summing to `0.91`, `0.93`, and `1.0` all abort with `CustomWeightSumInvariant`.
4. Unit test — weights summing to `0.95` abort with `CustomWeightSumInvariant` (catches the `<= 1.0` implementation mistake). This is the critical regression detector for the SCOPE.md config-comment error.

**Coverage Requirement**: All four named presets tested individually against the locked table. The `0.95` boundary case (R-09 overlap) is essential to detect wrong-invariant implementations.

---

### R-04: SR-05 rename partial — non-Rust references survive

**Severity**: High
**Likelihood**: High
**Impact**: The Rust tool handler is renamed; the Python integration harness still calls `context_retrospective`. The MCP router returns "tool not found" — a runtime failure invisible to `cargo build`. Protocol files referencing the old name misdirect agents that follow them.

**Test Scenarios**:
1. Static gate (mandatory pre-merge): `grep -r "context_retrospective" .` at the repository root returns zero matches outside the historically-excluded directories listed in SPECIFICATION.md §SR-05. This grep must be run as a pre-merge check, not just once during development.
2. Integration test: `test_protocol.py` tool list must contain `"context_cycle_review"` and must NOT contain `"context_retrospective"` (AC-13 positive and negative assertion).
3. Integration test: call `context_cycle_review(feature_cycle: "col-022")` via the Python harness (`client.py`) and assert a valid structured response — confirms the rename propagated through the MCP router.
4. Unit test: `classify_tool("context_cycle_review")` in `unimatrix-observe/src/session_metrics.rs` returns the expected classification (the old `"context_retrospective"` test assertion is updated).
5. Verify the audit log strings in `tools.rs` (lines ~1457, ~1734) reference `context_cycle_review` and `context_cycle_review/lesson-learned` respectively — not the old names.

**Coverage Requirement**: Build-passing is explicitly insufficient. The grep sweep and the Python integration call are both required gates. The SPECIFICATION.md SR-05 checklist enumerates 31 specific locations across 14 files — every item must be checked.

---

### R-05: `custom` preset missing-field permutations

**Severity**: High
**Likelihood**: Med
**Impact**: An operator writes `preset = "custom"` and omits either `[confidence] weights` or `[knowledge] freshness_half_life_hours`. If startup does not abort, `resolve_confidence_params()` encounters a `None` where it expects `Some` and either panics or silently uses zero/default values — producing incorrect confidence scores without any operator feedback.

**Test Scenarios** — four-case truth table (AC-25 and ADR-006):

| Case | `[confidence] weights` | `[knowledge] freshness_half_life_hours` | Expected |
|------|----------------------|-----------------------------------------|---------|
| Both present | `{ base=0.12, usage=0.16, fresh=0.34, help=0.04, corr=0.06, trust=0.20 }` | `24.0` | Startup succeeds; `ConfidenceParams` populated from these values |
| Weights absent | absent | `24.0` | Abort: `ConfigError::CustomPresetMissingWeights`; error names missing field |
| Half-life absent | present (valid) | absent | Abort: `ConfigError::CustomPresetMissingHalfLife`; error names missing field |
| Both absent | absent | absent | Abort: `CustomPresetMissingWeights` (detected first in validate_config order) |

1. Unit test for each row above by calling `validate_config()` with the corresponding `UnimatrixConfig` built directly.
2. Assert that `custom` + missing `[knowledge] freshness_half_life_hours` does NOT inherit the value from global config (cross-level inheritance is prohibited by ADR-003 — see R-10).
3. Assert that error messages name the specific missing field, not a generic "custom preset misconfigured".
4. Assert that the "both present" case populates `ConfidenceParams` with the supplied values, not the `collaborative` defaults.

**Coverage Requirement**: All four permutations must be named unit tests. `ConfigError` variant names must match SPECIFICATION.md §Domain Models exactly.

---

### R-06: `freshness_half_life_hours` precedence chain wrong

**Severity**: High
**Likelihood**: Med
**Impact**: An operator using `preset = "operational"` with `[knowledge] freshness_half_life_hours = 336.0` expects the override to take effect. If the precedence chain is wrong, the preset's built-in 720.0h is used silently. The operator has no feedback that the override was ignored.

**Test Scenarios** — four-case truth table (ADR-006 §freshness_half_life_hours precedence chain):

| Case | Preset | `[knowledge]` override | Expected `params.freshness_half_life_hours` |
|------|--------|----------------------|--------------------------------------------|
| Named, no override | `operational` | absent (`None`) | 720.0 (preset built-in) |
| Named, with override | `operational` | `Some(336.0)` | 336.0 |
| `custom`, half_life absent | `custom` | `None` | Abort (`CustomPresetMissingHalfLife`) |
| `custom`, half_life present | `custom` | `Some(24.0)` | 24.0 |

1. Unit test each row by calling `resolve_confidence_params()` with the appropriate `UnimatrixConfig` and asserting `params.freshness_half_life_hours`.
2. Unit test that `Preset::Collaborative` with `[knowledge] freshness_half_life_hours = Some(48.0)` resolves to `48.0` (override applies to all named presets including collaborative).
3. Audit: grep for assignments to `freshness_half_life_hours` outside of `config.rs` and `confidence.rs` defaults — confirm `resolve_confidence_params` is the single resolution site.

**Coverage Requirement**: All four precedence cases, plus the collaborative-with-override case. All must be named unit tests with the case description as a comment.

---

### R-07: `[server] instructions` injection bypass

**Severity**: High
**Likelihood**: Med
**Impact**: A malicious or misconfigured `instructions` string containing a prompt-injection payload is returned verbatim in the MCP `initialize` handshake to every connecting AI agent for the lifetime of the server process. The `ContentScanner` is a secondary defense — file permissions are the first.

**Test Scenarios**:
1. Unit test: `validate_config()` with `instructions` containing a known injection pattern from `ContentScanner`'s 26-regex set aborts with `ConfigError::InstructionsInjection`.
2. Unit test: `instructions` of exactly 8192 bytes passes the length check; 8193 bytes aborts with `ConfigError::InstructionsTooLong` before `scan_title()` runs.
3. Unit test: confirm the length check short-circuits before the scanner — a 9000-byte injection string returns `InstructionsTooLong`, not `InstructionsInjection`. This verifies the guard ordering in `validate_config`.
4. Unit test: a well-formed multi-line instructions string passes validation unchanged.
5. Integration test: server started with `instructions = "Test domain guidance."` returns that exact string in `ServerInfo.instructions` from the MCP `initialize` response (AC-05).

**Coverage Requirement**: Length-before-scan ordering is a security invariant that must be tested, not just assumed. Both guards must be independently tested as failure conditions.

---

### R-08: `[confidence] weights` silently active for named presets

**Severity**: High
**Likelihood**: Low
**Impact**: An operator sets `[confidence] weights` and `preset = "authoritative"`. If `resolve_confidence_params()` reads `[confidence]` for named presets, the operator's custom weights override the preset weights without any warning — the preset system is subverted. The preset selection becomes a no-op.

**Test Scenarios**:
1. Unit test: `resolve_confidence_params()` with `preset = Authoritative` and `confidence.weights = Some(ConfidenceWeights { base: 0.99, ... })` returns `ConfidenceParams` equal to `confidence_params_from_preset(Preset::Authoritative)`. The `[confidence]` values have no effect.
2. Unit test: the same call emits a `tracing::warn!` log entry indicating the `[confidence]` section was ignored (AC-23 warning behavior).
3. Unit test: the resolved `ConfidenceParams.w_trust` equals 0.22 (authoritative preset value) even when `[confidence] weights` specifies `trust = 0.50`.
4. Apply to all four named presets to confirm `[confidence]` is gated exclusively on `preset == Custom`.

**Coverage Requirement**: Named presets must be shown to be immune to `[confidence]` presence. Warn-and-ignore behavior must be tested, not just the final value.

---

### R-09: Weight sum validation using wrong invariant

**Severity**: High
**Likelihood**: Med
**Impact**: SCOPE.md config schema comment says `sum must be <= 1.0`. ADR-005 invariant is `(sum - 0.92).abs() < 1e-9`. An implementation using the SCOPE comment would accept `custom` weights summing to 0.95, breaking the confidence scale. Named preset tests written against the wrong threshold would also fail to detect invalid presets.

**Test Scenarios**:
1. Unit test: `validate_config()` with `custom` weights summing to `0.95` aborts with `CustomWeightSumInvariant` — proving `<= 1.0` is not the rule.
2. Unit test: `validate_config()` with `custom` weights summing to `0.92` (within `1e-9`) passes validation.
3. Unit test: `validate_config()` with `custom` weights summing to `0.920000001` (outside `1e-9` tolerance) aborts.
4. Unit test: `validate_config()` with `custom` weights summing to `0.919999999` (outside `1e-9` tolerance) aborts.
5. Code audit: confirm no occurrence of `sum <= 1.0` as a validation condition in `config.rs`.

**Coverage Requirement**: The `0.95` case is the critical regression detector for the SCOPE-comment mistake. Both sides of the `1e-9` boundary (slightly above and below 0.92) must be tested.

---

### R-10: Cross-level custom preset weight inheritance

**Severity**: High
**Likelihood**: Med
**Impact**: Global config has `[confidence] weights`. Per-project config sets `preset = "custom"` with no `[confidence]` section. ADR-003 explicitly prohibits cross-level weight inheritance. If the merge incorrectly grafts global `[confidence]` onto the per-project config, `custom` runs with silently-inherited weights — the operator has no idea which weights are active.

**Test Scenarios**:
1. Unit test: `merge_configs(global_with_confidence_weights, project_with_custom_and_no_weights)` → `validate_config()` aborts with `CustomPresetMissingWeights`. Global weights must NOT be visible.
2. Unit test: `merge_configs(global_with_no_confidence, project_with_custom_and_no_weights)` → same abort result. The prohibition holds regardless of global weight presence.
3. Unit test: `merge_configs(global_with_custom_and_weights_A, project_with_custom_and_weights_B)` → per-project weights B win; global weights A are not present in the merged config.

**Coverage Requirement**: All three cases must be named unit tests. The ADR-003 cross-level prohibition must be cited in a comment on each test.

---

### R-11: `[agents] session_capabilities` privilege escalation via Admin

**Severity**: High
**Likelihood**: Low
**Impact**: If `"Admin"` passes the `session_capabilities` allowlist validation, every unknown agent connecting to the server receives Admin-level access — bypassing the `alc-002` protected-agent system. Full server compromise for all connecting agents.

**Test Scenarios**:
1. Unit test (AC-19): `validate_config()` with `session_capabilities = ["Admin"]` aborts with `ConfigError::InvalidSessionCapability`.
2. Unit test: `session_capabilities = ["Read", "Admin"]` aborts — `Admin` is rejected even when mixed with valid values.
3. Unit test: `session_capabilities = ["Read", "Write", "Search"]` passes validation (valid permissive set).
4. Unit test: `session_capabilities = ["Read", "Search"]` passes validation (valid strict set).
5. Unit test: `session_capabilities = ["admin"]` (lowercase) aborts — case-insensitive or case-sensitive, the spec implies exact match; verify the behavior is deterministic.

**Coverage Requirement**: `Admin` exclusion must be an explicit allowlist check, not an implicit blocklist. All valid and invalid combinations must be tested.

---

### R-12: `freshness_half_life_hours` validation gap

**Severity**: High
**Likelihood**: Low
**Impact**: `NaN` or `Infinity` values reaching `freshness_score()` produce `NaN` throughout the confidence computation — all scores become `NaN`, propagating into search rankings and the coherence gate. `0.0` causes division-by-zero. Values > 87600.0 make all knowledge permanently fresh (score approaches 1.0 for any entry age).

**Test Scenarios**:
1. Unit test (AC-16): `validate_config()` with `freshness_half_life_hours = 0.0` aborts with `InvalidHalfLifeValue`.
2. Unit test: `freshness_half_life_hours = -1.0` aborts with `InvalidHalfLifeValue`.
3. Unit test: `freshness_half_life_hours = f64::NAN` aborts with `InvalidHalfLifeValue`.
4. Unit test: `freshness_half_life_hours = f64::INFINITY` aborts with `InvalidHalfLifeValue`.
5. Unit test (AC-17): `freshness_half_life_hours = 87600.001` aborts with `HalfLifeOutOfRange`.
6. Unit test: `freshness_half_life_hours = 87600.0` passes (inclusive upper bound).
7. Unit test: `freshness_half_life_hours = f64::MIN_POSITIVE` passes (smallest valid positive value).

**Coverage Requirement**: IEEE 754 special values (`NaN`, `Infinity`, `-0.0`) must each be tested explicitly. The `87600.0` exact-boundary case (inclusive) must pass.

---

### R-13: ContentScanner warm-up ordering violation

**Severity**: Med
**Likelihood**: Med
**Impact**: `load_config()` is called before `ContentScanner` is warmed in the startup sequence. If `validate_config()` calls `scan_title()` before `ContentScanner::global()` is initialized, the scanner's `OnceLock` initializes on first call — this is safe in the happy path but the ordering invariant is not tested. If `ContentScanner::global()` were refactored to a non-lazy pattern in future, the current implicit ordering assumption would break silently.

**Test Scenarios**:
1. Unit test: call `validate_config()` with a known injection string in `instructions` — must return `InstructionsInjection` when called without any prior `ContentScanner::global()` warm-up (verifying the warm-up inside `load_config` is not required for `validate_config` standalone testability).
2. Code review gate: `load_config` must contain `let _scanner = ContentScanner::global();` at its top, before any `validate_config` call, with a comment documenting the ordering invariant (ARCHITECTURE.md §ContentScanner ordering, SPECIFICATION.md Constraint #9).
3. Integration test: server startup with a clean `instructions` value succeeds — exercises the warm path in the full startup sequence.

**Coverage Requirement**: The explicit warm call with a comment is a code-level requirement. PR review must verify its presence.

---

### R-14: AgentRegistry session_caps not propagated through resolve_or_enroll wrapper

**Severity**: High
**Likelihood**: Med
**Impact**: `AgentRegistry::resolve_or_enroll()` in `infra/registry.rs` calls `store.agent_resolve_or_enroll(agent_id, permissive, session_caps)`. The server-infra wrapper must pass the config-derived `session_caps`. If the wrapper is updated to pass the config `permissive` bool but not `session_caps` (or passes `None`), configured capability sets are never applied to auto-enrolled agents. No compile error — `None` is a valid value.

**Test Scenarios**:
1. Integration test (AC-06 strict): server configured with `default_trust = "strict"`, `session_capabilities = ["Read", "Search"]`; call any tool as an unknown agent; assert enrolled agent's capability set is `[Read, Search]` and does not contain `Write`.
2. Integration test: server configured with `session_capabilities = ["Read"]`; enroll an unknown agent; assert capabilities are exactly `["Read"]`.
3. Unit test: the `resolve_or_enroll` call path in `infra/registry.rs` passes `Some(config_session_caps)`, not `None`, when the config specifies session capabilities.
4. Ensure tests exercise the full path through `AgentRegistry::resolve_or_enroll()` → `store.agent_resolve_or_enroll()`, not just the store method in isolation.

**Coverage Requirement**: End-to-end enrollment path must be covered. Store-layer tests alone are insufficient — the server-infra wrapper is the risk surface.

---

## Integration Risks

### IR-01: `unimatrix-engine` API change — full call site sweep

`ConfidenceParams` gains 6 new fields. `compute_confidence` and `freshness_score` signatures change. Callers in `unimatrix-engine` tests (~15 functions), `unimatrix-server` background tick, confidence refresh paths, and `unimatrix-observe` must all be updated. A partial migration compiles if struct-update syntax is used but produces incorrect runtime behavior if pre-migration positional args were used.

**Test scenario**: Run `cargo test --all` with `ConfidenceParams::default()` at all migrated call sites; assert zero test failures. Any test that asserts a specific confidence score and still passes after migration confirms `Default` = prior behavior. Any newly-failing test identifies a broken call site or a test that must be updated.

### IR-02: `agent_resolve_or_enroll` third parameter — compile-safe, behavior risk

All existing call sites must pass `None` as the third argument. Rust enforces arity — a missed site produces a compile error (safe failure mode). The behavior risk is the inverse: if `AgentRegistry::resolve_or_enroll()` passes `None` when it should pass `Some(config_caps)`, behavior is wrong but the code compiles. See R-14.

**Test scenario**: Integration test confirming that non-`None` `session_caps` actually flow through to the enrolled `AgentRecord.capabilities` field.

### IR-03: `SearchService` — all four hardcoded comparisons replaced

Four occurrences of `entry.category == "lesson-learned"` in `search.rs` (lines ~413, 418, 484, 489). A partial replacement leaves hardcoded behavior for some search paths, making the feature behave differently depending on which search code path is exercised.

**Test scenario**: `grep "lesson-learned"` in `search.rs` returns zero matches (AC-03). Integration test with `boosted_categories = ["decision"]` confirms "decision" entries receive boost while "lesson-learned" entries do not — demonstrating the hardcoded path is gone, not just supplemented.

### IR-04: Background tick receives stale `ConfidenceParams`

The background tick is spawned once at startup with `Arc<ConfidenceParams>`. If `Arc::new(resolve_confidence_params(&config)?)` runs before config is fully loaded and merged, the tick operates on defaults or partially-merged params for the server's entire lifetime.

**Test scenario**: Server started with `preset = "empirical"` (w_fresh = 0.34); trigger a background confidence refresh for a known entry; assert the entry's updated confidence score reflects higher freshness weighting than it would under the collaborative defaults.

### IR-05: `CategoryAllowlist::from_categories` vs `new()` delegation

`new()` must delegate to `from_categories(INITIAL_CATEGORIES.to_vec())`. If `new()` is independently reimplemented and the two implementations diverge, the default category set seen by existing tests differs from the set seen by config-initialized allowlists.

**Test scenario**: `CategoryAllowlist::new()` and `CategoryAllowlist::from_categories(INITIAL_CATEGORIES.iter().cloned().collect())` produce identical results. `new().is_allowed("outcome")` returns `true`; `new().is_allowed("hypothetical_new_category")` returns `false`.

---

## Edge Cases

### EC-01: Empty categories list

`categories = []` is syntactically valid TOML and passes character-constraint and count (`<= 64`) validation. An empty allowlist causes all `context_store` calls to fail post-restart (every category is rejected). The spec does not explicitly set a minimum count. Test and document the chosen behavior — either reject at validation time or accept as a valid (if degenerate) configuration.

### EC-02: `boosted_categories` as empty list

`boosted_categories = []` is valid — no categories receive the provenance boost. The `HashSet` in `SearchService` is empty. Verify no panic occurs in search re-ranking when the set is empty and confirm all search results are unaffected by the boost.

### EC-03: Custom weights with a weight at exactly 0.0

`weights = { base = 0.0, usage = ..., fresh = ..., help = ..., corr = ..., trust = ... }` — zero weights are in `[0.0, 1.0]` and the spec does not prohibit them. A zero weight means that dimension is unmonitored but the validation must not reject it. The sum invariant still applies.

### EC-04: `freshness_half_life_hours` at IEEE boundary values

- `87600.0` (exact upper bound): must pass.
- `87600.001`: must fail with `HalfLifeOutOfRange`.
- `-0.0` (IEEE negative zero): `> 0.0` is false for `-0.0` in IEEE 754 — must be rejected.
- `f64::MIN_POSITIVE` (smallest positive f64, ~5e-324): must pass as technically `> 0.0`, even though it produces extreme decay. Document whether this is intended or whether a practical minimum should be added.

### EC-05: Per-project config file exists but is empty (zero bytes)

An empty file is valid TOML (no content). Serde `#[serde(default)]` should produce `UnimatrixConfig::default()`. Must not be treated as a parse error. Merge result: per-project = defaults → all fields fall through to global or compiled defaults.

### EC-06: Config file at exactly 65536 bytes (64 KB boundary)

Must pass the size cap check (inclusive). A file at 65537 bytes must be rejected with `FileTooLarge`. Test both sides of the boundary.

### EC-07: Config file is a symlink to a world-writable target

`std::fs::metadata()` follows symlinks and reports target permissions. A symlink pointing to a world-writable file must abort startup — the attacker can write the file the server will read. Test: create symlink → world-writable file; assert `ConfigError::WorldWritable`.

### EC-08: `session_capabilities` with duplicate values

`session_capabilities = ["Read", "Read", "Write"]` — the spec validates against an allowlist but does not mention deduplication. If the implementation converts to a `HashSet`, duplicates are silently deduplicated. If stored as a `Vec`, duplicates may cause unexpected behavior in capability checks. Document and test the chosen behavior.

---

## Security Risks

### SR-SEC-01: `[server] instructions` — universal prompt injection surface

**Untrusted input**: Operator-controlled string (≤ 8 KB after validation), passed verbatim to every connecting AI agent in the MCP `initialize` handshake.
**Blast radius**: Every agent session is compromised from first connection, for the lifetime of the server process. The attack is persistent and universal.
**Mitigations**: (1) File permission enforcement — world-writable abort is the primary defense; (2) Length cap at 8 KB before scanner — limits payload size; (3) `ContentScanner.scan_title()` with 26 injection patterns — secondary pattern-based defense.
**Residual risk**: Novel injection techniques not in the 26-regex set bypass `ContentScanner`. File permission enforcement is the stronger defense — if an attacker controls the config file, no validation fully compensates.
**Test requirement**: R-07 scenarios 1-4. Verify length check precedes scanner (scenario 3).

### SR-SEC-02: `[agents] session_capabilities` — privilege escalation via Admin exclusion

**Untrusted input**: Capability strings validated against a closed allowlist.
**Blast radius**: If `"Admin"` passes, every auto-enrolled agent receives Admin-level access, bypassing `alc-002` protected-agent logic. Full server compromise.
**Mitigation**: Strict allowlist — only `{"Read", "Write", "Search"}` accepted.
**Test requirement**: R-11 scenarios 1-5. Explicit rejection of `"Admin"` (exact-case and lowercase).

### SR-SEC-03: `[knowledge] categories` — knowledge base schema gate

**Untrusted input**: Category name strings validated against `[a-z0-9_-]`, max 64 chars, max 64 total.
**Blast radius**: An empty or minimal category list blocks all `context_store` calls post-restart. Existing knowledge base data is not affected; only new entries fail.
**Residual path**: An operator with config write access who wants to deny new knowledge stores can set `categories = []`. This is a config integrity problem (not a bypass vulnerability), addressed by file permission enforcement.
**Test requirement**: AC-10 — invalid char (`"Cat!"`), oversized name (65 chars), oversized list (65 entries). EC-01 — empty list behavior documented and tested.

### SR-SEC-04: Config file symlink / TOCTOU

**Attack**: Attacker creates `~/.unimatrix/config.toml` as a symlink to a file they control. Server reads attacker's content.
**Mitigation**: `metadata()` (not `symlink_metadata()`) reports target permissions. World-writable target → abort. Group-writable target → warn. Implementation must read the file immediately after the permission check in the same function call — no yield points between check and read.
**Test requirement**: EC-07 — symlink to world-writable file aborts startup. Code review: `metadata()` used, not `symlink_metadata()`.

### SR-SEC-05: Config file size cap and TOML parser memory DoS

**Attack**: A crafted config file of several MB causes the TOML parser to allocate large amounts of memory, stalling server startup.
**Mitigation**: 64 KB read-to-buffer cap before any TOML parse call. Implementation must use `Vec<u8>` buffer, check `len() > 65536`, then pass to `toml::from_str()` — not `toml::from_reader(file)`.
**Test requirement**: R-16 scenarios 1-3. AC-15 — 65537-byte file aborts before parse.

---

## Failure Modes

### FM-01: Startup abort — descriptive error, actionable message

All `ConfigError` variants must produce messages identifying: (a) the file path, (b) the specific field or constraint violated, (c) valid values or range where applicable. Test each `ConfigError` variant's `Display` implementation for these three elements. A message of "config error" is a test failure.

### FM-02: `resolve_confidence_params()` returns Err after `validate_config()` passes

This indicates a logic gap — `validate_config` must catch all conditions that cause `resolve_confidence_params` to fail. If this occurs, it is a startup abort with an internal error message. Delivery team must keep `validate_config` and `resolve_confidence_params` synchronized: any new `custom` validation gate belongs in `validate_config`, not deferred to resolution time.

### FM-03: TOML parse error on malformed file

Abort startup. Error message includes file path and the TOML parser's error detail (line/column if available). The serde/toml error must be wrapped into `ConfigError::MalformedToml { path, detail }`, not swallowed.

### FM-04: `dirs::home_dir()` = None

Degrade to `UnimatrixConfig::default()`. Emit `tracing::warn!("home_dir() returned None; config not loaded; using compiled defaults")`. No abort. Server functions as if no config file were present.

### FM-05: Config file deleted between permission check and read

A standard `io::Error` must be wrapped into a `ConfigError` with the file path. Must not panic. Server aborts startup with a message identifying the file path and the I/O error.

### FM-06: `from_preset(Custom)` called directly

This is a logic error and panics by design. No recovery path. Code review and R-18 tests guard against this reaching production.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (`toml` dependency conflict) | — | Resolved: `toml = "0.8"` pinned to `unimatrix-server` only. `cargo tree` run at delivery time. No active test needed beyond clean `cargo build`. |
| SR-02 (ConfidenceParams missing 6 weight fields) | R-01, R-02 | Resolved by ADR-001: struct extended to 9 fields; `compute_confidence` uses `params.w_*`. R-01 confirms fields are load-bearing; R-02 SR-10 test is the mandatory regression guard. |
| SR-03 (ContentScanner ordering) | R-13 | Resolved by architecture: explicit `ContentScanner::global()` warm call at top of `load_config`. R-13 confirms scan behavior; code review gate confirms warm call presence. |
| SR-04 (`[confidence]` stub promoted to live) | R-05, R-08, R-09 | Resolved by ADR-004: `ConfidenceConfig` is a real struct with `weights: Option<ConfidenceWeights>`. R-05 covers all four custom-preset permutations; R-08 confirms named presets ignore `[confidence]`; R-09 confirms correct sum invariant (`0.92`, not `<= 1.0`). |
| SR-05 (context_retrospective rename blast radius) | R-04 | Resolved by SPECIFICATION.md §SR-05 checklist: 31 specific locations across 14 files. R-04 requires grep sweep + Python integration call as mandatory pre-merge gates. Build-passing is explicitly insufficient. |
| SR-06 (two-level merge semantics ambiguity) | R-10, R-22 | Resolved by ADR-003: replace semantics. R-10 tests cross-level custom preset inheritance prohibition. R-22 tests the merge false-negative at the explicitly-set-default boundary. |
| SR-07 (CategoryAllowlist constructor split) | R-17 | Resolved by architecture: `new()` delegates to `from_categories(INITIAL_CATEGORIES)`. R-17 confirms `new()` is unaffected by config and existing tests pass unchanged. |
| SR-08 (crate boundary — session_capabilities) | R-14 | Resolved by ADR-002: plain `Vec<Capability>` parameter. R-14 covers the server-infra wrapper path to confirm `session_caps` is not silently `None`. |
| SR-09 (exact preset values required) | R-03, R-09 | Resolved by ADR-005: exact weight table locked. R-03 asserts sum invariant for all four named presets against the locked table. R-09 catches the `<= 1.0` implementation mistake. |
| SR-10 (collaborative = default guard) | R-02 | Resolved by ADR-005 + mandatory SR-10 test. R-02 specifies the exact test form including the required comment text. |
| SR-11 (freshness_half_life_hours precedence chain) | R-06 | Resolved by ADR-006: single resolution site `resolve_confidence_params()`. R-06 covers all four precedence-chain cases including the collaborative-with-override case. |
| SR-12 (`[confidence]` stub promoted, delivery must not skip) | R-05, R-08 | Resolved by ADR-004. `[confidence]` is now live for `custom` preset; full validation and test coverage required. See SR-04 row. |
| SR-13 (W3-1 unblocked by ConfidenceParams) | R-01 | Resolved by ADR-001: `ConfidenceParams` carries all 9 fields. R-01 confirms weight fields are load-bearing at startup. W3-1 reads `ConfidenceParams` from server startup state without additional config parsing. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios | Notes |
|----------|-----------|-------------------|-------|
| Critical | 6 (R-01–R-06) | 24 scenarios | SR-10 test and SR-05 grep sweep are mandatory pre-PR gates |
| High | 8 (R-07–R-12, R-14) | 28 scenarios | R-09 sum-invariant uses `(sum-0.92).abs() < 1e-9`, not `<= 1.0`; R-11 `Admin` exclusion is an explicit allowlist check |
| Med | 8 (R-13, R-15–R-22) | 18 scenarios | R-15 container/CI `None` path must have a unit test; R-18 `from_preset(Custom)` panic is by design but must be audited |
| Integration | 5 (IR-01–IR-05) | 7 scenarios | IR-01 full test suite run with zero failures is the primary migration gate |
| Edge Cases | 8 (EC-01–EC-08) | 8 scenarios | EC-01 empty categories behavior must be explicitly documented in code |
| Security | 5 (SR-SEC-01–SR-SEC-05) | 10 scenarios | SR-SEC-01 and SR-SEC-02 are highest blast-radius; ordering test for length-before-scan is required |

**Mandatory pre-PR gates** (build-passing alone is insufficient):
1. SR-10 test present with "fix the weight table, not the test" comment verbatim
2. `grep -r "context_retrospective" .` returns zero results outside excluded historical directories
3. All four AC-25 freshness precedence cases have named unit tests
4. Weight sum validation uses `(sum - 0.92).abs() < 1e-9` — confirmed by the `0.95` test case
5. `[confidence] weights` confirmed inert for all four named presets (R-08 scenarios)

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection" and "risk pattern" — MCP tools unavailable in this agent context; queried all six ADRs, SCOPE.md, SCOPE-RISK-ASSESSMENT.md, and SPECIFICATION.md as primary evidence sources.
- Stored: nothing novel to store — all risks are feature-specific to dsn-001. The recurring pattern "SCOPE.md config-comment constraint (`<= 1.0`) contradicts the ADR-governing invariant (`= 0.92`) — always use the ADR as the authoritative source" would be worth storing if this class of spec/ADR discrepancy recurs across two or more features in a future session.
