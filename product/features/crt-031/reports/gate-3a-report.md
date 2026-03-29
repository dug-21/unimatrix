# Gate 3a Report: crt-031

> Gate: 3a (Design Review)
> Date: 2026-03-29
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All 7 components match architecture decomposition and ADR-001 decisions |
| Specification coverage | PASS | All 27 ACs and all functional requirements (FR-01 through FR-20) have corresponding pseudocode |
| Risk coverage | PASS | All 11 risks mapped to test scenarios; R-01, R-02, R-11 (Critical) fully addressed |
| Interface consistency | PASS | Shared types in OVERVIEW.md are consistent across all component files; no contradictions |
| Knowledge stewardship compliance | PASS | Both agent reports contain stewardship blocks with Queried and Stored entries |

---

## Detailed Findings

### Architecture Alignment

**Status**: PASS

**Evidence**:

The OVERVIEW.md correctly enumerates all 7 architecture components and matches ARCHITECTURE.md's Component Breakdown table exactly: `infra/categories`, `infra/config.rs`, `main.rs`, `services/status`, `mcp/response/status`, `background.rs`, and `eval/profile/layer.rs + 6 literal sites`.

The data flow diagram in OVERVIEW.md faithfully reproduces the architecture's Component Interactions diagram, including the correct threading of `Arc<CategoryAllowlist>` through `ServiceLayer::new()` → `StatusService`, and through `spawn_background_tick` → `background_tick_loop` → `run_single_tick` → `maintenance_tick`.

Component pseudocode files align with architecture ADR decisions:
- `categories.md`: constructor hierarchy (`from_categories_with_policy` → `from_categories` → `new()`) matches ADR-001 decision 1 exactly.
- `config.md`: `Default` returning `vec![]` and serde default fns returning `["lesson-learned"]` matches ADR-001 decision 2 and architecture §SR-09.
- `status.md`: StatusReport summary (adaptive-only) vs JSON (all categories) asymmetry matches ADR-001 decision 2, documented in pseudocode with the required code comment.
- `background.md`: uses `list_adaptive()` once per tick (not per-category `is_adaptive()` loop), matching architecture §Component 5 and R-06 constraint.
- `categories.md`: module split layout (`mod.rs` + `lifecycle.rs`) matches architecture §Component 1.

