# Specification: crt-032 — w_coac Reduction to 0.0 (PPR Transition Phase 2)

## Feature Summary

Change the compiled default of `w_coac` (co-access affinity fusion weight) from `0.10` to `0.0` in `crates/unimatrix-server/`. Update all documentation, comments, and test assertions that encode the old default. Write and store the architecture decision record.

---

## Domain Model

| Term | Definition |
|------|-----------|
| `w_coac` | Fusion weight for the direct co-access affinity boost term in `compute_fused_score`. Range [0.0, 1.0]. |
| `default_w_coac()` | Serde default function used when `w_coac` is absent from a TOML config file. |
| `InferenceConfig::default()` | Programmatic default struct used when constructing `InferenceConfig` without TOML. |
| `make_weight_config()` | Test helper that constructs an `InferenceConfig` with all six weights set to their current defaults. Used in per-field validation rejection tests. |
| Compiled default | The value returned by `default_w_coac()` and embedded in `InferenceConfig::default()`. |
| Intentional fixture | A test that constructs `FusionWeights` or `InferenceConfig` with explicit `w_coac: 0.10` to test scoring math with a non-zero weight. These do NOT assert the default. |
| Default-assertion test | A test that verifies what the compiled default of `w_coac` is. These MUST change. |
| Fusion weight sum invariant | `w_sim + w_nli + w_conf + w_coac + w_util + w_prov ≤ 1.0`. With new default: `0.85 ≤ 1.0`. |

---

## Functional Requirements

### FR-01: default_w_coac() returns 0.0

`fn default_w_coac() -> f64` in `config.rs` must return `0.0`.

**Verification**: Read function body.

### FR-02: InferenceConfig::default() uses w_coac: 0.0

The compiled-defaults struct literal inside `impl Default for InferenceConfig` must set `w_coac: 0.0`.

**Verification**: Read struct literal.

### FR-03: Doc comment on w_coac field updated

The field-level doc comment on `InferenceConfig.w_coac` (currently `Default: 0.10`) must read `Default: 0.0`.

**Verification**: Read comment at field definition.

### FR-04: Doc comments referencing 0.95 sum updated

Two doc comments in `InferenceConfig` encode the sum `0.95`:
- `w_prov` field doc: `Defaults sum to 0.95` → `Defaults sum to 0.85`
- `w_phase_explicit` field doc: `Total weight sum with defaults: 0.95 + 0.02 + 0.05 = 1.02` → `Total weight sum with defaults: 0.85 + 0.02 + 0.05 = 0.92`

**Verification**: Read both field doc comments.

### FR-05: FusionWeights comment in search.rs updated

Line 118 of `src/services/search.rs` comment on `w_coac` field:
- From: `default 0.10 — co-access affinity (lagging signal)`
- To: `default 0.0 (zeroed in crt-032; PPR subsumes co-access signal)`

**Verification**: Read FusionWeights struct field comments.

### FR-06: make_weight_config() helper updated to w_coac: 0.0

The test helper in `config.rs` must set `w_coac: 0.0`. This propagates the correct default to all per-field validation tests that use the helper.

**Verification**: Read helper body.

### FR-07: Default-assertion test updated

`test_inference_config_weight_defaults_when_absent` must assert `inf.w_coac.abs() < 1e-9` (expecting `0.0`) with message `"w_coac default must be 0.0"`.

**Verification**: Read test body.

### FR-08: Inline sum comment in partial-TOML test updated

The comment inside `test_inference_config_partial_toml_gets_defaults_not_error` that lists the per-field sum must be updated from `0.10` to `0.0` for w_coac, and the total from `1.00` to `0.90`.

**Verification**: Read test body comment.

### FR-09: ADR-001 written and stored in Unimatrix

ADR-001 for crt-032 must exist at `product/features/crt-032/architecture/ADR-001-w_coac-zero-default.md` and must be stored in Unimatrix via `context_store`.

**Verification**: File exists; Unimatrix returns entry on search.

---

## Non-Functional Requirements

### NFR-01: All tests pass

`cargo test --workspace` passes with zero failures.

### NFR-02: No changes outside unimatrix-server

No files outside `crates/unimatrix-server/` are modified.

### NFR-03: w_coac field retained

`InferenceConfig.w_coac` field definition, serde attribute, and validate() range check remain in place.

