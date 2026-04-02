# Agent Report: crt-038-agent-1-pseudocode

## Task Completed

Produced pseudocode files for all five components of crt-038, consolidated into the
four output files specified by the spawn prompt.

## Files Produced

- `product/features/crt-038/pseudocode/OVERVIEW.md`
- `product/features/crt-038/pseudocode/effective-short-circuit.md`
- `product/features/crt-038/pseudocode/config-defaults.md`
- `product/features/crt-038/pseudocode/dead-code-removal.md`

## Components Covered

| Wave | Component | Pseudocode File |
|------|-----------|----------------|
| 1a | FusionWeights::effective() short-circuit (search.rs) | effective-short-circuit.md |
| 1b | Config default constants (config.rs) | config-defaults.md |
| 2 | run_post_store_nli removal (nli_detection.rs + store_ops.rs + mod.rs) | dead-code-removal.md |
| 2 | maybe_run_bootstrap_promotion removal (nli_detection.rs + background.rs) | dead-code-removal.md |
| 2 | NLI auto-quarantine guard removal (background.rs) | dead-code-removal.md |

## Source Documents Read

- `product/features/crt-038/IMPLEMENTATION-BRIEF.md`
- `product/features/crt-038/architecture/ARCHITECTURE.md`
- `product/features/crt-038/specification/SPECIFICATION.md`
- `product/features/crt-038/RISK-TEST-STRATEGY.md`
- `product/features/crt-038/architecture/ADR-001-effective-zero-nli-short-circuit.md`
- `product/features/crt-038/architecture/ADR-004-nli-detection-module-merge-deferred.md`

## Key Design Findings from Source Code Inspection

### Finding 1: default_nli_enabled() already returns false

The live source at `config.rs:637` returns `false`, not `true` as the
IMPLEMENTATION-BRIEF.md and ARCHITECTURE.md list. The architecture documents the
change as "true → false", but the actual code already has `false`. This is flagged
in `config-defaults.md` — delivery must verify before treating it as a line to change.
If the value is already `false`, no edit is needed for that specific function, but the
test update still applies.

### Finding 2: parse_nli_contradiction_from_metadata is a cascaded deletion

`nli_auto_quarantine_allowed` (Component 5 deletion) has a private helper
`parse_nli_contradiction_from_metadata` (line ~1297 in background.rs) that is not
listed in any of the architecture's deletion checklists. After removing
`nli_auto_quarantine_allowed`, this function becomes callerless dead code and clippy
will fail on it. The dead-code-removal pseudocode explicitly calls this out and adds
it to the deletion list and grep verification checklist.

### Finding 3: nli_enabled / nli_auto_quarantine_threshold propagation chain is four deep

The two parameters to be stripped from `process_auto_quarantine` propagate through four
function signatures:
`spawn_background_tick` -> `background_tick_loop` -> `run_single_tick` -> `maintenance_tick`
All four signatures (and their call sites) must be updated atomically. This is called
out explicitly in dead-code-removal.md with a grep instruction to find the
`spawn_background_tick` call site.

### Finding 4: FusionWeights struct uses explicit field construction (no Copy spread at nli_available=true path)

The existing `nli_available=true` fast path at line 152 uses an explicit field-by-field
`FusionWeights { w_sim: self.w_sim, ... }` pattern rather than `return *self`. The
pseudocode notes this and instructs delivery to verify whether `Copy` is derived on
`FusionWeights` before choosing the spread syntax. If `Copy` is not derived, the
explicit field form must be used (matching the existing pattern in the function).

## Open Questions

1. **default_nli_enabled() current value**: The source shows `false` at line 638. If
   the IMPLEMENTATION-BRIEF documents "true → false", either the live source was already
   patched (unlikely given git status), or the brief reflects the pre-feature state
   that was changed during a prior session. Delivery should `git blame` line 638 and
   confirm the history before deciding whether to edit this function.

2. **#[allow(clippy::too_many_arguments)] retention**: After removing `nli_cfg: NliStoreConfig`
   from `StoreService::new`, the remaining parameter count must be checked. If it falls
   to 10 or fewer, clippy may not require the allow annotation and it can be removed.
   If still needed, retain it.

3. **nli_handle field on StoreService**: The `nli_handle` field (line 101 store_ops.rs)
   is not deleted by this feature (it is only `nli_cfg` that is removed). Delivery should
   verify whether `nli_handle` still has callers in `store_ops.rs` post-deletion; if
   callerless, it may warrant a follow-up issue but is out of scope for crt-038.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_search` for "FusionWeights scoring formula implementation patterns" (category: pattern) — returned entries #3207, #3206, #3181. Entry #3206 confirmed that `w_phase_histogram` and `w_phase_explicit` are additive terms exempt from the NLI-absent re-normalization denominator. This directly informed the pseudocode for test 1 and test 3 (additive terms must pass through unchanged in all effective() paths).
- Queried: `mcp__unimatrix__context_search` for "crt-038 architectural decisions" (category: decision) — returned entries #4007, #4006, #4005 (ADR-001, ADR-002, ADR-003 for crt-038). These confirmed the architecture's design decisions were already captured in Unimatrix.
- Deviations from established patterns: `parse_nli_contradiction_from_metadata` cascaded deletion (not in architecture's symbol checklist) — flagged in pseudocode and agent report. No stored pattern violated.
