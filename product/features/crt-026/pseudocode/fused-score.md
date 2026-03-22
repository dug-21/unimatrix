# crt-026: Component — `FusedScoreInputs` / `FusionWeights` / `compute_fused_score` (`services/search.rs`)

File: `crates/unimatrix-server/src/services/search.rs`
Wave: 1

---

## Purpose

Replace the three WA-2 extension stubs in `search.rs` with the implemented fields and
logic. Extend the fused scoring formula with two new terms. Update
`FusionWeights::from_config` and `FusionWeights::effective`. Add the histogram total
pre-computation and `phase_histogram_norm` derivation to the scoring loop.

After this component, no `WA-2 extension:` comment may remain in `search.rs` (AC-14).

---

## Current WA-2 Stubs to Replace

| Line | Current stub | Replacement |
|------|-------------|-------------|
| 55 | `/// WA-2 extension: add 'phase_boost_norm: f64' here when WA-2 is implemented.` | Two new fields on `FusedScoreInputs` |
| 89 | `/// WA-2 extension: add 'w_phase: f64' here when WA-2 is implemented.` | Two new fields on `FusionWeights` |
| 179 | `/// WA-2 extension: add 'w_phase * inputs.phase_boost_norm' term when WA-2 is implemented.` | Two new terms in `compute_fused_score` |

---

## Modification 1: `FusedScoreInputs` (replaces stub at line 55)

Replace the stub comment with two new field declarations. Insert them after `prov_norm`
(the last existing field), and update the struct doc-comment.

Updated struct doc-comment header addition (append to the existing doc block):
```
/// crt-026 (WA-2): Two phase fields added. phase_explicit_norm is always 0.0
/// in crt-026 (W3-1 reserved placeholder, ADR-003). Do not remove these fields —
/// W3-1 depends on them as named, stable, learnable dimensions (NFR-06).
```

New fields:
```
/// crt-026: Category histogram affinity (WA-2).
/// p(entry.category) from the session's category_counts histogram, normalized to [0.0, 1.0].
/// 0.0 when session has no prior stores (cold start), entry.category not in histogram,
/// or ServiceSearchParams.category_histogram is None.
/// Computed in the scoring loop as: count[entry.category] / total_count.
pub phase_histogram_norm: f64,

/// crt-026: Explicit phase term (WA-2, ADR-003 placeholder).
/// Always 0.0 in crt-026. Reserved for W3-1 (GNN training).
/// W3-1 will populate this from a learned phase-to-category relevance model.
/// DO NOT remove: W3-1 depends on this named field. Comment cites ADR-003 as guard.
pub phase_explicit_norm: f64,
```

---

## Modification 2: `FusionWeights` (replaces stub at line 89)

Replace the stub comment with two new fields. Update the struct invariant doc-comment.

Updated invariant doc-comment (replaces existing):
```
/// Invariant (enforced by InferenceConfig::validate at startup):
///   w_sim + w_nli + w_conf + w_coac + w_util + w_prov <= 1.0  (sum of six core terms)
///   Each core field individually in [0.0, 1.0].
///
/// w_phase_histogram and w_phase_explicit are additive terms excluded from this
/// constraint. Their sum does not enter the six-term sum check. With defaults,
/// total sum = 0.95 + 0.02 + 0.0 = 0.97, within <= 1.0.
///
/// Per-field range [0.0, 1.0] is enforced by InferenceConfig::validate for all eight fields.
```

New fields (add after `w_prov`):
```
pub w_phase_histogram: f64,  // crt-026: default 0.02 — histogram affinity (ADR-004, ASS-028 calibrated)
pub w_phase_explicit: f64,   // crt-026: default 0.0  — W3-1 placeholder (ADR-003)
```

---

## Modification 3: `FusionWeights::from_config`

Add the two new fields to the struct literal:

```
pub(crate) fn from_config(cfg: &crate::infra::config::InferenceConfig) -> FusionWeights {
    FusionWeights {
        w_sim:             cfg.w_sim,
        w_nli:             cfg.w_nli,
        w_conf:            cfg.w_conf,
        w_coac:            cfg.w_coac,
        w_util:            cfg.w_util,
        w_prov:            cfg.w_prov,
        w_phase_histogram: cfg.w_phase_histogram,   // crt-026: NEW
        w_phase_explicit:  cfg.w_phase_explicit,    // crt-026: NEW
    }
}
```