### NFR-04: CO_ACCESS_STALENESS_SECONDS unchanged

The constant and all three of its call sites are unchanged.

### NFR-05: compute_search_boost and compute_briefing_boost retained

Both functions remain. No call sites are removed.

---

## Acceptance Criteria

| ID | Criterion | Verification Method |
|----|-----------|-------------------|
| AC-01 | `default_w_coac()` returns `0.0` | Read function body |
| AC-02 | `InferenceConfig::default()` struct literal has `w_coac: 0.0` | Read struct literal |
| AC-03 | `cargo test --workspace` passes | Run command, check output |
| AC-04 | No test asserts the default value of `w_coac` is `0.10` | Search for `w_coac.*0\.10` in default-assertion contexts |
| AC-05 | Fusion weight sum with all defaults = 0.85 ≤ 1.0 | Arithmetic: 0.25+0.35+0.15+0.0+0.05+0.05 |
| AC-06 | ADR-001 written and stored in Unimatrix | File exists; Unimatrix search returns entry |
| AC-07 | `CO_ACCESS_STALENESS_SECONDS` is unchanged | Read constant definition and verify 3 call sites present |
| AC-08 | `compute_search_boost` and `compute_briefing_boost` functions remain | Grep for function definitions |
| AC-09 | No changes to CO_ACCESS table schema or recording logic | No changes in store crate or schema migration files |
| AC-10 | `w_coac` field remains in `InferenceConfig` struct | Read struct definition |
| AC-11 | `w_coac` field doc comment says `Default: 0.0` | Read field doc comment |
| AC-12 | `w_prov` field doc comment says `Defaults sum to 0.85` | Read field doc comment |
| AC-13 | `w_phase_explicit` field doc comment references `0.85` sum | Read field doc comment |
| AC-14 | `FusionWeights.w_coac` comment in search.rs says `default 0.0` | Read FusionWeights field comment |
| AC-15 | `make_weight_config()` helper sets `w_coac: 0.0` | Read helper body |
| AC-16 | Partial-TOML test inline comment updated to reflect `w_coac = 0.0` | Read test comment |

---

## Constraints

### C-01: Single-crate change

All code changes are in `crates/unimatrix-server/src/infra/config.rs` and `crates/unimatrix-server/src/services/search.rs` only.

### C-02: No schema migration

No database migration files are created or modified.

### C-03: Backward-compatible default only

`w_coac` field remains valid in the range [0.0, 1.0]. Operators with explicit non-zero values in their config files are unaffected.

### C-04: Do not change search.rs FusionWeights test fixtures

All `FusionWeights { w_coac: 0.10, ... }` struct literals in `search.rs` tests are intentional inputs for scoring math tests and must NOT change.

### C-05: Do not re-run eval harness

Phase 1 measurement is the gate evidence. Delivery gate = `cargo test --workspace` pass only.

---

## Test Classification Reference

This classification prevents delivery agents from incorrectly changing intentional fixtures.

### Must Change (default-assertion tests in config.rs)

| Test | File | Why |
|------|------|-----|
| `test_inference_config_weight_defaults_when_absent` | config.rs | Asserts compiled default is 0.10 — must assert 0.0 |
| `make_weight_config()` helper | config.rs | Sets w_coac: 0.10 as "the default" — must set 0.0 |
| Inline comment in `test_inference_config_partial_toml_gets_defaults_not_error` | config.rs | Comment references w_coac as 0.10 in sum calculation |

### Must NOT Change (intentional fixtures)

| Test | File | Why |
|------|------|-----|
| `test_inference_config_validate_accepts_sum_exactly_one` | config.rs line 4828 | Sets w_coac: 0.10 to construct a sum-exactly-1.0 scenario |
| All `FusionWeights { w_coac: 0.10, ... }` literals | search.rs (many lines) | Intentional scoring-math inputs; do not test the default |

### Pass Naturally (no change needed)

| Test | Why |
|------|-----|
| `test_inference_config_default_weights_sum_within_headroom` | Uses `<= 0.95 + 1e-9`; 0.85 ≤ 0.95 passes |
| Sum assertion in `test_inference_config_weight_defaults_when_absent` | Same upper-bound assertion; passes at 0.85 |
| Per-field rejection tests using `make_weight_config()` | None compute a sum; they mutate one field and check error messages |
