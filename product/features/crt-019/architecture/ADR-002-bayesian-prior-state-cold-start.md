## ADR-002: Bayesian Prior State Management and Cold-Start Behavior

### Context

Change 1 replaces Wilson score with a Bayesian Beta-Binomial posterior mean:
`score = (helpful_count + α₀) / (total_votes + α₀ + β₀)`.

`α₀` and `β₀` are estimated via method of moments from the population of entries with ≥1 vote.
SR-01 flagged that with few voted entries (historically common in Unimatrix), the estimation may
be unstable. SR-02 flagged that `α₀`, `β₀`, `observed_spread`, and `confidence_weight` are all
computed during the same refresh tick and must be consistent with each other.

Three design questions:
1. What threshold of voted entries triggers empirical estimation vs cold-start fallback?
2. How is the state stored and accessed atomically?
3. What are the cold-start defaults?

### Decision

**Cold-start threshold: ≥10 entries with at least one vote.**

Below 10 voted entries, cold-start defaults `α₀ = 3.0, β₀ = 3.0` are used. Above or at 10,
empirical method-of-moments estimation is applied. Rationale: 10 entries provides a stable
population mean. With fewer, the posterior can be dominated by a single unusual entry (e.g. one
entry with 50 helpful votes skews the mean). The threshold is conservative given Unimatrix's
current entry count (192 entries, historically few votes) — the cold-start path is expected to
be active for the foreseeable future.

**Cold-start defaults `α₀ = 3.0, β₀ = 3.0` (symmetric prior, 6 pseudo-votes):**

At zero votes, `score = (0 + 3) / (0 + 6) = 0.50` — identical to the current Wilson neutral
prior. This is a neutral starting position that is responsive immediately: a single helpful vote
yields `score = (1 + 3) / (1 + 6) ≈ 0.57`; a single unhelpful vote yields `≈ 0.43`. Lowering
the dead-weight floor for unvoted entries is achieved through the weight rebalancing
(W_HELP 0.14→0.12), not by making the prior aggressive.

**State location: `ConfidenceState` (see ADR-001).**

All four values `{ alpha0, beta0, observed_spread, confidence_weight }` live in a single
`Arc<RwLock<ConfidenceState>>` on the server side. They are updated atomically — one `write()`
call at the end of each tick covers all four. This ensures that a search call can never read
a `confidence_weight` calibrated to a different `observed_spread` than the one used to compute
the current `alpha0`/`beta0`.

**Method of moments estimation:**

For each entry with ≥1 vote, compute its observed helpfulness rate `p_i = helpful_count_i /
total_votes_i`. Compute the population mean `μ = mean(p_i)` and variance `σ² = var(p_i)`.
Use method of moments on the Beta distribution: `α₀ = μ * (μ*(1-μ)/σ² - 1)`,
`β₀ = (1-μ) * (μ*(1-μ)/σ² - 1)`. Clamp both to [0.5, 50.0] to prevent degeneracy when
variance is near zero (all entries identical helpfulness rate → infinite α₀/β₀).

**Prior stability between ticks:**

`ConfidenceState` is initialized at server startup with cold-start defaults. Reads return these
defaults until the first maintenance tick completes. Between ticks, stale reads are acceptable —
the prior drifts by at most one tick cycle (15 minutes). This is operationally negligible given
that votes accumulate over hours or days.

**Signature implications:**

`compute_confidence(entry, now, alpha0, beta0)` — the calling site snapshots the current
`alpha0`/`beta0` via `ConfidenceState.read()` before the loop and passes them as f64 arguments.
The engine function is pure: same inputs → same output. Test scenarios can pass explicit values.

`UsageService::record_mcp_usage` — the `compute_confidence` closure passed to
`store.record_usage_with_confidence` must capture `alpha0`/`beta0`. The store's
`record_usage_with_confidence` currently takes `Option<&dyn Fn(&EntryRecord, u64) -> f64>`. This
signature must be widened to accept a capturing closure: change to
`Option<Box<dyn Fn(&EntryRecord, u64) -> f64 + Send>>` or adapt via an explicit wrapper type.
This is a localized change inside `store.rs` / `services/usage.rs`.

### Consequences

**Easier:**
- Cold-start defaults produce the same neutral result as the old Wilson guard — no behavioral
  regression for unvoted entries on day one.
- Empirical estimation self-calibrates as votes accumulate; no manual tuning required.
- State consistency is guaranteed by the single `RwLock` update.

**Harder:**
- The `compute_confidence` function signature now carries two additional parameters that must
  be threaded through all call sites.
- The `record_usage_with_confidence` closure signature must be updated to accommodate captured
  state (cannot use a bare function pointer when `alpha0`/`beta0` must be arguments).
- The empirical estimation code path needs its own unit tests to verify method-of-moments
  produces expected values and clamps at boundaries.
- With ≥10 voted entries, `alpha0`/`beta0` change each tick — this means confidence values
  are not purely deterministic from stored fields alone; they also depend on the current tick's
  population statistics. This is acceptable and intentional, but implementers must understand
  that the stored `confidence` value in the DB reflects the tick-time parameters.