---

## Modification 4: `FusionWeights::effective`

The `effective` method must pass both new fields through UNCHANGED in both paths:
NLI-active and NLI-absent. The NLI-absent re-normalization denominator must enumerate
only the five core terms — phase fields are NOT in the denominator.

### NLI-active path (nli_available = true)

Current returns `self` effectively. Update the explicit struct return to include new fields:

```
if nli_available {
    return FusionWeights {
        w_sim:             self.w_sim,
        w_nli:             self.w_nli,
        w_conf:            self.w_conf,
        w_coac:            self.w_coac,
        w_util:            self.w_util,
        w_prov:            self.w_prov,
        w_phase_histogram: self.w_phase_histogram,   // crt-026: pass through unchanged
        w_phase_explicit:  self.w_phase_explicit,    // crt-026: pass through unchanged
    };
}
```

### NLI-absent path (nli_available = false)

The denominator is exactly five terms. Phase fields are NOT in the denominator.
The zero-denominator guard path also needs both new fields:

```
// NLI absent — zero out w_nli, re-normalize the five core terms only.
// w_phase_histogram and w_phase_explicit are passed through unchanged (ADR-004, R-06).
let denom = self.w_sim + self.w_conf + self.w_coac + self.w_util + self.w_prov;
// NOTE: w_phase_histogram and w_phase_explicit are NOT in the denominator.

if denom == 0.0 {
    tracing::warn!(
        "FusionWeights::effective: all non-NLI weights are 0.0; \
         fused_score will be 0.0 for all candidates"
    );
    return FusionWeights {
        w_sim: 0.0,
        w_nli: 0.0,
        w_conf: 0.0,
        w_coac: 0.0,
        w_util: 0.0,
        w_prov: 0.0,
        w_phase_histogram: self.w_phase_histogram,   // crt-026: pass through unchanged
        w_phase_explicit:  self.w_phase_explicit,    // crt-026: pass through unchanged
    };
}

FusionWeights {
    w_sim:             self.w_sim  / denom,
    w_nli:             0.0,
    w_conf:            self.w_conf / denom,
    w_coac:            self.w_coac / denom,
    w_util:            self.w_util / denom,
    w_prov:            self.w_prov / denom,
    w_phase_histogram: self.w_phase_histogram,   // crt-026: pass through unchanged (not re-normalized)
    w_phase_explicit:  self.w_phase_explicit,    // crt-026: pass through unchanged (not re-normalized)
}
```

---

## Modification 5: `compute_fused_score` (replaces stub at line 179)

Replace the stub comment with two new additive terms. The existing six terms are unchanged.

Updated function doc-comment (add to the existing preconditions block):
```
/// crt-026: Two phase terms added. phase_explicit_norm is always 0.0 in crt-026
/// (ADR-003 placeholder). The histogram term contributes at most 0.02 with defaults.
/// status_penalty is still applied at the call site: final_score = compute_fused_score(...) * penalty.
```

Updated function body:
```
pub(crate) fn compute_fused_score(inputs: &FusedScoreInputs, weights: &FusionWeights) -> f64 {
    weights.w_sim  * inputs.similarity
        + weights.w_nli  * inputs.nli_entailment
        + weights.w_conf * inputs.confidence
        + weights.w_coac * inputs.coac_norm
        + weights.w_util * inputs.util_norm
        + weights.w_prov * inputs.prov_norm
        + weights.w_phase_histogram * inputs.phase_histogram_norm
        // crt-026: ADR-003 placeholder — always 0.0 in crt-026; W3-1 will populate phase_explicit_norm
        + weights.w_phase_explicit  * inputs.phase_explicit_norm
}
```

The ADR-003 comment on the `w_phase_explicit` line is required (IMPLEMENTATION-BRIEF constraint 9,
V-2 variance mitigation). It prevents future removal of this term as dead code.

---

## Modification 6: Scoring loop — histogram pre-computation and per-candidate norm

The scoring loop is in `SearchService::search`, at the block starting with
`let mut scored: Vec<(EntryRecord, f64, f64)>` (around line 752 of search.rs).

### Before the loop: compute total once

Insert after the `let effective_weights = self.fusion_weights.effective(nli_available);` line
and before `let mut scored: Vec<...>`:

