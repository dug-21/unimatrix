# Agent Report: crt-038-agent-2-spec

## Output

- `product/features/crt-038/specification/SPECIFICATION.md`

## Key Decisions Made

1. **AC-02 short-circuit is a correctness requirement, not an optimization.** The `effective()`
   re-normalization divides by `denom = 0.85` when `w_nli==0.0`, producing `w_sim'≈0.588,
   w_conf'≈0.412`. This deviates from the ASS-039 evaluated formula. The short-circuit guard
   (`if self.w_nli == 0.0 { return *self; }`) is specified as a code change to `search.rs`,
   not a config workaround.

2. **Ordering constraint made explicit.** AC-01 + AC-02 must be implemented and tested before
   the AC-12 eval gate runs. Rationale: an eval on the wrong scoring path invalidates the MRR
   baseline. A five-step delivery sequence is specified.

3. **Shared helper symbols enumerated from source.** `nli_detection_tick.rs` line 34 was read
   directly: `use crate::services::nli_detection::{current_timestamp_secs, format_nli_metadata, write_nli_edge}`.
   All three are `pub(crate)` in `nli_detection.rs`. Specified as SR-06 guard in AC-13.

4. **AC-14 treats SCOPE.md Background Research as superseded by the AC.** Background Research
   implied `NliStoreConfig` fields might be retained; AC-14 requires full deletion. The spec
   follows AC-14 as authoritative (SR-04 recommendation).

5. **FusionWeights default sum stays 0.92.** New defaults: `0.50 + 0.00 + 0.35 + 0.00 + 0.00
   + 0.00 + 0.02 + 0.05 = 0.92`. The test `test_fusion_weights_default_sum_unchanged_by_crt030`
   needs only a message update, not a value change.

6. **500-line constraint applied only to new/modified files.** background.rs (4,229) and
   nli_detection.rs (1,373) are pre-existing violations; explicitly documented as non-blocking
   for this feature.

## Open Questions Surfaced

- Whether ASS-039 eval was run under `nli_enabled=true, w_nli=0.0` (effective(true), no
  re-normalization) or under `nli_enabled=false` on a build without the AC-02 short-circuit
  (re-normalized formula). Affects validity of the MRR=0.2913 baseline for AC-12.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — 16 entries returned; entry #4003
  (FusionWeights::effective(false) re-normalization when w_nli zeroed) directly supported the
  AC-02 specification; entry #3985 (ASS-037 NLI verdict table) confirmed dead-code scope.