Technology choices are all consistent with established ADRs:
- `RwLock<HashSet<String>>` for `adaptive` field (established by ADR-003/entry #86).
- `#[serde(default = "fn")]` attribute on new fields (established by existing `boosted_categories`).
- Poison recovery via `.unwrap_or_else(|e| e.into_inner())` throughout.
- `StatusService` new field follows `observation_registry` pattern from col-023.

The `run_single_tick` gains `category_allowlist: &Arc<CategoryAllowlist>` (reference, not owned `Arc`) — consistent with the pattern used for `confidence_params` and `phase_freq_table` in the existing signature, and correctly threaded to both `StatusService::new()` and `maintenance_tick()`.

**R-02 specific check**: The pseudocode explicitly enumerates all 4 `StatusService::new()` construction sites in `status.md` §Site 1–4 and `background.md` §Body change 1. The architecture's concern about the `run_single_tick` bypass site is fully addressed with an explicit `Arc::clone(category_allowlist)` prescription and the warning: "Must NOT be `Arc::new(CategoryAllowlist::new())`". The OVERVIEW.md data flow also shows `run_single_tick` threading the `category_allowlist` ref to both `StatusService::new()` and `maintenance_tick()`.

**`services/mod.rs` coverage check**: `status.md` covers `services/mod.rs` (ServiceLayer::new), `services/status.rs`, and `mcp/response/status.rs` in a single component file with clearly delineated file sections. This is appropriate given the tight coupling.

**`background.md` wiring check**: The `run_single_tick` wiring uses the `&Arc<CategoryAllowlist>` threaded parameter, not `CategoryAllowlist::new()` inline. The pseudocode calls `Arc::clone(category_allowlist)` at the `StatusService::new()` call site, satisfying the operator-arc requirement from ARCHITECTURE.md and risk R-02.

---

### Specification Coverage

**Status**: PASS

**Evidence**:

All 20 functional requirements are addressed:

| FR | Component File | Coverage |
|----|---------------|---------|
| FR-01 `adaptive_categories` field | `config.md` | Struct definition, serde annotation |
| FR-02 Serde default | `config.md` | `default_adaptive_categories()` fn |
| FR-03 `validate_config` cross-check | `config.md` | Insertion pseudocode, fail-fast on first error |
| FR-04 `ConfigError` variant | `config.md` | Display impl mirrors `BoostedCategoryNotInAllowlist` |
| FR-05 `from_categories_with_policy` | `categories.md` | Full constructor pseudocode |
| FR-06 `new()` backward compat | `categories.md` | Delegation chain preserved |
| FR-07 `is_adaptive` method | `categories.md` | Reads adaptive lock, returns bool |
| FR-08 Poison recovery | `categories.md` | `.unwrap_or_else` on all new RwLock accesses |
| FR-09 `main.rs` wiring | `main.md` | Both sites updated with `from_categories_with_policy` |
| FR-10 `merge_configs` | `config.md` | Project-overrides-global block after boosted block |
| FR-11 `context_status` lifecycle | `status.md` | Both formatters, sorted Vec, asymmetry comment |
| FR-12 `maintenance_tick` stub | `background.md` | Step 10b with `list_adaptive()` + `#409` comment |
| FR-13 `add_category` unchanged | `categories.md` | Doc comment addition only, no logic change |
| FR-14 `KnowledgeConfig::default()` | `config.md` | `boosted_categories: vec![]` in Default impl |
| FR-15 `eval/profile/layer.rs` fix | `eval-layer.md` | One-line replacement pseudocode |
| FR-16 Seven literals removed | `eval-layer.md` | All 7 sites enumerated with replacement pattern |
| FR-17 `main_tests.rs` rewrite | `test-plan/main.md` | Full test function pseudocode provided |
| FR-18 workaround removal | `test-plan/config.md` | Named test cleanup step |
| FR-19 Pre-implementation grep | `test-plan/config.md` | Named mandatory step with grep commands |
| FR-20 README update | Not in pseudocode | Correctly deferred to implementation (no pseudocode needed) |

Non-functional requirements:
- NFR-01 (zero behavior change): `from_categories` delegation chain preserves `new()` behavior; `config.md` notes serde path unchanged.
- NFR-02 (file size): module split documented in `categories.md` with `mod.rs + lifecycle.rs` layout.
- NFR-03 (no panic on poisoned lock): poison recovery on `adaptive` lock in `is_adaptive` and `list_adaptive` in `categories.md`.
- NFR-04 (no schema changes): no schema-related pseudocode anywhere — correct.
- NFR-05 (circular dependency): confirmed in `config.md` integration notes and `eval-layer.md` §Circular Dependency Verification.
- NFR-06 (fail-fast startup): `validate_config` fails on first mismatch, same as boosted check.
- NFR-07 (parameter count): `#[allow(clippy::too_many_arguments)]` noted in `background.md`.

No scope additions detected. Pseudocode does not implement unrequested features. The `lifecycle.rs` stub file is explicitly minimal (no pub items) per architecture requirement.

---

### Risk Coverage

**Status**: PASS

**Evidence**:

All 11 risks from RISK-TEST-STRATEGY.md are mapped to test scenarios in the test plans:

**R-01 (Critical — parallel-list collision)**: Addressed with:
- Mandatory pre-implementation grep step in `test-plan/config.md` §MANDATORY Pre-Implementation Grep Steps.
- Four named tests: `test_validate_config_adaptive_error_isolated_from_boosted` (AC-25), `test_validate_config_boosted_error_isolated_from_adaptive`, `test_validate_config_both_parallel_lists_zeroed_ok`, plus the fixture audit.
- `test-plan/OVERVIEW.md` maps R-01 to these exact test functions.
- The canonical fixture pattern (zero BOTH lists) is documented with a Rust code example in multiple places.

**R-02 (Critical — StatusService 4 construction sites)**: Addressed with:
- Pre-implementation grep step in `test-plan/status.md` enumerating all 4 sites.
- Compile-time catch for sites 3 and 4 (test helpers in `status.rs`).
- `test_run_single_tick_uses_operator_arc_not_fresh` (grep verification) for the silent failure at `run_single_tick`.
- `test_status_service_compute_report_has_lifecycle` (runtime test) confirming the Arc reaches `compute_report()`.
- `test-plan/OVERVIEW.md` maps R-02 to all three test functions.
- Risk register citation of the bypass pattern and `background.md` pseudocode prescribing `Arc::clone(category_allowlist)` explicitly at the `run_single_tick` site.

**R-11 (Critical — KnowledgeConfig::default() change)**: Addressed with:
- Mandatory pre-implementation grep in `test-plan/config.md`.
- `test_knowledge_config_default_boosted_is_empty` (AC-17).
- `test_knowledge_config_default_adaptive_is_empty` (AC-27).
- `test_serde_default_boosted_categories_is_lesson_learned` (AC-18 rewrite).
- The ARCHITECTURE.md §SR-09 enumerates the two known affected tests and the grep requirement.

**R-03 (Medium)**: `test_add_category_defaults_to_pinned` and `test_validate_passes_is_adaptive_false_simultaneously` in `test-plan/categories.md`.

**R-04 (High)**: Compile gate (`cargo check -p unimatrix-server`) documented as a blocking pre-implementation step in `test-plan/categories.md`.

**R-05 (Medium)**: Grep verification for `#[allow(clippy::too_many_arguments)]` in `test-plan/background.md`.

**R-06 (Low)**: Code review verification of lock hygiene in `test-plan/background.md` §Lock hygiene.

**R-07 (High)**: `test_merge_configs_adaptive_project_wins` and `test_merge_configs_adaptive_global_fallback` in `test-plan/config.md`.

**R-08 (Medium)**: `test_category_lifecycle_sorted_alphabetically` and `test_category_lifecycle_json_sorted` in `test-plan/status.md`. Also addressed in pseudocode: OVERVIEW.md constraint #4 notes the sort requirement, and `status.md` includes a defensive `.sort_by()` in `compute_report()`.

**R-09 (Low)**: `test_new_is_adaptive_lesson_learned_true` (AC-13) in `test-plan/categories.md`.

**R-10 (High)**: Both test files confirm at least 2 named tests per file. `test-plan/background.md` lists `test_maintenance_tick_stub_logs_adaptive_categories` and `test_maintenance_tick_stub_silent_when_adaptive_empty`. `test-plan/status.md` lists multiple status report tests for the formatter.

All integration and edge-case risks (I-01 through I-04, E-01 through E-07) are also addressed.

---

### Interface Consistency

**Status**: PASS

**Evidence**:

Shared types defined in OVERVIEW.md are used consistently across all component pseudocode files:

| Shared Type | OVERVIEW.md Definition | Component File Usage |
|-------------|----------------------|---------------------|
| `CategoryAllowlist.adaptive` | `RwLock<HashSet<String>>` | `categories.md` uses identical field declaration |
| `KnowledgeConfig.adaptive_categories` | `Vec<String>`, serde default `["lesson-learned"]`, Default `vec![]` | `config.md` matches exactly |
| `KnowledgeConfig.boosted_categories` Default change | `vec![]` (was `["lesson-learned"]`) | `config.md` matches; `eval-layer.md` correctness check mentions this |
| `ConfigError::AdaptiveCategoryNotInAllowlist` | `{ path: PathBuf, category: String }` | `config.md` matches; Display format uses `{category:?}` |
| `default_boosted_categories_set()` | `pub fn() -> HashSet<String>` | `config.md` documents as `pub`, `eval-layer.md` uses `crate::infra::config::default_boosted_categories_set()` |
| `StatusReport.category_lifecycle` | `Vec<(String, String)>`, Default `vec![]`, sorted alphabetically | `status.md` uses identical type; defensive sort present in `compute_report()` |
| `StatusService` new field | `category_allowlist: Arc<CategoryAllowlist>` | `status.md` uses identical field name |
| `StatusService::new()` signature | new final param `category_allowlist: Arc<CategoryAllowlist>` | `status.md` signature matches; `background.md` site 2 uses `Arc::clone(category_allowlist)` |
| `ServiceLayer::new()` signature | new param `category_allowlist: Arc<CategoryAllowlist>` | `status.md` §services/mod.rs section covers this; `main.md` passes `Arc::clone(&categories)` |
| `spawn_background_tick` new param | `category_allowlist: Arc<CategoryAllowlist>` (param 23) | `background.md` full new signature matches OVERVIEW.md count |
| `background_tick_loop` new param | same | `background.md` matches |
| `maintenance_tick` new param | `category_allowlist: Arc<CategoryAllowlist>` (param 12) | `background.md` matches — note `maintenance_tick` gets `&Arc<CategoryAllowlist>` per spec FR-12 |
| `run_single_tick` new param | `&Arc<CategoryAllowlist>` (ref, final param) | `background.md` uses reference, consistent with OVERVIEW.md constraint #6 |

No contradictions found between OVERVIEW.md shared types and per-component usage.

Data flow coherence: The flow from `config.toml` → `KnowledgeConfig` → `CategoryAllowlist::from_categories_with_policy` → `Arc<CategoryAllowlist>` → (ServiceLayer + background tick) is consistently expressed across all component files.

One minor observation on `status.md` §`compute_report()`: it calls `list_categories()` and then `is_adaptive()` per item in a loop (two lock acquisitions per category). The OVERVIEW.md constraint #7 states "maintenance_tick Step 10b stub: calls `list_adaptive()` once (not per-category `is_adaptive()` — R-06)". This R-06 constraint applies to the `maintenance_tick` stub specifically, not to `compute_report()`. The `compute_report()` calling `is_adaptive()` per-category is not a violation — R-06 only applies to the tick. The pseudocode notes this explicitly. This is correct.

---

### Knowledge Stewardship Compliance

**Status**: PASS

**Evidence**:

**Pseudocode agent** (`crt-031-agent-1-pseudocode-report.md`):
- `## Knowledge Stewardship` section present.
- Two `Queried:` entries documented: context_search for RwLock poison recovery patterns, context_briefing returning #3775, #3770, #86.
- Deviations section confirms patterns applied.
- No `Stored:` entry — report states "Deviations from established patterns: none. All design choices follow established patterns." No novel patterns requiring storage were encountered. This is acceptable for a read-only agent that found no novel material.

**Test plan agent** (`crt-031-agent-2-testplan-report.md`):
- `## Knowledge Stewardship` section present.
- Two `Queried:` entries documented: context_briefing returning 19 entries, context_search for validate_config fixture patterns.
- `Stored:` entry: entry #3776 "Test plan for parallel config list fields: grep fixtures first, add isolation test per list" stored via context_store. Entry number and title present.

Both design-phase agents (architect at crt-031-agent-1-architect-report.md and risk-strategist at crt-031-agent-3b-risk-report.md) are not part of this gate's validation scope (these were Session 1 artifacts). The two agents whose outputs this gate validates both satisfy stewardship requirements.

---

## R-01 Special Focus: Parallel-List Collision

The R-01 risk is specifically addressed in the test plans with isolation tests. From `test-plan/config.md`:

1. `test_validate_config_adaptive_error_isolated_from_boosted` (AC-25) — uses `boosted_categories: vec![]` explicitly while testing `adaptive_categories` error path.
2. `test_validate_config_boosted_error_isolated_from_adaptive` — uses `adaptive_categories: vec![]` explicitly while testing `boosted_categories` error path.
3. `test_validate_config_both_parallel_lists_zeroed_ok` — canonical pattern test.

All three tests use the required zeroing pattern. The `test-plan/OVERVIEW.md` risk-to-test mapping table includes all four R-01 test functions. The mandatory pre-implementation grep step for `KnowledgeConfig {` struct literals is a named step, not an optional note.

---

## R-02 Special Focus: StatusService Construction Sites

The test plan correctly enumerates all 4 sites. Verification that pseudocode also covers `services/mod.rs` (ServiceLayer::new) and `mcp/response/status.rs`: `status.md` has three explicit file sections covering `services/status.rs`, `services/mod.rs`, and `mcp/response/status.rs`. The `run_single_tick` bypass site is specifically called out as "CRITICAL" in `background.md` with the constraint stated four times (once in the CRITICAL comment, once in the Note, once referenced in the error handling section, and once in the key test scenarios). This satisfies the requirement that the architecture's R-02 concern be addressed at the pseudocode level.

---

## R-11 Special Focus: Pre-implementation grep as blocking step

`test-plan/config.md` §MANDATORY Pre-Implementation Grep Steps names the grep as a numbered mandatory step titled "MANDATORY Pre-Implementation Grep Steps", not an optional suggestion. The step specifies two grep commands (`KnowledgeConfig::default()` and `UnimatrixConfig::default()`), identifies the known affected test (`main_tests.rs` lines 393–404), and requires the output to be documented in the PR description (AC-26). This satisfies the R-11 requirement that the grep be documented as a blocking step.

---

## Rework Required

None.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returning entries relevant to crt-031 validation patterns. No novel recurring gate failure patterns identified across this review; the design is sound and the test plans are unusually thorough. No store action required — the specific patterns (parallel list isolation, StatusService bypass, Default/serde split) are already stored in entries #3771, #3774, #3776 by the design-phase agents.
- Stored: nothing novel to store — the validation patterns checked here are already documented in Unimatrix entries referenced by the agents under review.
