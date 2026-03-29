# Implementation Brief: crt-032 — w_coac Reduction to 0.0 (PPR Transition Phase 2)

**GH Issue**: #415
**Branch**: feature/crt-032
**Crate**: `crates/unimatrix-server/` only
**Delivery gate**: `cargo test --workspace` pass — no eval re-run required

---

## What This Delivers

Changes the compiled default of `w_coac` (co-access affinity fusion weight) from `0.10` to `0.0`. Phase 1 measurement proved the direct co-access boost contributes nothing that PPR doesn't already carry through graph edges. This is a default-value change with cascading doc comment and test assertion updates. No structural code changes, no schema migrations.

---

## Precise Change Inventory

All changes are in `crates/unimatrix-server/src/`. No other crates.

### `src/infra/config.rs` — Production Code

| Site | Line | Change |
|------|------|--------|
| `default_w_coac()` return value | 622 | `0.10` → `0.0` |
| `InferenceConfig::default()` struct literal | 549 | `w_coac: 0.10` → `w_coac: 0.0` |
| `w_coac` field doc comment | 358 | `Default: 0.10` → `Default: 0.0` |
| `w_prov` field doc comment | 367 | `Defaults sum to 0.95` → `Defaults sum to 0.85` |
| `w_phase_explicit` field doc comment | 381 | `Total weight sum with defaults: 0.95 + 0.02 + 0.05 = 1.02` → `Total weight sum with defaults: 0.85 + 0.02 + 0.05 = 0.92` |

### `src/infra/config.rs` — Test Code

| Site | Line | Change |
|------|------|--------|
| `make_weight_config()` helper | 4729 | `w_coac: 0.10` → `w_coac: 0.0` |
| `test_inference_config_weight_defaults_when_absent` assertion | 4754–4756 | `(inf.w_coac - 0.10).abs() < 1e-9` → `inf.w_coac.abs() < 1e-9`; message `"w_coac default must be 0.10"` → `"w_coac default must be 0.0"` |
| Inline comment in `test_inference_config_partial_toml_gets_defaults_not_error` | 4883 | `// Total sum: 0.40 + 0.25 + 0.15 + 0.10 + 0.05 + 0.05 = 1.00` → `// Total sum: 0.40 + 0.25 + 0.15 + 0.00 + 0.05 + 0.05 = 0.90` |

### `src/services/search.rs` — Comment Only

| Site | Line | Change |
|------|------|--------|
| `FusionWeights.w_coac` field comment | 118 | `default 0.10 — co-access affinity (lagging signal)` → `default 0.0 (zeroed in crt-032; PPR subsumes co-access signal)` |

---

## What Must NOT Change

- `CO_ACCESS_STALENESS_SECONDS` — governs data lifecycle; independent of w_coac
- `compute_search_boost` function definition and call site — Phase 3 removal
- `compute_briefing_boost` function definition — Phase 3 removal
- All `FusionWeights { w_coac: 0.10, ... }` struct literals in `search.rs` tests — these are intentional scoring-math inputs, not default assertions
- `test_inference_config_validate_accepts_sum_exactly_one` (line 4828) — sets `w_coac: 0.10` to construct a sum-exactly-1.0 scenario; intentional fixture
- CO_ACCESS table schema, GRAPH_EDGES, co-access recording logic
- `w_coac` field definition, serde attribute, and validate() range check on `InferenceConfig`

---

## ADR

ADR-001 crt-032 is written at `product/features/crt-032/architecture/ADR-001-w_coac-zero-default.md` and stored in Unimatrix as entry **#3785**.

---

## Delivery Wave

Single wave. All changes are in two files; no dependencies between changes. Recommended order:

1. `config.rs` production changes (two definition sites + three doc comments)
2. `config.rs` test changes (helper + one assertion + one comment)
3. `search.rs` comment change (one line)
4. `cargo test --workspace` — confirm all pass

---

## Acceptance Criteria (Delivery Gate)

| AC | Description |
|----|-------------|
| AC-01 | `default_w_coac()` returns `0.0` |
| AC-02 | `InferenceConfig::default()` struct literal has `w_coac: 0.0` |
| AC-03 | `cargo test --workspace` passes with zero failures |
| AC-04 | No test asserts the default value of `w_coac` is `0.10` |
| AC-05 | Fusion weight sum with all defaults = 0.85 ≤ 1.0 |
| AC-06 | ADR-001 written at `architecture/ADR-001-w_coac-zero-default.md` and stored in Unimatrix (#3785) |
| AC-07 | `CO_ACCESS_STALENESS_SECONDS` unchanged, still referenced at 3 call sites |
| AC-08 | `compute_search_boost` and `compute_briefing_boost` functions remain |
| AC-09 | No changes to CO_ACCESS table schema or co-access recording logic |
| AC-10 | `w_coac` field remains in `InferenceConfig` struct |
| AC-11 | `w_coac` field doc comment says `Default: 0.0` |
| AC-12 | `w_prov` field doc comment says `Defaults sum to 0.85` |
| AC-13 | `w_phase_explicit` field doc comment references `0.85` sum and `0.92` total |
| AC-14 | `FusionWeights.w_coac` comment in search.rs says `default 0.0` |
| AC-15 | `make_weight_config()` helper sets `w_coac: 0.0` |
| AC-16 | Partial-TOML test inline comment updated to reflect `w_coac = 0.0` and sum `0.90` |
