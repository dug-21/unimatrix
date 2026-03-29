# Gate 3b Report: crt-031

> Gate: 3b (Code Review)
> Date: 2026-03-29
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | PASS | All 7 components implemented as designed; no significant departures |
| Architecture compliance | PASS | Module split, constructor hierarchy, ADR-001 decisions all honored |
| Interface implementation | PASS | All 13 integration-surface entries from ARCHITECTURE.md implemented correctly |
| Test case alignment | PASS | All test plan scenarios implemented; AC-05 through AC-27 covered |
| Code quality — compilation | PASS | `cargo build --workspace` clean; 14 warnings (pre-existing), 0 errors |
| Code quality — no stubs | PASS | No `todo!()`, `unimplemented!()`, FIXME; only intentional TODO(#409) stub |
| Code quality — no unwrap | PASS | All `.unwrap()` occurrences are inside test code |
| Code quality — file size | WARN | `services/mod.rs` (651), `main.rs` (1414), `background.rs` (4075) exceed 500 lines — pre-existing, not introduced by crt-031; `categories/mod.rs` (133) correctly split |
| Security | PASS | No hardcoded secrets; input validation at startup via `validate_config`; no path traversal risks |
| AC-19: no literal in layer.rs | PASS | `grep 'lesson-learned' eval/profile/layer.rs` returns zero hits |
| AC-20: no HashSet literals | PASS | All 6 test-infra sites replaced with `default_boosted_categories_set()` |
| R-02: operator Arc threaded | PASS | `run_single_tick` passes `Arc::clone(category_allowlist)` at line 462; no inline `CategoryAllowlist::new()` |
| R-11: Default returns vec![] | PASS | `KnowledgeConfig::default().boosted_categories` and `.adaptive_categories` both return `vec![]` |
| AC-12: existing tests pass | PASS | `cargo test -p unimatrix-server categories` shows 67 passed / 0 failed |
| AC-23: zero test failures (lib) | PASS | `cargo test -p unimatrix-server --lib` shows 2379 passed / 0 failed |
| Full workspace tests | WARN | 2 pre-existing flaky tests (`col018_topic_signal_from_*`) fail under concurrent load; pass in isolation; unrelated to crt-031 changes |
| cargo audit | WARN | `cargo-audit` not installed in this environment; cannot run |
| Knowledge stewardship | PASS | All 5 rust-dev agent reports have `Queried:` and `Stored:` / "nothing novel" entries |

## Detailed Findings

### Pseudocode Fidelity

**Status**: PASS

**Evidence**:

Component 1 (`infra/categories/mod.rs`): All three constructors implemented exactly as pseudocode specifies — `from_categories_with_policy` as canonical, `from_categories` delegating with `["lesson-learned"]`, `new()` delegating to `from_categories`. `is_adaptive` reads only `adaptive` lock with poison recovery. `list_adaptive` collects and sorts. `lifecycle.rs` stub committed with `mod lifecycle;` declaration in `mod.rs`.

Component 2 (`infra/config.rs`): `KnowledgeConfig` gains `adaptive_categories` with `#[serde(default = "default_adaptive_categories")]`. `Default` impl returns `vec![]` for both `boosted_categories` and `adaptive_categories`. `default_boosted_categories_set()` is `pub`. `ConfigError::AdaptiveCategoryNotInAllowlist` added with correct Display format. `validate_config` inserts adaptive cross-check after boosted check reusing `category_set`. `merge_configs` adds adaptive block with project-overrides-global semantics.

Component 3 (`main.rs`): Both construction sites (lines ~550 and ~946) updated to extract `adaptive_categories` from config and call `from_categories_with_policy`. Both `ServiceLayer::new()` calls and both `spawn_background_tick` calls pass `Arc::clone(&categories)`.

Component 4 (`services/status.rs` + `services/mod.rs`): `StatusService` gains `category_allowlist: Arc<CategoryAllowlist>` field. All 4 construction sites updated (2 test helpers at lines 1927 and 2082 use `CategoryAllowlist::new()`; `ServiceLayer::new` and `run_single_tick` use operator-loaded Arc). `compute_report()` populates `category_lifecycle` sorted alphabetically.

Component 5 (`mcp/response/status.rs`): `StatusReport` gains `category_lifecycle: Vec<(String, String)>`. `Default` returns `Vec::new()`. Summary formatter shows only adaptive categories; JSON formatter uses `BTreeMap` for deterministic output. Both match the intentional asymmetry from ADR-001 decision 2.

Component 6 (`background.rs`): `spawn_background_tick`, `background_tick_loop`, `run_single_tick`, and `maintenance_tick` all gain `Arc<CategoryAllowlist>` parameters. Step 10b stub correctly placed between Step 10 and Step 11 with `list_adaptive()` once per tick, non-empty guard, and `TODO(#409)` comment.

Component 7 (eval-layer + 6 test sites): `eval/profile/layer.rs` line 277 replaced as specified. All 6 test-infrastructure sites updated to `crate::infra::config::default_boosted_categories_set()`.

### Architecture Compliance

**Status**: PASS

**Evidence**: ADR-001 decisions are all honored:
- Decision 1 (constructor hierarchy): `new → from_categories → from_categories_with_policy` chain implemented
- Decision 2 (status format asymmetry): summary adaptive-only, JSON all — confirmed in `format_status_report`
- Decision 4 (Default vs serde separation): `Default` returns `vec![]`, serde fn returns `["lesson-learned"]`

Two independent `RwLock` fields — no contention on hot validate path. Module split produces `infra/categories/` with `mod.rs` at 133 lines (well within 500-line limit). Import path `crate::infra::categories::CategoryAllowlist` unchanged.

### Interface Implementation

**Status**: PASS

All 13 integration-surface entries from ARCHITECTURE.md verified in the implementation:
- `from_categories_with_policy`, `is_adaptive`, `list_adaptive` — all present in `mod.rs`
- `KnowledgeConfig::adaptive_categories` — serde default `["lesson-learned"]`, `Default` `[]`
- `KnowledgeConfig::Default::boosted_categories` — changed to `vec![]`
- `default_boosted_categories_set` — `pub fn` at module level in `config.rs`
- `ConfigError::AdaptiveCategoryNotInAllowlist` — variant present with correct `{path, category}` fields
- `StatusReport::category_lifecycle` — `Vec<(String, String)>`, Default `vec![]`
- `StatusService::new` — new `category_allowlist` param as final argument
- `ServiceLayer::new` and `with_rate_config` — new `category_allowlist` param forwarded
- `spawn_background_tick`, `background_tick_loop`, `maintenance_tick` — new `Arc<CategoryAllowlist>` param
- `layer.rs` Step 12 — `profile.config_overrides.knowledge.boosted_categories.iter().cloned().collect()`

### Test Case Alignment

**Status**: PASS

**Evidence**:

`lifecycle_tests.rs` (251 lines) implements all test scenarios from `test-plan/categories.md`:
- AC-05, AC-06, AC-07: `test_is_adaptive_*` tests present
- AC-08: `test_poison_recovery_is_adaptive` + `test_poison_recovery_list_adaptive` with `poison_adaptive_lock` helper
- AC-13, R-09: `test_new_delegates_adaptive_policy`
- E-01, E-02, E-04: `test_from_categories_with_policy_empty_adaptive`, `_all_adaptive`, `_duplicate_adaptive_deduplicates`
- E-03, E-06: `test_is_adaptive_single_char_category`, `test_is_adaptive_case_sensitive`
- R-03: `test_add_category_defaults_to_pinned`, `test_validate_passes_is_adaptive_false_simultaneously`
- R-06, R-08: `test_list_adaptive_returns_sorted`, `test_list_adaptive_sorted`

`config.rs` test section (from line 3114) implements all `test-plan/config.md` scenarios including AC-01 through AC-27, validate_config cross-checks, merge_configs test, and `default_boosted_categories_set` helper test.

`mcp/response/status.rs` (from line 1132) and `services/status.rs` (from line 2284) implement status test scenarios including I-02, R-08, AC-09.

`background.rs` tests (from line 3892) cover: compile-gate for `spawn_background_tick` accepting `Arc<CategoryAllowlist>` (AC-10/R-10), `maintenance_tick` signature check (AC-11/R-05), Step 10b guard logic test (AC-10), R-02 operator-Arc test.

`main_tests.rs` (line 397): `test_default_config_boosted_categories_is_lesson_learned` rewritten to assert serde path returns `["lesson-learned"]` AND `Default` returns `vec![]` (FR-17, AC-18).

### Code Quality — Compilation and Stubs

**Status**: PASS

`cargo build --workspace` completes with 0 errors and 14 warnings (pre-existing).

No `todo!()`, `unimplemented!()`, or placeholder functions. The only `TODO` comment in the new code is `// TODO(#409): ...` in `background.rs` Step 10b — this is the intentional, documented insertion point specified by AC-11 and FR-12.

### Code Quality — File Size

**Status**: WARN

Files modified by crt-031 that exceed 500 lines:
- `services/mod.rs`: 651 lines (pre-existing; was already this size before crt-031 added ~10 lines)
- `main.rs`: 1414 lines (pre-existing)
- `background.rs`: 4075 lines (pre-existing)
- `infra/config.rs`: 6131 lines (pre-existing)
- `services/status.rs`: 2413 lines (pre-existing)
- `mcp/response/status.rs`: 1516 lines (pre-existing)

The crt-031 spec (NFR-02) specifically required the `categories.rs` file to be split if it would breach 500 lines — this was done correctly (`categories/mod.rs` is now 133 lines). The other large files are pre-existing architectural debt not scoped for remediation in crt-031. Gate 3b only gates on files created or split by this feature, not pre-existing ones.

### Security

**Status**: PASS

- No hardcoded credentials or secrets
- Input validated at startup via `ConfigError::AdaptiveCategoryNotInAllowlist` (fail-fast, NFR-06)
- No path traversal risks introduced; `adaptive_categories` is a list of category names, not paths
- No command injection surface
- No `.unwrap()` in non-test code for new additions; all lock accesses use `.unwrap_or_else(|e| e.into_inner())`
- Serialization: `adaptive_categories` is `Vec<String>` deserialized by serde — no structural deserialization vulnerabilities
- `cargo-audit` not installed; WARN noted but not a code issue

### AC-19 / AC-20 Literal Removal

**Status**: PASS

`grep -n 'lesson-learned' crates/unimatrix-server/src/eval/profile/layer.rs` — **zero hits**.

`grep -rn 'HashSet::from.*lesson-learned' crates/unimatrix-server/src/{server.rs,infra/shutdown.rs,test_support.rs,services/index_briefing.rs,uds/listener.rs}` — **zero hits** (only hit in entire codebase is a doc comment in `config.rs` line 137 describing the old pattern, not a literal).

### R-02: Operator Arc Threaded

**Status**: PASS

`background.rs` line 462: `Arc::clone(category_allowlist)` — the reference parameter from `run_single_tick`'s `category_allowlist: &Arc<CategoryAllowlist>` is cloned and passed to `StatusService::new()`. No `CategoryAllowlist::new()` inside `run_single_tick` body. The `test_spawn_background_tick_has_category_allowlist_as_param_23` test explicitly verifies this via a custom empty-adaptive allowlist that would differ from `CategoryAllowlist::new()`.

### R-11: Default Returns vec![]

**Status**: PASS

`KnowledgeConfig::default().boosted_categories` is `vec![]` (changed from `vec!["lesson-learned"]`). `KnowledgeConfig::default().adaptive_categories` is `vec![]`. Both asserted in `test_default_config_boosted_categories_is_lesson_learned` in `main_tests.rs`.

### AC-12: Existing Tests

**Status**: PASS

`cargo test -p unimatrix-server categories` output: 67 passed / 0 failed. All 20 pre-existing test names from `test-plan/categories.md` are present in `tests.rs` and pass.

### AC-23 / Test Results

**Status**: PASS (with WARN for pre-existing flaky tests)

`cargo test -p unimatrix-server --lib`: **2379 passed / 0 failed**.

Full `cargo test --workspace`: 2379 passed / **2 failed** (`col018_topic_signal_from_file_path`, `col018_topic_signal_from_feature_id`). These two tests:
1. Fail only under concurrent test load, pass in isolation
2. Reside in `uds/listener.rs` which was changed by only one line in crt-031 (the `default_boosted_categories_set()` literal replacement — unrelated to `col018`)
3. Are consistent with the pre-existing concurrent-pool timeout issue tracked in GH #303
4. The lib-only run (which covers all crt-031 new code) is clean

### Knowledge Stewardship

**Status**: PASS

All 5 rust-dev agent reports (`crt-031-agent-3` through `crt-031-agent-7`) contain `## Knowledge Stewardship` sections with `Queried:` entries showing prior knowledge was consulted before implementation. Three reports have `Stored:` entries (entries #3778, #3777, #3779, #3780). Agent-7 has "nothing novel to store" with a reason.

---

## Rework Required

None.

## Knowledge Stewardship

- Stored: nothing novel to store — this gate found a clean implementation with no systemic patterns requiring extraction. Pre-existing flaky tests under concurrent load are already documented in project memory and GH #303.
