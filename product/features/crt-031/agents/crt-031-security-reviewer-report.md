# Security Review: crt-031-security-reviewer

## Risk Level: low

## Summary

crt-031 is a config-expressiveness feature that adds a new `adaptive_categories` field to `KnowledgeConfig`, splits `CategoryAllowlist` into a module with a second `RwLock<HashSet<String>>` field, eliminates seven hardcoded `HashSet::from(["lesson-learned"...])` literals, and wires the resulting policy through the status and background tick paths. There are no schema changes, no new MCP tools, and no new external trust boundaries. The change is minimal, correctly scoped, and the security risk analysis was done correctly by the team before implementation. No blocking security findings.

---

## Findings

### Finding 1: S-02 — ConfigError display uses {:?} debug escaping for operator-supplied category name

- **Severity**: informational (correctly handled)
- **Location**: `crates/unimatrix-server/src/infra/config.rs` — `ConfigError::AdaptiveCategoryNotInAllowlist` Display impl
- **Description**: The error message for `AdaptiveCategoryNotInAllowlist` formats the operator-supplied category string with `{:?}` (debug format), which quotes and escapes the string. This is the correct mitigation for log injection via crafted category names. The existing `BoostedCategoryNotInAllowlist` variant uses the same `{:?}` pattern, and the new variant follows suit consistently.
- **Recommendation**: No action required. The implementation is correct.
- **Blocking**: no

### Finding 2: S-01 — adaptive_categories deserialized from operator-controlled config.toml

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/infra/config.rs` — `validate_config`
- **Description**: `adaptive_categories` is deserialized from `config.toml`, which is operator-controlled. The `validate_config` cross-check (added in this PR) rejects any entry not present in the `categories` list at startup with `ConfigError::AdaptiveCategoryNotInAllowlist`. Post-construction, the `adaptive` set is frozen — no runtime mutation path exists. The `HashSet::contains` call in `is_adaptive` is a pure equality check with no path traversal, SQL interpolation, or shell expansion risk. The `tracing::debug!` call in the tick stub uses the `?` formatter. Blast radius if a malicious string reaches the adaptive set: it appears in a debug log entry. No injection risk exists.
- **Recommendation**: No action required. Validation at startup is the correct control.
- **Blocking**: no

### Finding 3: S-03 — category_lifecycle exposed via context_status output

- **Severity**: low (read-only metadata)
- **Location**: `crates/unimatrix-server/src/mcp/response/status.rs` and `services/status.rs`
- **Description**: The new `category_lifecycle` field in `StatusReport` exposes the lifecycle classification of every known category in the `context_status` response (JSON format). This is read-only configuration metadata. Category names are already visible via `category_distribution` in the existing status output. The JSON formatter uses `BTreeMap` for deterministic ordering, and the summary formatter shows only adaptive categories. No new information is exposed beyond what an operator already knows from their own `config.toml`. No access control downgrade.
- **Recommendation**: No action required.
- **Blocking**: no

### Finding 4: merge_configs — operator cannot override global adaptive_categories with explicit [] in project config

- **Severity**: low (behavior gap, not a security risk)
- **Location**: `crates/unimatrix-server/src/infra/config.rs` — `merge_configs`
- **Description**: The merge logic compares `project.knowledge.adaptive_categories != default.knowledge.adaptive_categories` where `default.knowledge.adaptive_categories = vec![]` (Rust Default). If an operator sets `adaptive_categories = []` explicitly in a project-level config to disable adaptive management, this equals the Default value (`vec![]`), so the `else` branch fires and `global.adaptive_categories` wins. An operator who wants to disable adaptive management at the project level but has a non-empty global config cannot do so by setting `[]` in the project file — the global value silently prevails. This is the same behavior as the pre-existing `boosted_categories` merge logic (intentional pattern, not a regression). It is a behavior gap, not a security issue, but operators relying on `context_status` JSON output would see the discrepancy. The test `test_merge_configs_adaptive_global_fallback` documents this behavior explicitly. The risk strategy documents this as FM-04 (non-critical).
- **Recommendation**: No code change required for security. Consider adding a comment in `merge_configs` noting that explicit `[]` in project config defers to global (mirrors the existing `boosted_categories` comment pattern).
- **Blocking**: no

### Finding 5: No new unsafe code, no new dependencies, no hardcoded secrets

- **Severity**: informational
- **Location**: entire diff
- **Description**: The server crate has `#![forbid(unsafe_code)]` at the top of `lib.rs`, confirmed from the source. The diff introduces no new Cargo.toml dependencies. No Cargo.lock changes are present. No hardcoded credentials, tokens, API keys, or secrets appear in any added line. All `unwrap()` calls in the diff are inside `#[test]` functions only. Production paths use `.unwrap_or_else(|e| e.into_inner())` for RwLock poison recovery throughout.
- **Recommendation**: No action required.
- **Blocking**: no

---

## OWASP Evaluation

