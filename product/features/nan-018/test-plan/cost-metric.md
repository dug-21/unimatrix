# Test Plan — Cost metric (`eval/runner/cost.rs`, `token_proxy`)

**Component**: `eval/runner/cost.rs` — `token_proxy(result)` + per-profile `cost = Σ token_proxy`; `ProfileResult.cost_tokens: f64`.
**Wave**: 1. **Primary risks**: R-07 (token-proxy infidelity, Med), R-08 (non-determinism, Low).

## Unit test expectations

### R-07 — token-weighting, not k-weighting (the binding property) — AC-09
- `test_cost_same_k_different_token_load_differs`: two result sets with the **same k** (e.g. both 5 results) but different per-result token loads (50-token snippets vs 500-token snippets) yield **different** `cost`. **This is the load-bearing assertion** — proves cost is token-weighted, not k-weighted. Assert `cost_short != cost_long` and `cost_long > cost_short`.
- `test_token_proxy_monotonic_on_length`: a strictly longer text yields a strictly larger `token_proxy` (order-preserving on length).
- `test_cost_is_sum_of_token_proxy`: `cost == Σ token_proxy(r)` over the returned set on a known set with hand-computable proxy values (assert the exact sum).

### R-08 — determinism — AC-09
- `test_token_proxy_deterministic`: `token_proxy(result)` called repeatedly on the same entry returns the **identical** f64.
- `test_cost_deterministic_across_runs`: summed cost over a fixed result set is identical across repeated computation (no map-order / float drift).

## Concrete behaviors
- `token_proxy` is the documented faithful-subword tier (tokenizers crate) per ADR-003, with word×1.3 documented fallback. Assert the chosen tier is what runs (not silently the fallback).
- `k` is surfaced as a **secondary** axis (via `entries.len()`), cost is primary — assert both appear in the report (cross-ref `report-extensions.md`).

## Edge cases
- Empty result set ⇒ `cost == 0.0` (assert exactly).
- Unicode / multi-byte text: `token_proxy` handles non-ASCII without panic and counts consistently (cross-ref edge_cases discipline).

## Out-of-band (NOT a test — NFR-08, R-07 item 3)
The proxy formula and its **stated error bars** are documented in ADR-003 (Wave-1) and the Band-2 config-knob reference (Wave-2), labeled explicitly as a **proxy** (not a real tokenizer). Doc-review checklist item — flagged in OVERVIEW §5. Absolute tokenizer fidelity is **out of scope** (RISK-TEST-STRATEGY R-07): the tested controls are token-weighting, monotonicity, determinism, and honest labeling.
