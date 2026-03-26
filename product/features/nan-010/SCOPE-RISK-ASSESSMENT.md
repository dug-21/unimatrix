# Scope Risk Assessment: nan-010

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `profile-meta.json` sidecar is a new artifact contract: `eval run` must write it and `eval report` must find it; any partial run (crash after result files, before meta flush) leaves the report in silent backward-compat mode rather than failing clearly | Med | Med | Architect must flush `profile-meta.json` atomically (write to `.tmp` then rename), and document what "absent file" means vs. "corrupt file" |
| SR-02 | `aggregate.rs` is at 488 lines; adding `check_distribution_targets` may breach the 500-line workspace limit, forcing an unplanned module split mid-feature | Med | High | Pre-split `aggregate.rs` into `aggregate/mod.rs` + `aggregate/distribution.rs` before adding new code, or confirm line count allows inline addition |
| SR-03 | `render.rs` is at 499 lines; any incidental change (import, doc comment) triggers the limit; the new `render_distribution_gate.rs` module boundary must be established before implementation begins | High | High | Architect must define the exact module extraction boundary for `render_distribution_gate.rs` upfront — not as a follow-on |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | The scope states `mrr_floor` is an absolute floor, not a delta — but the TOML gives no guidance on how users should choose floor values. Without guardrails, users will set floors too low (useless) or too high (always-fail). The scope explicitly defers "no target auto-derivation" | Low | Med | Spec must include a documentation constraint: the harness should print the actual observed baseline MRR in the Distribution Gate table as a reference, even if it does not compute the floor automatically |
| SR-05 | Multi-profile case: when both a `distribution_change = true` profile and a `distribution_change = false` profile exist in the same run, Section 5 renders differently per profile. The scope says "per-profile independence" but does not clarify whether Section 5 appears once or once per profile — a rendering boundary ambiguity | Med | Med | Spec writer must define the Section 5 rendering structure explicitly for the multi-profile case |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | Dual-type constraint (pattern #3574, #3550): `runner/output.rs` and `report/mod.rs` maintain independent type copies. The scope chooses the sidecar file (option 2) to avoid triggering this constraint — but if any implementation detail causes `ScenarioResult` to be touched, both copies must be updated in sync. This constraint has caused rework in nan-007, nan-008, nan-009 | High | Med | Architect must make the sidecar boundary a hard constraint in the architecture: zero changes to `ScenarioResult` fields. If that proves impossible, the dual-type sync must be treated as a three-site update (runner + report + tests) |
| SR-07 | `parse_profile_toml` currently strips `[profile]` before deserializing `UnimatrixConfig` (SCOPE.md §Background, lines 102–109). Extracting `distribution_change` and `distribution_targets` must happen before stripping. Any refactor that changes the strip ordering would silently drop the new fields without a parse error | Med | Low | Spec must include a regression test: a TOML with `distribution_change = true` must be rejected at parse time if targets are missing, verifying extraction precedes stripping |

## Assumptions

- **SCOPE.md §Background, "CC@k and ICD are already fully computed"**: Assumes `mean_cc_at_k`, `mean_icd`, and `mean_mrr` are already present in `AggregateStats` from nan-008. If those fields were renamed or removed in a subsequent feature, the distribution gate aggregation has no data source.
- **SCOPE.md §Background, "Section 5 render path is a standalone block"**: Assumes the render.rs refactor from nan-009 did not alter the Section 5 block structure. If it was partially merged with adjacent sections, the conditional replacement is more invasive than scoped.
- **SCOPE.md §Design Decisions #4**: Assumes `eval report` always has access to the results directory path and can derive `profile-meta.json` location from it. If `eval report` is ever called with individual file arguments rather than a directory, the sidecar lookup fails silently.

## Design Recommendations

- **SR-03, SR-02**: Establish module boundaries (`render_distribution_gate.rs`, and if needed `aggregate/distribution.rs`) as the first architectural decision — before any type design. Line limits are a hard constraint, not a cleanup task.
- **SR-06**: The dual-type constraint (pattern #3574) is the highest-risk integration point in this feature. The sidecar approach is correct; architect should document it as an explicit ADR so future contributors don't inadvertently embed metadata in `ScenarioResult`.
- **SR-01**: Define the partial-run failure semantics for `profile-meta.json` absence in the spec (AC-11 covers backward compat, but not mid-run crash recovery). Atomic write via rename is the standard mitigation.
- **SR-05**: The multi-profile Section 5 rendering structure must be resolved in the specification before implementation — it affects the render loop structure in `render.rs`.
