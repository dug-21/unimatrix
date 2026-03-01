# Pseudocode: detection-agent

## Purpose

Implements 7 agent hotspot detection rules in `crates/unimatrix-observe/src/detection/agent.rs`. Each rule is a unit struct implementing `DetectionRule`.

## File: `crates/unimatrix-observe/src/detection/agent.rs`

```
use crate::types::{EvidenceRecord, HookType, HotspotCategory, HotspotFinding, ObservationRecord, Severity};
use super::{DetectionRule, input_to_file_path, input_to_command_string, truncate};
use std::collections::{HashMap, HashSet};
```

### Rule 1: ContextLoadRule (FR-01.1)

```
struct ContextLoadRule;

const CONTEXT_LOAD_THRESHOLD_KB: f64 = 100.0;

impl DetectionRule for ContextLoadRule:
    name() -> "context_load"
    category() -> Agent

    detect(records):
        // Sum response_size from Read PostToolUse until first Write or Edit PostToolUse
        sort records by ts
        total_kb = 0.0
        evidence = []

        for record in records (sorted by ts):
            if record.hook == PostToolUse:
                tool = record.tool
                if tool == "Write" or tool == "Edit":
                    break  // stop at first mutation
                if tool == "Read":
                    if let Some(size) = record.response_size:
                        kb = size as f64 / 1024.0
                        total_kb += kb
                        evidence.push(EvidenceRecord {
                            description: "Read before first write",
                            ts: record.ts,
                            tool: Some("Read"),
                            detail: format file_path + size
                        })

        if total_kb > CONTEXT_LOAD_THRESHOLD_KB:
            return [HotspotFinding {
                category: Agent,
                severity: Warning,
                rule_name: "context_load",
                claim: "Loaded {total_kb:.0} KB before first write/edit",
                measured: total_kb,
                threshold: CONTEXT_LOAD_THRESHOLD_KB,
                evidence: evidence (truncated to first 10)
            }]
        return []
```

### Rule 2: LifespanRule (FR-01.2)

```
struct LifespanRule;

const LIFESPAN_THRESHOLD_MINS: f64 = 45.0;

impl DetectionRule for LifespanRule:
    name() -> "lifespan"
    category() -> Agent

    detect(records):
        // Match SubagentStart/SubagentStop pairs by session
        // Note: tool field contains agent_type for SubagentStart
        starts: HashMap<String, Vec<(u64, String)>>  // session_id -> [(ts, agent_type)]
        stops: HashMap<String, Vec<u64>>              // session_id -> [ts]

        for record in records:
            match record.hook:
                SubagentStart => starts[session_id].push((ts, tool.unwrap_or_default()))
                SubagentStop => stops[session_id].push(ts)

        findings = []
        for (session_id, start_list) in starts:
            stop_list = stops.get(session_id).unwrap_or_default()
            // Sort both by ts
            // Pair them: first start with first stop, etc.
            for (i, (start_ts, agent_type)) in start_list.iter().enumerate():
                if i < stop_list.len():
                    duration_mins = (stop_list[i] - start_ts) as f64 / 60000.0
                    if duration_mins > LIFESPAN_THRESHOLD_MINS:
                        findings.push(HotspotFinding {
                            category: Agent,
                            severity: Warning,
                            rule_name: "lifespan",
                            claim: "Agent '{agent_type}' ran for {duration_mins:.0} minutes",
                            measured: duration_mins,
                            threshold: LIFESPAN_THRESHOLD_MINS,
                            evidence: [start evidence, stop evidence]
                        })
        return findings
```

### Rule 3: FileBreadthRule (FR-01.3)

```
struct FileBreadthRule;

const FILE_BREADTH_THRESHOLD: f64 = 20.0;

impl DetectionRule for FileBreadthRule:
    name() -> "file_breadth"
    category() -> Agent

    detect(records):
        file_paths: HashSet<String> = {}
        evidence = []

        for record in records:
            if record.tool in ["Read", "Write", "Edit"]:
                if let Some(path) = input_to_file_path(record.input):
                    file_paths.insert(path.clone())
                    evidence.push(EvidenceRecord with path)

        count = file_paths.len() as f64
        if count > FILE_BREADTH_THRESHOLD:
            return [HotspotFinding {
                category: Agent, severity: Warning,
                rule_name: "file_breadth",
                claim: "Accessed {count} distinct files",
                measured: count, threshold: FILE_BREADTH_THRESHOLD,
                evidence: deduplicated list of paths with access counts
            }]
        return []
```

### Rule 4: RereadRateRule (FR-01.4)

