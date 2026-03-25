## ADR-002: `cycle_ts_to_obs_millis` Is a Named Mandatory Dependency for PhaseStats

### Context

SR-01 in SCOPE-RISK-ASSESSMENT.md identifies a high-likelihood, high-severity risk: the
`PhaseStats` computation must filter observations by phase time windows, which requires
converting `cycle_events.timestamp` (epoch seconds, `i64`) to the millisecond unit used by
`observations.ts_millis` (`i64`). If an implementation agent writes `ts_secs * 1000` inline
rather than using the existing helper, they introduce a silent semantic error with no type-system
guard — the same bug col-024 ADR-002 (#3372) was designed to prevent.

`cycle_ts_to_obs_millis(ts_secs: i64) -> i64` already exists in
`crates/unimatrix-server/src/services/observation.rs` at line 495. It uses `saturating_mul(1000)`
to guard against `i64::MAX` overflow (E-05 edge case, tested in T-LCO-09). This guard is
meaningful; inline multiplication does not provide it.

The function is currently `fn` (module-private). PhaseStats computation in the handler is in
the same file, so visibility is not a problem for the handler. However, if `compute_phase_stats`
is extracted to a separate module (e.g., `mcp/phase_stats.rs`), the function must be made
`pub(crate)`.

### Decision

1. `cycle_ts_to_obs_millis` from `crates/unimatrix-server/src/services/observation.rs` is the
   **only permitted conversion path** from cycle_events seconds to observation milliseconds in
   PhaseStats computation code.

2. All inline `* 1000` multiplications for timestamp conversion in PhaseStats code are
   prohibited. Implementation agents must call `cycle_ts_to_obs_millis(ts)`.

3. If `compute_phase_stats` is extracted to a module separate from `observation.rs`, add
   `pub(crate)` to `cycle_ts_to_obs_millis`. No other visibility change is required.

4. The spec must include a test case for PhaseStats where the phase boundary is verified at a
   known millisecond-level timestamp, catching any inline conversion regression.

This ADR cross-references col-024 ADR-002 (#3372) which established the same rule for
`load_cycle_observations`.

### Consequences

Easier:
- Saturating-mul overflow safety inherited automatically.
- Code search for `* 1000` in PhaseStats code is a complete lint for violations.
- Same reasoning as col-024 ADR-002 — consistent pattern across the codebase.

Harder:
- `pub(crate)` visibility change required if code is extracted to a separate module.
- Implementation agents must be told explicitly: "do not multiply, call the function."
