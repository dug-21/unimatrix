# Scope Risk Assessment: crt-038

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `effective(false)` re-normalization shifts conf-boost-c weights to w_sim'≈0.588, w_conf'≈0.412 — diverging from the ASS-039 evaluated formula that produced MRR=0.2913 (entry #4003) | High | High | AC-02 short-circuit is the correct fix: `effective()` must return weights unchanged when `w_nli==0.0`, regardless of `nli_available`. Delivery must confirm eval was run on the same scoring path as production. |
| SR-02 | MRR gate (AC-12) may be evaluated on the wrong scoring path if the short-circuit (AC-02) is implemented after the eval run rather than before | High | Med | Eval run must be performed after AC-02 is implemented. Enforce this ordering in the delivery sequence. |
| SR-03 | 13 test functions in `nli_detection.rs` and 4 in `background.rs` cover the removed code paths; accidental retention of any test referencing removed symbols will cause compile failure rather than a clean test pass | Med | Med | Delivery should grep for removed symbol names before claiming AC-09 complete. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | `NliStoreConfig` fields (`nli_post_store_k`, `nli_entailment_threshold`, etc.) are listed as retained in Background Research but AC-14 requires the struct be deleted entirely — these are contradictory | Med | Med | Delivery must treat AC-14 as authoritative: delete `NliStoreConfig` and all its fields. InferenceConfig retains the same-named fields separately. |
| SR-05 | Scope states w_util=0.00, w_prov=0.00 in new defaults; zeroing these eliminates signals that may be non-zero in existing entries — any query-time ranking relying on utilization or provenance signals silently loses that contribution | Low | Low | Document the signal zeroing explicitly in the PR; confirm no operator configs override these fields in production. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | `run_graph_inference_tick` in `nli_detection_tick.rs` shares helpers with the functions being removed from `nli_detection.rs`. Post-removal, compiler will surface missing imports only if they are file-local. Cross-module shared helpers (e.g., `write_edges_with_cap`, `format_nli_metadata`) must remain in `nli_detection.rs` or be moved to `nli_detection_tick.rs` (AC-13) | High | Med | Architect must specify which shared helpers survive in `nli_detection.rs` and which live in `nli_detection_tick.rs`. Do not defer this to delivery discovery. |
| SR-07 | `background.rs` at 4,229 lines is pre-existing over-limit. Removing ~60 lines does not resolve the violation. The 500-line constraint in SCOPE.md cannot be satisfied here; confirming this is pre-existing protects delivery from a gate failure | Low | High | Add an explicit note in the spec that the 500-line limit applies to new files only; pre-existing violations are tracked separately. |

## Assumptions

- **SCOPE.md §Background/Codebase State**: Assumes the ASS-039 eval was run with `nli_enabled=true` and `w_nli=0.0` (effective(true) path, no re-normalization). If the eval was run via a different code path, the MRR=0.2913 baseline does not transfer. This assumption is critical — if wrong, AC-12 has no valid baseline to compare against.
- **SCOPE.md §Non-Goals**: Assumes `run_graph_inference_tick` has no compile-time dependency on any of the three removed functions. Not verified in the scope; architect must confirm.
- **SCOPE.md §Constraints**: Assumes `NliStoreConfig` fields are exclusively consumed by the NLI spawn block. If any field is read elsewhere (e.g., operator tooling, tests), deletion will break callers not listed in the scope.

## Design Recommendations

- **SR-01 / SR-02**: Implement AC-02 (`effective()` short-circuit for `w_nli==0.0`) as step 1, before any eval run. The spec should enforce this ordering explicitly.
- **SR-06**: The spec must enumerate which symbols in `nli_detection.rs` are shared with `nli_detection_tick.rs` and must be retained. This is the highest compilation-breakage risk in the removal.
- **SR-03 / SR-04**: The spec should include a symbol checklist (removed functions, removed struct, removed test names) that delivery must grep-verify against the compiled codebase before marking AC-09 and AC-14 complete.
