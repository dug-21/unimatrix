# Pseudocode: detection-session

## Purpose

Implements 5 session hotspot rules in `crates/unimatrix-observe/src/detection/session.rs`:
- 1 existing from col-002 (SessionTimeoutRule) moved from detection.rs
- 4 new (ColdRestartRule, CoordinatorRespawnsRule, PostCompletionWorkRule, ReworkEventsRule)

## File: `crates/unimatrix-observe/src/detection/session.rs`

```
use crate::types::{EvidenceRecord, HookType, HotspotCategory, HotspotFinding, ObservationRecord, Severity};
use super::{DetectionRule, input_to_file_path, input_to_command_string, truncate};
use std::collections::{HashMap, HashSet};
```

### Existing Rule: SessionTimeoutRule (moved from detection.rs)

Exact copy of existing implementation including the TIMEOUT_GAP_MS constant.
No logic changes. Just moved to this file.

### New Rule 1: ColdRestartRule (FR-03.1)

```
pub(crate) struct ColdRestartRule;

const COLD_RESTART_GAP_MS: u64 = 30 * 60 * 1000;  // 30 minutes

impl DetectionRule for ColdRestartRule:
    name() -> "cold_restart"
    category() -> Session

    detect(records):
        if records.is_empty(): return []

        // Sort by timestamp
        sorted = records sorted by ts

        // Track all file paths read before each gap
        files_read_before: HashSet<String> = {}
        findings = []
        prev_ts = sorted[0].ts

        for record in sorted:
            gap = record.ts - prev_ts
            if gap > COLD_RESTART_GAP_MS:
                // Potential cold restart. Check next burst of reads for overlap.
                // Collect reads in the 5-minute window after the gap
                let burst_window_end = record.ts + 5 * 60 * 1000
                let burst_reads: records with ts in [record.ts, burst_window_end]
                    where tool == "Read"
                    extract file_path from input

                let overlap: burst_reads intersection with files_read_before
                if !overlap.is_empty():
                    findings.push(HotspotFinding {
                        category: Session, severity: Warning,
                        rule_name: "cold_restart",
                        claim: "{gap/60000:.0}-minute gap followed by {overlap.len()} re-reads of previously accessed files",
                        measured: gap as f64 / 60000.0,
                        threshold: COLD_RESTART_GAP_MS as f64 / 60000.0,
                        evidence: [gap duration evidence, overlapping file paths]
                    })

            // Track reads
            if record.tool == Some("Read"):
                if let Some(path) = input_to_file_path(record.input):
                    files_read_before.insert(path)

            prev_ts = record.ts

        return findings
```

Note: The implementation should use a forward scan approach. After detecting a gap, scan forward from the gap point to check the burst reads, rather than requiring a second pass. This is more natural given records are already sorted.

### New Rule 2: CoordinatorRespawnsRule (FR-03.2)

```
pub(crate) struct CoordinatorRespawnsRule;

const COORDINATOR_RESPAWN_THRESHOLD: f64 = 3.0;

impl DetectionRule for CoordinatorRespawnsRule:
    name() -> "coordinator_respawns"
    category() -> Session

    detect(records):
        coordinator_spawns = 0
        evidence = []

        for record in records:
            if record.hook == SubagentStart:
                if let Some(agent_type) = &record.tool:
                    let lower = agent_type.to_lowercase()
                    if lower.contains("scrum-master")
                        || lower.contains("coordinator")
                        || lower.contains("lead"):
                        coordinator_spawns += 1
                        evidence.push(EvidenceRecord {
                            description: "Coordinator spawn",
                            ts: record.ts,
                            tool: Some(agent_type.clone()),
                            detail: format!("Coordinator agent '{agent_type}' spawned")
                        })

        if coordinator_spawns as f64 > COORDINATOR_RESPAWN_THRESHOLD:
            return [HotspotFinding {
                category: Session, severity: Warning,
                rule_name: "coordinator_respawns",
                claim: "{coordinator_spawns} coordinator respawns detected",
                measured: coordinator_spawns as f64,
                threshold: COORDINATOR_RESPAWN_THRESHOLD,
                evidence
            }]
        return []
```

### New Rule 3: PostCompletionWorkRule (FR-03.3)