```
struct RereadRateRule;

const REREAD_THRESHOLD: f64 = 3.0;

impl DetectionRule for RereadRateRule:
    name() -> "reread_rate"
    category() -> Agent

    detect(records):
        read_counts: HashMap<String, u64> = {}  // file_path -> count

        for record in records:
            if record.tool == Some("Read"):
                if let Some(path) = input_to_file_path(record.input):
                    *read_counts.entry(path).or_default() += 1

        reread_files: files where count > 1
        reread_count = reread_files.len() as f64

        if reread_count > REREAD_THRESHOLD:
            return [HotspotFinding {
                category: Agent, severity: Info,
                rule_name: "reread_rate",
                claim: "{reread_count} files re-read multiple times",
                measured: reread_count, threshold: REREAD_THRESHOLD,
                evidence: list of paths with their read counts
            }]
        return []
```

### Rule 5: MutationSpreadRule (FR-01.5)

```
struct MutationSpreadRule;

const MUTATION_SPREAD_THRESHOLD: f64 = 10.0;

impl DetectionRule for MutationSpreadRule:
    name() -> "mutation_spread"
    category() -> Agent

    detect(records):
        mutated_files: HashSet<String> = {}

        for record in records:
            if record.tool in ["Write", "Edit"]:
                if let Some(path) = input_to_file_path(record.input):
                    mutated_files.insert(path)

        count = mutated_files.len() as f64
        if count > MUTATION_SPREAD_THRESHOLD:
            return [HotspotFinding {
                category: Agent, severity: Warning,
                rule_name: "mutation_spread",
                claim: "Mutations spread across {count} files",
                measured: count, threshold: MUTATION_SPREAD_THRESHOLD,
                evidence: list of mutated file paths
            }]
        return []
```

### Rule 6: CompileCyclesRule (FR-01.6)

```
struct CompileCyclesRule;

const COMPILE_CYCLES_THRESHOLD: f64 = 6.0;

impl DetectionRule for CompileCyclesRule:
    name() -> "compile_cycles"
    category() -> Agent

    detect(records):
        compile_count = 0
        evidence = []

        for record in records:
            if record.tool == Some("Bash") and record.hook == PreToolUse:
                if let Some(input) = &record.input:
                    cmd = input_to_command_string(input)
                    if is_compile_command(&cmd):
                        compile_count += 1
                        evidence.push(EvidenceRecord {
                            description: "Compile command",
                            ts: record.ts,
                            tool: Some("Bash"),
                            detail: truncate(&cmd, 200)
                        })

        if compile_count as f64 > COMPILE_CYCLES_THRESHOLD:
            return [HotspotFinding {
                category: Agent, severity: Warning,
                rule_name: "compile_cycles",
                claim: "{compile_count} compile/check cycles detected",
                measured: compile_count as f64, threshold: COMPILE_CYCLES_THRESHOLD,
                evidence
            }]
        return []

fn is_compile_command(cmd: &str) -> bool:
    // Match: cargo check, cargo test, cargo build, cargo clippy
    // With optional flags after the subcommand
    // Also match with env vars prefix: RUSTFLAGS=... cargo check
    let trimmed = cmd.trim()
    // Strip env var prefixes (KEY=VALUE before cargo)
    let cargo_part = find "cargo" in trimmed, take from "cargo" onwards
    if no "cargo" found: return false
    let after_cargo = cargo_part.strip_prefix("cargo").trim_start()
    after_cargo.starts_with("check") || after_cargo.starts_with("test")
        || after_cargo.starts_with("build") || after_cargo.starts_with("clippy")
```

### Rule 7: EditBloatRule (FR-01.7)

```
struct EditBloatRule;

const EDIT_BLOAT_THRESHOLD_KB: f64 = 50.0;

impl DetectionRule for EditBloatRule:
    name() -> "edit_bloat"
    category() -> Agent

    detect(records):
        edit_sizes = []
        evidence = []

        for record in records:
            if record.tool == Some("Edit") and record.hook == PostToolUse:
                if let Some(size) = record.response_size:
                    kb = size as f64 / 1024.0
                    edit_sizes.push(kb)
                    if kb > EDIT_BLOAT_THRESHOLD_KB:
                        evidence.push(EvidenceRecord {
                            description: "Large Edit response",
                            ts: record.ts,
                            tool: Some("Edit"),
                            detail: format!("{kb:.1} KB response")
                        })

        if edit_sizes.is_empty(): return []

        avg_kb = edit_sizes.iter().sum::<f64>() / edit_sizes.len() as f64
        if avg_kb > EDIT_BLOAT_THRESHOLD_KB:
            return [HotspotFinding {
                category: Agent, severity: Info,
                rule_name: "edit_bloat",
                claim: "Average Edit response is {avg_kb:.1} KB (threshold {EDIT_BLOAT_THRESHOLD_KB} KB)",
                measured: avg_kb, threshold: EDIT_BLOAT_THRESHOLD_KB,
                evidence
            }]
        return []
```

## Error Handling

- All rules handle `None` tool, `None` input gracefully by skipping records
- `input_to_file_path` returns `None` for unexpected input shapes -- rules skip
- Empty records set returns empty findings
- No panics from any rule

## Key Test Scenarios

Per rule:
1. Fires above threshold with correctly-shaped synthetic records
2. Silent below threshold
3. Empty records returns empty findings
4. Records with None/missing fields are skipped gracefully
