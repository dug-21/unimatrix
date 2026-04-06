## ADR-005: Curation Health Logic Extracted to services/curation_health.rs

### Context

`services/status.rs` is currently 3537 lines (verified by wc -l). The 500-line cap in
the Rust workspace rules applies per file. The file already exceeds this limit and exists
as a large service with many phases. Adding Phase 7c inline would add approximately
150-200 lines of computation logic and new types, further embedding curation concerns in
an already large file.

SR-06 recommends pre-planning extraction rather than reacting to the limit mid-implementation.
The curation health types and computation logic (`CurationSnapshot`, `CurationBaseline`,
baseline window computation, trend computation) are self-contained and do not depend on
any other status phase's state.

The `unimatrix_observe::baseline` crate already establishes the pattern for standalone
baseline computation modules. Curation health follows the same structural pattern: a set
of types, a pure computation function over a data window, and a threshold constant.

### Decision

Create `crates/unimatrix-server/src/services/curation_health.rs` as a new module.

**Exported from `curation_health.rs`**:
- `pub struct CurationSnapshot` — raw per-cycle counts
- `pub struct CurationBaselineRow` — slim projection from `CycleReviewRecord` for baseline
- `pub struct CurationBaseline` — per-metric mean/stddev
- `pub struct CurationBaselineComparison` — σ position, history count, anomaly flag
- `pub struct CurationHealthSummary` — aggregate for `context_status`
- `pub struct CurationHealthBlock` — `context_cycle_review` output container
- `pub enum TrendDirection` — `Increasing`, `Decreasing`, `Stable`
- `pub const CURATION_SIGMA_THRESHOLD: f64 = 1.5`
- `pub const CURATION_MIN_HISTORY: usize = 3`
- `pub const CURATION_MIN_TREND_HISTORY: usize = 6`
- `pub async fn compute_curation_snapshot(store: &SqlxStore, feature_cycle: &str, cycle_start_ts: i64, review_ts: i64) -> Result<CurationSnapshot, ServiceError>`
- `pub fn compute_curation_baseline(rows: &[CurationBaselineRow], n: usize) -> Option<CurationBaseline>`
- `pub fn compare_to_baseline(snapshot: &CurationSnapshot, baseline: &CurationBaseline, history_count: usize) -> CurationBaselineComparison`
- `pub fn compute_trend(rows: &[CurationBaselineRow]) -> Option<TrendDirection>`
- `pub fn compute_curation_summary(rows: &[CurationBaselineRow]) -> Option<CurationHealthSummary>`

**`services/status.rs` Phase 7c** (minimal addition):

```rust
// Phase 7c: curation health (crt-047)
let curation_window = store
    .get_curation_baseline_window(CURATION_BASELINE_WINDOW)
    .await
    .unwrap_or_default();
let curation_health = curation_health::compute_curation_summary(&curation_window);
```

**`CURATION_BASELINE_WINDOW: usize = 10`** stays in `services/status.rs`, consistent with
`PENDING_REVIEWS_K_WINDOW_SECS` location (SCOPE.md Constraints).

**`services/mod.rs`** must expose `pub mod curation_health`.

The `compute_curation_snapshot()` function is called from `context_cycle_review` in
`tools.rs`, not from `status.rs`. The separation of concerns is:

- `curation_health.rs`: all types and pure compute functions
- `cycle_review_index.rs`: all store I/O (reads and writes)
- `tools.rs` (`context_cycle_review`): orchestrates compute_snapshot + store_cycle_review
- `status.rs` Phase 7c: calls get_curation_baseline_window + compute_curation_summary

### Consequences

- **Easier**: `status.rs` receives approximately 15-20 lines of Phase 7c code rather than
  150-200. The file grows minimally.
- **Easier**: Curation health types and logic are unit-testable in isolation without
  standing up the full StatusService.
- **Easier**: Future per-topic σ baselines (SCOPE.md non-goal) can be added to
  `curation_health.rs` without modifying `status.rs`.
- **Harder**: One additional file to maintain. The `services/mod.rs` module declaration
  must be updated.
- **Consequence**: `compute_curation_snapshot()` has an async signature (it runs SQL) and
  lives in the server crate. The pure functions (`compute_curation_baseline`, etc.) are
  synchronous and could potentially move to `unimatrix-observe` in a future refactor. They
  stay in the server crate for now to avoid cross-crate changes in this feature.
