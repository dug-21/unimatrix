## ADR-001: Three-Dimension Lambda Weights (crt-048)

Supersedes: Unimatrix entry #179 (ADR-003, crt-005: Lambda Dimension Weighting Strategy)

### Context

Lambda, the composite coherence metric, previously carried four dimensions with weights
confidence_freshness=0.35, graph_quality=0.30, contradiction_density=0.20,
embedding_consistency=0.15. The freshness dimension scored entries by wall-clock age:
an entry was stale if `max(updated_at, last_accessed_at)` was older than 24 hours.

crt-036 replaced wall-clock retention with cycle-based retention. Entries now survive or
deprecate based on feature cycle history, not access recency. This invalidated the 24h
staleness proxy: long-lived ADRs and conventions were correctly retained by cycle-based
logic but incorrectly penalized by Lambda's freshness dimension. The observed result was
Lambda trending toward zero for structural reasons unrelated to actual knowledge quality
(stale_entries growing ~84/day while graph_quality was simultaneously improving).

The crt-005 ADR-003 justification was "confidence freshness directly affects every search
query." Post-crt-036, this no longer holds: cycle-based retention ensures entries with no
learning value are deprecated; surviving entries are maintained by definition.

Three candidate approaches were evaluated in GH #425 (cycle-anchored freshness,
freeze-aware dampening, two-speed decay). All three attempt to recalibrate the time
constant. GH #520 owner decision chose a fourth option: drop the time dimension entirely.

### Decision

Remove `confidence_freshness` as a Lambda input. Lambda becomes a 3-dimension structural
integrity metric: graph quality, contradiction density, embedding consistency.

Re-normalized weights, derived by proportional scaling of the original 0.30:0.20:0.15
ratio (preserving the 2:1.33:1 structural relationship):

  graph_quality:           0.30 / 0.65 = 0.4615... → **0.46**
  contradiction_density:   0.20 / 0.65 = 0.3077... → **0.31**
  embedding_consistency:   0.15 / 0.65 = 0.2308... → **0.23**
  Sum: 0.46 + 0.31 + 0.23 = **1.00** (exact)

These are the f64 literal values in `DEFAULT_WEIGHTS`. While 1.0 is exactly
representable in IEEE 754 binary64, the individual literals 0.46, 0.31, and 0.23 are
not binary-exact, so their sum as f64 may not be bitwise equal to 1.0. The
`lambda_weight_sum_invariant` test uses `(total - 1.0_f64).abs() < f64::EPSILON` to
guard against this.

The `embedding_consistency` dimension remains optional (excluded and remaining weights
re-normalized when `embed_dim` is `None`). With freshness removed, the 2-of-3
re-normalization when embedding is absent is:
  graph_quality / (0.46 + 0.31) = 0.46 / 0.77 ≈ 0.5974
  contradiction_density / 0.77 = 0.31 / 0.77 ≈ 0.4026

All three surviving dimensions are structural and domain-neutral: they measure graph
integrity, semantic contradiction, and embedding alignment — none embed a cadence
assumption about how frequently the platform is used.

`compute_lambda()` signature becomes:
```rust
pub fn compute_lambda(
    graph_quality: f64,
    embedding_consistency: Option<f64>,
    contradiction_density: f64,
    weights: &CoherenceWeights,
) -> f64
```

`generate_recommendations()` signature becomes:
```rust
pub fn generate_recommendations(
    lambda: f64,
    threshold: f64,
    graph_stale_ratio: f64,
    embedding_inconsistent_count: usize,
    total_quarantined: u64,
) -> Vec<String>
```

`CoherenceWeights` struct becomes:
```rust
pub struct CoherenceWeights {
    pub graph_quality: f64,
    pub embedding_consistency: f64,
    pub contradiction_density: f64,
}
```

### Consequences

Easier:
- Lambda increases monotonically as structural health improves, not as clock time advances.
  Long-lived ADRs and conventions no longer drag Lambda toward zero.
- Per-source coherence (`coherence_by_source`) becomes genuinely diagnostic: each trust
  source is scored on structural health alone, eliminating the recency-of-write bias.
- Lambda is domain-neutral and cadence-agnostic: platforms with non-daily-cadence access
  produce honest health signals.
- Three surviving dimensions are all independently verifiable by operators.
- A future cycle-relative freshness dimension remains possible as a 4th dimension in a
  separate feature (data is available via `cycle_review_index`).

Harder:
- `confidence_freshness_score` and `stale_confidence_count` disappear from all three
  output formats (text, markdown, JSON). Operators with custom scripts parsing these JSON
  fields will see the fields absent after upgrade. Release notes must document the removal.
- Lambda values from before and after this feature are not directly comparable: the scale
  shifts because the heaviest weight (0.35) is removed.
- The `staleness_threshold_constant_value` test is deleted; the constant itself survives
  only for `run_maintenance()`. Future contributors must read the comment to understand why.
