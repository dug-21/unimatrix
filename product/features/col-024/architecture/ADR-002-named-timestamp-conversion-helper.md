## ADR-002: Named Conversion Helper for Timestamp Unit Mismatch

### Context

`cycle_events.timestamp` is stored as `INTEGER` representing Unix epoch **seconds**
(confirmed: `handle_cycle_event` writes `unix_now_secs() as i64`). The
`observations.ts_millis` column is stored as Unix epoch **milliseconds**.

The three-step `load_cycle_observations` algorithm compares cycle-event timestamps
against observation timestamps as window boundaries. Without an explicit conversion,
a raw `* 1000` literal will appear somewhere in the query-construction code. This
is a silent high-severity correctness bug (SR-01) that is invisible to the type
system: both are `i64`, both compile, both produce wrong results if the literal
is accidentally omitted or duplicated.

Existing code in `observation.rs` performs comparable arithmetic (e.g., `45_i64 *
86_400_000` for day-to-ms conversion in `observation_stats`) using inline literals
with no naming. The cycle_events boundary conversion is higher-stakes because an
off-by-1000 error silently drops all observations from a feature cycle rather than
producing a compile error.

### Decision

Introduce a module-private named helper function in `services/observation.rs`:

```rust
/// Convert a cycle_events.timestamp (Unix epoch seconds) to the millisecond
/// unit used by observations.ts_millis.
///
/// cycle_events.timestamp is written by unix_now_secs() (i64 seconds).
/// observations.ts_millis is written by (unix_now_secs() as i64).saturating_mul(1000).
/// Both tables use the same epoch; this function bridges the unit difference.
#[inline]
fn cycle_ts_to_obs_millis(ts_secs: i64) -> i64 {
    ts_secs.saturating_mul(1000)
}
```

All comparisons between cycle-event timestamps and observation timestamps must go
through this function. No raw `* 1000` multiplication is permitted in
`load_cycle_observations` or any helper it calls.

`saturating_mul` prevents i64 overflow on adversarially large timestamp values,
consistent with the existing `(event.timestamp as i64).saturating_mul(1000)` usage
in `extract_observation_fields`.

### Consequences

- Easier: the unit conversion has one definition; any future search for "seconds
  to millis" finds it immediately. A future reader of the query-building code sees
  `cycle_ts_to_obs_millis(ts)` and understands the intent without inspecting the
  magnitude of the literal.
- Easier: if a future schema migration changes `cycle_events.timestamp` to
  milliseconds, the fix is in exactly one place.
- Harder: one additional indirection; negligible at runtime (inlined).
- No change to `observations.ts_millis` write path or `cycle_events.timestamp`
  write path — both remain as-is.
