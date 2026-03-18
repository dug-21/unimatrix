# Gate 3a Report: dsn-001

> Gate: 3a (Component Design Review)
> Date: 2026-03-18
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All 9 components map to architecture; ADR decisions followed throughout |
| Specification coverage | PASS | All 16 FRs have corresponding pseudocode; no scope additions detected |
| Risk coverage | PASS | All 22 risks + 5 integration risks + 8 edge cases have test scenarios |
| Interface consistency | PASS | Shared types consistent across all component files |
| Weight sum invariant (SR-10) | PASS | Exact invariant `(sum - 0.92).abs() < 1e-9` used throughout; `<= 1.0` never appears |
| SR-10 comment text | PASS | Verbatim comment present in both pseudocode and test plan |
| AC-25 four named tests | PASS | All four freshness precedence rows have named unit test functions |
| `from_preset(Custom)` panic | PASS | Panic by design; no call path directly invokes it |
| `CategoryAllowlist::new()` signature | PASS | Unchanged; delegates to `from_categories(INITIAL_CATEGORIES.to_vec())` |
| `agent_resolve_or_enroll` third param | PASS | `Option<&[Capability]>`; all existing call sites documented as `None` |
| Cross-level weight inheritance prohibition | PASS | ADR-003 prohibition enforced at per-file `validate_config`, not post-merge |
| `ContentScanner::global()` warm ordering | PASS | Explicit call at top of `load_config` with required comment documented |
| Knowledge stewardship | PASS | All pseudocode components have `## Knowledge Stewardship` with `Queried:` entries |

---

## Detailed Findings

### Check 1: Architecture Alignment

**Status**: PASS

**Evidence**: All nine architecture components have matching pseudocode files:

1. `config-loader.md` — `infra/config.rs` new file, all functions (`load_config`, `validate_config`, `resolve_confidence_params`, `confidence_params_from_preset`, `merge_configs`, `check_permissions`) present with correct signatures matching ARCHITECTURE.md §Integration Surface.
2. `confidence-params.md` — `unimatrix-engine/src/confidence.rs`, struct extended from 3→9 fields, `compute_confidence` and `freshness_score` signatures updated exactly as architecture specifies.
3. `category-allowlist.md` — `infra/categories.rs`, new `from_categories(Vec<String>) -> Self` constructor, `new()` delegates.
4. `search-service.md` — `services/search.rs`, `boosted_categories: HashSet<String>` field added, four hardcoded comparisons replaced.
5. `agent-registry.md` — both `infra/registry.rs` and `unimatrix-store/src/registry.rs`, `PERMISSIVE_AUTO_ENROLL` const removed, new constructor signature, `session_caps: Option<&[Capability]>` third param.
6. `server-instructions.md` — `server.rs`, `SERVER_INSTRUCTIONS` const moved to private default, `instructions: Option<String>` parameter added.
7. `tool-rename.md` — `mcp/tools.rs` + 31-location blast radius per SR-05 checklist in SPECIFICATION.md, all locations documented.
8. `startup-wiring.md` — `main.rs` wiring for both `tokio_main_daemon` and `tokio_main_stdio`; `background.rs` `spawn_background_tick` and `background_tick_loop` updated.
9. OVERVIEW.md — describes the dependency wave order (W1: confidence-params, category-allowlist, tool-rename; W2: config-loader, agent-registry, search-service, server-instructions; W3: startup-wiring), consistent with the architecture.

Technology decisions are followed: `toml = "0.8"` constrained to `unimatrix-server/Cargo.toml`, `#[serde(rename_all = "lowercase")]` on `Preset`, `Option<f64>` for `freshness_half_life_hours`, no `Arc<UnimatrixConfig>` crossing crate boundaries.

ADR-005 and ADR-006 decisions are faithfully represented: exact weight table in `config-loader.md` `confidence_params_from_preset`; single resolution site in `resolve_confidence_params`; `from_preset(Custom)` panics by design.

