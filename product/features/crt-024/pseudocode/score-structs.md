# crt-024: FusedScoreInputs and FusionWeights Structs — Pseudocode

## Purpose

Define the two structs that carry per-candidate signal inputs and config-driven weights into
`compute_fused_score`. These live in `crates/unimatrix-server/src/services/search.rs`.

`FusedScoreInputs` is the per-candidate value type constructed in the scoring loop — one instance
per candidate per search call. It is also the feature vector interface for W3-1: each field is a
learnable dimension that GNN training will eventually replace with learned weights.

`FusionWeights` wraps the config-loaded weight values and provides the `effective()` method for
NLI re-normalization. It is constructed once in `SearchService::new` from `InferenceConfig` and
stored as a field on `SearchService`.

---

## File: `crates/unimatrix-server/src/services/search.rs`

Add both structs near the top of the file, after the existing constants block and before the
`SearchService` struct definition. They are module-level items, not impl blocks.

---

## `FusedScoreInputs` Struct

```
/// Per-candidate signal inputs for the fused scoring formula (crt-024, ADR-004).
///
/// All fields are f64 in [0.0, 1.0] by the time compute_fused_score is called.
/// Field normalization is the caller's responsibility (see SearchService scoring loop).
///
/// This struct is the feature vector interface for W3-1 (GNN training). Each field
/// is a named, learnable dimension. Do not add signals outside this struct.
///
/// WA-2 extension: add `phase_boost_norm: f64` here when WA-2 is implemented.
pub(crate) struct FusedScoreInputs {
    /// HNSW cosine similarity (bi-encoder recall). Already in [0, 1].
    pub similarity: f64,

    /// NLI cross-encoder entailment score (cross-encoder precision).
    /// Already in [0, 1] when model produces valid softmax output.
    /// Set to 0.0 when NLI is absent or disabled — the weight is then
    /// re-normalized away by FusionWeights::effective(nli_available: false).
    pub nli_entailment: f64,

    /// Wilson score composite confidence (EntryRecord.confidence). Already in [0, 1].
    pub confidence: f64,

    /// Co-access affinity normalized to [0, 1].
    /// Computed as: raw_boost / MAX_CO_ACCESS_BOOST.
    /// 0.0 when entry has no co-access history or boost_map lookup misses.
    pub coac_norm: f64,

    /// Utility delta normalized to [0, 1] via shift-and-scale (FR-05, ADR-001 crt-024).
    /// Formula: (utility_delta + UTILITY_PENALTY) / (UTILITY_BOOST + UTILITY_PENALTY).
    /// Maps: Ineffective (-0.05) -> 0.0, neutral (0.0) -> 0.5, Effective (+0.05) -> 1.0.
    pub util_norm: f64,

    /// Provenance boost normalized to [0, 1] (FR-06, ADR-001 crt-024).
    /// Formula: prov_boost / PROVENANCE_BOOST, guarded for PROVENANCE_BOOST == 0.0.
    /// Binary in practice: 1.0 for boosted categories, 0.0 for all others.
    pub prov_norm: f64,
}
```

---

## `FusionWeights` Struct

```
/// Config-driven fusion weights for the six-term ranking formula (crt-024, ADR-003).
///
/// Constructed from InferenceConfig in SearchService::new. Stored as a field on SearchService.
/// Not derived from InferenceConfig at every search call — built once and cloned if needed.
///
/// Invariant (enforced by InferenceConfig::validate at startup):
///   w_sim + w_nli + w_conf + w_coac + w_util + w_prov <= 1.0
///   Each field individually in [0.0, 1.0].
///
/// WA-2 extension: add `w_phase: f64` here when WA-2 is implemented.
pub(crate) struct FusionWeights {
    pub w_sim:  f64,   // default 0.25 — bi-encoder similarity
    pub w_nli:  f64,   // default 0.35 — NLI entailment (dominant precision signal)
    pub w_conf: f64,   // default 0.15 — confidence tiebreaker
    pub w_coac: f64,   // default 0.10 — co-access affinity (lagging signal)
    pub w_util: f64,   // default 0.05 — effectiveness classification
    pub w_prov: f64,   // default 0.05 — category provenance hint
}
```

