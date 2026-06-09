# Test Plan — Engine penalty entry point (`graph_penalty_with`, `GraphPenaltyParams`)

**Component**: `crates/unimatrix-engine/src/graph.rs` — new `GraphPenaltyParams` (Copy; `Default` = consts) + `graph_penalty_with`; `graph_penalty` becomes a thin wrapper.
**Wave**: 1. **Primary risks**: R-01 (missed site → default shift, **Critical**), R-13 (multiplier/shape-param), clamp-coupling correctness.

## Unit test expectations

### R-01 — default-equivalence (the cheapest early signal, SR-08) — AC-01(a), NFR-01

For **every status-shape branch** of `graph_penalty` AND the clamp:
- `test_graph_penalty_with_default_equals_graph_penalty_{shape}`: assert
  `graph_penalty_with(node, &graph, &entries, &GraphPenaltyParams::default()) == graph_penalty(node, &graph, &entries)`
  for each shape: orphan (dangling deprecated), clean-replacement chain (depth 1 and depth ≥2), partial-supersession, dead-end chain, fallback, superseded-Active.
- `test_graph_penalty_with_default_equals_named_const_{shape}`: where the result is a single penalty branch, assert it equals the named const (`ORPHAN_PENALTY=0.75`, etc.). **Assert the const value literally** (#3548), not "some default".
- Run **per-shape across the fixtures** — the five status topologies each drive a distinct branch.

### Clamp coupling (delivery-critical, ADR-001) — AC-01(a)

The hop-decay branch (`graph.rs:530-531`): `raw = clean_replacement * hop_decay^(d-1)`, clamped to `[0.10, clean_replacement]` where the **upper bound is `params.clean_replacement`**, NOT the const.
- `test_graph_penalty_with_clamp_ceiling_tracks_swept_clean_replacement`: with `clean_replacement` swept to a non-default (e.g. `0.25`), assert a depth-2 replacement is clamped to **≤ 0.25** (the swept ceiling), not the `0.40` const.
- `test_graph_penalty_with_depth2_le_depth1_monotonicity`: for any `clean_replacement` value, a depth-2 replacement penalty is **≤** the depth-1 penalty (the monotonicity the clamp coupling preserves).
- `test_graph_penalty_with_clamp_lower_bound_literal`: the lower bound `0.10` stays a literal — a tiny swept `clean_replacement` does not drop the floor below `0.10`.

### R-13 — multiplier semantics (severity-only scaling) — AC-01

(Multiplier *resolution* — TOML → `GraphPenaltyParams` — is built in `with_rate_config` and tested in `search-threading.md`. Here, test the engine consumes the resolved params correctly.)
- `test_graph_penalty_with_scaled_severities_change_output`: with severity params scaled (as the multiplier would), the orphan/clean/partial/dead_end/fallback branches change; `hop_decay` and `max_traversal_depth` behavior is unchanged.
- `test_graph_penalty_with_max_depth_truncates_not_panics`: `max_traversal_depth` set below the deepest fixture chain → defined truncation, **not a panic** (edge case).

### Wrapper integrity
- `test_graph_penalty_is_thin_wrapper`: `graph_penalty(..)` is observably identical to `graph_penalty_with(.., &GraphPenaltyParams::default())` across all existing `graph_tests.rs` ordering-invariant cases — existing tests pass unchanged (NFR-01).
- `test_graph_penalty_params_default_references_consts`: `GraphPenaltyParams::default()` fields equal the named consts (triangulation co-anchor with `penalty-config.md`).

## Edge cases (RISK-TEST-STRATEGY §Edge Cases)
- Multiplier extreme (m near 0, m large) drives a penalty outside `[0.10, clamp-upper]` → assert clamp **still applies after scaling**.
- `max_traversal_depth` below deepest chain → defined truncation.
- Chain whose head is itself deprecated (dead-end) → `find_terminal_active` returns no head → defined penalty (DEAD_END), not panic (shared with `trust-metric.md` redirect-to-head edge).

## Boundary note
This component is the **source of truth for default values**. Its `Default` impl referencing the consts is the anchor the config triangulation (R-02) and search-threading enumerated-site guard (R-01) both bind to.
