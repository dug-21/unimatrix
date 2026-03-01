# col-002b Acceptance Criteria Map

| AC-ID | Description | Verification Method | Verification Detail | Status |
|-------|-------------|--------------------|--------------------|--------|
| AC-01 | All 7 agent hotspot rules implemented with bootstrapped thresholds | test | Unit test per rule: context_load, lifespan, file_breadth, reread_rate, mutation_spread, compile_cycles, edit_bloat — each with synthetic records above and below threshold | PENDING |
| AC-02 | Both remaining friction hotspot rules implemented with bootstrapped thresholds | test | Unit test per rule: search_via_bash, output_parsing_struggle — each with synthetic records | PENDING |
| AC-03 | All 4 remaining session hotspot rules implemented with bootstrapped thresholds | test | Unit test per rule: cold_restart, coordinator_respawns, post_completion_work, rework_events — each with synthetic records | PENDING |
| AC-04 | All 5 scope hotspot rules implemented with bootstrapped thresholds | test | Unit test per rule: source_file_count, design_artifact_count, adr_count, post_delivery_issues, phase_duration_outlier — each with synthetic records | PENDING |
| AC-05 | Each rule includes evidence records (concrete tool call data that triggered detection) | test | For each rule's unit test: assert finding.evidence is non-empty, assert evidence contains tool name and timestamp | PENDING |
| AC-06 | Each rule is independently testable with unit tests using synthetic record data | test | Each rule has its own `#[cfg(test)] mod tests` with at least: fires_above_threshold, silent_below_threshold, handles_empty_records | PENDING |
| AC-07 | All 18 new rules register into the existing hotspot framework without modifying engine core | test | `default_rules(None)` returns 21 rules; `detect_hotspots(records, &default_rules(None))` runs all without panic | PENDING |
| AC-08 | Baseline computation produces per-metric mean and standard deviation from stored MetricVectors | test | `compute_baselines()` with 3 known MetricVectors — verify mean and stddev for total_tool_calls, session_count match expected values | PENDING |
| AC-09 | Phase-specific baselines computed per phase name (not aggregated across phases) | test | History with phases "3a" (durations: 100, 200, 300) and "3b" (durations: 50, 60, 70) — verify separate mean/stddev per phase | PENDING |
| AC-10 | Metrics exceeding mean + 1.5 stddev flagged as statistical outliers in the comparison table | test | BaselineEntry(mean=100, stddev=20), current=140 (outlier), current=120 (normal) — verify is_outlier flag | PENDING |
| AC-11 | Baseline comparison requires minimum 3 stored MetricVectors; report notes "insufficient history" with fewer | test | `compute_baselines()` with 0, 1, 2 vectors returns None; with 3 returns Some | PENDING |
| AC-12 | Comparison table included in `context_retrospective` report when baseline data is available | test | Integration test: store 3 MetricVectors, run retrospective, verify `report.baseline_comparison.is_some()` | PENDING |
| AC-13 | Phase duration outlier rule uses baseline data when available (3+ data points), falls back to absolute threshold otherwise | test | PhaseDurationOutlierRule with history (3+ entries for phase "3a") uses 2x mean; without history uses absolute | PENDING |
| AC-14 | No changes to MetricVector structure, OBSERVATION_METRICS table schema, or hook infrastructure | grep | `git diff` on MetricVector struct, OBSERVATION_METRICS table definition, and hook scripts shows no changes | PENDING |
| AC-15 | No new MCP tools or tool parameters — enhances existing `context_retrospective` response | grep | No new `#[tool]` annotations or tool registration in server crate | PENDING |
| AC-16 | All existing tests pass with no regressions (col-002 pipeline tests, store tests, server tests) | test | `cargo test --workspace` — zero failures | PENDING |
| AC-17 | Unit tests cover: each of the 18 detection rules, baseline mean/stddev computation, phase-specific baseline grouping, minimum history check, outlier flagging | test | 18 rule test modules + baseline module tests — all present and passing | PENDING |
| AC-18 | Integration tests cover: full retrospective with all rules active, retrospective with baseline comparison | test | Server integration test: write synthetic JSONL + 3 MetricVectors, call context_retrospective, verify hotspots and baseline in report | PENDING |
| AC-19 | `#![forbid(unsafe_code)]` maintained on `unimatrix-observe` | grep | `grep 'forbid(unsafe_code)' crates/unimatrix-observe/src/lib.rs` returns match | PENDING |
| AC-20 | No new crate dependencies | grep | `git diff crates/unimatrix-observe/Cargo.toml` and `crates/unimatrix-server/Cargo.toml` show no new `[dependencies]` entries | PENDING |
