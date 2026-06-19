# Scope Risk Assessment: vnc-040

Per-slug config overlay resolution (Feature A of #785). Reuses `merge_configs` / `load_single_config` / `validate_config`. HARD invariants: NLI + embedding models stay global; entire `[embedding]` section locked global. Historical evidence cited as Unimatrix entry IDs.

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | **Post-merge cross-level invariant gap.** AC-08 validates each per-slug file independently; #3905 proved a two-level merge passes per-file yet violates cross-field invariants (fusion-weight sum > 1.0) because non-default fields combine across levels. C6 adds a THIRD precedence layer, widening this surface. | High | High | Architect/spec MUST add a re-validation of the MERGED per-slug config (`validate_config(&merged)`) after `merge_configs`, not only per-file. Enumerate every sum/cross-field constraint and ask "can global field A + per-slug field B violate it?" |
| SR-02 | **Hidden `merge_configs` literal site / parallel-path drift.** #4070: the inline `InferenceConfig {…}` literal in `merge_configs` is the one site grep-for-spread misses; #4648 warns extending a security control to a parallel path exposes flaws in the original. C6 routes a new caller through `merge_configs` for a third layer it was not written for. | High | Med | Re-audit `merge_configs` for the C6 call shape before reuse; do not assume the existing arm covers a global→per-slug pairing identically to global→project. |
| SR-03 | **Security carve-out is necessary-not-sufficient for `[embedding]`.** #4655/#4649: hash pins need global-wins. SCOPE confirms the sha256 pin alone does NOT stop a per-slug `[embedding].model`/`dimensions` from *describing* a model the served handle is not (config-vs-handle divergence), defused today only by `VectorConfig::default()` luck. | High | Med | Lock the WHOLE `[embedding]` section global-wins inside the per-slug merge (symmetric with transport), per SCOPE Constraint. Prove the merged config can never describe a non-served model (AC-04). |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | **Fallthrough regression for single-project majority.** #4583: a silent config fallback on categories shipped a bug. Any deviation from byte-for-byte fallthrough silently changes behavior for every local UDS / single-project user (the majority) when no per-slug file exists. | High | Med | Make AC-02 a hard regression sentinel: assert per-slug-resolved == global-resolved byte-for-byte across all 10 inputs when no file present. The `None`-arm must pass global `Arc`s unchanged, never re-derive from a merge. |
| SR-05 | **10-input verdict checklist can silently drop an input.** Issue says "8 fields"; live signature is 9 + pre-existing `embed_handle` + the `[embedding]` section. A dropped/mis-classified input is invisible (mirrors crt-056 AC-1 rationale). | Med | Med | Spec the verdict as a CLOSED checklist of all 10 inputs + `[embedding]` section; each row proven global-locked or overlayable (AC-07). No "all 9 params" shorthand. |
| SR-06 | **Feature A/B split leaves an incomplete first pass.** #2397: incremental scope splits produce incomplete first designs. A and B share one file path; A is hand-placed, B seeds. A design that ignores B's seeding contract risks rework. | Med | Med | Architect records the shared `{base_dir}/{slug}/config.toml` contract and the leave-`adapt`-default decision so B builds on A without re-litigation. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | **crt-056 seam coupling — override-vs-handle.** The 6 overlayable `Arc`s derive from the merged config; fields 0–2 (NLI/embed/pool) must NEVER source from it. A merge that accidentally rebuilds a handle breaks crt-056 AC-2 (one model of each kind). | High | Med | Construct fields 0–2 by `Arc::clone` of the global handles UNCONDITIONALLY, outside any merge branch. Behaviorally prove exactly one NLI + one embedding model in memory at N≥2 slugs (AC-04). |
| SR-08 | **Per-slug field-coupling not model-coupling.** `nli_top_k`/`nli_enabled` (fields 3–4) declared overlayable; if any is silently model-coupled, a per-slug override could misbehave against the shared handle. | Med | Low | Spec must confirm fields 3–4 are runtime inference params, not model identity (SCOPE OQ-2 resolved — keep the proof in the spec). |

## Assumptions

- **A1 (SCOPE Background / Merge semantics):** `merge_configs` global→project semantics transfer cleanly to global→per-slug. If the existing arm has a project-specific assumption, SR-02 materializes. Verify at architecture.
- **A2 (SCOPE §AC-04 / `[embedding]` lock):** the per-slug vector index stays `VectorConfig::default()` (not config-driven). If a later change makes dims config-driven, the `[embedding]` divergence (SR-03) re-opens. Spec must note this dependency.
- **A3 (SCOPE Background / crt-056 seam):** C5 (#5190) proved on #789 and the seam at `main.rs:1089-1110` is stable. If crt-056's 9-param signature shifts, the whole verdict checklist (SR-05) is invalidated. The dependency on the crt-056 seam is load-bearing — design against the merged signature, not the issue's "8 fields".

## Design Recommendations

1. **SR-01 (highest):** mandate post-merge re-validation of the per-slug merged config as an explicit AC, not just per-file (AC-08 today is insufficient — #3905).
2. **SR-03 + SR-07:** make both model invariants enforceable BY CONSTRUCTION — handles `Arc::clone`d outside the merge, `[embedding]` whole-section global-wins — so no test gap can let them regress.
3. **SR-04:** treat AC-02 fallthrough as the single most important regression sentinel; the silent-majority blast radius dwarfs the multi-tenant feature gain.
