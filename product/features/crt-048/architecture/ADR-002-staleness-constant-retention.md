## ADR-002: Retain DEFAULT_STALENESS_THRESHOLD_SECS (crt-048)

### Context

`DEFAULT_STALENESS_THRESHOLD_SECS = 86400` (24 hours) was introduced in crt-005 as the
threshold for the Lambda freshness dimension: an entry was considered stale for Lambda
purposes if `max(updated_at, last_accessed_at)` was older than this value. crt-048
removes the Lambda freshness dimension entirely, eliminating three of the four call sites
for this constant:

  services/status.rs line 698: confidence_freshness_score(..., DEFAULT_STALENESS_THRESHOLD_SECS)  [REMOVED]
  services/status.rs line 769: oldest_stale_age(..., DEFAULT_STALENESS_THRESHOLD_SECS)            [REMOVED]
  services/status.rs line 796: confidence_freshness_score(..., DEFAULT_STALENESS_THRESHOLD_SECS) per-source [REMOVED]

One call site survives in `run_maintenance()`:

  services/status.rs line 1242: let staleness_threshold = coherence::DEFAULT_STALENESS_THRESHOLD_SECS;

This is inside the background tick's confidence refresh path. `run_maintenance()` identifies
entries with stale confidence scores (for re-computation via the Wilson-score pipeline), not
for Lambda. The staleness threshold governs which entries are eligible for confidence
re-computation; it has no relationship to Lambda's structural dimensions.

SCOPE.md Goal 7 says "remove DEFAULT_STALENESS_THRESHOLD_SECS if no other caller." The
SCOPE.md Implementation Notes section (written after the goals) corrects this: the constant
has a surviving caller and must not be removed. This ADR encodes the resolution so the
constraint cannot be missed during implementation.

SR-03 (High risk in SCOPE-RISK-ASSESSMENT.md) identified exactly this trap: an implementer
reading Goal 7 without the Implementation Notes would delete the constant, causing
`run_maintenance()` to silently compile with a hardcoded literal or fail to compile.

### Decision

`DEFAULT_STALENESS_THRESHOLD_SECS` is retained in `infra/coherence.rs` with an updated
doc comment:

```rust
/// Staleness threshold for confidence refresh: 24 hours in seconds.
///
/// Used by run_maintenance() in services/status.rs to identify entries eligible
/// for confidence score re-computation. NOT a Lambda input — the Lambda freshness
/// dimension was removed in crt-048.
pub const DEFAULT_STALENESS_THRESHOLD_SECS: u64 = 24 * 3600;
```

The constant value (86400) is unchanged. The `staleness_threshold_constant_value` test
in `infra/coherence.rs` is deleted along with the other freshness tests — it existed
to guard the Lambda use. The constant itself is verified implicitly by `run_maintenance()`
continuing to compile and function correctly.

### Consequences

Easier:
- `run_maintenance()` confidence refresh continues to work without modification.
- The surviving call site is explicit and documented; future contributors cannot confuse
  the refresh threshold with a Lambda weight.
- No config migration required: `[inference] freshness_half_life_hours` is a separate
  constant in a separate module (confidence scoring pipeline, not Lambda).

Harder:
- The constant remains in `infra/coherence.rs` despite the module's primary purpose
  (Lambda computation) no longer using it. A future refactor may want to move it to a
  more appropriate home (e.g., `services/maintenance.rs`), but that is out of scope.
- The deleted test (`staleness_threshold_constant_value`) was a named constant guard.
  If someone changes the value in the future, there is no test catching it. This is
  acceptable: the value is determined by the confidence refresh cadence, not by Lambda.
