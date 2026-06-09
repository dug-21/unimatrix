# ADR-001 (nan-018): Expose crt-014 Penalty Constants via `GraphPenaltyConfig` Threaded Through `graph_penalty_with`

### Context

The crt-014 topology-penalty coefficients are compile-time `pub const`s in `unimatrix-engine/src/graph.rs:41-59` (`ORPHAN_PENALTY=0.75`, `CLEAN_REPLACEMENT_PENALTY=0.40`, `HOP_DECAY_FACTOR=0.60`, `PARTIAL_SUPERSESSION_PENALTY=0.60`, `DEAD_END_PENALTY=0.65`, `FALLBACK_PENALTY=0.70`, `MAX_TRAVERSAL_DEPTH=10`, clamp lower-bound `0.10`). crt-014 ADR-006 (entry #1606) deliberately chose fixed consts over runtime config, reasoning that no empirical evidence yet justified tuning. nan-018 is exactly the instrument that produces that evidence: crt-053's steepness question is a **sweep**, and a sweep needs the levers exposed to a profile TOML. So nan-018 **partially supersedes** crt-014 ADR-006's "no runtime configurability" conclusion — for the *measurement* path only. This must be additive: deployed defaults and behavior are unchanged at default values (C-02, ASS-037 authority — nan-018 does not re-tune).

The known trap (SR-08; Unimatrix #4131/#4070/#3779, and the dual-default-encoding trap #4064): a config field added in one wave misses a construction/forwarding site or a serde default, silently changing behavior. AC-01 demands the default reproduce current behavior **bit-for-bit**.

Production reads of these consts occur in exactly one hot path: the Flexible-mode penalty loop in `services/search.rs:724-733` (`graph_penalty(...)` or `FALLBACK_PENALTY`). Every other reference is a test asserting ordering invariants against the `pub const`s. OQ-2 (resolved, architecture) requires individual per-constant exposure **plus** an optional single-multiplier overlay.

### Decision

**1. Engine: an additive, defaulted params struct + a new entry point.**

```rust
// unimatrix-engine/src/graph.rs
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GraphPenaltyParams {
    pub orphan: f64,
    pub clean_replacement: f64,
    pub hop_decay: f64,
    pub partial_supersession: f64,
    pub dead_end: f64,
    pub fallback: f64,
    pub max_traversal_depth: usize,
}
impl Default for GraphPenaltyParams {
    fn default() -> Self {
        Self {
            orphan: ORPHAN_PENALTY,
            clean_replacement: CLEAN_REPLACEMENT_PENALTY,
            hop_decay: HOP_DECAY_FACTOR,
            partial_supersession: PARTIAL_SUPERSESSION_PENALTY,
            dead_end: DEAD_END_PENALTY,
            fallback: FALLBACK_PENALTY,
            max_traversal_depth: MAX_TRAVERSAL_DEPTH,
        }
    }
}

pub fn graph_penalty_with(
    node_id: u64, graph: &TypedRelationGraph,
    entries: &[EntryRecord], params: &GraphPenaltyParams,
) -> f64 { /* current graph_penalty body, consts → params.* */ }

pub fn graph_penalty(node_id: u64, graph: &TypedRelationGraph, entries: &[EntryRecord]) -> f64 {
    graph_penalty_with(node_id, graph, entries, &GraphPenaltyParams::default())
}
```

The `const`s are **retained** as the single source of truth for the defaults (referenced by `Default`) and for the ordering-invariant tests. The clamp lower-bound `0.10` stays a literal in the body (it is a numerical floor, not a tunable severity). `max_traversal_depth` is carried in params (it is a documented lever per Goal 1) but the multiplier never scales it.

**Clamp-ceiling coupling — `clean_replacement` is an AMPLIFIED knob (intended, not incidental).** The hop-decay branch (`graph.rs:530-531`) computes `raw = clean_replacement * hop_decay^(d-1)` then clamps to `[0.10, clean_replacement]`. The **ceiling is `clean_replacement` itself**, by design: the rule is "a multi-hop (depth ≥ 2) replacement is never penalized more harshly than a clean depth-1 replacement of the same kind." Because `hop_decay < 1`, `raw` is always ≤ `clean_replacement` for `d ≥ 2`, so the ceiling is the natural monotonicity cap, not an independent severity. In the parameterized body, `graph_penalty_with` MUST clamp to `[0.10, params.clean_replacement]` (NOT to the `CLEAN_REPLACEMENT_PENALTY` const) — otherwise a swept `clean_replacement` would move the base but leave a stale const ceiling, allowing a depth-2 entry to be clamped *more harshly* than a depth-1 entry and inverting the formula's monotonicity.

Consequence for sweeps (ass-073 attribution): sweeping `clean_replacement` deliberately moves **both** the depth-1 base penalty AND the depth-≥2 clamp ceiling/anchor in the **same direction**. This is an **amplified knob** (one lever, coherent scaling of the whole clean-replacement severity family), **not** a confounded one — base and ceiling never diverge. ass-073 MUST read a `clean_replacement` sweep result as the **amplified** effect of that severity family, not as an isolated single-parameter perturbation. Splitting the ceiling into its own `GraphPenaltyParams` field was considered and **rejected**: an independent ceiling could drop below the base and break the depth-2 ≤ depth-1 monotonicity guarantee. The ceiling is definitionally `clean_replacement` and stays coupled to it.

**2. Server config: a defaulted section.**

```rust
// infra/config.rs
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(default)]
pub struct GraphPenaltyConfig {
    pub orphan: f64,                 // default_orphan()  = 0.75
    pub clean_replacement: f64,      // default_clean()   = 0.40
    pub hop_decay: f64,              // 0.60
    pub partial_supersession: f64,   // 0.60
    pub dead_end: f64,               // 0.65
    pub fallback: f64,               // 0.70
    pub max_traversal_depth: usize,  // 10
    pub multiplier: Option<f64>,     // OQ-2 overlay; default None
}
```

`UnimatrixConfig` gains `#[serde(default)] pub graph_penalty: GraphPenaltyConfig`. **Dual-default discipline (#4064):** each field's `impl Default` value AND its private `default_*()` serde fn MUST both resolve to the matching engine const — both referencing `unimatrix_engine::graph::*` so there is exactly one numeric source of truth and drift is impossible.

**3. Multiplier semantics (OQ-2).** When `multiplier = Some(m)` with `m` in `(0.0, 1.0]`, each *severity* penalty is scaled harsher (toward 0): `effective = base * m` for `orphan, clean_replacement, partial_supersession, dead_end, fallback`. `hop_decay` and `max_traversal_depth` are **not** scaled (shape, not severity). An explicit per-field override in the TOML takes precedence over the multiplier for that field (the multiplier is a convenience overlay applied only to fields left at default, never a replacement for per-constant access). Out-of-range `m` fails `validate()` at startup with a structured error.

**4. Search threading.** `SearchService` gains `graph_penalty_params: GraphPenaltyParams`, resolved once in `with_rate_config` from `config.graph_penalty` (apply multiplier, then per-field overrides). The Flexible loop calls `graph_penalty_with(entry.id, &typed_graph, &all_entries, &self.graph_penalty_params)`; the fallback branch uses `self.graph_penalty_params.fallback`. The resolved params reach search the same way `ppr_alpha` and friends already do.

**5. Enumerated default-equivalence sites (SR-08), tested earliest.** A `default_equivalence` test asserts simultaneously:
- For each of the five status shapes: `graph_penalty_with(.., &Default::default())` == `graph_penalty(..)` == the matching const.
- `GraphPenaltyParams::default()` field-by-field == the engine consts.
- `UnimatrixConfig::default().graph_penalty` each field == the engine const.
- A TOML omitting `[graph_penalty]` deserializes to those same values.
- `SearchService` built from default config yields `graph_penalty_params == GraphPenaltyParams::default()`.

The construction/forwarding sites to wire (the enumerated set): (a) `infra/config.rs` `UnimatrixConfig` field + `GraphPenaltyConfig` `Default` + serde `default_*` fns; (b) `services/search.rs` `SearchService` field + `with_rate_config` resolution + the Flexible-loop call + the fallback branch; (c) `eval/profile/layer.rs` already forwards the whole `config_overrides` to `with_rate_config`, so no eval-specific site is added — the params ride the existing `InferenceConfig`/config plumbing. This list is the SR-08 checklist; the default-equivalence test is its enforcement.

### Consequences

**Easier:** crt-053's steepness sweep becomes a profile-TOML diff (AC-01, AC-14). Per-constant exposure preserves the discovery that one penalty type may need adjusting, not all uniformly (OQ-2). The multiplier gives a one-knob coarse sweep. Ordering-invariant tests are untouched because the consts persist. The default-equivalence test is the cheapest early signal of a missed site.

**Harder:** Two penalty entry points now exist (`graph_penalty`, `graph_penalty_with`) — a delivery agent must route new production calls through `_with` or accept defaults knowingly. The multiplier-vs-per-field precedence rule is a small semantic surface that the Band-2 config-knob reference must document precisely to avoid operator confusion. The Band-2 config-knob reference spec MUST additionally flag `clean_replacement` as an **amplified knob** (it moves both the depth-1 base penalty and the depth-≥2 clamp ceiling together, same direction) so a sweep operator and ass-073 read its result as amplified, not isolated.

**Supersession note:** This partially supersedes crt-014 ADR-006 (#1606) — its "No runtime configurability" stance is corrected for the measurement path. The correction is recorded via `context_correct` on #1606 (not a typed edge), since supersession is a provenance operation. Deployed defaults are unchanged, so the *deployed* behavior crt-014 ADR-006 describes still holds.