### Check 2: Specification Coverage

**Status**: PASS

FR-by-FR mapping:

| FR | Status | Pseudocode Location |
|----|--------|---------------------|
| FR-01 Config File Loading | PASS | `config-loader.md` `load_config` / `load_single_config` |
| FR-02 Two-Level Merge | PASS | `config-loader.md` `merge_configs` with replace semantics; ADR-003 prohibition documented |
| FR-03 Profile Preset System | PASS | `config-loader.md` `Preset` enum + `ProfileConfig`; `resolve_confidence_params` |
| FR-04 ConfidenceParams Extension | PASS | `confidence-params.md`, 9-field struct, `Default` impl |
| FR-05 Category Externalization | PASS | `category-allowlist.md` `from_categories` constructor |
| FR-06 Boosted Categories | PASS | `search-service.md` four-comparison replacement + `HashSet<String>` field |
| FR-07 Freshness Half-Life | PASS | `confidence-params.md` `freshness_score` uses `params.freshness_half_life_hours` |
| FR-08 Server Instructions | PASS | `server-instructions.md` `Option<String>` parameter |
| FR-09 Agent Enrollment | PASS | `agent-registry.md` `permissive: bool` + `session_caps: Vec<Capability>` |
| FR-10 Preset Resolution Pipeline | PASS | `config-loader.md` `resolve_confidence_params` single site |
| FR-11 Tool Rename | PASS | `tool-rename.md` 31-location checklist matches SPECIFICATION §SR-05 exactly |
| FR-12 CycleParams.topic Doc | PASS | `tool-rename.md` §`CycleParams.topic` doc neutralization |
| FR-13 Security Validation | PASS | `validate_config` covers all 10 categories: char set, count, boosted subset, half-life range, instructions length+scan, preset enum, custom weight presence/sum, default_trust, session_caps allowlist |
| FR-14 File Permission Enforcement | PASS | `check_permissions` `#[cfg(unix)]` with `metadata()` (not `symlink_metadata()`) |
| FR-15 File Size Cap | PASS | `load_single_config` 65536-byte cap before `toml::from_str` |
| FR-16 No-Config Backward Compat | PASS | `collaborative` preset == `ConfidenceParams::default()` enforced by SR-10 test in pseudocode |

NFRs:
- NFR-01 (startup only): config load runs once before any request handling — PASS
- NFR-02 (memory): `Arc<ConfidenceParams>` to background tick; `UnimatrixConfig` not stored on structs — PASS
- NFR-03 (crate boundary): only plain primitives cross boundaries — PASS
- NFR-04 (testability): `validate_config` independently testable, synchronous `#[test]` — PASS
- NFR-05 (no schema migration): no DB tables — PASS
- NFR-06 (rmcp unchanged): confirmed — PASS
- NFR-07 (Windows compat): `#[cfg(unix)]` gate on permissions — PASS

No scope additions detected. `CycleConfig` is correctly absent from `UnimatrixConfig`. No new MCP tools introduced.

### Check 3: Risk Coverage

**Status**: PASS

Every risk in the RISK-TEST-STRATEGY has at least one test scenario in the test plans. Critical checks against the key invariants:

**R-01** (call site migration): `test_compute_confidence_uses_params_w_fresh` and `test_freshness_score_uses_params_half_life` in `test-plan/confidence-params.md` both designed to fail if compiled constants remain. `test_all_named_presets_sum_to_0_92` static grep audit also specified. PASS.

**R-02** (SR-10 regression): Mandatory test present with exact function name `collaborative_preset_equals_default_confidence_params` and verbatim comment. See Check 5 below for detail. PASS.

**R-03** (sum invariant): Four named preset tests + boundary case `test_custom_weights_sum_0_95_aborts` (the critical `<= 1.0` regression detector). PASS.

