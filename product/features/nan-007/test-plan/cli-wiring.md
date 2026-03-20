# Test Plan: CLI Wiring (`main.rs`)

**Component**: `crates/unimatrix-server/src/main.rs` (modified)
**AC coverage**: AC-15 (`--help` visibility), CLI dispatch pre-tokio
**Risk coverage**: R-07 (offline/live separation), R-11 (block_export_sync dispatch)

---

## What Is Tested Here

The CLI wiring layer is thin: it parses arguments via clap 4.x and dispatches to `run_snapshot`, `run_eval_command`, or the existing server commands. The correctness of each dispatch target is tested in its own component test plan. This plan focuses on:

1. CLI registration and `--help` visibility (AC-15).
2. Dispatch ordering: `Snapshot` and `Eval` arms are reached before tokio runtime init (C-09, C-10).
3. The content-sensitivity warning in `snapshot --help` (NFR-07).
4. Clap parse-time validation of invalid arguments.

---

## Unit Tests

Location: `crates/unimatrix-server/src/main.rs` or a dedicated `cli_tests` module.

### Test: `test_snapshot_command_parsed`

**Purpose**: The `Snapshot { out: PathBuf }` variant is successfully parsed from CLI arguments.
**Arrange**: Build `Command` from `["snapshot", "--out", "/tmp/snap.db"]`.
**Act**: `Command::try_parse_from(["unimatrix", "snapshot", "--out", "/tmp/snap.db"])`.
**Assert**: Returns `Ok(Command::Snapshot { out: PathBuf::from("/tmp/snap.db") })`. No parse error.
**Risk**: ADR-005 (clap variant registration)

### Test: `test_eval_scenarios_command_parsed`

**Purpose**: Nested `eval scenarios` subcommand parses correctly.
**Act**: `Command::try_parse_from(["unimatrix", "eval", "scenarios", "--db", "/tmp/snap.db", "--out", "/tmp/out.jsonl"])`.
**Assert**: Returns `Ok(Command::Eval { command: EvalCommand::Scenarios { db: .., out: .., .. } })`.
**Risk**: ADR-005

### Test: `test_eval_run_command_parsed`

**Purpose**: `eval run` parses with `--db`, `--scenarios`, `--configs`, `--out`, and optional `--k`.
**Act**: Parse with all required flags.
**Assert**: `Command::Eval { command: EvalCommand::Run { .. } }` with correct field values.
**Risk**: ADR-005

### Test: `test_eval_report_command_parsed`

**Purpose**: `eval report` parses with `--results` and `--out`; `--scenarios` is optional.
**Act**: Parse with and without `--scenarios`.
**Assert**: Both cases produce `EvalCommand::Report { .. }` without error.
**Risk**: ADR-005

### Test: `test_eval_scenarios_invalid_source_rejected`

**Purpose**: Invalid `--retrieval-mode` value is rejected at parse time by clap.
**Act**: `Command::try_parse_from(["unimatrix", "eval", "scenarios", "--db", "/tmp/snap.db", "--out", "/tmp/o.jsonl", "--retrieval-mode", "invalid"])`.
**Assert**: Returns `Err(...)`. Error message contains the valid values (`mcp`, `uds`, `all`).
**Risk**: Edge case

### Test: `test_eval_run_k_default`

**Purpose**: `--k` defaults to 5 when not supplied.
**Act**: Parse `eval run` without `--k`.
**Assert**: `k == 5` in the parsed struct.
**Risk**: FR-14

---

## Integration Tests (Python Subprocess)

Location: `product/test/infra-001/tests/test_eval_offline.py`

### Test: `test_cli_help_snapshot_visible`

**Purpose**: AC-15 — `unimatrix --help` output contains `snapshot`.
**Act**:
```bash
unimatrix --help
```
**Assert**: Exit code 0. stdout contains the string `"snapshot"`.
**Risk**: AC-15

### Test: `test_cli_help_eval_subcommands_visible`

**Purpose**: AC-15 — `unimatrix eval --help` output contains `scenarios`, `run`, and `report`.
**Act**:
```bash
unimatrix eval --help
```
**Assert**: Exit code 0. stdout contains `"scenarios"`, `"run"`, and `"report"`.
**Risk**: AC-15

### Test: `test_cli_snapshot_help_content_sensitivity_warning`

