## ADR-002: max_cycles_per_tick Cap in RetentionConfig, Not InferenceConfig

### Context

The GC pass iterates over purgeable cycles. After a long period without GC (e.g., a
new deployment that inherits 200 old cycles), the purgeable set could be very large.
Without a cap, the first tick after deployment would attempt to prune all 200 cycles
in sequence, each cycle requiring its own transaction and write-pool acquisition round.
This would dominate the background tick budget.

A configurable cap on how many cycles are pruned per tick is needed. The question is:
which config struct should own this field?

**Option A — Add `max_cycles_per_tick` to `InferenceConfig`.**
`InferenceConfig` is already the home for background tick batch caps:
`max_graph_inference_per_tick` (crt-029), `max_co_access_promotion_per_tick` (crt-034),
`heal_pass_batch_size` (bugfix-444). Following this precedent would keep all tick-rate
controls in one place.

Problem: `InferenceConfig` governs ML inference settings — NLI thresholds, fusion
weights, graph inference, PPR parameters, and the heal pass. Its cohesion is
"inference pipeline tuning." Adding a GC throughput cap is conceptually mismatched:
operators who want to tune how fast old cycle data is purged are not the same operators
who tune NLI thresholds or fusion weights. The two concerns belong to different
operator profiles.

Additionally, procedure entry #3911 (run_maintenance procedure) states: "Add a
configurable batch cap to InferenceConfig" for passes that iterate over entries with
ML involvement. The cycle GC is pure SQL with no ML inference cost; the `InferenceConfig`
guideline does not apply to it.

A third issue: `InferenceConfig` is passed as `&Arc<InferenceConfig>` through the
background tick chain. Adding a GC-specific field to it means every caller of
`run_maintenance()` — including tests that construct minimal `InferenceConfig::default()`
values — needs to be aware of a retention concept that does not belong to inference.

**Option B — Add `max_cycles_per_tick` to a new `RetentionConfig`.**
`RetentionConfig` is being added to `UnimatrixConfig` to hold the two core retention
parameters: `activity_detail_retention_cycles` and `audit_log_retention_days`. The
`max_cycles_per_tick` cap is a retention throughput knob — it directly governs how
fast the retention window advances. It belongs in the same struct.

This keeps all GC-related knobs in one operator-visible section (`[retention]` in
`config.toml`), separate from ML inference parameters.

**Decision: Option B.**

`max_cycles_per_tick: u32` is added to `RetentionConfig`. The default is 10.

Rationale for default 10:
- A typical deployment accumulates 1–5 new reviewed cycles per week.
- Pruning 10 cycles per tick means a backlog of up to 200 cycles is cleared in ~20
  background ticks (~5 hours at the 15-minute tick interval). This is fast enough to
  prevent unbounded growth but slow enough to avoid tick-budget monopolization.
- Each purgeable cycle's DELETE on a 152 MB table is bounded by the
  `idx_observations_session` index — actual lock hold time per cycle is expected to be
  milliseconds for a typical observations-per-cycle count. 10 cycles per tick is
  therefore conservative.
- Operators can raise this to 100 for large catch-up scenarios via `config.toml`.

`RetentionConfig::validate()` enforces `max_cycles_per_tick` in [1, 1000].

### Decision

Add `max_cycles_per_tick: u32` to `RetentionConfig` with `#[serde(default = "default_max_cycles_per_tick")]` where `default_max_cycles_per_tick()` returns `10`. Expose in `[retention]` TOML block. Add to `RetentionConfig::validate()` with range [1, 1000].

The complete `RetentionConfig` struct has three fields:
```rust
pub struct RetentionConfig {
    pub activity_detail_retention_cycles: u32,  // default 50, range [1, 10000]
    pub audit_log_retention_days: u32,           // default 180, range [1, 3650]
    pub max_cycles_per_tick: u32,                // default 10, range [1, 1000]
}
```

`run_maintenance()` adds `retention_config: &RetentionConfig` as a parameter. It is
passed from `run_single_tick()` which receives it as `&Arc<RetentionConfig>`, following
the identical threading pattern established for `inference_config: &Arc<InferenceConfig>`.

Config is loaded once at startup and never re-read from disk inside `run_maintenance()`,
satisfying SR-09.

### Consequences

Easier:
- All three GC knobs are visible together in the `[retention]` TOML section.
  Operators tuning retention behavior see a cohesive set of parameters.
- `InferenceConfig` remains cohesive around inference concerns.
- Adding future retention-related parameters (e.g., a per-table row cap, a minimum
  retained-cycle floor) belongs naturally in `RetentionConfig`.
- Tests that validate GC behavior construct a `RetentionConfig` and do not need a
  full `InferenceConfig`.

Harder:
- `run_maintenance()` acquires a second config parameter alongside `inference_config`.
  Call sites in `background.rs` and tests must thread `RetentionConfig` through.
  This is mechanical and follows an established pattern.
