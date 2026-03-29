# Risk-Based Test Strategy: crt-032

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | One of the two default definition sites (`default_w_coac()` or struct literal) is updated but not the other, leaving an inconsistent default depending on how InferenceConfig is constructed | High | Med | Critical |
| R-02 | A default-assertion test is left asserting `0.10` after the change, causing a build failure that masks other test failures | High | Med | Critical |
| R-03 | An intentional fixture test in search.rs is incorrectly changed from `w_coac: 0.10` to `0.0`, removing coverage of non-zero w_coac scoring paths | Med | Med | High |
| R-04 | Inline doc comments referencing `0.95` sum or `Default: 0.10` are not updated, creating misleading documentation for future delivery agents | Low | High | Med |
| R-05 | `CO_ACCESS_STALENESS_SECONDS` is accidentally removed or modified during delivery of adjacent changes | Med | Low | Med |
| R-06 | `compute_search_boost` or `compute_briefing_boost` function is accidentally removed (Phase 3 out of scope) | Med | Low | Med |
| R-07 | The partial-TOML test's inline comment still references `w_coac = 0.10` in the sum calculation after delivery | Low | Med | Low |

---

## Risk-to-Scenario Mapping

### R-01: Inconsistent Default Between Two Definition Sites

**Severity**: High
**Likelihood**: Med
**Impact**: TOML-deserialized configs and programmatically-constructed configs have different w_coac values, causing non-deterministic scoring behaviour depending on config load path.

**Test Scenarios**:
1. Deserialize an empty `[inference]` TOML block and assert `w_coac == 0.0` via `(inf.w_coac).abs() < 1e-9` — this exercises `default_w_coac()`.
2. Construct `InferenceConfig::default()` directly and assert `cfg.w_coac == 0.0` — this exercises the struct literal.
3. Both assertions must pass simultaneously in the same test run (existing `test_inference_config_weight_defaults_when_absent` + `test_inference_config_default_weights_sum_within_headroom` cover both paths).

**Coverage Requirement**: Both definition sites must be tested; a single test covering only one path is insufficient.

### R-02: Default-Assertion Test Left With Old Value

**Severity**: High
**Likelihood**: Med
**Impact**: Test failure on first CI run; blocks delivery gate.

**Test Scenarios**:
1. After delivery, grep for `w_coac.*0\.10` in default-assertion test contexts — must return no matches.
2. `test_inference_config_weight_defaults_when_absent` must assert `inf.w_coac.abs() < 1e-9` with message `"w_coac default must be 0.0"`.

**Coverage Requirement**: No test must encode `0.10` as the expected default.

### R-03: Intentional Fixture Changed in search.rs

**Severity**: Med
**Likelihood**: Med
**Impact**: Scoring-math tests with non-zero w_coac inputs silently produce different results; regression coverage for the non-default w_coac code path is lost.

**Test Scenarios**:
1. Count `FusionWeights { w_coac: 0.10, ... }` occurrences in search.rs before and after delivery — count must be identical.
2. Verify at least one search.rs test constructs `FusionWeights` with `w_coac: 0.10` explicitly after delivery.

**Coverage Requirement**: All intentional search.rs fixtures are unchanged post-delivery.

### R-04: Stale Doc Comments

**Severity**: Low
**Likelihood**: High
**Impact**: Misleads future delivery agents about the current default; low immediate damage but high future rework risk.

**Test Scenarios**:
1. Grep config.rs field doc for `Default: 0.10` on the w_coac field — must return no matches.
2. Grep config.rs for `Defaults sum to 0.95` — must return no matches.
3. Grep config.rs for `0.95 + 0.02 + 0.05 = 1.02` — must return no matches.
4. Grep search.rs FusionWeights.w_coac comment for `default 0.10` — must return no matches.

**Coverage Requirement**: All four comment sites verified updated.

### R-05: CO_ACCESS_STALENESS_SECONDS Accidentally Modified

**Severity**: Med
**Likelihood**: Low
**Impact**: Data lifecycle and cleanup semantics for co-access pairs are disrupted; affects status display and maintenance tick.

**Test Scenarios**:
1. Read `CO_ACCESS_STALENESS_SECONDS` definition — value must be unchanged from pre-delivery.
2. Verify the constant is still referenced at 3 call sites: search.rs prefetch, status.rs stats, status.rs maintenance tick.

**Coverage Requirement**: Constant value and all 3 call sites verified unchanged.

