# Pseudocode Overview: col-002b Detection Library + Baseline Comparison

## Components

| Component | Purpose | Files |
|-----------|---------|-------|
| detection-agent | 7 agent hotspot rules | `detection/agent.rs` |
| detection-friction | 2 new + 2 existing friction rules | `detection/friction.rs` |
| detection-session | 4 new + 1 existing session rules | `detection/session.rs` |
| detection-scope | 5 scope hotspot rules | `detection/scope.rs` |
| baseline | Baseline computation and comparison | `baseline.rs` |
| server-integration | context_retrospective enhancement | `tools.rs` (unimatrix-server) |

## Data Flow

```
ObservationRecord[] -----> detect_hotspots(records, rules) -----> HotspotFinding[]
                                |
                                +-- 3 existing col-002 rules (friction, session)
                                +-- 18 new col-002b rules (agent, friction, session, scope)
                                    PhaseDurationOutlierRule gets Option<&[MetricVector]> at construction

MetricVector[] (history) ---> compute_baselines(history) ---> Option<BaselineSet>
                                                                  |
MetricVector (current) + BaselineSet ---> compare_to_baseline() ---> Vec<BaselineComparison>
                                                                          |
                                                            build_report(..., baseline) ---> RetrospectiveReport
```

## Shared Types (new in types.rs)

- `BaselineSet { universal: HashMap<String, BaselineEntry>, phases: HashMap<String, HashMap<String, BaselineEntry>> }`
- `BaselineEntry { mean: f64, stddev: f64, sample_count: usize }`
- `BaselineComparison { metric_name: String, current_value: f64, mean: f64, stddev: f64, is_outlier: bool, status: BaselineStatus, phase: Option<String> }`
- `BaselineStatus { Normal, Outlier, NoVariance, NewSignal }`
- `RetrospectiveReport` extended with `baseline_comparison: Option<Vec<BaselineComparison>>`

## Module Restructuring

`detection.rs` (single file) becomes `detection/` directory:
- `detection/mod.rs` -- trait, engine, default_rules(), re-exports, helpers
- `detection/agent.rs` -- 7 agent rules (all new)
- `detection/friction.rs` -- 4 friction rules (2 existing moved from detection.rs + 2 new)
- `detection/session.rs` -- 5 session rules (1 existing moved from detection.rs + 4 new)
- `detection/scope.rs` -- 5 scope rules (all new)

The `mod.rs` re-exports everything that was public in the old `detection.rs` so imports remain unchanged.

## Sequencing Constraints

1. `types.rs` changes must come first (BaselineStatus, BaselineEntry, BaselineSet, BaselineComparison, RetrospectiveReport extension)
2. `detection/mod.rs` restructure before category modules (it defines the trait + helpers)
3. Detection category modules (agent, friction, session, scope) are independent of each other
4. `baseline.rs` is independent of detection modules
5. `report.rs` depends on types.rs changes
6. `server-integration` depends on all of the above

## Helper Functions (shared across detection modules)

From existing `detection.rs`, moved to `detection/mod.rs`:
- `input_to_command_string(input: &Value) -> String` -- extracts command from Bash tool input
- `contains_sleep_command(s: &str) -> bool` -- checks for sleep in command string
- `truncate(s: &str, max_len: usize) -> String` -- truncates long strings

New helper in `detection/mod.rs`:
- `input_to_file_path(input: &Value) -> Option<String>` -- extracts file_path from Read/Write/Edit tool input