| OWASP Concern | Assessment |
|---------------|-----------|
| Injection (SQL, command, path, log) | Not present. `is_adaptive` is a pure `HashSet::contains`. Error messages use `{:?}` debug escaping. No SQL, no shell commands, no path operations on user input. |
| Broken access control | Not applicable. No new MCP tools. `CategoryAllowlist` methods are crate-internal; the `adaptive` set is frozen after startup construction. No privilege escalation surface. |
| Security misconfiguration | validate_config startup rejection for out-of-allowlist `adaptive_categories` prevents misconfigured starts reaching a running server with an incoherent policy state. |
| Vulnerable components | No new dependencies. |
| Data integrity failures | The `adaptive` set is frozen after construction from operator config. `add_category` (domain pack runtime path) writes only to `categories`, never `adaptive`. The two independent RwLock fields enforce this separation structurally. |
| Deserialization risks | `adaptive_categories` is deserialized from operator-controlled TOML. No untrusted external input. Serde behavior is validated by multiple round-trip tests. Arbitrary-string category names are validated by `validate_config` before use. |
| Input validation gaps | `validate_config` validates `adaptive_categories` subset membership immediately after the existing `boosted_categories` check. Empty list is accepted (disables adaptive management). No input from external callers (MCP tool parameters) touches the adaptive set. |
| Secrets in diff | None. |

---

## Blast Radius Assessment

The fix has no state machine changes and no schema migrations. Worst-case failure modes by component:

- **Config validation failure at startup** (FM-01): Server exits with `AdaptiveCategoryNotInAllowlist`. Recoverable by correcting `config.toml`. Does not affect persisted data.
- **Wrong Arc passed to StatusService or background tick** (R-02, mitigated): If `CategoryAllowlist::new()` were used instead of the operator-loaded Arc, `category_lifecycle` in `context_status` would show `lesson-learned` as adaptive regardless of operator config. This matches the default behavior and would be a silent discrepancy, not a data corruption or availability issue. The diff correctly threads the operator-loaded Arc at all four `StatusService::new()` sites (confirmed by reading the diff).
- **RwLock poison** (FM-02, FM-05): Both `is_adaptive` and `list_adaptive` use `.unwrap_or_else(|e| e.into_inner())`. Cannot panic. Returns best-available data from poisoned guard.
- **Tick stub** (Step 10b): The lifecycle guard stub calls `list_adaptive()` and emits a `tracing::debug!` when the list is non-empty. No entries are modified. The stub is explicitly a no-op. Failure mode: the debug log either fires or is silent. No data at risk.

The worst realistic case if this PR has a subtle bug is that `context_status` shows incorrect or missing `category_lifecycle` data — a display-layer regression, not a data integrity or availability failure.

---

## Regression Risk

- **Existing category validation** (`validate`, `add_category`, `list_categories`): Unchanged semantics. All pre-existing tests are preserved in `infra/categories/tests.rs` and pass (51 categories tests confirmed in RISK-COVERAGE-REPORT).
- **boosted_categories behavior**: The `Default` impl change (`vec!["lesson-learned"]` → `vec![]`) is a test-infrastructure change, not a production behavior change. Production configs load via serde, which uses the `default_boosted_categories()` fn returning `["lesson-learned"]`. The test `test_default_config_boosted_categories_is_lesson_learned` is correctly rewritten to test the serde path, not Default. AC-17/AC-18/AC-27 guard against regression.
- **eval harness (layer.rs)**: The one-line change replaces a hardcoded literal with `profile.config_overrides.knowledge.boosted_categories.iter().cloned().collect()`. The profile config overrides always carry the serde-loaded value, so for any eval profile that does not explicitly override `boosted_categories`, the serde default `["lesson-learned"]` applies. Zero behavior change for all existing eval profiles.
- **ServiceLayer wiring**: All seven test infrastructure literal replacement sites now use `default_boosted_categories_set()` from `infra/config.rs`. The helper returns the same value as the replaced literals. No test behavior change.
- **Test count**: 3,470 tests pass per the RISK-COVERAGE-REPORT with 0 failures. 28 pre-existing xfails unchanged.

---

## PR Comments

- Posted 1 comment on PR #447 via `gh pr review 447 --comment`
- Blocking findings: no

---

## Knowledge Stewardship

- Searched Unimatrix for `config validation input security trust boundary category allowlist` — found ADR-007 (FEATURE_ENTRIES Trust-Level Gating) and ADR-007 (ServerError Variants for Validation) — both pre-existing, not directly applicable to crt-031 security surface.
- Searched Unimatrix for `RwLock poison recovery pattern` — found entry #734 (Graceful RwLock Fallback) confirming the `.unwrap_or_else(|e| e.into_inner())` pattern is already documented and this PR follows it correctly.
- Stored: nothing novel to store. The security risks in this PR (config injection via user-supplied strings in error messages, frozen-at-startup policy sets) are correctly handled using pre-existing patterns. The `{:?}` debug-escaping requirement for `ConfigError` variants and the operator-only trust boundary for `adaptive_categories` are instances of patterns already captured in Unimatrix (#734 and related ADRs). No new cross-feature anti-pattern emerges from this review.
