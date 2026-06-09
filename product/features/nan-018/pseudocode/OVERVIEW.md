# nan-018 Pseudocode — OVERVIEW

The *instrument*: tunability + trust/cost metrics + durable fixture corpus + drift guard,
delivered in two waves. Wave-1 = instrument core (AC-01…AC-09 + AC-14 exit). Wave-2 =
docs + forward-discipline (AC-10…AC-13), **zero code coupling to Wave-1** (NFR-04).

All names/types below are taken from the IMPLEMENTATION-BRIEF Integration Surface — do not invent.

## Components (file → wave)

| File | Component | Wave |
|------|-----------|------|
| `penalty-config.md` | `GraphPenaltyConfig` + `UnimatrixConfig.graph_penalty` (server `infra/config.rs`) | 1 |
| `engine-penalty.md` | `GraphPenaltyParams` + `graph_penalty_with` (engine `graph.rs`) | 1 |
| `search-threading.md` | `SearchService.graph_penalty_params` + `with_rate_config` (server `services/search.rs`) | 1 |
| `trust-metric.md` | `eval/runner/trust.rs`, `ExpectedAssertions`, `TrustOutcome`, `evaluate_trust` | 1 |
| `cost-metric.md` | `eval/runner/cost.rs`, `token_proxy`, per-profile cost sum | 1 |
| `corpus-loader.md` | `eval/corpus/{loader,assertions}.rs`, alias→id map, property resolution | 1 |
| `corpus-fixtures.md` | `eval/corpus/fixtures/` TOML entry-graphs + manifest stamp | 1 |
| `shape-hash.md` | `eval/shape/` ordered manifest, hash, drift-guard compare | 1 |
| `report-extensions.md` | `find_regressions` trust-flip + cost-growth; `ProfileResult` plumbing | 1 |
| `docs.md` | Band-1/2 docs under `docs/testing/` | 2 |
| `band3-recommendation.md` | Band-3 recommendation doc + Unimatrix convention/procedure | 2 |

## Data flow (Wave-1)

```
profile TOML --deser--> UnimatrixConfig.graph_penalty: GraphPenaltyConfig (serde defaults = consts)
                              | resolve_params() (multiplier overlay, per-field override wins)
                              v
                  GraphPenaltyParams (Copy; engine struct)  --stored once-->  SearchService.graph_penalty_params
                                                                                       |
fixture corpus TOML --loader--> snapshot DB + alias_map  --EvalServiceLayer::from_profile-->  TypedGraphState
                                                                                       |
                         running schema --shape::compute_hash--> compare to manifest stamp (drift guard)
                                                                                       |
                                                              SearchService.search() Flexible loop:
                                                                :727 fallback -> params.fallback
                                                                :729 normal   -> graph_penalty_with(.., &params)
                                                                                       v
                                                                          ScoredEntry list (Vec<ScoredEntry>)
                                                                  +--> existing metrics: P@K, MRR, CC@k, ICD, latency
                                                                  +--> evaluate_trust(entries, assertions, alias_map) -> TrustOutcome
                                                                  +--> cost_tokens = Σ token_proxy(result)
                                                                                       v
                                                              ProfileResult { ..., cost_tokens, trust }
                                                                                       v
                                              find_regressions(...) : OR-extend with trust-flip + cost-growth (ε=0.0 advisory)
                                                              report (exit-code semantics UNCHANGED, R-17)
```

## Shared types (Integration Surface — exact)

```rust
// unimatrix-engine/src/graph.rs (NEW)
pub struct GraphPenaltyParams {           // Copy; Default = engine consts
    pub orphan: f64, pub clean_replacement: f64, pub hop_decay: f64,
    pub partial_supersession: f64, pub dead_end: f64, pub fallback: f64,
    pub max_traversal_depth: usize,
}

// infra/config.rs (NEW)  #[serde(default)]
pub struct GraphPenaltyConfig {           // mirrors params + overlay
    pub orphan: f64, pub clean_replacement: f64, pub hop_decay: f64,
    pub partial_supersession: f64, pub dead_end: f64, pub fallback: f64,
    pub max_traversal_depth: usize, pub multiplier: Option<f64>,
}
// UnimatrixConfig gains: #[serde(default)] pub graph_penalty: GraphPenaltyConfig

pub type EntryRef = String;               // corpus alias, e.g. "chainA.head"
pub struct ExpectedAssertions {
    pub redirect_to_head: Vec<EntryRef>,
    pub forbidden_absent: Vec<EntryRef>,
    pub rank_below: Vec<(EntryRef, EntryRef)>,
}
// ScenarioRecord gains: pub assertions: Option<ExpectedAssertions>  (additive; existing `expected: Option<Vec<u64>>` untouched)

pub struct TrustOutcome { pub absence_pass: bool, pub rank_pass: bool, pub violations: Vec<String> }
// ProfileResult ADDS: pub cost_tokens: f64, pub trust: TrustOutcome

// corpus manifest stamp (TOML alongside fixtures)
//   manifest_version = 1   migration_number = 47 (legibility only, NOT hashed)   shape_hash = "<64-hex>"
```

**Penalty const defaults** (`graph.rs:41-59,531`): `ORPHAN_PENALTY=0.75`,
`CLEAN_REPLACEMENT_PENALTY=0.40`, `HOP_DECAY_FACTOR=0.60`, `PARTIAL_SUPERSESSION_PENALTY=0.60`,
`DEAD_END_PENALTY=0.65`, `FALLBACK_PENALTY=0.70`, `MAX_TRAVERSAL_DEPTH=10`; clamp `[0.10, clean_replacement]`.