---

## `FusionWeights::from_config` Constructor

Constructs a `FusionWeights` from an `InferenceConfig`. Called once in `SearchService::new`.
The `InferenceConfig` has already passed `validate()` at startup, so field values are trusted.

```
impl FusionWeights {
    /// Construct FusionWeights from the validated InferenceConfig.
    pub(crate) fn from_config(cfg: &InferenceConfig) -> FusionWeights {
        FusionWeights {
            w_sim:  cfg.w_sim,
            w_nli:  cfg.w_nli,
            w_conf: cfg.w_conf,
            w_coac: cfg.w_coac,
            w_util: cfg.w_util,
            w_prov: cfg.w_prov,
        }
    }
```

---

## `FusionWeights::effective` Method

Returns a derived weight set adjusted for NLI availability. When NLI is active, returns `self`
unchanged. When NLI is absent, sets `w_nli = 0.0` and re-normalizes the remaining five weights
so they sum to 1.0 (FR-07, ADR-003 Constraint 9).

The original `FusionWeights` is NOT mutated — `effective()` takes `&self` and returns a new value.

```
    /// Return an effective weight set adjusted for NLI availability.
    ///
    /// NLI active (nli_available = true): returns self unchanged.
    ///   The configured weights are used directly. No re-normalization.
    ///   This is the common path when NLI is enabled and the model is ready.
    ///
    /// NLI absent (nli_available = false): sets w_nli = 0.0, re-normalizes
    ///   the remaining five weights by dividing each by their sum.
    ///   This preserves the relative signal dominance ordering (Constraint 9, ADR-003):
    ///   sim remains dominant, conf secondary.
    ///
    /// Zero-denominator guard (R-02): if all five non-NLI weights are 0.0
    ///   (pathological but reachable config), returns all-zeros without panic.
    pub(crate) fn effective(&self, nli_available: bool) -> FusionWeights {
        if nli_available {
            // NLI active — use configured weights directly, no re-normalization.
            return FusionWeights {
                w_sim:  self.w_sim,
                w_nli:  self.w_nli,
                w_conf: self.w_conf,
                w_coac: self.w_coac,
                w_util: self.w_util,
                w_prov: self.w_prov,
            };
        }

        // NLI absent — zero out w_nli, re-normalize remaining five.
        let denom = self.w_sim + self.w_conf + self.w_coac + self.w_util + self.w_prov;

        if denom == 0.0 {
            // Pathological config: all non-NLI weights are zero.
            // Return all-zeros without panic. All candidates score 0.0.
            // Log at warn level; this config would have been nonsensical
            // even before NLI was introduced.
            tracing::warn!(
                "FusionWeights::effective: all non-NLI weights are 0.0; \
                 fused_score will be 0.0 for all candidates"
            );
            return FusionWeights {
                w_sim:  0.0,
                w_nli:  0.0,
                w_conf: 0.0,
                w_coac: 0.0,
                w_util: 0.0,
                w_prov: 0.0,
            };
        }

        // Divide each non-NLI weight by the five-weight denominator.
        // The result is a new FusionWeights where the five remaining fields sum to 1.0,
        // w_nli = 0.0, and relative ordering is preserved.
        FusionWeights {
            w_sim:  self.w_sim  / denom,
            w_nli:  0.0,
            w_conf: self.w_conf / denom,
            w_coac: self.w_coac / denom,
            w_util: self.w_util / denom,
            w_prov: self.w_prov / denom,
        }
    }
}  // end impl FusionWeights
```

---

## Normalization Helper Notes

The normalization of each signal field into `FusedScoreInputs` happens at the call site in the
scoring loop (inside `SearchService::search`), not in any method on `FusedScoreInputs`.

