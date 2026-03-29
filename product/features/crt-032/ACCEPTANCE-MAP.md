# Acceptance Map: crt-032 — w_coac Reduction to 0.0

## Verification Method Key

| Code | Method |
|------|--------|
| READ | Read the file at the specified location |
| GREP | Grep for pattern — absence or presence |
| RUN | Execute command and check output |
| ARITH | Arithmetic verification |

---

## Gate 3a — Build

| AC | Criterion | Method | Pass Condition |
|----|-----------|--------|---------------|
| AC-03 | cargo test --workspace passes | RUN `cargo test --workspace 2>&1 \| tail -30` | Zero test failures reported |

---

## Gate 3b — Implementation Correctness

### Production Changes

| AC | Criterion | Method | Pass Condition |
|----|-----------|--------|---------------|
| AC-01 | `default_w_coac()` returns `0.0` | READ config.rs lines 621–623 | Function body is `0.0` |
| AC-02 | `InferenceConfig::default()` uses `w_coac: 0.0` | READ config.rs around line 549 | Struct literal field is `w_coac: 0.0` |
| AC-05 | Fusion weight sum = 0.85 | ARITH 0.25+0.35+0.15+0.00+0.05+0.05 | Result = 0.85 ≤ 1.0 |
| AC-10 | `w_coac` field still in `InferenceConfig` | GREP `pub w_coac: f64` in config.rs | Field definition present |
| AC-11 | w_coac doc comment says `Default: 0.0` | GREP `Default: 0\.0` near `w_coac` field | Present |
| AC-12 | w_prov doc comment updated | GREP `Defaults sum to 0\.85` in config.rs | Present |
| AC-13 | w_phase_explicit doc comment updated | GREP `0\.85 \+ 0\.02 \+ 0\.05 = 0\.92` in config.rs | Present |
| AC-14 | FusionWeights comment updated | GREP `default 0\.0.*crt-032` in search.rs | Present on w_coac line |

### Test Changes

| AC | Criterion | Method | Pass Condition |
|----|-----------|--------|---------------|
| AC-04 | No test asserts `w_coac` default is `0.10` | GREP `w_coac.*0\.10.*default\|default.*w_coac.*0\.10` in config.rs test section | No matches in default-assertion context |
| AC-15 | `make_weight_config()` helper has `w_coac: 0.0` | READ config.rs lines 4724–4734 | `w_coac: 0.0` in helper body |
| AC-16 | Partial-TOML test comment updated | READ `test_inference_config_partial_toml_gets_defaults_not_error` comment | References `0.0` and `0.90` not `0.10` and `1.00` |

---

## Gate 3c — Non-Regression

| AC | Criterion | Method | Pass Condition |
|----|-----------|--------|---------------|
| AC-07 | `CO_ACCESS_STALENESS_SECONDS` unchanged | GREP `CO_ACCESS_STALENESS_SECONDS` | Present in definition + 3 call sites (search.rs ×1, status.rs ×2) |
| AC-08 | `compute_search_boost` function present | GREP `fn compute_search_boost` | Present |
| AC-08 | `compute_briefing_boost` function present | GREP `fn compute_briefing_boost` | Present |
| AC-08 | `compute_search_boost` call site present | GREP `compute_search_boost(` in search.rs | Present (call not removed) |
| AC-09 | No schema migration files added | GLOB `**/migrate*.sql` or migration files | No new files vs pre-delivery |
| AC-06 | ADR file exists | READ `architecture/ADR-001-w_coac-zero-default.md` | File readable, non-empty |
| — | search.rs fixture count unchanged | GREP `FusionWeights.*w_coac.*0\.10` in search.rs tests | Count matches pre-delivery baseline |

---

## Non-Negotiable Tests (Must Not Be Deleted or Xfail)

The following tests must exist and pass after delivery:

| Test Name | File | Why Non-Negotiable |
|-----------|------|--------------------|
| `test_inference_config_weight_defaults_when_absent` | config.rs | Asserts the compiled default via TOML deserialize path |
| `test_inference_config_default_weights_sum_within_headroom` | config.rs | Verifies sum invariant via Default::default() path |
| `test_inference_config_validate_accepts_sum_exactly_one` | config.rs | Verifies sum-exactly-1.0 is accepted (uses w_coac=0.10 as intentional fixture — must NOT change) |
| `test_inference_config_validate_rejects_w_coac_below_zero` | config.rs | Verifies w_coac field validation still active |
| `test_inference_config_partial_toml_gets_defaults_not_error` | config.rs | Verifies partial TOML uses new default (comment updated, assertion unchanged) |
