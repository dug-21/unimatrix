# Risk-Based Test Strategy: crt-031

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | `validate_config` test fixtures with custom `categories` fail with wrong error due to both `boosted_categories` and `adaptive_categories` defaulting to `["lesson-learned"]` — cross-check fires before the assertion under test | High | High | Critical |
| R-02 | `StatusService::compute_report()` does not currently hold `Arc<CategoryAllowlist>`; wiring it in is unconfirmed scope that could stall the delivery wave | High | Med | High |
| R-03 | `is_adaptive` called on the `adaptive` lock while `validate` is called on the `categories` lock concurrently — two independent `RwLock` fields can produce a visible window where `validate` passes but `is_adaptive` has not yet reflected a runtime `add_category` (domain pack path) | Med | Low | Medium |
| R-04 | Module split (`categories.rs` → `infra/categories/mod.rs + lifecycle.rs`) changes the internal file layout; any import path that goes below `crate::infra::categories` in tests or downstream crates breaks silently at compile time if re-exports in `mod.rs` are incomplete | Med | Med | High |
| R-05 | `spawn_background_tick` parameter count grows from 22 to 23; if the function already exceeds the implicit complexity threshold reviewers tolerate, the PR may be blocked at review for a refactor that was explicitly deferred (OQ-05 / SR-02) | Med | Low | Medium |
| R-06 | `maintenance_tick` stub calls `is_adaptive()` in the guard but the SPECIFICATION's FR-15 also requires logging the adaptive list via a separate `list_categories()` + `filter(is_adaptive)` pass — two lock acquisitions per tick instead of one, if not consolidated | Low | Med | Low |
| R-07 | `merge_configs` adaptive_categories field (FR-17) omitted during implementation — config merging silently drops the operator-specified list, substituting the default on every project-config read | High | Med | High |
| R-08 | `context_status` JSON `category_lifecycle` and summary "Adaptive categories" line diverge from the `StatusReport.category_lifecycle` Vec ordering — alphabetic sort (FR-10) not enforced, causing non-deterministic golden-output test failures | Med | Med | Medium |
| R-09 | `CategoryAllowlist::new()` used in `server.rs` default init carries the correct `["lesson-learned"]` default silently; a future change to `from_categories` default parameter breaks the policy wire without a compile error | Low | Low | Low |
| R-10 | Gate 3b delivers production code but missing entire test modules — historical pattern (#3579): stub guard in `background.rs` and status formatter in `mcp/response/status.rs` are easy to omit because they have no behavioral side-effects today | High | Med | High |

---

## Risk-to-Scenario Mapping

### R-01: validate_config cross-check collision with boosted_categories default
**Severity**: High
**Likelihood**: High
**Impact**: Test suite failures that surface as `BoostedCategoryNotInAllowlist` when the test was written to exercise `AdaptiveCategoryNotInAllowlist`, or vice versa. The confusion was already documented for `boosted_categories` in entry #2312; this feature doubles the collision surface. Tests that use `KnowledgeConfig { categories: <custom>, ..Default::default() }` without zeroing both parallel lists will fail with the wrong error variant — leading the implementer to fix the wrong thing.

**Test Scenarios**:
1. Unit test: `KnowledgeConfig { categories: vec!["x"], boosted_categories: vec![], adaptive_categories: vec![], .. }` — assert `validate_config` returns `Ok(())`.
2. Unit test: Config with `categories = ["lesson-learned"]` and `adaptive_categories = ["unknown"]` — assert error is `AdaptiveCategoryNotInAllowlist { category: "unknown" }`, NOT `BoostedCategoryNotInAllowlist`.
3. Unit test: Config with `categories = ["lesson-learned"]` and `boosted_categories = ["unknown"]` — assert error is `BoostedCategoryNotInAllowlist`, NOT `AdaptiveCategoryNotInAllowlist` (verify ordering: boosted check fires before adaptive check as specified in FR-04).
4. Audit: every existing `validate_config` test that uses a custom `categories` list must have `adaptive_categories: vec![]` — the test suite will catch this implicitly, but the implementer must update all helpers.

**Coverage Requirement**: Every `validate_config` test path (Ok, boosted error, adaptive error, empty adaptive list, multi-value adaptive list) must be independently exercisable without the other parallel-list default interfering. AC-16 must pass before gate 3b sign-off.

---

### R-02: StatusService CategoryAllowlist wiring is unconfirmed scope
**Severity**: High
**Likelihood**: Med
**Impact**: If `StatusService` does not already hold `Arc<CategoryAllowlist>` (Architecture OQ-01 is listed as open at spec time), the delivery agent must also thread the Arc through `StatusService::new()` and all its construction call sites. This is additional scope not reflected in the test count estimate (NFR-07) or the implementation guidance. If discovered mid-wave, the implementer may shortcut by passing the allowlist as a `compute_report` parameter instead — creating an API inconsistency with the field-based approach used elsewhere.

**Test Scenarios**:
1. Verify before coding: read `StatusService::new()` signature and field list; confirm whether `Arc<CategoryAllowlist>` is already present.
2. If not present: add a compile-level test that constructs `StatusService` with a custom `CategoryAllowlist` and calls `compute_report` — asserting that `category_lifecycle` reflects the injected policy (not a hardcoded default).
3. Integration test: `context_status` response for a server configured with `adaptive_categories = ["lesson-learned", "pattern"]` must show both as `"adaptive"` in JSON output.

**Coverage Requirement**: `StatusService::compute_report()` must not derive lifecycle data from a hardcoded source. The injected `CategoryAllowlist` policy must be visible in the `context_status` response.

---

### R-03: Two independent RwLock fields — domain pack add_category race
**Severity**: Med
**Likelihood**: Low
**Impact**: `add_category` writes to the `categories` lock; it does not touch the `adaptive` lock. A caller could observe a category that passes `validate()` (present in `categories`) but returns `false` from `is_adaptive()` even if the operator intended it to be adaptive — because `add_category` has no lifecycle parameter and always defaults to pinned. This is a design decision (ADR-001 §Harder), not a bug. The risk is that test authors or future implementers assume runtime-added categories can be adaptive without config restart.

**Test Scenarios**:
1. Unit test: call `add_category("new-cat")`, then call `is_adaptive("new-cat")` — assert `false` (pinned by default).
2. Unit test: confirm `validate("new-cat")` returns `Ok(())` after `add_category` but `is_adaptive("new-cat")` still returns `false` — both behaviors correct simultaneously.
3. Documentation test: the no-lifecycle-parameter behavior of `add_category` must be stated in a code comment — review for presence.

**Coverage Requirement**: The pinned-by-default behavior of `add_category` is tested explicitly. The asymmetry between `validate` success and `is_adaptive` result for runtime-added categories is documented.

---

### R-04: Module split import path breakage
**Severity**: Med
**Likelihood**: Med
**Impact**: `categories.rs` → `infra/categories/mod.rs + lifecycle.rs`. If `mod.rs` omits any `pub use lifecycle::*` re-export, all files that import `crate::infra::categories::CategoryAllowlist` or `INITIAL_CATEGORIES` will fail to compile. Rust's module system gives no "did you forget a re-export?" warning — it just reports a missing item at the import site, which could appear to be a different problem.

**Test Scenarios**:
1. Compile test: `cargo build -p unimatrix-server` succeeds after the split with zero changes to any file outside `infra/categories/`.
2. Import test: verify `use crate::infra::categories::CategoryAllowlist` resolves correctly from `background.rs`, `main.rs`, `server.rs`, and `services/status.rs` — four known import sites.
3. Test: existing `CategoryAllowlist` test suite (`cargo test -p unimatrix-server -- categories`) passes with zero test renames (AC-12).

**Coverage Requirement**: The module split must not require any changes to import paths in files outside `infra/categories/`. A single successful `cargo test` run after the split is sufficient.

---

### R-05: Parameter count friction at PR review
**Severity**: Med
**Likelihood**: Low
**Impact**: The 22→23 parameter growth on `spawn_background_tick` was explicitly noted in SR-02 and deferred via OQ-05. If a reviewer reopens the composite-struct question during code review, the PR stalls for a refactor that was deliberately out of scope.

**Test Scenarios**:
1. Verify the PR description explicitly references SR-02 / OQ-05 deference and links to crt-031 architecture document — reduces review friction.
2. Check that `#[allow(clippy::too_many_arguments)]` is present on `spawn_background_tick` before adding the parameter (already required per SPECIFICATION Constraint 4).

**Coverage Requirement**: No behavioral test needed. Confirm the allow attribute is present; confirm the PR description documents the deference decision.

---

### R-06: Double lock acquisition per tick in lifecycle guard
**Severity**: Low
**Likelihood**: Med
**Impact**: The stub as specified in FR-15 calls `list_categories()` (acquires `categories` read lock), filters via `is_adaptive()` (acquires `adaptive` read lock for each iteration), then immediately calls `is_adaptive()` again as the "representative category" guard. This is three lock acquisitions for a no-op stub. Not a correctness issue, but it sets a noisy pattern that #409 will likely replicate.

**Test Scenarios**:
1. Review the stub implementation for unnecessary repeated lock acquisitions — prefer collecting the adaptive list once and reusing.
2. Unit test: the stub must not hold any lock across an `await` boundary (the function is `async fn`). Verify the lock guard is dropped before any `.await` (Rust will enforce this if using standard locking but worth confirming explicitly).

**Coverage Requirement**: The stub implementation is reviewed for lock hygiene. No lock is held across an `.await` point.

---

### R-07: merge_configs omission silently drops adaptive_categories
**Severity**: High
**Likelihood**: Med
**Impact**: `merge_configs` applies project-wins-else-global logic for each `KnowledgeConfig` field. If `adaptive_categories` is not included in the merge block, a user with both a project-level and global-level config will silently use the default `["lesson-learned"]` regardless of what they configured — with no error, no warning, and no way to detect the drop except by inspecting `context_status` output.

**Test Scenarios**:
1. Unit test: construct a project config with `adaptive_categories = ["pattern"]` and a global config with `adaptive_categories = ["lesson-learned"]`. Assert `merge_configs` produces `["pattern"]` (project wins).
2. Unit test: project config omits `adaptive_categories` (empty/default), global config has `adaptive_categories = ["lesson-learned", "convention"]`. Assert merged result equals global config value.
3. Review: confirm `merge_configs` has an explicit `adaptive_categories` merge line analogous to the `boosted_categories` line (FR-17).

**Coverage Requirement**: Both project-wins and global-fallback paths for `adaptive_categories` in `merge_configs` are covered by unit tests.

---

### R-08: Non-deterministic golden-output test failure from unsorted category_lifecycle Vec
**Severity**: Med
**Likelihood**: Med
**Impact**: `StatusReport.category_lifecycle` is `Vec<(String, String)>` populated by iterating `list_categories()`. The order of `list_categories()` is not guaranteed (it reads from a `HashSet`). If the Vec is not sorted before formatting, the summary line "Adaptive categories: [lesson-learned]" and the JSON `category_lifecycle` object will have non-deterministic ordering. Golden-output tests that assert exact string equality will be flaky — passing on the first run, failing when the HashMap/HashSet iteration order changes across Rust versions.

**Test Scenarios**:
1. Unit test: assert `StatusReport.category_lifecycle` is sorted alphabetically by category name across multiple construction calls with the same input.
2. Golden-output test: assert the JSON `category_lifecycle` keys appear in sorted order (or use a key-insensitive comparison for the JSON, but the Vec must be sorted to ensure the summary is deterministic).
3. Unit test: assert the summary "Adaptive categories" line lists categories in alphabetical order when multiple adaptive categories are configured.

**Coverage Requirement**: FR-10 explicitly requires alphabetic sorting. The sort must be applied before storing in `category_lifecycle`, not just at format time.

---

### R-09: server.rs default init carries silent policy — future regression risk
**Severity**: Low
**Likelihood**: Low
**Impact**: `server.rs` uses `CategoryAllowlist::new()` as a default field initializer. This correctly carries `["lesson-learned"]` as adaptive via the delegation chain. But the delegation is implicit — a future change to `from_categories`'s default adaptive list would silently change `server.rs`'s behavior with no visible change in `server.rs` itself.

**Test Scenarios**:
1. Wiring test (AC-17): create `CategoryAllowlist::new()`, wrap in `Arc`, assert `is_adaptive("lesson-learned") == true` and `is_adaptive("decision") == false`. This test must live in `background.rs` or `server.rs` to catch the delegation chain.

**Coverage Requirement**: AC-17 wiring test is present and located in a file that directly exercises the `Arc<CategoryAllowlist>` passed to `maintenance_tick`.

---

### R-10: Gate 3b missing test modules — no-op stub and formatter are low-visibility targets
**Severity**: High
**Likelihood**: Med
**Impact**: The lifecycle guard stub in `background.rs` and the `category_lifecycle` formatter in `mcp/response/status.rs` produce no behavioral side effects in this feature. An implementer under time pressure may deliver the production code (config field, `is_adaptive`, `validate_config`) but omit the `background.rs` and `status.rs` test modules entirely. Historical pattern #3579 documents this exact failure mode: gate 3b delivers code but zero mandatory tests for specific modules. The tester at gate 3c would catch this, but it causes a wave rework cycle.

**Test Scenarios**:
1. Pre-gate 3b check: confirm test count in `background.rs` for the stub (AC-10, AC-11) — minimum 2 tests.
2. Pre-gate 3b check: confirm test count in `mcp/response/status.rs` for formatting (AC-09) — minimum 2 tests (one summary, one JSON).
3. Gate 3c: `cargo test -p unimatrix-server -- background` must show the stub-related tests by name; `cargo test -p unimatrix-server -- status` must show the lifecycle formatting tests.

**Coverage Requirement**: All four test modules enumerated in NFR-07 must have at least one passing test before gate 3b closes. The risk coverage report must not be accepted if `background` or `status` module tests are absent.

---

## Integration Risks

**I-01: Two-call-site main.rs update.** Both `from_categories_with_policy` call sites at approximately lines 550 and 940 must be updated in lockstep. Missing one call site means one code path (project config or global config) carries only the default policy regardless of `config.toml`. No compile error results — the old `from_categories` still compiles. The spec enumerates both sites (FR-09) but the implementer must search for both rather than relying on compiler guidance.

**I-02: StatusReport Default impl.** `StatusReport` has a `Default` impl used in `maintenance_tick` (SPECIFICATION Constraint 3 references line 816). The new `category_lifecycle` field must be included in the `Default` impl as `vec![]`. If omitted, the `Default` impl will fail to compile only if `Vec<(String, String)>` has no default — it does, so this will compile but produce an empty field silently. Tests must verify the `Default` impl produces the correct empty state.

**I-03: StatusReportJson uses HashMap, StatusReport uses Vec.** The two representations diverge in type: `Vec<(String, String)>` in `StatusReport` vs `HashMap<String, String>` in `StatusReportJson`. The conversion from Vec to HashMap is a `.collect()` operation. If the Vec is sorted but the HashMap serializes in non-deterministic order, JSON golden tests will be flaky. Use `serde_json::to_value` and compare the deserialized map, not the raw string, in JSON tests.

---

## Edge Cases

**E-01: Empty adaptive_categories.** `validate_config` must accept `[]`; `is_adaptive` must return `false` for all categories; maintenance tick must skip the debug log; `context_status` summary must omit the "Adaptive categories" line. All four behaviors from a single config value.

**E-02: All five categories marked adaptive.** `adaptive_categories = ["lesson-learned", "decision", "convention", "pattern", "procedure"]` — valid per spec, produces 5 adaptive categories. `context_status` JSON must label all five as `"adaptive"`.

**E-03: Single-character category name.** `adaptive_categories = ["x"]` where `categories = ["x"]` — edge of the cross-check regex/comparison. `validate_config` must accept it; `is_adaptive("x")` must return `true`.

**E-04: `adaptive_categories` contains a duplicate.** `adaptive_categories = ["lesson-learned", "lesson-learned"]` — the spec does not prohibit this. After `from_categories_with_policy`, the `HashSet` deduplicates silently. `context_status` output should show `lesson-learned` once. No error should be raised.

**E-05: Category added at runtime via `add_category` matching a name in `adaptive_categories`.** This is impossible by construction: `adaptive_categories` is loaded at startup and the `adaptive` HashSet is frozen after construction. `add_category` only writes to `categories`. The `adaptive` set cannot be mutated post-construction. Document this invariant in the struct.

**E-06: `config.toml` specifies `adaptive_categories = ["LESSON-LEARNED"]` (wrong case).** The `categories` list is case-sensitive (it is stored and compared as `String`, not normalized). `validate_config` would emit `AdaptiveCategoryNotInAllowlist` for `"LESSON-LEARNED"` even though `"lesson-learned"` is in `categories`. This is correct behavior but worth testing to ensure the error message identifies the case mismatch clearly.

---

## Security Risks

**S-01: Config injection via adaptive_categories.** `adaptive_categories` is deserialized from `config.toml`, which is operator-controlled. Category names are arbitrary strings. The `HashSet::contains` call in `is_adaptive` is a pure equality check — no path traversal, no SQL interpolation, no shell expansion. The `tracing::debug!` call in the stub uses the `?` formatter (`?adaptive_cats`) which is safe for untrusted strings. Blast radius if a malicious string enters: the debug log contains it. No injection risk.

**S-02: ConfigError Display includes operator-supplied category name.** `AdaptiveCategoryNotInAllowlist { category }` is formatted into an error message that is returned to the operator at startup. The category string is user-supplied. The format string uses `{category:?}` (debug escaping), which quotes and escapes the string. No log injection risk beyond what the operator can already write to their own config file.

**S-03: `context_status` exposes category policy.** The JSON output includes all category names and their lifecycle labels. This is read-only metadata about server configuration. No sensitive data is exposed; category names are already visible in `context_status` via `category_distribution`. No elevated risk.

---

## Failure Modes

**FM-01: `validate_config` fails at startup.** Server exits with `AdaptiveCategoryNotInAllowlist` error. The error message includes the config file path and the offending category name. The operator must add the category to `[knowledge] categories` before the listed category can be in `adaptive_categories`. Recovery: fix config, restart.

**FM-02: `adaptive` RwLock is poisoned.** `is_adaptive` recovers via `.unwrap_or_else(|e| e.into_inner())` and returns the best available `bool` from the poisoned guard data. This is the established pattern. It will not panic. The next `context_status` call or maintenance tick will use the recovered state. No special recovery procedure.

**FM-03: `categories.rs` module split introduces import error.** Build fails at compile time with a clear "unresolved import" error. Fix: add the missing `pub use` line in `mod.rs`. No runtime risk.

**FM-04: `merge_configs` missing adaptive_categories.** Server starts successfully. Operator's `adaptive_categories` config value is silently ignored; the default `["lesson-learned"]` applies. `context_status` output reveals the discrepancy. Operator has no indication the merge failed. Mitigation: R-07 test scenarios catch this before delivery.

**FM-05: `maintenance_tick` stub panics.** Cannot happen by construction — the stub contains only `HashSet` reads and a `tracing::debug!`. The only failure mode is lock poison, which is handled by `is_adaptive`'s recovery path.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (categories.rs 500-line limit) | R-04 | Architecture mandates module split to `infra/categories/mod.rs + lifecycle.rs`. R-04 covers the import-path regression risk introduced by the split. |
| SR-02 (22-parameter spawn_background_tick) | R-05 | Deferred per OQ-05. `BackgroundTickConfig` composite struct out of scope for crt-031. Architecture accepts the 23rd parameter with explicit `#[allow(clippy::too_many_arguments)]` justification. |
| SR-03 (boosted_categories + adaptive_categories default collision in test fixtures) | R-01 | The highest-priority risk in this document. Architecture codified the mandatory test construction pattern (ARCHITECTURE.md §Test Construction Pattern). Specification adds AC-16. R-01 covers test scenarios and gate enforcement. |
| SR-04 (summary/JSON asymmetry could silently omit pinned categories from operator audits) | R-08 | Architecture locked the asymmetry as intentional. R-08 covers the non-deterministic ordering risk that undermines golden-output tests. R-08 requires alphabetic sort on `category_lifecycle` Vec (FR-10 binding). |
| SR-05 (CategoryAllowlist call sites not all updated) | R-02, R-04, R-07 | Three distinct failure modes: StatusService wiring (R-02), module split re-exports (R-04), merge_configs omission (R-07). All three are covered with concrete test scenarios. |
| SR-06 (lifecycle guard stub must be designed as clear insertion point for #409) | R-10 | Stub form is fully specified in FR-15 and ARCHITECTURE.md. R-10 covers the delivery risk that the stub tests are omitted at gate 3b. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 1 (R-01) | 4 scenarios — cross-check isolation, ordering, helper audit |
| High | 4 (R-02, R-04, R-07, R-10) | 10 scenarios across wiring, compile, merge, gate checks |
| Medium | 4 (R-03, R-05, R-06, R-08) | 8 scenarios across domain-pack behavior, lock hygiene, sort |
| Low | 2 (R-09, R-10 overlap) | 2 scenarios — delegation chain wiring test |

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for lesson-learned failures gate rejection — found #3579 (gate 3b missing test modules) and #2758 (gate 3c false PASS claims); both elevated R-10 to High severity.
- Queried: `/uni-knowledge-search` for risk pattern RwLock background tick — found #1560, #1542 (background tick state cache and error semantics); confirmed two-lock approach does not introduce novel risk beyond documented patterns.
- Queried: `/uni-knowledge-search` for CategoryAllowlist config validation test fixtures — found #2312 (boosted_categories default trap, directly informs R-01) and #3770 (KnowledgeConfig parallel list pattern, confirms SR-03 scope).
- Stored: nothing novel to store — R-01 is an instance of the existing #2312 pattern; no new cross-feature pattern emerges from this assessment alone.
