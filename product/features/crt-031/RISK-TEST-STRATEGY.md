# Risk-Based Test Strategy: crt-031

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | `validate_config` test fixtures with custom `categories` fail with wrong error due to both `boosted_categories` and `adaptive_categories` defaulting to `["lesson-learned"]` — cross-check fires before the assertion under test | High | High | Critical |
| R-02 | `StatusService::new()` has three direct construction sites beyond `ServiceLayer::new()`: `run_single_tick` in `background.rs` (line ~446), and two test helpers in `services/status.rs`. Architecture specifies only `ServiceLayer` wiring. All four sites must receive `Arc<CategoryAllowlist>` or `context_status` will silently return stale/empty lifecycle data from the tick path | High | High | Critical |
| R-03 | `is_adaptive` called on the `adaptive` lock while `validate` is called on the `categories` lock concurrently — two independent `RwLock` fields can produce a visible window where `validate` passes but `is_adaptive` has not yet reflected a runtime `add_category` (domain pack path) | Med | Low | Medium |
| R-04 | Module split (`categories.rs` → `infra/categories/mod.rs + lifecycle.rs`) changes the internal file layout; any import path that goes below `crate::infra::categories` in tests or downstream crates breaks silently at compile time if re-exports in `mod.rs` are incomplete | Med | Med | High |
| R-05 | `spawn_background_tick` parameter count grows from 22 to 23; if the function already exceeds the implicit complexity threshold reviewers tolerate, the PR may be blocked at review for a refactor that was explicitly deferred (OQ-05 / SR-02) | Med | Low | Medium |
| R-06 | `maintenance_tick` stub calls `is_adaptive()` in the guard but SPECIFICATION FR-12 also requires logging via `list_adaptive()` — if implemented with two sequential lock acquisitions per category per tick rather than one `list_adaptive()` call, the stub sets a noisy pattern for #409 to inherit | Low | Med | Low |
| R-07 | `merge_configs` adaptive_categories field (FR-10) omitted during implementation — config merging silently drops the operator-specified list, substituting the default on every project-config read | High | Med | High |
| R-08 | `StatusReport.category_lifecycle` populated from `list_categories()` which reads a `HashSet` — iteration order is non-deterministic, causing golden-output test failures that are flaky across Rust versions if the Vec is not sorted before formatting | Med | Med | Medium |
| R-09 | `CategoryAllowlist::new()` used in `server.rs` default init carries the correct `["lesson-learned"]` default silently; a future change to `from_categories` default parameter breaks the policy wire without a compile error | Low | Low | Low |
| R-10 | Gate 3b delivers production code but missing entire test modules — historical pattern (#3579): stub guard in `background.rs` and status formatter in `mcp/response/status.rs` are low-visibility targets that produce no behavioral side-effects in this feature and are easy to omit | High | Med | High |
| R-11 | `KnowledgeConfig::default()` change from `boosted_categories: vec!["lesson-learned"]` to `vec![]` causes silent assertion failures in any test that constructs via `Default` and implicitly expects `["lesson-learned"]` — failures appear as unrelated assertion errors, not compile errors (#3774) | High | High | Critical |

---

## Risk-to-Scenario Mapping

### R-01: validate_config cross-check collision with boosted_categories default
**Severity**: High
**Likelihood**: High
**Impact**: Test suite failures that surface as `BoostedCategoryNotInAllowlist` when the test was written to exercise `AdaptiveCategoryNotInAllowlist`, or vice versa. The collision was already documented for `boosted_categories` alone in entry #2312; this feature doubles the collision surface to two parallel lists. Tests that use `KnowledgeConfig { categories: <custom>, ..Default::default() }` without zeroing both parallel lists will fail with the wrong error variant — leading the implementer to fix the wrong thing.

**Test Scenarios**:
1. Unit test: `KnowledgeConfig { categories: vec!["x"], boosted_categories: vec![], adaptive_categories: vec![], freshness_half_life_hours: None }` — assert `validate_config` returns `Ok(())`.
2. Unit test: Config with `categories = ["lesson-learned"]` and `adaptive_categories = ["unknown"]`, `boosted_categories: vec![]` — assert error is exactly `AdaptiveCategoryNotInAllowlist { category: "unknown" }`, NOT `BoostedCategoryNotInAllowlist`.
3. Unit test: Config with `categories = ["lesson-learned"]` and `boosted_categories = ["unknown"]`, `adaptive_categories: vec![]` — assert error is `BoostedCategoryNotInAllowlist`, confirming the check ordering (boosted before adaptive, per FR-03).
4. Audit: grep `KnowledgeConfig {` across `crates/` and confirm every test fixture using a custom `categories` list zeroes both `boosted_categories` and `adaptive_categories`.

**Coverage Requirement**: Every `validate_config` test path (Ok, boosted error, adaptive error, empty adaptive, multi-value adaptive) must be independently exercisable without the other parallel-list default interfering. AC-25 (SR-03 fixture isolation test) must pass before gate 3b.

---

### R-02: StatusService::new() has three bypassed construction sites beyond ServiceLayer
**Severity**: High
**Likelihood**: High
**Impact**: The architecture specifies adding `Arc<CategoryAllowlist>` as a field on `StatusService` and wiring it through `ServiceLayer::new()`. Confirmed from source: `run_single_tick` in `background.rs` (line ~446) constructs `StatusService::new()` directly, bypassing `ServiceLayer` entirely. Additionally, `services/status.rs` contains two test helper functions that construct `StatusService::new()` directly (lines ~1886 and ~2038). Historical pattern entry #3216 documents this exact bypass pattern from dsn-001 where a parameter was silently dropped via the `run_single_tick` direct construction path. If these three sites are not updated, `context_status` called from the maintenance tick will return empty `category_lifecycle` data. The test helpers will fail to compile after the constructor signature change, making this a compile-time catch — but only for the test helpers. The `run_single_tick` path compiles if `CategoryAllowlist::new()` is used as a default, silently producing incorrect lifecycle output from the tick path.

**Test Scenarios**:
1. Pre-implementation: grep `StatusService::new` across the entire codebase and list all construction sites. The architecture doc must enumerate all four: `services/mod.rs`, `background.rs::run_single_tick`, and two in `services/status.rs` test helpers.
2. Compile test: after adding `category_allowlist: Arc<CategoryAllowlist>` to `StatusService::new`, `cargo check --workspace` must fail until all four construction sites are updated — this is the primary safety net.
3. Integration test: `context_status` response populated via the `run_single_tick` path (not the serving path) must include `category_lifecycle` with correct labels — not an empty vec.
4. Wiring assertion: for each of the three non-ServiceLayer construction sites, assert the `CategoryAllowlist` passed reflects the correct policy (not a fresh `CategoryAllowlist::new()` that ignores operator config).

**Coverage Requirement**: All four `StatusService::new()` construction sites must be updated. A compile failure for missing parameter is the minimum bar; a runtime test that calls `compute_report()` from the tick path and asserts non-empty `category_lifecycle` is the full bar.

---

### R-03: Two independent RwLock fields — domain pack add_category race
**Severity**: Med
**Likelihood**: Low
**Impact**: `add_category` writes to the `categories` lock; it does not touch the `adaptive` lock. A caller could observe a category that passes `validate()` but returns `false` from `is_adaptive()` — because `add_category` has no lifecycle parameter and always defaults to pinned. This is a design decision (ADR-001 §Harder), not a bug. The risk is that test authors or future #409 implementers assume runtime-added categories can be adaptive without a config restart.

**Test Scenarios**:
1. Unit test: call `add_category("new-cat")`, then call `is_adaptive("new-cat")` — assert `false`.
2. Unit test: confirm `validate("new-cat")` returns `Ok(())` after `add_category` but `is_adaptive("new-cat")` still returns `false` — both behaviors simultaneously correct.
3. Code comment: the no-lifecycle-parameter behavior of `add_category` must be stated in a doc comment — confirm presence in review.

**Coverage Requirement**: The pinned-by-default behavior of `add_category` is tested explicitly. The asymmetry between `validate` success and `is_adaptive` result for runtime-added categories is documented.

---

### R-04: Module split import path breakage
**Severity**: Med
**Likelihood**: Med
**Impact**: `categories.rs` → `infra/categories/mod.rs + lifecycle.rs`. If `mod.rs` omits any `pub use lifecycle::*` re-export, all files that import `crate::infra::categories::CategoryAllowlist` or `INITIAL_CATEGORIES` will fail to compile with a missing-item error that could be misattributed to the new fields rather than the split.

**Test Scenarios**:
1. Compile test: `cargo build -p unimatrix-server` succeeds after the split with zero changes to any file outside `infra/categories/`.
2. Import test: verify `use crate::infra::categories::CategoryAllowlist` resolves correctly from `background.rs`, `main.rs`, `server.rs`, and `services/status.rs` — four known import sites.
3. Regression test: existing `CategoryAllowlist` tests (`cargo test -p unimatrix-server -- categories`) pass with zero test renames (AC-12).

**Coverage Requirement**: The module split must not require changes to import paths in files outside `infra/categories/`. A successful `cargo test` run after the split is sufficient.

---

### R-05: Parameter count friction at PR review
**Severity**: Med
**Likelihood**: Low
**Impact**: The 22→23 parameter growth on `spawn_background_tick` was explicitly noted in SR-02 and deferred via OQ-05. If a reviewer reopens the composite-struct question during code review, the PR stalls for a refactor that was deliberately out of scope.

**Test Scenarios**:
1. Verify the PR description explicitly references the SR-02 / OQ-05 deferral and links to the crt-031 architecture document.
2. Confirm `#[allow(clippy::too_many_arguments)]` is present on `spawn_background_tick` before adding the parameter (already required per SPECIFICATION Constraint 6).

**Coverage Requirement**: No behavioral test needed. Confirm the allow attribute is present; confirm the PR description documents the deferral decision.

---

### R-06: Double lock acquisition in lifecycle guard stub
**Severity**: Low
**Likelihood**: Med
**Impact**: A naive stub implementation calls `list_categories()` (acquires `categories` read lock), then iterates calling `is_adaptive()` per-category (acquires `adaptive` read lock once per item). FR-12 specifies using `list_adaptive()` to get the full adaptive list in one call, then logging it. Implementing this as repeated per-category `is_adaptive()` calls instead is not incorrect but sets a noisy pattern for #409 to inherit.

**Test Scenarios**:
1. Review the stub implementation for unnecessary repeated lock acquisitions — prefer `list_adaptive()` called once.
2. Unit test: verify no lock guard is held across an `await` boundary in the stub (the function is `async fn` and Rust will catch this at compile time if a non-Send guard crosses an await, but worth confirming explicitly).

**Coverage Requirement**: Stub implementation reviewed for lock hygiene. No lock held across an `.await` point.

---

### R-07: merge_configs omission silently drops adaptive_categories
**Severity**: High
**Likelihood**: Med
**Impact**: `merge_configs` applies project-wins-else-global logic for each `KnowledgeConfig` field. If `adaptive_categories` is absent from the merge block, a user with both project-level and global-level configs silently uses the default `["lesson-learned"]` regardless of configuration — with no error and no warning. Only `context_status` output would reveal the discrepancy.

**Test Scenarios**:
1. Unit test: project config `adaptive_categories = ["pattern"]`, global config `adaptive_categories = ["lesson-learned"]` — assert `merge_configs` produces `["pattern"]` (project wins).
2. Unit test: project config omits `adaptive_categories` (empty/default), global config has `adaptive_categories = ["lesson-learned", "convention"]` — assert merged result equals global config value.
3. Review: confirm `merge_configs` has an explicit `adaptive_categories` merge line analogous to the `boosted_categories` line (FR-10).

**Coverage Requirement**: Both project-wins and global-fallback paths for `adaptive_categories` in `merge_configs` are covered by unit tests before gate 3b.

---

### R-08: Non-deterministic golden-output test failure from unsorted category_lifecycle Vec
**Severity**: Med
**Likelihood**: Med
**Impact**: `StatusReport.category_lifecycle` is `Vec<(String, String)>` populated by iterating `list_categories()`. The order of `list_categories()` is not guaranteed (reads from a `HashSet`). If the Vec is not sorted before formatting, the summary and JSON output have non-deterministic ordering, causing golden-output tests to be flaky.

**Test Scenarios**:
1. Unit test: assert `StatusReport.category_lifecycle` is sorted alphabetically by category name across multiple construction calls with the same input.
2. JSON test: assert the `category_lifecycle` entries appear in sorted order in JSON output; use deserialized comparison, not raw string equality.
3. Unit test: assert the summary "Adaptive categories" line lists categories in alphabetical order when multiple adaptive categories are configured.

**Coverage Requirement**: Sort must be applied before storing in `category_lifecycle`, not just at format time. FR-11 binding.

---

### R-09: server.rs default init carries silent policy — future regression risk
**Severity**: Low
**Likelihood**: Low
**Impact**: `server.rs` uses `CategoryAllowlist::new()` as a default field initializer. This correctly carries `["lesson-learned"]` as adaptive via the delegation chain. But the delegation is implicit — a future change to `from_categories`'s default adaptive list would silently change `server.rs`'s behavior with no visible change in `server.rs` itself.

**Test Scenarios**:
1. Wiring test (AC-13): create `CategoryAllowlist::new()`, assert `is_adaptive("lesson-learned") == true` and `is_adaptive("decision") == false`. This test must exist in `categories/mod.rs` tests.

**Coverage Requirement**: AC-13 wiring test is present and covers the delegation chain from `new()` through `from_categories` to `from_categories_with_policy`.

---

### R-10: Gate 3b missing test modules — stub and formatter are low-visibility targets
**Severity**: High
**Likelihood**: Med
**Impact**: The lifecycle guard stub in `background.rs` and the `category_lifecycle` formatter in `mcp/response/status.rs` produce no behavioral side effects in this feature. An implementer under time pressure may deliver the production code (config field, `is_adaptive`, `validate_config`) but omit the `background.rs` and `status.rs` test modules entirely. Historical pattern #3579 documents this exact failure mode from nan-009.

**Test Scenarios**:
1. Pre-gate 3b check: confirm tests for AC-10 and AC-11 are present in `background.rs` — minimum 2 named tests for the stub.
2. Pre-gate 3b check: confirm tests for AC-09 are present in `mcp/response/status.rs` — minimum 2 tests (one summary, one JSON format).
3. Gate 3c: `cargo test -p unimatrix-server -- background` and `cargo test -p unimatrix-server -- status` must both show the new lifecycle-related tests by name.

**Coverage Requirement**: All test modules must have at least one passing test before gate 3b closes. The risk coverage report must not be accepted if `background` or `status` module tests are absent.

---

### R-11: KnowledgeConfig::default() change causes silent test assertion failures
**Severity**: High
**Likelihood**: High
**Impact**: Changing `KnowledgeConfig::default()` from `boosted_categories: vec!["lesson-learned"]` to `vec![]` does not produce a compile error. Any test that constructs via `Default` and then asserts `boosted_categories == ["lesson-learned"]` will now fail with a seemingly unrelated assertion error mid-implementation. Historical pattern #3774 documents this exact failure mode. The known affected test is `main_tests.rs` line 393; additional tests may exist. The implementer must grep before writing any code. Failing to do the pre-implementation grep means the failures appear mid-wave and consume debugging time.

**Test Scenarios**:
1. Pre-implementation (mandatory FR-19): run `grep -rn "KnowledgeConfig::default()" crates/` and inspect every hit for implicit reliance on `boosted_categories == ["lesson-learned"]`.
2. Pre-implementation: run `grep -rn "UnimatrixConfig::default()" crates/` for the same reason.
3. AC-17: add a dedicated unit test `test_knowledge_config_default_boosted_is_empty` asserting `KnowledgeConfig::default().boosted_categories.is_empty()` — this test guards against future regression of the Default impl.
4. AC-18: rewrite `test_default_config_boosted_categories_is_lesson_learned` to parse an empty TOML string and assert `knowledge.boosted_categories == ["lesson-learned"]` — covering the serde path, not `Default`.
5. AC-27: a second unit test `test_knowledge_config_default_adaptive_is_empty` asserting `KnowledgeConfig::default().adaptive_categories.is_empty()`.

**Coverage Requirement**: Pre-implementation grep is documented in the PR description (AC-26). Both AC-17 and AC-18 tests are present. No test in the workspace constructs via `Default` and asserts `boosted_categories == ["lesson-learned"]` after the PR lands.

---

## Integration Risks

**I-01: Two-call-site main.rs update.** Both `from_categories_with_policy` call sites at approximately lines ~550 and ~940 must be updated in lockstep. Missing one call site means one code path (project config or global config) carries only the delegation-default `["lesson-learned"]` policy regardless of `config.toml`. No compile error results — the old `from_categories` still compiles. The spec enumerates both sites (FR-09) but the implementer must search for both rather than relying on compiler guidance. Both `ServiceLayer::new()` call sites in `main.rs` must also pass `Arc::clone(&categories)` for the `StatusService` field.

**I-02: StatusReport Default impl.** `StatusReport` has a `Default` impl. The new `category_lifecycle: Vec<(String, String)>` field must be included in the `Default` impl as `vec![]`. Since `Vec<(String,String)>` has a `Default`, the compile will succeed regardless — the risk is silent empty output if the field initialization is omitted from `Default`. Tests must verify the `Default` impl produces an empty vec.

**I-03: StatusReportJson HashMap vs StatusReport Vec.** `StatusReport` uses `Vec<(String, String)>` for `category_lifecycle`; the JSON representation likely uses `HashMap<String, String>` or an ordered structure. If the Vec is sorted but the HashMap serializes in non-deterministic order, JSON golden tests will be flaky. Use `serde_json::to_value` and compare the deserialized map, not raw string equality, in JSON tests.

**I-04: run_single_tick receives the operator-configured Arc or a fresh default.** The `run_single_tick` function signature must thread `Arc<CategoryAllowlist>` from `background_tick_loop` → `run_single_tick` → `StatusService::new`. If the implementer shortcts by constructing `CategoryAllowlist::new()` inline at the `run_single_tick` call site, the tick path will always carry `["lesson-learned"]` regardless of operator config — the correct value but not the operator-configured value. For this feature the values happen to match, but the pattern is fragile. The Arc from startup must be threaded, not reconstructed.

---

## Edge Cases

**E-01: Empty adaptive_categories.** `validate_config` must accept `[]`; `is_adaptive` must return `false` for all categories; maintenance tick must skip the debug log entirely; `context_status` summary must omit the "Adaptive categories" line. All four behaviors from a single config value — must be tested together.

**E-02: All five categories marked adaptive.** `adaptive_categories = ["lesson-learned", "decision", "convention", "pattern", "procedure"]` — valid per spec, produces 5 adaptive categories. `context_status` JSON must label all five as `"adaptive"` and none as `"pinned"`.

**E-03: Single-character category name.** `adaptive_categories = ["x"]` where `categories = ["x"]` — edge of the cross-check string comparison. `validate_config` must accept it; `is_adaptive("x")` must return `true`.

**E-04: `adaptive_categories` contains a duplicate.** `adaptive_categories = ["lesson-learned", "lesson-learned"]` — the spec does not prohibit this. After `from_categories_with_policy`, the `HashSet` deduplicates silently. `context_status` output should show `lesson-learned` once. No error should be raised.

**E-05: Category added at runtime via `add_category` matching a name in `adaptive_categories`.** Impossible by construction — `adaptive_categories` is loaded at startup and the `adaptive` HashSet is frozen after construction. `add_category` only writes to `categories`. Document this invariant in the struct doc comment.

**E-06: `config.toml` specifies `adaptive_categories = ["LESSON-LEARNED"]` (wrong case).** The `categories` list is case-sensitive. `validate_config` would emit `AdaptiveCategoryNotInAllowlist` for `"LESSON-LEARNED"` even though `"lesson-learned"` is in `categories`. This is correct behavior; the error message should identify the case mismatch by quoting the offending value.

**E-07: Serde round-trip for both new fields.** `KnowledgeConfig` with `adaptive_categories = ["lesson-learned", "pattern"]` must survive a `toml::to_string` / `toml::from_str` round-trip with both fields intact and in the correct order (entry #885: serde-heavy types need explicit round-trip tests).

---

## Security Risks

**S-01: Config injection via adaptive_categories.** `adaptive_categories` is deserialized from `config.toml`, which is operator-controlled. Category names are arbitrary strings. The `HashSet::contains` call in `is_adaptive` is a pure equality check — no path traversal, no SQL interpolation, no shell expansion. The `tracing::debug!` call in the stub uses the `?` formatter which is safe for untrusted strings. Blast radius if a malicious string enters: the debug log contains it. No injection risk.

**S-02: ConfigError Display includes operator-supplied category name.** `AdaptiveCategoryNotInAllowlist { category }` is formatted into an error message returned to the operator at startup. The category string is user-supplied. The format string should use `{category:?}` (debug escaping) to quote and escape the string, consistent with `BoostedCategoryNotInAllowlist`. No log injection risk beyond what the operator can already write to their own config file.

**S-03: `context_status` exposes category policy.** The JSON output includes all category names and their lifecycle labels. This is read-only metadata about server configuration. Category names are already visible in `context_status` via `category_distribution`. No elevated risk.

---

## Failure Modes

**FM-01: `validate_config` fails at startup.** Server exits with `AdaptiveCategoryNotInAllowlist` error. The error message includes the config file path and the offending category name. Recovery: add the category to `[knowledge] categories` in `config.toml` and restart.

**FM-02: `adaptive` RwLock is poisoned.** `is_adaptive` recovers via `.unwrap_or_else(|e| e.into_inner())` and returns the best available result from the poisoned guard. Will not panic. Established pattern followed throughout.

**FM-03: `categories.rs` module split introduces import error.** Build fails at compile time with a clear "unresolved import" error. Fix: add the missing `pub use` line in `mod.rs`. No runtime risk.

**FM-04: `merge_configs` missing adaptive_categories.** Server starts successfully. Operator's `adaptive_categories` config value is silently ignored; the default `["lesson-learned"]` applies. `context_status` output reveals the discrepancy. No indication from startup that the merge failed. Mitigation: R-07 test scenarios catch this before delivery.

**FM-05: `maintenance_tick` stub panics.** Cannot happen by construction — the stub contains only `HashSet` reads and a `tracing::debug!`. The only failure mode is lock poison, handled by `list_adaptive()`'s recovery path.

**FM-06: `run_single_tick` uses a fresh `CategoryAllowlist::new()` instead of the startup Arc.** Server starts and runs. Maintenance tick reports lifecycle correctly (because `new()` defaults to `["lesson-learned"]`). But if the operator configured `adaptive_categories = []` or a non-default set, the tick ignores that and always uses `["lesson-learned"]`. No error. Mitigation: R-02 test scenario 4 and I-04 catch this pattern.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (categories.rs 500-line limit) | R-04 | Architecture mandates module split to `infra/categories/mod.rs + lifecycle.rs`. R-04 covers the import-path regression risk introduced by the split. |
| SR-02 (22-parameter spawn_background_tick) | R-05 | Deferred per OQ-05. `BackgroundTickConfig` composite struct out of scope for crt-031. Architecture accepts the 23rd parameter with explicit `#[allow(clippy::too_many_arguments)]` justification. |
| SR-03 (boosted_categories + adaptive_categories default collision in test fixtures) | R-01 | Critical priority. Architecture codified the mandatory test construction pattern (ARCHITECTURE.md §SR-03). Specification adds AC-24 and AC-25. R-01 covers test scenarios and gate enforcement. |
| SR-04 (summary/JSON asymmetry could silently omit pinned categories from operator audits) | R-08 | Architecture locked the asymmetry as intentional. R-08 covers the non-deterministic ordering risk that undermines golden-output tests. Alphabetic sort required on `category_lifecycle` Vec. |
| SR-05 (CategoryAllowlist call sites not all updated) | R-02, R-04, R-07 | Three distinct failure modes: StatusService wiring (R-02, now confirmed Critical — three bypass sites exist beyond ServiceLayer), module split re-exports (R-04), merge_configs omission (R-07). All covered with concrete test scenarios. |
| SR-06 (lifecycle guard stub must be designed as clear insertion point for #409) | R-10 | Stub form fully specified in FR-12 and ARCHITECTURE.md §Component 5. R-10 covers the delivery risk that stub tests are omitted at gate 3b. |
| SR-07 (eval harness config path — OQ-5) | — | Resolved: one-line fix confirmed in ARCHITECTURE.md §OQ-5. `profile.config_overrides.knowledge.boosted_categories` is accessible at Step 12. No architecture-level risk remains. Covered by AC-19 (grep verification). |
| SR-08 (shared helper circular dependency for test literals) | — | Resolved: `default_boosted_categories_set()` in `infra/config.rs` has no upward dependency on any of the seven test sites. Architecture confirms importability. No architecture-level risk; implementation detail verified. |
| SR-09 (KnowledgeConfig::default() callers after Default impl change) | R-11 | Critical priority. Architecture §SR-09 enumerates two known test sites. R-11 requires a pre-implementation grep (FR-19) and covers both serde test rewrite and Default unit test guards. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 3 (R-01, R-02, R-11) | 13 scenarios — fixture collision, three bypass sites, pre-implementation grep, serde rewrite |
| High | 4 (R-04, R-07, R-10, plus I-04) | 10 scenarios — compile test, merge test, gate module checks |
| Medium | 3 (R-03, R-05, R-08) | 6 scenarios — domain-pack pinned behavior, allow attribute, sort enforcement |
| Low | 2 (R-06, R-09) | 3 scenarios — lock hygiene, delegation chain test |

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for `lesson-learned failures gate rejection` — found #3579 (gate 3b missing test modules, elevated R-10 to High) and #2758 (gate 3c false PASS claims).
- Queried: `/uni-knowledge-search` for `risk pattern CategoryAllowlist config validation` — found #3770 (parallel list pattern), #3771 (parallel list collision trap), #2312 (boosted_categories default trap). All directly inform R-01.
- Queried: `/uni-knowledge-search` for `serde default impl change test breakage silent failure` — found #3774 (Default/serde split silent failure, confirmed R-11 as Critical), #885 (serde-heavy types need round-trip tests, informs E-07), #3773 (literal duplication root cause).
- Queried: `/uni-knowledge-search` for `background tick parameter threading` — found #3216 (arc-threading gap + hidden run_single_tick bypass, directly identified R-02 severity upgrade and the three non-ServiceLayer `StatusService::new()` construction sites), #2553 (service constructor signature propagates to background.rs test helpers, confirmed R-02 scope).
- Stored: R-02 upgrade is an instance of the existing #3216 pattern — no new cross-feature pattern to store. The `StatusService::new()` bypass at `run_single_tick` is already documented in #3216 and #2553.