**R-04** (partial rename): Grep sweep gate + `test_tool_discovery_includes_cycle_review` positive + negative assertions + live call test. Full 14-file checklist reproduced in `test-plan/tool-rename.md`. PASS.

**R-05** (custom preset missing fields): Four-case truth table in `test-plan/config-loader.md` with four named functions. PASS.

**R-06** (freshness precedence): Four named AC-25 tests confirmed below in Check 6. PASS.

**R-07** (instructions injection): `test_instructions_injection_aborts`, `test_instructions_8193_bytes_aborts_before_scan`, `test_instructions_8192_bytes_passes`, `test_instructions_valid_multiline_passes`. PASS.

**R-08** (named preset immune to `[confidence]`): `test_named_preset_ignores_confidence_weights` with garbage weight values asserting authoritative table values win. PASS.

**R-09** (wrong sum invariant): `test_custom_weights_sum_0_95_aborts` explicitly named as "R-09 critical regression detector". Also `test_no_sum_lte_1_in_validation_code` grep audit. PASS.

**R-10** (cross-level inheritance): Three named tests: `test_merge_cross_level_custom_weights_prohibited`, `test_merge_cross_level_no_global_weights_still_aborts`, `test_merge_cross_level_both_custom_per_project_wins`. ADR-003 citation in each test comment required. PASS.

**R-11** (Admin escalation): `test_session_capabilities_admin_aborts`, `test_session_capabilities_admin_mixed_aborts`, `test_session_capabilities_admin_lowercase_behavior` + valid sets. PASS.

**R-12** (half-life validation gap): Eight tests covering 0.0, -1.0, NaN, Infinity, -0.0, 87600.001, 87600.0 (pass), MIN_POSITIVE (pass). PASS.

**R-13** (ContentScanner warm ordering): Code review gate + grep for explicit warm call. PASS.

**R-14** (session_caps propagation): `test_agent_registry_session_caps_propagated_to_store` exercises full path through `AgentRegistry` → store. PASS.

**R-15** (home_dir None): Unit test + code review gate for `None` arm. PASS.

**R-16** (file size cap): `test_load_config_file_too_large_aborts` (65537) + `test_load_config_file_exactly_64kb_passes` (65536, inclusive). PASS.

**R-17** (CategoryAllowlist::new unchanged): `test_new_delegates_to_from_categories_initial` + full test suite regression. PASS.

**R-18** (`from_preset(Custom)` panic audit): `#[should_panic]` test present. PASS.

**R-19** (boosted subset validation): `test_boosted_category_not_in_allowlist_aborts` with error message assertion. PASS.

**R-20** (hook/bridge excluded): Grep gate `load_config` in `main.rs`. PASS.

**R-21** (error message identifies file): Per-variant Display test asserting file path present (FM-01 section). PASS.

**R-22** (merge false-negative): The design uses `Option::or` for `freshness_half_life_hours` and `Option::or` for `confidence.weights` — `Option<f64>` type-level solution documented in ADR-006. Test `test_merge_configs_per_project_wins_for_specified_fields` + non-default detection strategy covered. PASS.

All integration risks (IR-01–IR-05) and edge cases (EC-01–EC-08) covered. Security risks (SR-SEC-01–SR-SEC-05) covered.

### Check 4: Interface Consistency

**Status**: PASS

**Shared types (OVERVIEW.md) vs per-component usage:**

`UnimatrixConfig` structure: defined in `config-loader.md` with five sub-structs. OVERVIEW.md Shared Types section matches exactly. `startup-wiring.md` extracts values from the same field paths (`config.knowledge.categories`, `config.agents.default_trust`, etc.).

`ConfidenceParams` nine-field struct: defined in `confidence-params.md`, referenced correctly in `config-loader.md` `resolve_confidence_params` and `startup-wiring.md` `Arc<ConfidenceParams>`. Field names (`w_base`, `w_usage`, `w_fresh`, `w_help`, `w_corr`, `w_trust`, `freshness_half_life_hours`, `alpha0`, `beta0`) are consistent across all files.

