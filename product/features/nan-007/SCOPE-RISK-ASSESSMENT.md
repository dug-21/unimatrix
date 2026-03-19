# Scope Risk Assessment: nan-007

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `transport-async-rw` blanket impl is a transitive dependency not listed in Cargo.toml. If rmcp 0.17 removes or gates this impl, `UnimatrixUdsClient` tests break silently at compile time. | High | Med | Architect should pin `rmcp` to an exact version and add a compile-time assert or integration test that exercises the UDS transport path directly, making breakage loud. |
| SR-02 | `VACUUM INTO` on large production databases (100k+ entries with graph_edges, co_access, shadow_evaluations) may take seconds and lock the DB for the duration. If snapshot is taken against a live daemon DB file, WAL checkpointing may conflict. | Med | Med | Architect should confirm whether snapshot must be taken with the daemon stopped or whether WAL-mode isolation guarantees a safe online copy. Document the supported operating mode. |
| SR-03 | The eval runner constructs one `ServiceLayer` per profile config, loading a vector index per profile per run. For large snapshots and multiple candidate profiles, memory footprint may be prohibitive (HNSW index is in-memory). | Med | Med | Architect should design the runner to load the vector index once and inject it into each profile's `EvalServiceLayer`, or document a scenario-count/profile-count limit. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | The offline path (D1–D4) and the live simulation layer (D5–D6) have different runtime requirements. D1–D4 require no running daemon; D5–D6 require a live daemon with known socket paths. Conflating them in a single delivery risks the D5/D6 daemon fixture becoming a blocker for D1–D4 acceptance. | High | Med | Spec writer should define acceptance gating such that D1–D4 can be validated independently with no daemon. D5/D6 have separate acceptance criteria gated on the `daemon_server` pytest fixture (entry #1928). |
| SR-05 | `UnimatrixHookClient` hook socket path is an open question in the scope (§ Open Questions #3). If `ProjectPaths` does not currently expose the hook socket path, the Python client cannot discover it without either hardcoding a convention or requiring the caller to supply it. | Med | High | Spec writer must pin the hook socket path convention before implementation. Architect to confirm `ProjectPaths` exposes this or add it. |
| SR-06 | Scope excludes automated CI gates but includes the eval report as a human-reviewed artifact. The boundary between "harness infrastructure" (in scope) and "CI gate logic" (out of scope) may blur if downstream features (W1-4, W2-4) push to automate the zero-regression check. | Low | Low | Non-goal is clearly stated. Spec writer should include a constraint against adding CI gate logic to the report subcommand. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | `ServiceLayer` wires up the analytics write queue at construction time. If `eval run`'s `EvalServiceLayer` does not suppress analytics writes while operating against a read-only snapshot, `enqueue_analytics` will fail silently (fire-and-forget) but still attempt to write — violating AC-05. | High | High | Architect must design `EvalServiceLayer::from_profile()` to disable or no-op the analytics queue at construction. The read-only SQLite enforcement alone does not stop the in-memory queue from being populated. |
| SR-08 | `ConfidenceWeights` sum invariant (must equal 0.92 ± 1e-9) means hand-authored candidate profiles that omit even one weight field will fail config loading with no helpful error. The constraint is invisible to users authoring profile TOMLs. | Med | High | Spec writer should require a validation error message that names the invariant and lists the current sum when config loading fails. Architect should confirm the error path is user-readable, not a raw serde parse failure. |
| SR-09 | `eval run` replays scenarios in-process using the same code path as production search. If any W1-4 or W2-4 inference code is partially loaded in a profile (e.g., `nli_model` path present but model not yet downloaded), `EvalServiceLayer` construction may panic or return an opaque error at eval time rather than at profile parse time. | Med | Med | Architect should ensure `EvalServiceLayer::from_profile()` validates model paths at construction and returns a structured error, never panics. |

## Assumptions

| SCOPE.md Section | Assumption | Risk if Wrong |
|-----------------|------------|---------------|
| § Key Technical Findings — MCP UDS framing | MCP UDS uses newline-delimited JSON (verified against rmcp 0.16.0). | If rmcp changes framing in a patch, `UnimatrixUdsClient` breaks silently — no Rust-side test will catch it. |
| § Proposed Approach — D3 | `kendall_tau()` in `unimatrix-engine::test_scenarios` is reused directly by the eval runner in production binary code. This function lives under `test_scenarios.rs` and may be gated behind `#[cfg(test)]` or a `test-support` feature flag. | Eval runner cannot call it from production code without a feature flag or module restructure. Architect must verify accessibility before committing to this approach. |
| § Proposed Approach — D1 | Snapshot can be taken while the daemon is stopped (sync, pre-tokio, rusqlite path). | If users attempt snapshot against a live daemon's WAL-mode DB, atomicity guarantees differ from a stopped-daemon copy. Operating mode must be documented. |

## Design Recommendations

- **SR-07 (Critical)**: Analytics queue suppression in `EvalServiceLayer` must be an explicit design decision, not an afterthought. Design the `ServiceLayer` construction API to accept a `AnalyticsMode::ReadOnly` variant at construction.
- **SR-04 (High)**: Split acceptance criteria into two independent groups: D1–D4 (offline, no daemon) and D5–D6 (live, daemon required). This preserves the offline eval gate for W1-4/W2-4 even if D5/D6 hit daemon fixture issues.
- **SR-01 (High)**: Pin `rmcp` exact version and add a smoke test that exercises the UDS `serve()` path. Transitive feature flag reliance is a fragile dependency.
- **SR-05 (Med)**: Hook socket path convention must be resolved before architecture. If `ProjectPaths` needs a new field, that is an architectural surface change.
- **SR-03 (Med)**: Vector index memory usage per profile should be estimated against the expected snapshot size. If index-per-profile is prohibitive, the architecture must share the index.

## Knowledge Stewardship

- Queried: /uni-knowledge-search for "lesson-learned failures gate rejection" — no directly applicable failures; all outcomes passed validation.
- Queried: /uni-knowledge-search for "risk pattern" (category: pattern) — no nan/eval-domain patterns found. Entry #1928 (daemon fixture pattern) and #919 (Rust-only vs. infra-001 scope boundary decision) were relevant.
- Stored: nothing novel to store — risks are feature-specific to nan-007; no cross-feature pattern visible yet.
