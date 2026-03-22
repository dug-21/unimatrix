# crt-026 Agent 4 — Search Component Report

**Agent ID**: crt-026-agent-4-search
**Component**: ServiceSearchParams + FusedScoreInputs / FusionWeights / compute_fused_score
**File**: `crates/unimatrix-server/src/services/search.rs`
**GH Issue**: #341

---

## Work Completed

### Component A — ServiceSearchParams

Added two new fields after `retrieval_mode`:

- `session_id: Option<String>` — for logging/tracing (WA-2, not used in scoring)
- `category_histogram: Option<HashMap<String, u32>>` — pre-resolved histogram, None on cold start

Updated all construction sites outside `search.rs` to include the two new fields:
- `crates/unimatrix-server/src/eval/runner/replay.rs` — `session_id: None, category_histogram: None`
- `crates/unimatrix-server/src/services/briefing.rs` — already updated by concurrent agent
- `crates/unimatrix-server/src/uds/listener.rs` — already updated by concurrent agent
- `crates/unimatrix-server/src/mcp/tools.rs` — already updated by concurrent agent

### Component B — FusedScoreInputs / FusionWeights / compute_fused_score

**FusedScoreInputs**: Replaced WA-2 stub at line 55 with two new fields:
- `phase_histogram_norm: f64` — p(entry.category) from session histogram, [0.0, 1.0]
- `phase_explicit_norm: f64` — ADR-003 placeholder, always 0.0 in crt-026

**FusionWeights**: Replaced WA-2 stub at line 89, updated invariant doc-comment:
- `w_phase_histogram: f64` — default 0.02 (ASS-028 calibrated, ADR-004)
- `w_phase_explicit: f64` — default 0.0 (W3-1 placeholder, ADR-003)
- Doc-comment updated: "sum of six core terms <= 1.0; w_phase_histogram and w_phase_explicit are additive terms excluded from this constraint"

**FusionWeights::from_config**: Added both new fields from InferenceConfig.

**FusionWeights::effective**: Updated both NLI-active and NLI-absent paths to pass phase fields through unchanged. NLI-absent denominator is exactly five terms (w_sim+w_conf+w_coac+w_util+w_prov) — phase fields NOT in denominator (R-06 invariant).

**compute_fused_score**: Replaced WA-2 stub at line 179, added two new terms:
```
+ weights.w_phase_histogram * inputs.phase_histogram_norm
// crt-026: ADR-003 placeholder — always 0.0 in crt-026; W3-1 will populate phase_explicit_norm
+ weights.w_phase_explicit  * inputs.phase_explicit_norm
```

**Scoring loop**: Added histogram pre-computation before loop (`histogram_total: u32`) and per-candidate `phase_histogram_norm` derivation. `phase_explicit_norm: 0.0` set at FusedScoreInputs construction site with ADR-003 comment.

**Existing struct literals**: Updated all ~20 FusedScoreInputs and FusionWeights test literals to include the new fields with `0.0` values (cold-start safe, preserves pre-crt-026 test behavior).

---

## Tests

All tests from `test-plan/fused-score.md` and `test-plan/search-params.md` implemented.

**Gate-blocking tests** (all pass):
- `test_histogram_boost_score_delta_at_p1_equals_weight` — delta >= 0.02 at p=1.0
- `test_cold_start_search_produces_identical_scores` — cold start produces identical scores
- `test_absent_category_phase_histogram_norm_is_zero` — absent category → 0.0
- `test_fusion_weights_effective_nli_absent_excludes_phase_from_denominator` — R-06

**Additional tests** (all pass):
- `test_60_percent_concentration_score_delta` — 0.6 concentration → 0.012 delta
- `test_status_penalty_applied_after_histogram_boost` — (base+boost)*penalty ordering
- `test_phase_histogram_norm_zero_when_total_is_zero` — no div-by-zero, no NaN
- `test_phase_explicit_norm_placeholder_fields_present` — ADR-003 placeholder confirmed
- `test_fusion_weights_effective_nli_active_phase_fields_pass_through` — NLI-active pass-through
- `test_service_search_params_has_session_fields` — struct fields exist
- `test_service_search_params_with_session_data` — histogram values correct
- `test_service_search_params_empty_histogram_maps_to_none` — handler contract

**Test results**: 87 pass / 0 fail in `services::search` module. Full workspace: 0 failures.

---

## Verification

- `cargo build --workspace` — zero errors
- `cargo test --workspace` — zero failures
- No WA-2 extension stub comments remain in search.rs (AC-14 satisfied)
- No `todo!()`, `unimplemented!()` in non-test code
- `cargo clippy --package unimatrix-server` — zero errors
- `cargo fmt --package unimatrix-server` — applied

---

## Issues / Deviations

None. Implementation follows pseudocode exactly. No silent deviations.

Note: several files (`briefing.rs`, `uds/listener.rs`, `mcp/tools.rs`) were already updated by concurrent agents by the time this agent processed them. This is expected swarm behavior.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` fusion score patterns — found entry #3156 (WA-2 affinity boost design pattern) and #2964 (signal fusion pattern) which confirmed the additive boost approach.
- Stored: entry #3182 "Extending FusedScoreInputs/FusionWeights: additive phase terms excluded from NLI-absent re-normalization denominator" via `/uni-store-pattern`

Key finding worth storing: The five-term denominator in `FusionWeights::effective()` must be hardcoded as an explicit sum (not derived dynamically) to prevent accidental inclusion of future additive fields. Approximately 20 existing struct literals require exhaustive field updates when new fields are added to `FusedScoreInputs` or `FusionWeights` — this is a mechanical but large change that must be done carefully.