Key normalization formulas (for implementer reference — pseudocode of the call site, documented
in full in search-service.md):

```
// coac_norm: raw boost from boost_map divided by the engine constant
use unimatrix_engine::coaccess::MAX_CO_ACCESS_BOOST;
coac_norm = boost_map.get(&entry.id).copied().unwrap_or(0.0) / MAX_CO_ACCESS_BOOST;

// util_norm: shift-and-scale to map [-UTILITY_PENALTY, +UTILITY_BOOST] -> [0, 1]
use unimatrix_engine::effectiveness::{UTILITY_BOOST, UTILITY_PENALTY};
let raw_delta = utility_delta(categories.get(&entry.id).copied());  // unchanged function
util_norm = (raw_delta + UTILITY_PENALTY) / (UTILITY_BOOST + UTILITY_PENALTY);

// prov_norm: divide by PROVENANCE_BOOST (already imported as local const PROVENANCE_BOOST)
let raw_prov = if boosted_categories.contains(&entry.category) { PROVENANCE_BOOST } else { 0.0 };
prov_norm = if PROVENANCE_BOOST == 0.0 { 0.0 } else { raw_prov / PROVENANCE_BOOST };

// nli_entailment: cast NliScores.entailment (f32) to f64 at the call site
nli_entailment = nli_scores[i].entailment as f64;  // safe: f32 -> f64 is lossless
```

---

## Error Handling

Neither struct has methods that can fail. All invariants are enforced before these structs are
constructed:
- `FusionWeights` values come from `InferenceConfig` validated at startup
- `FusedScoreInputs` values are computed inline from already-resolved data; each normalization
  formula is guarded at its call site (see search-service.md for per-guard details)

---

## Key Test Scenarios

### T-SS-01: `FusionWeights::effective` — NLI active path (AC-13, R-09)

Construct `FusionWeights` with defaults. Call `effective(true)`. Assert all six fields equal
the originals unchanged. Assert `w_nli == 0.35` (not zeroed). Assert sum == 0.95.

### T-SS-02: `FusionWeights::effective` — NLI absent path, five-weight denominator (AC-06, R-02)

Construct `FusionWeights` with defaults (w_sim=0.25, w_nli=0.35, w_conf=0.15, w_coac=0.10,
w_util=0.05, w_prov=0.05). Call `effective(false)`. Assert:
- `w_nli == 0.0`
- `w_sim ≈ 0.4167` (0.25 / 0.60)
- `w_conf ≈ 0.2500` (0.15 / 0.60)
- `w_coac ≈ 0.1667` (0.10 / 0.60)
- `w_util ≈ 0.0833` (0.05 / 0.60)
- `w_prov ≈ 0.0833` (0.05 / 0.60)
- sum of all six ≈ 1.0 (within f64 epsilon)

This verifies Constraint 9 from ADR-003: sim remains dominant, conf secondary.

### T-SS-03: `FusionWeights::effective` — zero-denominator guard (R-02)

Construct `FusionWeights` with w_sim=0, w_conf=0, w_coac=0, w_util=0, w_prov=0, w_nli=1.0.
Call `effective(false)`. Assert no panic. Assert all returned fields are 0.0.

### T-SS-04: `FusionWeights::effective` — single non-zero remaining weight (R-02)

Construct `FusionWeights` with w_sim=0.5, all others 0.0. Call `effective(false)`. Assert
`w_sim_eff == 1.0` and all other fields are 0.0. Confirms re-normalization works with a
single remaining dimension.

### T-SS-05: `FusionWeights::effective` — does not mutate original

After calling `effective(false)`, assert the original `FusionWeights` still has `w_nli == 0.35`.
`effective()` returns a new value; the receiver is not modified.

### T-SS-06: `FusionWeights::from_config` maps fields correctly

Construct an `InferenceConfig` with non-default weight values (e.g., w_sim=0.30, w_nli=0.30).
Call `FusionWeights::from_config(&cfg)`. Assert each field matches the config field exactly.
