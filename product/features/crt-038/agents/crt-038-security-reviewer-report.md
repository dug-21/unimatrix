# Security Review: crt-038-security-reviewer

## Risk Level: low

## Summary

crt-038 applies the conf-boost-c scoring formula (weight default changes) and surgically removes three NLI dead-code paths (post-store detection, bootstrap promotion, NLI auto-quarantine guard). No new external input surfaces, no new dependencies, and no trust boundary changes are introduced. The change reduces attack surface by eliminating `run_post_store_nli`, which previously accepted embedding vectors and entry content strings and passed them to the NLI cross-encoder. Three cosmetic low-severity findings were identified; none are blocking.

## Findings

### Finding 1: Stale doc comment in config.rs
- **Severity**: Low
- **Location**: `crates/unimatrix-server/src/infra/config.rs:325`
- **Description**: The doc comment for `max_contradicts_per_tick` still reads "Per-call cap on total edges written during `run_post_store_nli`." The function was deleted in this PR. The field is now exclusively used by `run_graph_inference_tick`.
- **Recommendation**: Update the doc comment in a follow-up commit.
- **Blocking**: No

### Finding 2: Partial config sum assertion weakened in test
- **Severity**: Low / Informational
- **Location**: `crates/unimatrix-server/src/infra/config.rs:5205`
- **Description**: The partial-config deserialization test previously asserted `sum <= 1.0`. The new assertion is `sum > 0.0` because conf-boost-c defaults allow a partial config sum to exceed 1.0 when an operator supplies `w_nli=0.40` alongside the new defaults. The test comment correctly explains that `validate()` catches the over-limit case at runtime — the assertion weakening is intentional and well-documented. No security regression.
- **Recommendation**: Acceptable as-is; no change needed.
- **Blocking**: No

### Finding 3: Doc comment in nli_detection_tick.rs references deleted function
- **Severity**: Low
- **Location**: `crates/unimatrix-server/src/services/nli_detection_tick.rs:3` (not modified in this PR)
- **Description**: Module-level doc comment references `maybe_run_bootstrap_promotion` as a historical counterpart. The function no longer exists. Documentation only.
- **Recommendation**: Update in a follow-up commit alongside the ADR-004 module merge.
- **Blocking**: No

## Blast Radius Assessment

The highest-impact change is `FusionWeights::effective()` — every `context_search` and `context_briefing` call passes through it. If the short-circuit guard were misplaced (after rather than before the re-normalization branch), ranking would silently degrade to w_sim≈0.588, w_conf≈0.412 with no runtime error. Guard placement was verified directly in `search.rs:161` — the guard fires **before** the `nli_available` branch, which is correct.

Worst case for a subtle bug anywhere in this diff: suboptimal search ranking — no data corruption, no denial of service, no information disclosure. Failure mode is safe.

Dead-code removal reduces attack surface: `run_post_store_nli` accepted embedding vectors and content strings and routed them to an NLI cross-encoder. Its removal eliminates that inference path entirely.

## Regression Risk

- All removed symbols verified absent from compiled source (doc comments only): `run_post_store_nli`, `maybe_run_bootstrap_promotion`, `NliStoreConfig`, `NliQuarantineCheck`, `nli_auto_quarantine_allowed`, `write_edges_with_cap`.
- All retained symbols verified present at definition and call sites: `write_nli_edge`, `format_nli_metadata`, `current_timestamp_secs`.
- `process_auto_quarantine` signature and call site updated atomically — confirmed in diff and source.
- `cargo build --workspace` passes clean.
- Clippy errors in workspace (`unimatrix-engine`, `unimatrix-observe`) are pre-existing; zero errors originate in `unimatrix-server` (the changed crate).

## Non-Blocking Gate Note (integrity, not security)

The AC-12 eval gate (MRR ≥ 0.2913 on 1,585 scenarios, eval output with commit hash attached to PR) is **not yet satisfied** in the PR description at time of review. This is a correctness gate documented in the architecture, not a security finding. Merge must not proceed until eval output is attached.

## PR Comments
- Posted 1 review comment on PR #484 (via `gh pr review --comment`)
- Blocking findings: No

## Knowledge Stewardship

Nothing novel to store — the "dead-code removal reduces attack surface" pattern for NLI inference paths is feature-specific and does not generalize beyond this PR. The "eval gate depends on correct scoring path being implemented first" anti-pattern is already captured in Unimatrix entry #4003.