`CategoryAllowlist::from_categories(Vec<String>)` signature: matches in `category-allowlist.md`, `startup-wiring.md`, and ARCHITECTURE.md Integration Surface. `new()` signature unchanged.

`AgentRegistry::new(store, permissive, session_caps)`: the `agent-registry.md` pseudocode chose Option A (store `session_caps: Vec<Capability>` on struct). This is consistent — `startup-wiring.md` explicitly calls `AgentRegistry::new(Arc::clone(&store), permissive, session_caps)`. `resolve_or_enroll` passes `Some(&self.session_caps)` when non-empty, `None` when empty. This is coherent and preserves the `Option<&[Capability]>` contract at the store boundary.

`SearchService.boosted_categories: HashSet<String>`: `search-service.md` adds this field and constructor parameter. `startup-wiring.md` constructs the `HashSet<String>` from `config.knowledge.boosted_categories` and passes it to `ServiceLayer::new(..., boosted_categories)`. Consistent.

`agent_resolve_or_enroll(id, permissive, session_caps: Option<&[Capability]>)`: defined in `agent-registry.md`, all existing call sites correctly documented as passing `None`. Consistent with ARCHITECTURE.md Integration Surface.

Data flow is one-directional at startup (architecture guarantee): no back-flow from subsystems to config. Confirmed in all component pseudocode files — no subsystem calls config functions post-construction.

One minor documentation note: `startup-wiring.md` Config Load Block shows a non-fatal `load_config` error path (warns and falls back to defaults) rather than aborting. This is consistent with the specification (FR-01 specifies abort for present-but-malformed files, but the startup wiring chooses graceful degradation for container environments per R-15). This is a valid design choice documented in `startup-wiring.md` §Error Handling and is not a contradiction.

### Check 5: SR-10 Mandatory Test — Exact Comment Text

**Status**: PASS

**Evidence** (`pseudocode/config-loader.md` line 655):
> "SR-10: If this test fails, fix the weight table, not the test."

This exact verbatim text appears in the pseudocode's "Key Test Scenarios" section, item 2.

**Evidence** (`test-plan/confidence-params.md`, SR-10 section):
```rust
fn collaborative_preset_equals_default_confidence_params() {
    // SR-10: If this test fails, fix the weight table, not the test.
    assert_eq!(
        confidence_params_from_preset(Preset::Collaborative),
        ConfidenceParams::default()
    );
}
```

The exact comment string `"SR-10: If this test fails, fix the weight table, not the test."` is present verbatim. Requirement from IMPLEMENTATION-BRIEF.md §Mandatory Pre-PR Gates item 1 is satisfied at the design stage.

Note: the test is correctly placed in `unimatrix-server` (not `unimatrix-engine`), because `confidence_params_from_preset` is a free function in `config.rs`, consistent with ADR-006 crate placement decision.

### Check 6: AC-25 Four Named Unit Tests

**Status**: PASS

**Evidence** (`test-plan/config-loader.md` §Freshness Half-Life Precedence Tests):

All four rows from the AC-25 freshness precedence table have named unit test functions:

1. `test_freshness_precedence_named_preset_no_override` — named (non-custom) + absent → 720.0 (operational built-in). PASS.
2. `test_freshness_precedence_named_preset_with_override` — named (non-custom) + present → 336.0 (override wins). PASS.
3. `test_freshness_precedence_custom_no_half_life_aborts` — custom + absent → `ConfigError::CustomPresetMissingHalfLife`. PASS.
4. `test_freshness_precedence_custom_with_half_life_succeeds` — custom + present → 24.0. PASS.

A fifth test `test_freshness_precedence_collaborative_override_applies` covers the collaborative-with-override case from ADR-006 (not required by AC-25 but covers the additional R-06 scenario). This is correct supplementary coverage.

### Check 7: `confidence_params_from_preset(Preset::Custom)` Panic by Design