## Load-bearing invariants (every component must honor)

1. **Two penalty sites, both in `services/search.rs`** — `:727` fallback (→ `params.fallback`),
   `:729` normal (→ `graph_penalty_with`). `background.rs:583` is a `tracing::error!` log
   string and is NOT touched.
2. **Clamp coupling (ADR-001)** — `graph_penalty_with` clamps to `[0.10, params.clean_replacement]`,
   NOT to the const. Ceiling tracks the swept value. Lower bound `0.10` is a literal.
   `clean_replacement` is an *amplified* knob (moves base + ceiling together).
3. **Dual-default discipline (#4064)** — for every lever, the serde `default_*()` fn AND the
   `Default` impl BOTH resolve to the engine const. One numeric source of truth (the engine const).
4. **Bit-for-bit default equivalence (NFR-01)** — `graph_penalty(..)` == `graph_penalty_with(.., &Default::default())`
   == const, for every shape branch and the clamp.
5. **Property assertions only in the primary corpus (C-04)** — no literal-ID, no null `expected`;
   loader rejects both. Aliases resolved at load time.
6. **Vacuous-pass guards (R-11)** — rank-below: A absent ⇒ pass, B absent (A present) ⇒ FAIL.
7. **Hash determinism (NFR-03)** — ordered/versioned manifest, sorted vectors, fixed float format.
8. **Severity split (R-13)** — primary corpus hash mismatch = HARD ERROR (abort); snapshot = WARN.
9. **Exit-code invariance (R-17)** — adding trust/cost to `find_regressions` does NOT change
   `eval report` exit code (body-only reporting).
10. **Wave independence (NFR-04)** — Wave-1 acceptance passes with zero Wave-2 artifacts present.

## Sequencing constraints

- Engine penalty (`engine-penalty.md`) is the source of truth for default consts; build first —
  `penalty-config.md` references its consts, `search-threading.md` consumes `GraphPenaltyParams`.
- `corpus-loader.md` produces the alias_map that `trust-metric.md` consumes; loader before trust eval.
- `shape-hash.md` and `corpus-fixtures.md` are coupled: the fixtures carry the stamp the shape module verifies.
- `report-extensions.md` consumes `TrustOutcome` + `cost_tokens`; build after trust + cost.
- Wave-2 (`docs.md`, `band3-recommendation.md`) reference Wave-1 behavior only conceptually; no code import.

## Open questions / gaps flagged

- **R-04 column manifest completeness** is a NAMED HUMAN delivery gate, not codeable. `shape-hash.md`
  enumerates the *declared* manifest; a test proves sensitivity to the declared set only. The human
  certifies the set is complete. Pseudocode flags this; it cannot close it.
- **`ConfidenceWeights`/`ConfidenceParams` field set** (hash manifest input 3) — the exact dimension
  names must be read from the live struct at delivery. `shape-hash.md` enumerates the input *category*
  and the access pattern; the concrete field list is a delivery read against the live type (flagged).
- **`RetrievalMode::Flexible` fallback resolution** — `use_fallback` is computed upstream of :727;
  pseudocode assumes the resolved `self.graph_penalty_params.fallback` replaces the `FALLBACK_PENALTY`
  const at :727 without altering the `use_fallback` predicate. Confirmed against source (search.rs:724-733).

## Knowledge Stewardship

- Queried (consulted before/while authoring this Stage-3a pseudocode):
  - #4897 (ADR-001 penalty/clamp coupling) — fixed the clamp ceiling to `params.clean_replacement` (not the const) in engine-penalty.md and Load-bearing invariant #2; drove `clean_replacement` as the amplified knob that moves base + ceiling together.
  - #4895 (ADR-002 hash manifest) — shaped shape-hash.md: ordered/versioned manifest, embed-model-in-hash, SHA-256, severity split (primary = HARD ERROR, snapshot = WARN); informs invariants #7 and #8.
  - #4896 (ADR-003 token-proxy) — defined cost-metric.md `token_proxy` (token-weighted, subword tier + word×1.3 fallback, char/4 rejected) and the proxy labeling in ProfileResult.
  - #4075 (search-pipeline submodule convention) — justified the per-file decomposition under `eval/` (loader, assertions, trust, cost, shape) so no implementation file exceeds the 500-line limit.
  - #4888 (positive-relevance baseline) — informed trust-metric.md asymmetric semantics so rank/absence assertions extend, not replace, existing P@K/MRR positive-relevance metrics.
  - #4064 (dual-default trap) — drove invariant #3: both the serde `default_*()` fn and the `Default` impl resolve to one engine-const source of truth; guards the empty-TOML byte-identity tests.
  - #2610 (HashMap eval non-determinism) — drove invariant #7 (sorted vectors, fixed float format, ordered manifest) so the shape hash is reproducible in-process and cross-process.
- Stored: nothing novel. Every artifact here is a single-feature instance of an already-recorded ADR/pattern (the seven entries above); no new cross-feature pattern, procedure, or lesson emerged at pseudocode time. Reassess at retro once delivery confirms the multiplier override-detection mechanism (OQ #3) and the R-04 manifest column set, which may yield a reusable lesson.
- Deviations from established patterns: none.
