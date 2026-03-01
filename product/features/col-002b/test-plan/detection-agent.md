# Test Plan: detection-agent

## Component: 7 agent hotspot rules in `detection/agent.rs`

## Test Module: `#[cfg(test)] mod tests` within `agent.rs`

### Shared Test Helpers (local or from mod.rs)

```rust
fn make_read_post(ts: u64, file_path: &str, response_size: u64) -> ObservationRecord
fn make_write_post(ts: u64, file_path: &str) -> ObservationRecord
fn make_edit_post(ts: u64, file_path: &str, response_size: u64) -> ObservationRecord
fn make_read_pre(ts: u64, file_path: &str) -> ObservationRecord
fn make_subagent_start(ts: u64, agent_type: &str) -> ObservationRecord
fn make_subagent_stop(ts: u64) -> ObservationRecord
fn make_bash_pre(ts: u64, command: &str) -> ObservationRecord
```

### ContextLoadRule Tests

| Test | Scenario | Expected |
|------|----------|----------|
| `test_context_load_exceeds_threshold` | 5 Read PostToolUse (each 25KB) before first Write -> 125KB | Finding with measured=125.0 |
| `test_context_load_below_threshold` | 2 Read PostToolUse (each 25KB) before Write -> 50KB | No finding |
| `test_context_load_no_writes` | 10 Read PostToolUse (each 15KB) with no Write ever -> 150KB | Finding (all reads count when no write boundary) |
| `test_context_load_empty_records` | Empty input | No findings |
| `test_context_load_write_first` | Write at ts=1, then Reads | measured=0 (Write comes first, stops accumulation) |

Risk coverage: R-01 (fires above threshold), R-12 (response_size access)

### LifespanRule Tests

| Test | Scenario | Expected |
|------|----------|----------|
| `test_lifespan_exceeds_threshold` | SubagentStart at ts=0, SubagentStop at ts=50min -> 50 min | Finding |
| `test_lifespan_below_threshold` | Start+Stop within 30 min | No finding |
| `test_lifespan_empty_records` | Empty input | No findings |
| `test_lifespan_no_stop` | SubagentStart only, no matching Stop | No finding (no complete pair) |
| `test_lifespan_multiple_agents` | 2 agents: one short, one long | Finding for the long one only |

Risk coverage: R-01, R-12 (SubagentStart/Stop handling)

### FileBreadthRule Tests

| Test | Scenario | Expected |
|------|----------|----------|
| `test_file_breadth_exceeds_threshold` | 25 distinct file paths across Read/Write/Edit | Finding with measured=25 |
| `test_file_breadth_below_threshold` | 15 distinct files | No finding |
| `test_file_breadth_dedup` | Same file read 5 times | Counted once |
| `test_file_breadth_empty` | Empty input | No findings |
| `test_file_breadth_no_file_path` | Records with None input | No findings |

Risk coverage: R-01, R-12 (file_path extraction from different tool inputs)

### RereadRateRule Tests

| Test | Scenario | Expected |
|------|----------|----------|
| `test_reread_rate_exceeds_threshold` | 5 files each read twice, 1 file read once -> 5 re-reads | Finding |
| `test_reread_rate_below_threshold` | 2 files read twice -> 2 re-reads | No finding (threshold 3) |
| `test_reread_rate_no_rereads` | 10 unique files each read once | No finding |
| `test_reread_rate_empty` | Empty input | No findings |

Risk coverage: R-01, R-12

### MutationSpreadRule Tests

| Test | Scenario | Expected |
|------|----------|----------|
| `test_mutation_spread_exceeds` | 12 distinct Write/Edit file paths | Finding |
| `test_mutation_spread_below` | 8 distinct paths | No finding |
| `test_mutation_spread_dedup` | Same file edited 3 times | Counted once |
| `test_mutation_spread_empty` | Empty input | No findings |

Risk coverage: R-01, R-12

### CompileCyclesRule Tests

| Test | Scenario | Expected |
|------|----------|----------|
| `test_compile_cycles_exceeds` | 8 `cargo test` commands | Finding |
| `test_compile_cycles_below` | 4 `cargo test` | No finding |
| `test_compile_cycles_variations` | `cargo test`, `cargo check`, `cargo build --workspace`, `cargo clippy`, `RUSTFLAGS=... cargo check` | All counted |
| `test_compile_cycles_non_compile` | `cargo add serde`, `cargo fmt` | Not counted |
| `test_compile_cycles_empty` | Empty input | No findings |

Risk coverage: R-01, R-04 (regex variations), R-12

### EditBloatRule Tests

| Test | Scenario | Expected |
|------|----------|----------|
| `test_edit_bloat_exceeds` | 5 Edit PostToolUse averaging 60KB | Finding |
| `test_edit_bloat_below` | 5 Edits averaging 30KB | No finding |
| `test_edit_bloat_mixed` | Some large, some small, average above threshold | Finding |
| `test_edit_bloat_no_edits` | Only Read records | No findings |
| `test_edit_bloat_empty` | Empty input | No findings |

Risk coverage: R-01, R-12 (response_size access on Edit)

## Assertions Pattern

Every test asserts:
1. findings.len() == expected count
2. For findings that fire: rule_name, category, measured, threshold
3. For findings that fire: evidence is non-empty (R-01, AC-05)
4. For silent tests: findings.is_empty()
