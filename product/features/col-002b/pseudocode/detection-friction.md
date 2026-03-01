# Pseudocode: detection-friction

## Purpose

Implements 4 friction hotspot rules in `crates/unimatrix-observe/src/detection/friction.rs`:
- 2 existing from col-002 (PermissionRetriesRule, SleepWorkaroundsRule) moved from detection.rs
- 2 new (SearchViaBashRule, OutputParsingStruggleRule)

## File: `crates/unimatrix-observe/src/detection/friction.rs`

```
use crate::types::{EvidenceRecord, HookType, HotspotCategory, HotspotFinding, ObservationRecord, Severity};
use super::{DetectionRule, input_to_command_string, contains_sleep_command, truncate};
use std::collections::HashMap;
```

### Existing Rule: PermissionRetriesRule (moved from detection.rs)

Exact copy of existing implementation. No logic changes. Just moved to this file.
Struct visibility: `pub(crate)` (re-exported via mod.rs default_rules).

### Existing Rule: SleepWorkaroundsRule (moved from detection.rs)

Exact copy of existing implementation. No logic changes. Just moved to this file.

### New Rule 1: SearchViaBashRule (FR-02.1)

```
pub(crate) struct SearchViaBashRule;

const SEARCH_VIA_BASH_THRESHOLD_PCT: f64 = 5.0;

impl DetectionRule for SearchViaBashRule:
    name() -> "search_via_bash"
    category() -> Friction

    detect(records):
        total_bash = 0
        search_bash = 0
        evidence = []

        for record in records:
            if record.tool == Some("Bash") and record.hook == PreToolUse:
                total_bash += 1
                if let Some(input) = &record.input:
                    cmd = input_to_command_string(input)
                    if is_search_command(&cmd):
                        search_bash += 1
                        evidence.push(EvidenceRecord {
                            description: "Search command via Bash",
                            ts: record.ts,
                            tool: Some("Bash"),
                            detail: truncate(&cmd, 200)
                        })

        if total_bash == 0: return []

        pct = (search_bash as f64 / total_bash as f64) * 100.0
        if pct > SEARCH_VIA_BASH_THRESHOLD_PCT:
            return [HotspotFinding {
                category: Friction, severity: Info,
                rule_name: "search_via_bash",
                claim: "{pct:.1}% of Bash calls are search commands ({search_bash}/{total_bash})",
                measured: pct, threshold: SEARCH_VIA_BASH_THRESHOLD_PCT,
                evidence
            }]
        return []

fn is_search_command(cmd: &str) -> bool:
    let trimmed = cmd.trim()
    // Check for find, grep, rg, ag as first word in any pipeline segment
    for segment in trimmed.split(|c| c == ';' || c == '\n'):
        let seg = segment.trim()
        // Strip leading env vars or cd prefix
        if seg.starts_with("find ") || seg == "find"
            || seg.starts_with("grep ") || seg == "grep"
            || seg.starts_with("rg ") || seg == "rg"
            || seg.starts_with("ag ") || seg == "ag":
            return true
    false
```

### New Rule 2: OutputParsingStruggleRule (FR-02.2)

```
pub(crate) struct OutputParsingStruggleRule;

const OUTPUT_PARSING_THRESHOLD: f64 = 2.0;
const OUTPUT_PARSING_WINDOW_MS: u64 = 3 * 60 * 1000;  // 3 minutes

impl DetectionRule for OutputParsingStruggleRule:
    name() -> "output_parsing_struggle"
    category() -> Friction

    detect(records):
        // Collect Bash commands with pipes
        piped_commands: Vec<(u64, String, String)> = []  // (ts, base_command, full_command)

        for record in records (sorted by ts):
            if record.tool == Some("Bash") and record.hook == PreToolUse:
                if let Some(input) = &record.input:
                    cmd = input_to_command_string(input)
                    if cmd.contains('|'):
                        // Split on first pipe to get base command
                        let parts: split on '|', first part = base_cmd, rest = filter
                        let base = parts[0].trim()
                        piped_commands.push((record.ts, base.to_string(), cmd))

        // Group by base command, check for filter variations within window
        findings = []
        // For each unique base command, collect all filter variations
        let mut by_base: HashMap<String, Vec<(u64, String)>> = HashMap::new()
        for (ts, base, full) in piped_commands:
            by_base.entry(base).or_default().push((ts, full))

        for (base, entries) in by_base:
            // Find entries within 3-minute windows
            // Sort by ts
            entries.sort_by_key(|(ts, _)| *ts)

            // Sliding window: for each entry, count distinct filter suffixes within window
            for i in 0..entries.len():
                let window_start = entries[i].0
                let window_end = window_start + OUTPUT_PARSING_WINDOW_MS
                let in_window: collect entries[j] where entries[j].0 <= window_end

                // Extract distinct filters (part after first pipe)
                let filters: HashSet of filter parts from in_window
                if filters.len() as f64 > OUTPUT_PARSING_THRESHOLD:
                    // Create finding (deduplicate by base command)
                    findings.push(HotspotFinding {
                        category: Friction, severity: Info,
                        rule_name: "output_parsing_struggle",
                        claim: "Command '{base}' piped through {filters.len()} different filters within 3 minutes",
                        measured: filters.len() as f64,
                        threshold: OUTPUT_PARSING_THRESHOLD,
                        evidence: entries in window as EvidenceRecords
                    })
                    break  // one finding per base command

        return findings
```

## Error Handling

- `input_to_command_string` handles None/unexpected shapes returning empty string
- Zero total_bash avoids division by zero in SearchViaBashRule
- Empty records returns empty findings for all rules

## Key Test Scenarios

### SearchViaBashRule
1. 20 Bash calls, 2 are `find .` and `grep pattern` -> 10% > 5% threshold -> fires
2. 20 Bash calls, 0 search -> 0% -> silent
3. `echo "finding"` is NOT a search command
4. Empty records -> empty findings

### OutputParsingStruggleRule
1. `cargo test | grep FAIL` then `cargo test | tail -20` then `cargo test | head -5` within 3 min -> 3 filters > 2 -> fires
2. `cargo test | grep FAIL` then `cargo build | tail -20` -> different base commands -> silent
3. Same command repeated (not variation) -> 1 filter -> silent
4. Commands outside 3-min window -> separate windows -> may not fire
