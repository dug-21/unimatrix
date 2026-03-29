## ADR-007: ppr_blend_weight Dual Role — Intentional Single Parameter

### Context

`ppr_blend_weight` (default 0.15) appears in two places in Step 6d:

**Role 1 — Blending for existing HNSW candidates:**
```
new_sim = (1 - ppr_blend_weight) * sim + ppr_blend_weight * ppr_score
```
This is a linear interpolation between the original HNSW similarity and the PPR score.
At `ppr_blend_weight = 0.15`, the similarity is 85% HNSW + 15% PPR.

**Role 2 — Initial similarity for PPR-only entries (not in HNSW pool):**
```
initial_sim = ppr_blend_weight * ppr_score
```
PPR-only entries have no HNSW component. Their initial similarity is set as the PPR score
weighted by `ppr_blend_weight`. At `ppr_blend_weight = 0.15` and `ppr_score = 0.10`
(above inclusion threshold of 0.05), `initial_sim = 0.015`.

SR-04 in the risk assessment identifies this dual role: "raising the blend weight to
improve existing-candidate adjustment also raises the floor similarity for newly injected
entries." This could produce unintuitive tuning behavior if the two roles pull in different
directions.

Two options:
1. **Single parameter**: `ppr_blend_weight` serves both roles. Simpler config; fewer
   parameters for operators to reason about. The dual role is documented and intentional.
   The semantic unity: "ppr_blend_weight represents how much to trust the PPR signal,
   whether adjusting an existing entry or initializing a new one."
2. **Two parameters**: `ppr_blend_weight` (role 1) + `ppr_inject_weight` (role 2). Full
   independent tuning. But adds config complexity; most operators will not need independent
   tuning in v1. Adding a parameter post-ship is a config format change that requires
   migration notes.

The SCOPE.md Proposed Approach says: "ppr_blend_weight serves dual roles here — blending
for existing candidates AND setting floor similarity for new ones. This is intentional:
the weight represents 'how much to trust PPR signal' in both cases."

### Decision

`ppr_blend_weight` serves both roles. A single parameter is used for v1.

The rationale is that both roles express the same semantic concept: "how much weight to
assign the PPR signal relative to the HNSW signal." For existing candidates, this is a
blend. For new PPR-only candidates, there is no HNSW signal, so the PPR weight is the
full initial score.

The `ppr_blend_weight` range is `[0.0, 1.0]` inclusive:
- At `0.0`: PPR has no influence on existing candidates; PPR-only entries get `initial_sim = 0.0`
  (they would be ranked at the bottom and unlikely to survive floor scoring).
- At `1.0`: existing candidates' similarity is fully replaced by PPR score; PPR-only
  entries get `initial_sim = ppr_score` (full PPR score as similarity).
- At default `0.15`: modest PPR influence for existing candidates; low but non-zero
  similarity floor for PPR-only entries.

The doc-comment on `ppr_blend_weight` in `InferenceConfig` must explicitly document both
roles and reference this ADR.

If operational experience shows the two roles require independent tuning (e.g., operators
want high blend weight for existing candidates but low injection weight for new ones),
a `ppr_inject_weight` field can be added in a follow-up without breaking existing configs
(it would default to `ppr_blend_weight` for backward compatibility).

### Consequences

- Simpler config: five PPR fields instead of six. The "five new fields" count in SCOPE.md
  Goals item 6 is correct.
- Operators tuning `ppr_blend_weight` upward to increase PPR influence on existing
  candidates must accept that PPR-only entries' floor similarity also rises. This is
  documented as expected behavior.
- The `FusionWeights` six-weight sum constraint is not violated: PPR influence enters
  through pool expansion and the `similarity` term only — there is no `w_ppr` added to
  `FusionWeights`.
- Future addition of `ppr_inject_weight` is non-breaking: new field with `#[serde(default)]`
  defaults to the current `ppr_blend_weight` value.