```
pub(crate) struct PostCompletionWorkRule;

const POST_COMPLETION_THRESHOLD_PCT: f64 = 8.0;

impl DetectionRule for PostCompletionWorkRule:
    name() -> "post_completion_work"
    category() -> Session

    detect(records):
        if records.is_empty(): return []

        // Find the LAST TaskUpdate with "completed" status
        completion_boundary = find_completion_boundary(records)
        if completion_boundary.is_none(): return []

        let boundary_ts = completion_boundary.unwrap()
        let total = records.len()
        let post_count = records.iter().filter(|r| r.ts > boundary_ts).count()

        let pct = (post_count as f64 / total as f64) * 100.0
        if pct > POST_COMPLETION_THRESHOLD_PCT:
            return [HotspotFinding {
                category: Session, severity: Info,
                rule_name: "post_completion_work",
                claim: "{pct:.1}% of tool calls occurred after task completion ({post_count}/{total})",
                measured: pct, threshold: POST_COMPLETION_THRESHOLD_PCT,
                evidence: [completion boundary record, sample post-completion records]
            }]
        return []
```

### New Rule 4: ReworkEventsRule (FR-03.4)

```
pub(crate) struct ReworkEventsRule;

impl DetectionRule for ReworkEventsRule:
    name() -> "rework_events"
    category() -> Session

    detect(records):
        // Detect TaskUpdate records where status goes from completed -> in_progress
        // This requires tracking task status transitions
        // The input field for TaskUpdate contains {"status": "completed"} or {"status": "in_progress"}

        task_states: HashMap<String, String> = {}  // task_id/subject -> last known status
        rework_evidence = []

        // Process records in ts order
        sorted = records sorted by ts

        for record in sorted:
            // TaskUpdate appears as tool="TaskUpdate" in observation records
            if record.tool.as_deref() == Some("TaskUpdate"):
                if let Some(input) = &record.input:
                    let status = input.get("status").and_then(|v| v.as_str())
                    let task_id = input.get("taskId").or(input.get("subject"))
                        .and_then(|v| v.as_str()).unwrap_or("unknown")

                    if let Some(status) = status:
                        let prev = task_states.get(task_id)
                        if prev == Some(&"completed".to_string()) && status == "in_progress":
                            rework_evidence.push(EvidenceRecord {
                                description: "Task rework: completed -> in_progress",
                                ts: record.ts,
                                tool: Some("TaskUpdate"),
                                detail: format!("Task '{task_id}' reopened")
                            })
                        task_states.insert(task_id.to_string(), status.to_string())

        if !rework_evidence.is_empty():
            return [HotspotFinding {
                category: Session, severity: Warning,
                rule_name: "rework_events",
                claim: "{rework_evidence.len()} task rework event(s) detected",
                measured: rework_evidence.len() as f64,
                threshold: 1.0,  // any occurrence
                evidence: rework_evidence
            }]
        return []
```

### Shared Helper: find_completion_boundary

```
fn find_completion_boundary(records: &[ObservationRecord]) -> Option<u64>:
    // Find the LAST TaskUpdate with "completed" in input
    let mut last_completion_ts: Option<u64> = None

    for record in records:
        if record.tool.as_deref() == Some("TaskUpdate"):
            if let Some(input) = &record.input:
                if let Some(status) = input.get("status").and_then(|v| v.as_str()):
                    if status == "completed":
                        match last_completion_ts:
                            None => last_completion_ts = Some(record.ts)
                            Some(prev) => if record.ts > prev:
                                last_completion_ts = Some(record.ts)

    last_completion_ts
```

This helper is also used by PostDeliveryIssuesRule in detection-scope, so it should be in `detection/mod.rs` as `pub(crate)`.

## Error Handling

- All rules handle empty records
- ColdRestartRule handles single-record edge case (no gap possible)
- PostCompletionWorkRule handles no TaskUpdate records (no boundary = no finding)
- ReworkEventsRule handles missing status or taskId fields gracefully
- Division by zero guarded in PostCompletionWorkRule (empty records returns early)

## Key Test Scenarios

### ColdRestartRule
1. 35-min gap + re-reads of previously read files -> fires
2. 35-min gap + reads of NEW files (no overlap) -> silent
3. 25-min gap (below threshold) + re-reads -> silent
4. Empty records -> empty

### CoordinatorRespawnsRule
1. 4 SubagentStart with "uni-scrum-master" -> fires (> 3)
2. 2 SubagentStart with coordinator names -> silent (not > 3)
3. SubagentStart with "uni-rust-dev" -> not a coordinator -> silent
4. Empty records -> empty

### PostCompletionWorkRule
1. 100 records, completion at record 80, 20 after -> 20% > 8% -> fires
2. 100 records, completion at record 98 -> 2% -> silent
3. No TaskUpdate records -> no boundary -> silent

### ReworkEventsRule
1. TaskUpdate completed then TaskUpdate in_progress for same task -> fires
2. TaskUpdate completed only -> silent
3. TaskUpdate in_progress then completed (normal flow) -> silent