```
// crt-026: Pre-compute histogram total once before the scoring loop (WA-2, ADR-002).
// All per-candidate phase_histogram_norm values derive from this single read.
// If category_histogram is None (cold start), total = 0 and all norms will be 0.0.
let category_histogram = params.category_histogram.as_ref();
let histogram_total: u32 = category_histogram
    .map(|h| h.values().copied().sum())
    .unwrap_or(0);
```

### Inside the loop: compute phase_histogram_norm per candidate

Insert after the `prov_norm` computation and before the `FusedScoreInputs { ... }` literal:

```
// crt-026: phase_histogram_norm = p(entry.category) from session histogram (WA-2).
// Division is safe: guarded by histogram_total > 0 check.
// 0.0 when: cold start (histogram_total == 0), or entry.category not in histogram.
let phase_histogram_norm: f64 = if histogram_total > 0 {
    category_histogram
        .and_then(|h| h.get(&entry.category))
        .copied()
        .unwrap_or(0) as f64
        / histogram_total as f64
} else {
    0.0
};
```

### Update the `FusedScoreInputs { ... }` literal

Add the two new fields to the existing struct literal:

```
let inputs = FusedScoreInputs {
    similarity: *sim,
    nli_entailment,
    confidence: entry.confidence,
    coac_norm,
    util_norm,
    prov_norm,
    phase_histogram_norm,                         // crt-026: histogram affinity
    // crt-026: ADR-003 placeholder — always 0.0; W3-1 will populate this field
    phase_explicit_norm: 0.0,
};
```

---

## Division-by-Zero Guard Analysis (R-09)

The primary defense against division by zero is the handler's `is_empty() → None` mapping,
which prevents a `Some(empty_map)` from reaching the scoring loop. The secondary defense
is the `if histogram_total > 0` guard in the loop itself.

Proof of correctness:
- `category_histogram = None` → `histogram_total = 0` → `phase_histogram_norm = 0.0` (no division)
- `category_histogram = Some(non_empty)` → `histogram_total >= 1` → division is safe
- A `Some(empty_map)` cannot reach this path if the handler correctly maps `is_empty()` to `None`
  (guarded by test scenario 1 in R-09)

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| `category_histogram = None` | `histogram_total = 0`, `phase_histogram_norm = 0.0` for all |
| `entry.category` not in histogram | `.and_then(...).copied().unwrap_or(0)` → 0 count → 0.0 norm |
| `histogram_total = 0` (guard) | `phase_histogram_norm = 0.0` (no division attempted) |
| `phase_explicit_norm = 0.0` | Multiplied by `w_phase_explicit` (also 0.0) → contributes exactly 0.0 |

---

## Key Test Scenarios

See `test-plan/fused-score.md` for the full test plan. Key scenarios:

1. **AC-12 / R-01 (gate blocker)**: p=1.0 concentration. Two candidates with identical
   six-term inputs. One has `phase_histogram_norm = 1.0`, other has `0.0`. Assert score
   delta = exactly `0.02 * 1.0 = 0.02`.

2. **AC-08 / R-02 (gate blocker)**: All inputs identical to a pre-crt-026 call, plus
   `phase_histogram_norm = 0.0, phase_explicit_norm = 0.0`. Assert output exactly equals
   the six-term formula result (no floating-point drift from zero terms).

3. **AC-13 / R-01 scenario 3**: Category not in histogram → `phase_histogram_norm = 0.0`
   → boost = 0.0.

4. **R-01 scenario 2**: 60% concentration `{"decision": 3, "pattern": 2}` → `p("decision") = 0.6`
   → score delta = `0.02 * 0.6 = 0.012` (exactly).

5. **R-06 (gate blocker)**: `FusionWeights::effective(false)` — assert `w_phase_histogram`
   returned unchanged (= 0.02), re-normalization denominator does NOT include it.

6. **R-07 (AC-09)**: `FusedScoreInputs` has `phase_explicit_norm: f64` field. `FusionWeights`
   has `w_phase_explicit: f64` field. Both are present in struct definitions (compilation test).

7. **R-08 (AC-10)**: Candidate with `status_penalty = 0.5` and histogram match.
   Assert `final_score = compute_fused_score(&inputs_with_histogram, &weights) * 0.5`.
   The boost is inside fused score, not added after penalty.

8. **R-09**: `histogram_total = 0` → `phase_histogram_norm = 0.0` (no division, no NaN).

9. **AC-14 / R-14**: No `WA-2 extension:` string remains in `search.rs` after implementation.
