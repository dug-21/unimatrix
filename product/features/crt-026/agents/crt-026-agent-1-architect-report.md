# Agent Report: crt-026-agent-1-architect

## Status: Complete

## Artifacts Produced

- `/workspaces/unimatrix/product/features/crt-026/architecture/ARCHITECTURE.md`
- `/workspaces/unimatrix/product/features/crt-026/architecture/ADR-001-fused-score-inputs-integration.md`
- `/workspaces/unimatrix/product/features/crt-026/architecture/ADR-002-pre-resolve-histogram-in-handler.md`
- `/workspaces/unimatrix/product/features/crt-026/architecture/ADR-003-w-phase-explicit-zero-placeholder.md`
- `/workspaces/unimatrix/product/features/crt-026/architecture/ADR-004-no-weight-rebalancing.md`

## Unimatrix Entry IDs

| ADR | Entry ID |
|-----|----------|
| ADR-001: FusedScoreInputs Integration | #3161 |
| ADR-002: Pre-Resolve Histogram in Handler | #3162 |
| ADR-003: w_phase_explicit=0.0 Placeholder | #3163 |
| ADR-004: No Weight Rebalancing | #3164 |

## Open Questions — Resolved

**OQ-A (SR-02): Does InferenceConfig::validate() accept sum=0.955?**
Yes. validate() computes `w_sim + w_nli + w_conf + w_coac + w_util + w_prov <= 1.0` —
the six original fields only. The new phase weight fields are NOT included in this sum
check. The six-field sum remains 0.95; the total including phase terms is 0.955, which
passes `<= 1.0`. No existing test asserts `sum == 0.95` exactly against defaults.
The FusionWeights doc-comment invariant must be updated to clarify the distinction.

**OQ-B (SR-08): HookRequest::ContextSearch session_id field name and sanitization order**
Field name is `session_id: Option<String>` with `#[serde(default)]` in
`unimatrix-engine/src/wire.rs`. In `handle_context_search` (listener.rs lines 796-803),
`sanitize_session_id` is already applied on the value before any session registry access.
The histogram pre-resolution must be placed after the sanitize check, before
`ServiceSearchParams` construction — the existing function structure enforces this ordering.

**OQ-C (SR-07): WA-4a forward-compatibility with pre-resolution pattern**
Pre-resolution is correct and sufficient for crt-026. However, WA-4a (proactive injection)
resolves candidates without a user query — no handler on the call stack to pre-resolve.
WA-4a will likely need `Arc<SessionRegistry>` on `SearchService`, reopening ADR-002.
Documented in ADR-002 Consequences and ARCHITECTURE.md Integration Points. No code change
required in crt-026.

**OQ-D (SR-09): status_penalty application order relative to compute_fused_score**
Confirmed at search.rs lines 798-800:
```rust
let fused = compute_fused_score(&inputs, &effective_weights);
let penalty = penalty_map.get(&entry.id).copied().unwrap_or(1.0);
let final_score = fused * penalty;
```
status_penalty is applied AFTER compute_fused_score returns. The histogram boost
(inside compute_fused_score per ADR-001) participates in the pre-penalty fused score.
Application order: (fused_score_including_histogram) * status_penalty. Matches C-06.

## Key Design Decisions

1. **Boost integrated into compute_fused_score** (ADR-001): First-class W3-1 dimension;
   status_penalty applies uniformly; WA-2 stubs at lines 55/89/179 resolved.

2. **Pre-resolve histogram in handler** (ADR-002): SearchService remains dependency-free
   of SessionRegistry. ServiceSearchParams gains `session_id` and `category_histogram`
   fields. Follows crt-025 SR-07 pre-snapshot pattern.

3. **w_phase_explicit=0.0 placeholder** (ADR-003): No static phase→category mapping
   (phase strings are opaque per WA-1 ADR). Field exists for W3-1 compatibility.
   AC-07 dropped from specification as confirmed.

4. **No weight rebalancing** (ADR-004): Sum 0.95→0.955 is within <= 1.0 invariant.
   FusionWeights::effective() NLI-absent denominator must NOT include phase weight fields.

## Critical Implementation Notes for Delivery

- `FusionWeights::effective()` NLI-absent path: exclude `w_phase_histogram` and
  `w_phase_explicit` from the re-normalization denominator. They are pass-through fields
  in both NLI paths.
- All existing struct literal constructions of `FusedScoreInputs` and `FusionWeights`
  in tests must be updated with the two new fields.
- `InferenceConfig::default()` struct literal must gain `w_phase_explicit: 0.0` and
  `w_phase_histogram: 0.005` (or use `..Default::default()` per pattern #2730).
- The `phase_explicit_norm = 0.0` assignment in the scoring loop must carry a comment
  citing ADR-003 to prevent removal as dead code.
- `record_category_store` must be called AFTER the duplicate guard
  (`insert_result.duplicate_of.is_none()`) and BEFORE confidence seeding.

## Blocking Issues

None.