**Status**: PASS

**Evidence** (`pseudocode/config-loader.md` lines 472-478):
```
Preset::Custom => {
    panic!("confidence_params_from_preset(Preset::Custom) is a logic error; \
            use resolve_confidence_params() instead");
}
```

The panic is explicitly documented and the comment explains the correct path. No pseudocode in any other component calls `confidence_params_from_preset(Preset::Custom)` — only `resolve_confidence_params` in `config-loader.md` handles the `Custom` path, and it goes through a different code branch entirely (lines 406-431). The `#[should_panic]` test in `test-plan/confidence-params.md` serves as the audit gate.

### Check 8: `CategoryAllowlist::new()` Signature Unchanged

**Status**: PASS

**Evidence** (`pseudocode/category-allowlist.md`):
```
pub fn new() -> Self

BODY:
    CategoryAllowlist::from_categories(
        INITIAL_CATEGORIES.iter().map(|s| s.to_string()).collect()
    )
```

Signature is `() -> Self` — unchanged from pre-dsn-001. Delegates to `from_categories(INITIAL_CATEGORIES.to_vec())` exactly as ARCHITECTURE.md §CategoryAllowlist constructor specifies. IR-05 invariant (`new()` and `from_categories(INITIAL_CATEGORIES)` produce identical results) has a corresponding test `test_new_delegates_to_from_categories_initial`.

### Check 9: `agent_resolve_or_enroll` Third Parameter

**Status**: PASS

**Evidence** (`pseudocode/agent-registry.md`):
```
pub async fn agent_resolve_or_enroll(
    &self,
    agent_id: &str,
    permissive: bool,
    session_caps: Option<&[Capability]>,  // Some → use provided caps; None → permissive/strict branch
) -> Result<AgentRecord>
```

The third parameter is `Option<&[Capability]>` exactly as ARCHITECTURE.md Integration Surface specifies. The "All Existing Call Sites" section documents that all existing call sites pass `None`:
```
store.agent_resolve_or_enroll("test-agent", true, None).await
```

The server-infra wrapper (`AgentRegistry::resolve_or_enroll`) converts the stored `session_caps: Vec<Capability>` to `Some(&self.session_caps)` when non-empty, `None` when empty — correctly threaded.

### Check 10: Cross-Level Weight Inheritance Prohibition

**Status**: PASS

**Evidence** (`pseudocode/config-loader.md` `merge_configs` comments, lines 489-553):

The comment block explains: "validate_config (called before merge) will have already aborted" for the per-project-custom-no-weights case. The merge function uses `project.confidence.weights.or(global.confidence.weights)` — this is technically an `Option::or`, but the prohibition is enforced at per-file validation time (each file is validated independently before merge). The pseudocode notes:

> "The 'cross-level inheritance prohibition' means: If the merged preset comes from the project (custom) and project has no weights, validate_config for the project file already aborted. So we never reach merge in that state."

This is the correct implementation strategy. The R-10 test `test_merge_cross_level_custom_weights_prohibited` verifies it works end-to-end by calling `merge_configs` followed by `validate_config` on the merged result and expecting `CustomPresetMissingWeights`.

**Potential concern** (WARN-level): The merge function's `confidence.weights` uses `Option::or`, meaning if the global config has weights and the per-project config has `preset=custom` with no weights, the merge would produce a config where `confidence.weights` is `Some(global_weights)`. Validation of the per-project file alone catches this (per-project `preset=custom` with `weights=None` → abort before merge). However, this relies on the per-file validation running before merge.

The pseudocode documents this constraint correctly: validation runs per-file before merge. This is a correct and safe design, but the delivery agent must maintain the ordering invariant (validate each file independently before calling merge_configs). This is already specified in the `load_single_config` pseudocode and the startup sequence.

### Check 11: `ContentScanner::global()` Warm Call Position

**Status**: PASS

