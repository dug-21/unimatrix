# Scope Risk Assessment: dsn-001 (Revised — Preset System Expansion)

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `toml = "0.8"` adds first TOML dependency. Transitive deps may conflict. | Med | Low | Resolved by architecture (pinned `toml = "0.8"`, run `cargo tree`). No action. |
| SR-02 | `ConfidenceParams` struct in `unimatrix-engine` currently carries only `freshness_half_life_hours`, `alpha0`, `beta0`. It does NOT carry the six weight constants (W_BASE, W_USAGE, W_FRESH, W_HELP, W_CORR, W_TRUST). The preset system sets all six weights — if those values never enter `ConfidenceParams`, the preset is loaded but silently never applied to the confidence formula. | High | High | Architect must add the six weight fields to `ConfidenceParams` and wire them through `compute_confidence`. The current architecture design is incomplete for preset support. |
| SR-09 | Preset weight values are described as "architect deliverable requiring domain science validation" — exact numbers are not in SCOPE.md. Shipping wrong preset values (e.g., `empirical` with low W_FRESH) silently miscalibrates all confidence scoring for that domain. There is no post-deployment signal that a preset is wrong until operator trust erodes. | High | Med | Spec writer must define exact numeric values with a rationale note for each preset before delivery begins. Values must sum to ≤ 0.92 (the stored-factor invariant). An incorrect sum causes silent score compression or expansion on every entry. |
| SR-10 | `collaborative` preset must reproduce current compiled defaults exactly. The current constants are: W_BASE=0.16, W_USAGE=0.16, W_FRESH=0.18, W_HELP=0.12, W_CORR=0.14, W_TRUST=0.16, half_life=168.0h. If the `collaborative` preset table ships with even one digit wrong, AC-01 ("all existing tests pass") will silently pass (tests do not assert weight values) while production confidence scores diverge from pre-dsn-001 behavior. | High | Med | Spec writer must include a table row for `collaborative` with exact numeric values matching the live constants. The delivery team must add a unit test that asserts `ConfidenceParams::from_preset("collaborative") == ConfidenceParams::default()`. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-05 | `context_retrospective` → `context_cycle_review` rename blast radius spans Rust source, protocol files, skill files, research docs, CLAUDE.md. Partial rename compiles but breaks runtime callers. | High | High | Unchanged from v1. Spec must include exhaustive file checklist; build passing is insufficient gate. |
| SR-11 | `[knowledge] freshness_half_life_hours` overrides the preset's built-in value "when set." AC-25 states: when `preset = "custom"` and `[knowledge] freshness_half_life_hours` is also set, `[knowledge]` wins; when only `[confidence]` is used, `freshness_half_life_hours` must be specified there. The priority chain (preset built-in → `[knowledge]` override → `[confidence]` field for `custom`) creates three sources of truth for one value. If the resolution order is not implemented and tested explicitly, an operator who sets only `[knowledge] freshness_half_life_hours` with a named preset may unknowingly have it silently ignored. | Med | Med | Spec writer must enumerate all combinations (named preset + no [knowledge] override; named preset + [knowledge] override; custom + [knowledge] override; custom without [knowledge]) with expected behavior for each. Architect must make the priority chain explicit in code with a single resolution site. |
| SR-12 | The forward-compat stub for `[confidence]` (ADR-004) was designed before the preset expansion. Now `[confidence] weights` is an active, live section for `custom` preset — not a stub. The stub design assumed the section would be ignored until W3-1. If the `ConfidenceConfig` struct was scaffolded as an empty stub (as ADR-004 describes), it must now be promoted to a real struct with validation before delivery. | Med | Low | Resolved by scope: `[confidence] weights` is now a real field per AC-24. Delivery team must not treat this as a stub to be skipped — it requires full validation and test coverage. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-08 | `agent_resolve_or_enroll` signature change in `unimatrix-store` (adds `session_caps` parameter). Architecture resolves crate boundary via plain parameter passing. | Med | Low | Resolved by ADR-002. Pass `None` at all existing call sites. |
| SR-13 | W3-1 GNN cold-start is described as reading the active preset's weight vector. If W3-1 begins implementation before dsn-001 ships and `ConfidenceParams` does not yet carry the six weights (SR-02), W3-1 will either define its own weight-access mechanism (format conflict) or be blocked. | Med | Med | dsn-001 must ship a `ConfidenceParams` that includes all six weight fields before W3-1 implementation starts. SR-02 resolution is a precondition for W3-1 unblocking. |

## Assumptions

- **SCOPE.md §Preset Weight Table**: Assumes the ordering relationships shown (e.g., `empirical` has "very high" W_FRESH) are validated domain science. If the illustrative values are used as-is without review, the `empirical` preset ships with W_FRESH="very high" — a reasonable ordering claim, but not a validated number.
- **SCOPE.md §AC-22**: Assumes `collaborative` exact weights equal current compiled constants. This must be verified numerically, not assumed.
- **SCOPE.md §Goals #7**: Assumes "preserve all compiled defaults unchanged when no config file is present" can be tested automatically. Without a weight-equality assertion in the test suite, this assumption is unverifiable and AC-01 provides no protection against a weight regression.

## Design Recommendations

- **SR-02 (Critical)**: `ConfidenceParams` must be extended to include the six weight fields before delivery begins. The architecture's current struct definition covers only `freshness_half_life_hours`, `alpha0`, `beta0`. A preset that sets weights but never injects them into `compute_confidence` is dead configuration.
- **SR-09 (High)**: Exact preset numeric values must be committed to the spec before delivery — not left as "architect deliverable." The delivery team cannot implement a correctness test against a value described as "illustrative."
- **SR-10 (High)**: Add one mandatory unit test: `ConfidenceParams::from_preset(Preset::Collaborative) == ConfidenceParams::default()`. This is the only mechanical guard against a silent backward-compat regression.
