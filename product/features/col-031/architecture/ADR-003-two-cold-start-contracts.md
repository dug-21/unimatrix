## ADR-003: Two Cold-Start Contracts for One Method

### Context

`PhaseFreqTable::phase_affinity_score` has two distinct callers with different
cold-start requirements:

1. **PPR (#398)** — will call `phase_affinity_score` directly to build a
   personalization vector: `personalization[v] = hnsw_score[v] * phase_affinity_score(v.id, v.category, phase)`.
   For PPR, the correct cold-start value is `1.0` — a neutral multiplier that
   preserves the HNSW score unchanged (`hnsw_score × 1.0 = hnsw_score`). If
   `phase_affinity_score` returned `0.0` on cold start, PPR would suppress all
   seed weights to zero, producing a degenerate personalization vector.

2. **Fused scoring** — uses `phase_affinity_score` to set
   `FusedScoreInputs.phase_explicit_norm`, which is multiplied by
   `w_phase_explicit = 0.05`. The correct cold-start value here is `0.0` —
   producing scores bit-for-bit identical to pre-col-031 when no phase history
   exists. If fused scoring used `1.0`, it would apply a uniform `0.05` additive
   boost to every candidate, changing relative rankings even before any
   phase signal exists.

These requirements cannot be reconciled by a single return value. Two mechanisms
are needed:

**Option A**: Two methods (`phase_affinity_score_for_ppr` and
`phase_affinity_score_for_fused`). Rejected — the underlying computation is
identical; only the caller's use of the result differs. Two methods would
duplicate the lookup logic.

**Option B**: One method (`phase_affinity_score`) always returns `1.0` on
cold start; fused scoring guards on `use_fallback` before calling the method.
The guard short-circuits to `phase_explicit_norm = 0.0` without ever calling
`phase_affinity_score`. The method's doc comment explicitly names both callers
and their respective cold-start contracts.

### Decision

Use Option B: `phase_affinity_score` returns `1.0` when `use_fallback = true`,
phase is absent, or entry is absent from the bucket. Fused scoring must check
`use_fallback` on the handle *before* calling `phase_affinity_score`. When
`use_fallback = true` or `current_phase = None`, fused scoring sets
`phase_explicit_norm = 0.0` directly without calling the method.

The lock is acquired once before the scoring loop (not per-entry):

```rust
let phase_snapshot = match &params.current_phase {
    None => None,
    Some(phase) => {
        let guard = freq_table.read().unwrap_or_else(|e| e.into_inner());
        if guard.use_fallback {
            None  // cold-start: phase_explicit_norm = 0.0 for all candidates
        } else {
            Some(guard.extract_phase_snapshot(phase))
        }
        // guard dropped here — lock released before scoring loop
    }
};
```

The method's doc comment must explicitly state: "Returns 1.0 on cold-start —
neutral PPR multiplier. Fused scoring must guard on `use_fallback` before
calling this method."

This decision is documented in SR-06 of SCOPE-RISK-ASSESSMENT.md.

### Consequences

**Easier**:
- PPR (#398) can call `phase_affinity_score` directly without conditional logic —
  cold-start produces the correct neutral value automatically.
- Pre-col-031 score identity is preserved for fused scoring during cold start.
- Single method, single lookup implementation.
- The lock is never held across the scoring loop (performance).

**Harder**:
- The two cold-start behaviors are implicit in the caller's guard pattern, not
  enforced by the type system. Future callers of `phase_affinity_score` must
  read the doc comment to understand which cold-start contract applies to them.
- If PPR mistakenly routes through fused scoring rather than calling
  `phase_affinity_score` directly, the cold-start semantics will be wrong
  (SR-06). This is a doc comment and code-review concern, not a runtime guard.