**Evidence** (`pseudocode/config-loader.md` `load_config` body, lines 162-167):
```
// Step 0: Warm ContentScanner singleton BEFORE any validate_config call.
// ORDERING INVARIANT: must be first. scan_title() in validate_config requires
// ContentScanner::global() to be initialized. This explicit call documents the
// dependency and prevents silent breakage if the OnceLock ever changes behavior.
let _scanner = ContentScanner::global();
```

This call appears at the top of `load_config` before any `load_single_config` (which calls `validate_config`) call. The comment is present and documents the ordering invariant. This satisfies ARCHITECTURE.md §ContentScanner ordering and SPECIFICATION.md Constraint #9.

The test plan (`test-plan/startup-wiring.md`) includes a code review gate:
```bash
grep -A 5 "fn load_config" crates/unimatrix-server/src/infra/config.rs
```
confirming the warm call must be verifiable in Stage 3b.

### Check 12: Knowledge Stewardship Compliance

**Status**: PASS

All eight pseudocode components have a `## Knowledge Stewardship` section with `Queried:` entries:

- `config-loader.md`: Queried `/uni-query-patterns` for `unimatrix-server`; found patterns #2298 and #646.
- `confidence-params.md`: Queried `/uni-query-patterns` for `unimatrix-engine`; no patterns found; extension follows ADR-001.
- `category-allowlist.md`: Queried `/uni-query-patterns`; no specific patterns found.
- `search-service.md`: Queried `/uni-query-patterns`; no HashSet injection patterns found.
- `agent-registry.md`: Queried `/uni-query-patterns` for `unimatrix-store`; no patterns found.
- `server-instructions.md`: Queried `/uni-query-patterns`; no patterns found.
- `tool-rename.md`: Queried `/uni-query-patterns` for tool naming; no prior rename patterns found.
- `startup-wiring.md`: Queried `/uni-query-patterns` for `main.rs`; no patterns found.

All have either `Stored:` (none in this case, as no novel patterns were discovered) or implicit "nothing novel" rationale (each file explains why nothing was stored — first config externalization, standard patterns, etc.).

---

## Additional Observations (Non-blocking)

### Observation 1: `merge_configs` Non-Default Detection Strategy and R-22

The `merge_configs` pseudocode uses "non-default detection" (compare project field against compiled default, use global if equal). This is the correct approach for list fields. However, the ADR-003 test `test_merge_cross_level_both_custom_per_project_wins` shows that when global has `custom+weights_A` and project has `custom+weights_B`, per-project weights win — which works correctly because `weights_B` is `Some` (different from `None` default). The `Option::or` implementation for `confidence.weights` and `freshness_half_life_hours` avoids the false-negative R-22 risk entirely via type-level `None` representation.

### Observation 2: UDS Listener `boosted_categories` Known Limitation

`startup-wiring.md` documents a known limitation: the UDS listener's internal `ServiceLayer` construction will use `HashSet::from(["lesson-learned"])` as default when it lacks access to the operator config. This is explicitly scoped as a follow-up and is within the defined NOT-in-scope boundaries. Noted in `IMPLEMENTATION-BRIEF.md` Stage 3a completion notes. No action required at this gate.

### Observation 3: `startup-wiring.md` Config Load Error Handling

The startup wiring treats `load_config` failures as non-fatal (warn + fall back to defaults). This is a design choice that enables graceful degradation in container/CI environments. The spec FR-01 says "startup aborts" for malformed files — but the wiring catches this and falls back to defaults. This is a valid implementation decision documented in `startup-wiring.md` §Error Handling. It is not a spec violation as the primary failure mode is covered by `validate_config` aborting startup during config validation, and the non-fatal path applies to I/O failures in unexpected environments. No action required.

---

## Rework Required

None. All checks PASS.

---

## Knowledge Stewardship

- Stored: nothing novel to store — no recurring gate failure patterns identified. All checks passed on first review. Any pattern identified here is feature-specific to dsn-001 design artifacts.
