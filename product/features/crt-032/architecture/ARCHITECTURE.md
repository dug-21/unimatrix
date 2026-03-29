# Architecture: crt-032 — w_coac Reduction to 0.0 (PPR Transition Phase 2)

## Overview

This feature changes a single compiled default value — `w_coac` from `0.10` to `0.0` — and updates all documentation and test assertions that encode the old default. No structural changes to the scoring pipeline, no schema migrations, no new abstractions.

All changes are confined to `crates/unimatrix-server/`. The change is backward-compatible: operators who have set `w_coac` explicitly in their config file are unaffected.

---

## Affected Components

### 1. `src/infra/config.rs` — Two production definition sites

The `w_coac` default lives in two distinct locations that must both change:

| Site | Location | Change |
|------|----------|--------|
| `default_w_coac()` serde function | Line 621–623 | `0.10` → `0.0` |
| `InferenceConfig::default()` compiled struct | Line 549 | `w_coac: 0.10` → `w_coac: 0.0` |

These two sites are independent: `default_w_coac()` governs deserialization when the field is absent from TOML; the struct literal governs the `Default` impl used when constructing `InferenceConfig` programmatically. Both must change or the default is inconsistent (SR-01).

### 2. `src/infra/config.rs` — Inline doc comment updates

Three inline doc references encode the old sum figure and must be updated:

| Location | Old text | New text |
|----------|----------|---------|
| Line 358 (field doc comment) | `Default: 0.10` | `Default: 0.0` |
| Line 367 (w_prov doc comment) | `Defaults sum to 0.95` | `Defaults sum to 0.85` |
| Line 381 (w_phase_explicit doc comment) | `Total weight sum with defaults: 0.95 + 0.02 + 0.05 = 1.02` | `Total weight sum with defaults: 0.85 + 0.02 + 0.05 = 0.92` |

### 3. `src/services/search.rs` — FusionWeights struct comment

| Location | Old text | New text |
|----------|----------|---------|
| Line 118 (FusionWeights field comment) | `default 0.10 — co-access affinity (lagging signal)` | `default 0.0 (zeroed in crt-032; PPR subsumes co-access signal)` |

### 4. `src/infra/config.rs` — Test changes

Only tests that assert the compiled default must change. Tests that use `w_coac: 0.10` as an intentional fixture input do NOT change.

**Tests that must change:**

| Test / Site | Line | Change Required |
|-------------|------|----------------|
| `make_weight_config()` helper | 4729 | `w_coac: 0.10` → `w_coac: 0.0` |
| `test_inference_config_weight_defaults_when_absent` | 4754–4756 | `(inf.w_coac - 0.10).abs()` → `inf.w_coac.abs()` (i.e., expect `0.0`); update message string |
| Same test sum comment | 4768 | Bound `<= 0.95` passes with 0.85 — no change needed to the assertion itself |

**Tests that use `make_weight_config()` and compute sums:**

The `make_weight_config()` helper is used in per-field negative/positive rejection tests (lines 4894, 4904, 4914, 4923, 4933, 4943). None of these tests compute a sum — they mutate one field and check validation error messages. They continue to work correctly with `w_coac: 0.0` in the helper.

**Sum assertion in helper-dependent tests:**

No helper-dependent test computes a sum expecting `0.95`. The two sum assertions (lines 4768, 4779) use the helper or `InferenceConfig::default()` directly — both use `<= 0.95 + 1e-9` (upper bound), which passes naturally at 0.85.

**Intentional fixture test — line 4828:**

`test_inference_config_validate_accepts_sum_exactly_one` manually sets all six weights to sum to 1.0, including `w_coac: 0.10`. This test exercises the sum-exactly-1.0 acceptance path — it is an intentional non-default fixture and must NOT change.

**Inline comment in `test_inference_config_partial_toml_gets_defaults_not_error` — line 4883:**

Comment says `// Total sum: 0.40 + 0.25 + 0.15 + 0.10 + 0.05 + 0.05 = 1.00` where `0.10` is the w_coac default. With the new default `0.0`, the partial TOML test sum becomes `0.40 + 0.25 + 0.15 + 0.00 + 0.05 + 0.05 = 0.90`. The assertion uses `<= 1.0 + 1e-9` (upper bound) and passes naturally. However, the comment is now incorrect and must be updated to reflect the new sum.

---

## Non-Changes (Explicitly Out of Scope)

| Component | Reason |
|-----------|--------|
| `compute_search_boost` call in search.rs | Phase 3 removal — not in scope |
| `compute_briefing_boost` in coaccess.rs | Already dead; Phase 3 removes it |
| `CO_ACCESS_STALENESS_SECONDS` | Governs data lifecycle independently; must NOT change |
| `w_coac` field on `InferenceConfig` | Field remains valid; only default changes |
| CO_ACCESS table schema | No schema migration |
| `FusionWeights` struct in search.rs | Struct unchanged; only the comment on line 118 updates |
| All `FusionWeights { w_coac: 0.10, ... }` test fixtures in search.rs | Intentional test inputs for scoring math — do NOT change |

---

## Data Flow (Unchanged)

The scoring pipeline is unaffected in structure. With `w_coac=0.0`:

```
compute_search_boost(...)  →  boost_map populated
FusedScoreInputs { coac_norm, ... }  →  built as before
compute_fused_score(inputs, weights)  →  weights.w_coac * inputs.coac_norm = 0.0 * x = 0.0
```

The boost call still runs (Phase 3 removes it); the product is zero. No scoring path changes.

---

## Fusion Weight Sum Invariant

With new defaults: `0.25 + 0.35 + 0.15 + 0.00 + 0.05 + 0.05 = 0.85`

The `validate()` method at config.rs line 920–933 enforces `sum ≤ 1.0`. Constraint satisfied with 0.15 headroom. The existing `<= 0.95` test assertion in `test_inference_config_weight_defaults_when_absent` also passes (0.85 ≤ 0.95). No changes to `validate()` are needed.

---

## ADR

See `architecture/ADR-001-w_coac-zero-default.md`.