**Purpose**: NFR-07 — `unimatrix snapshot --help` includes a warning about snapshot content sensitivity.
**Act**:
```bash
unimatrix snapshot --help
```
**Assert**: Exit code 0. stdout contains text about content sensitivity (e.g., `"sensitive"`, `"agent_id"`, `"session_id"`, `"do not commit"`, or equivalent). The exact wording is up to the implementer; assert at least one relevant keyword is present.
**Risk**: NFR-07

### Test: `test_cli_snapshot_dispatched_without_daemon`

**Purpose**: C-09/C-10 — `unimatrix snapshot` runs and exits (with an error for missing project) without requiring a running daemon or tokio runtime error.
**Act**: `unimatrix snapshot --out /tmp/test.db` with no `--project-dir` and no daemon running.
**Assert**: Either exits 0 (if project dir discovery finds a default) or exits with a descriptive error about the missing project directory. Does NOT crash with "Cannot start a runtime from within a Tokio runtime" or similar async error.
**Risk**: R-11, C-09

### Test: `test_cli_eval_scenarios_dispatched_without_daemon`

**Purpose**: `unimatrix eval scenarios` runs without requiring a daemon.
**Act**: `unimatrix eval scenarios --db /tmp/nonexistent.db --out /tmp/out.jsonl`.
**Assert**: Exits with non-zero code (DB not found) and descriptive error. Does NOT hang or crash with runtime errors.
**Risk**: R-11

### Test: `test_cli_eval_report_dispatched_without_daemon`

**Purpose**: `eval report` is purely sync — no async, no daemon.
**Act**: `unimatrix eval report --results /tmp/empty_dir --out /tmp/report.md` where `empty_dir` exists.
**Assert**: Exit code 0 (empty results dir produces empty-indicator report). No daemon needed.
**Risk**: C-09

---

## Specific Assertions and Expected Behaviors

### Pre-tokio dispatch ordering (C-09, C-10)

The dispatch structure in `main()` must place `Command::Snapshot` and `Command::Eval { .. }` in the sync block before `tokio::main` or `#[tokio::main]` is entered. Specifically:

```rust
// Pseudocode of expected dispatch ordering in main():
match command {
    // -- PRE-TOKIO SYNC BLOCK --
    Some(Command::Snapshot { out }) => run_snapshot(project_dir.as_deref(), &out)?,
    Some(Command::Eval { command: eval_cmd }) => run_eval_command(eval_cmd, project_dir.as_deref())?,
    Some(Command::Export { .. }) => run_export(...)?,  // existing pre-tokio
    // ... other pre-tokio commands ...

    // -- POST-TOKIO ASYNC BLOCK --
    Some(Command::Serve { .. }) | None => {
        tokio::runtime::Builder::new_multi_thread()...run(async { ... })
    }
}
```

Verification: Call `unimatrix snapshot` or `unimatrix eval report` and confirm they complete without the tokio runtime being initialized. This is structurally enforced by not calling `block_on` at the outer level — each command uses `block_export_sync` internally if it needs async.

### Eval subcommand dispatch

`run_eval_command` dispatches to:
- `EvalCommand::Scenarios` → `run_scenarios(db, source, limit, out)` via `block_export_sync`.
- `EvalCommand::Run` → `run_eval(db, scenarios, configs, k, out)` via `block_export_sync`.
- `EvalCommand::Report` → `run_report(results, scenarios_opt, out)` (pure sync, no bridge needed).

The `block_export_sync` bridge creates a current-thread tokio runtime scoped to the call. It must not be called from within an existing tokio runtime (R-11). All dispatch arms that use it must be called from the sync pre-tokio block.

---

## Knowledge Stewardship

Queried: /uni-query-patterns for "evaluation harness testing patterns edge cases" — found entries #1204 (Test Plan Must Cross-Reference Pseudocode for Edge-Case Behavior Assertions), #157 (Test infrastructure is cumulative), #128 (Risk drives testing)
Queried: /uni-query-patterns for "nan-007 architectural decisions" (category: decision, topic: nan-007) — found ADR-005 (nested eval subcommand via clap 4.x inner enum), ADR-004 (no new crate; modules live in unimatrix-server src/), ADR-001 (snapshot + eval dispatched pre-tokio via block_export_sync)
Queried: /uni-query-patterns for "integration test harness patterns infra" — found entries #238 (Testing Infrastructure Convention), #748 (TestHarness Server Integration Pattern), #129 (Concrete assertions)
Stored: nothing novel to store — test plan agents are read-only; patterns are consumed not created