### R-06: compute_search_boost or compute_briefing_boost Accidentally Removed

**Severity**: Med
**Likelihood**: Low
**Impact**: Phase 3 removal done out-of-scope without proper design; premature removal may break the partial PPR pipeline.

**Test Scenarios**:
1. Grep for `fn compute_search_boost` — must exist post-delivery.
2. Grep for `fn compute_briefing_boost` — must exist post-delivery.
3. Grep for `compute_search_boost(` call site in search.rs — must exist post-delivery.

**Coverage Requirement**: Both function definitions and the active call site present.

### R-07: Partial-TOML Test Comment Not Updated

**Severity**: Low
**Likelihood**: Med
**Impact**: Comment says `0.10` for w_coac in sum calculation; sum figure is wrong; misleads reader about what the test is actually verifying.

**Test Scenarios**:
1. Read comment inside `test_inference_config_partial_toml_gets_defaults_not_error` — must reference `0.0` for w_coac and total of `0.90` (not `1.00`).

**Coverage Requirement**: Comment updated or removed.

---

## Integration Risks

**PPR disabled + w_coac=0.0**: When both `ppr_blend_weight=0.0` and `w_coac=0.0`, co-access signal is completely absent from scoring. This is intentional and accepted per ADR-001 crt-032. No integration test needed — it is a documented operator choice, not a code defect.

**Serde deserialization interaction**: The `#[serde(default = "default_w_coac")]` attribute and the `InferenceConfig::default()` struct literal are independent paths. Both must be updated (R-01). There is no integration test needed beyond the existing TOML parse tests — these already exercise the serde path.

---

## Edge Cases

- TOML config with `w_coac = 0.0` explicitly: must pass validation (not rejected as invalid). Covered by existing `test_inference_config_validate_accepts_all_zeros` (no change needed).
- TOML config with `w_coac = 1.0` and all others 0.0: must pass validation. Existing tests cover this.
- TOML config absent (all defaults): sum must be 0.85. Covered by updated `test_inference_config_weight_defaults_when_absent`.
- Partial TOML with only `w_nli = 0.40`: sum `0.40 + 0.25 + 0.15 + 0.00 + 0.05 + 0.05 = 0.90 <= 1.0`. Assertion passes naturally; only the comment needs updating.

---

## Security Risks

**Untrusted input**: `w_coac` is a config field read from an operator-supplied TOML file. The existing `validate()` method enforces `[0.0, 1.0]` per-field range and `sum <= 1.0`. No new attack surface is introduced by this feature — it only changes a default value.

**Blast radius**: Minimal. A misconfigured `w_coac` value that slips past validation can only affect search result ordering — no data corruption, no auth bypass, no privilege escalation.

---

## Failure Modes

**Both definition sites updated, tests still fail**: Likely a test using `make_weight_config()` that computes a sum. Check if any test does exact `== 0.95` comparison rather than `<= 0.95` upper bound — none were found in the audit, but verify.

**search.rs tests fail after delivery**: Delivery agent incorrectly changed intentional fixtures. Revert changes to search.rs test fixtures; only the comment on line 118 should change in search.rs.

**validate() rejects sum after change**: Cannot happen — 0.85 < 1.0. If this error appears, a non-default weight was accidentally changed.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (dual definition sites) | R-01 | Architecture enumerates both sites; spec has separate ACs (AC-01, AC-02) |
| SR-02 (stale sum comments) | R-04 | Spec ACs AC-11, AC-12, AC-13 cover all three config.rs comment sites |
| SR-03 (make_weight_config sum ripple) | R-02 | Spec classifies tests; make_weight_config updated to 0.0; sum assertions use upper bound |
| SR-04 (search.rs comment) | R-04 | Spec AC-14 covers FusionWeights comment update |
| SR-05 (fixture vs default confusion) | R-03 | Spec Test Classification Reference explicitly lists what must and must not change |
| SR-06 (PPR blend interaction) | — | Documented in ADR-001 crt-032 (#3785); no test needed |
| SR-07 (CO_ACCESS_STALENESS_SECONDS) | R-05 | AC-07 + R-05 scenario verifies constant unchanged |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 2 (R-01, R-02) | 5 scenarios |
| High | 1 (R-03) | 2 scenarios |
| Medium | 3 (R-04, R-05, R-06) | 9 scenarios |
| Low | 1 (R-07) | 1 scenario |
