# Pseudocode: detection-scope

## Purpose

Implements 5 scope hotspot rules in `crates/unimatrix-observe/src/detection/scope.rs`. All rules are new (no existing rules moved here).

## File: `crates/unimatrix-observe/src/detection/scope.rs`

```
use crate::types::{EvidenceRecord, HookType, HotspotCategory, HotspotFinding, MetricVector, ObservationRecord, Severity};
use super::{DetectionRule, input_to_file_path, input_to_command_string, truncate, find_completion_boundary};
use std::collections::{HashMap, HashSet};
```

### Rule 1: SourceFileCountRule (FR-04.1)

```
pub(crate) struct SourceFileCountRule;

const SOURCE_FILE_COUNT_THRESHOLD: f64 = 6.0;

impl DetectionRule for SourceFileCountRule:
    name() -> "source_file_count"
    category() -> Scope

    detect(records):
        // Count distinct *.rs file paths in Write tool input (first Write = new file creation)
        written_rs_files: HashSet<String> = {}
        evidence = []

        for record in records:
            if record.tool.as_deref() == Some("Write") and record.hook == PostToolUse:
                if let Some(path) = input_to_file_path(&record.input):
                    if path.ends_with(".rs"):
                        if written_rs_files.insert(path.clone()):
                            // First write to this path = potential new file
                            evidence.push(EvidenceRecord {
                                description: "New .rs file written",
                                ts: record.ts,
                                tool: Some("Write"),
                                detail: path
                            })

        let count = written_rs_files.len() as f64
        if count > SOURCE_FILE_COUNT_THRESHOLD:
            return [HotspotFinding {
                category: Scope, severity: Warning,
                rule_name: "source_file_count",
                claim: "{count} new .rs source files created",
                measured: count, threshold: SOURCE_FILE_COUNT_THRESHOLD,
                evidence
            }]
        return []
```

### Rule 2: DesignArtifactCountRule (FR-04.2)

```
pub(crate) struct DesignArtifactCountRule;

const DESIGN_ARTIFACT_THRESHOLD: f64 = 25.0;

impl DetectionRule for DesignArtifactCountRule:
    name() -> "design_artifact_count"
    category() -> Scope

    detect(records):
        artifact_paths: HashSet<String> = {}
        evidence = []

        for record in records:
            if record.tool.as_deref() == Some("Write") || record.tool.as_deref() == Some("Edit"):
                if let Some(path) = input_to_file_path(&record.input):
                    if path.contains("product/features/"):
                        if artifact_paths.insert(path.clone()):
                            evidence.push(EvidenceRecord {
                                description: "Design artifact modified",
                                ts: record.ts,
                                tool: record.tool.clone(),
                                detail: path
                            })

        let count = artifact_paths.len() as f64
        if count > DESIGN_ARTIFACT_THRESHOLD:
            return [HotspotFinding {
                category: Scope, severity: Info,
                rule_name: "design_artifact_count",
                claim: "{count} design artifacts created/modified under product/features/",
                measured: count, threshold: DESIGN_ARTIFACT_THRESHOLD,
                evidence
            }]
        return []
```

### Rule 3: AdrCountRule (FR-04.3)

```
pub(crate) struct AdrCountRule;

const ADR_COUNT_THRESHOLD: f64 = 3.0;

impl DetectionRule for AdrCountRule:
    name() -> "adr_count"
    category() -> Scope

    detect(records):
        adr_paths: HashSet<String> = {}
        evidence = []

        for record in records:
            if record.tool.as_deref() == Some("Write"):
                if let Some(path) = input_to_file_path(&record.input):
                    // Match ADR-* pattern in filename
                    if let Some(filename) = path.rsplit('/').next():
                        if filename.starts_with("ADR-"):
                            if adr_paths.insert(path.clone()):
                                evidence.push(EvidenceRecord {
                                    description: "ADR created",
                                    ts: record.ts,
                                    tool: Some("Write"),
                                    detail: path
                                })

        let count = adr_paths.len() as f64
        if count > ADR_COUNT_THRESHOLD:
            return [HotspotFinding {
                category: Scope, severity: Info,
                rule_name: "adr_count",
                claim: "{count} ADRs created (threshold: {ADR_COUNT_THRESHOLD})",
                measured: count, threshold: ADR_COUNT_THRESHOLD,
                evidence
            }]
        return []
```

### Rule 4: PostDeliveryIssuesRule (FR-04.4)

```
pub(crate) struct PostDeliveryIssuesRule;

impl DetectionRule for PostDeliveryIssuesRule:
    name() -> "post_delivery_issues"
    category() -> Scope

    detect(records):
        // Find completion boundary (same logic as PostCompletionWorkRule)
        let boundary_ts = find_completion_boundary(records)
        if boundary_ts.is_none(): return []
        let boundary = boundary_ts.unwrap()

        // Count `gh issue create` commands after completion
        let mut issue_creates = Vec::new()

        for record in records:
            if record.ts > boundary:
                if record.tool.as_deref() == Some("Bash") and record.hook == PreToolUse:
                    if let Some(input) = &record.input:
                        let cmd = input_to_command_string(input)
                        if cmd.contains("gh issue create"):
                            issue_creates.push(EvidenceRecord {
                                description: "Post-delivery issue creation",
                                ts: record.ts,
                                tool: Some("Bash"),
                                detail: truncate(&cmd, 200)
                            })

        if !issue_creates.is_empty():
            return [HotspotFinding {
                category: Scope, severity: Warning,
                rule_name: "post_delivery_issues",
                claim: "{issue_creates.len()} issues created after task completion",
                measured: issue_creates.len() as f64,
                threshold: 1.0,  // any occurrence (>0)
                evidence: issue_creates
            }]
        return []
```

### Rule 5: PhaseDurationOutlierRule (FR-04.5)

```
pub(crate) struct PhaseDurationOutlierRule {
    // Per-phase baselines: phase_name -> (mean_duration_secs, count)
    phase_baselines: HashMap<String, (f64, usize)>,
}

const ABSOLUTE_PHASE_DURATION_THRESHOLD_SECS: f64 = 7200.0;  // 2 hours

impl PhaseDurationOutlierRule:
    pub fn new(history: Option<&[MetricVector]>) -> Self:
        let mut phase_baselines = HashMap::new()

        if let Some(vectors) = history:
            // Collect durations per phase name across all historical vectors
            let mut by_phase: HashMap<String, Vec<f64>> = HashMap::new()
            for mv in vectors:
                for (phase_name, phase_metrics) in &mv.phases:
                    by_phase.entry(phase_name.clone())
                        .or_default()
                        .push(phase_metrics.duration_secs as f64)

            for (phase_name, durations) in by_phase:
                if durations.len() >= 3:
                    let mean = durations.iter().sum::<f64>() / durations.len() as f64
                    phase_baselines.insert(phase_name, (mean, durations.len()))

        PhaseDurationOutlierRule { phase_baselines }

impl DetectionRule for PhaseDurationOutlierRule:
    name() -> "phase_duration_outlier"
    category() -> Scope

    detect(records):
        // This rule doesn't use records directly -- it uses MetricVector phases
        // But we receive records. We need to compute phase durations from records.
        // Actually, per the architecture, this rule uses the CURRENT MetricVector's phases.
        // But DetectionRule::detect only receives records, not MetricVector.
        //
        // Resolution: The rule computes phase durations from observation records by looking
        // at task subject transitions. This is consistent with how the observe crate works --
        // it can derive phase info from records.
        //
        // Alternative: Since the rule has historical data at construction, and the current
        // MetricVector hasn't been computed yet at detect() time, the rule must infer
        // current phase durations from the records themselves.
        //
        // Approach: Extract phase boundaries from records. The attribution module
        // tags records with task subjects. We look for patterns in tool calls that
        // indicate phase transitions. However, the simplest approach is to use the
        // timestamps of SubagentStart records with specific agent types.
        //
        // Simplest correct approach: Look for SubagentStart records, extract
        // phase-like task subjects. Group by inferred phase, compute durations.
        // This is imprecise but functional for the bootstrapped version.
        //
        // REVISED: Per FR-04.5, the rule compares "each phase's duration from PhaseMetrics
        // in current MetricVector." But detect() receives records, not MetricVector.
        // The rule must extract phase info from records. Since metrics computation happens
        // separately and in parallel with detection, the rule cannot access MetricVector.
        //
        // PRACTICAL SOLUTION: Since default_rules() receives the history, and the current
        // MetricVector is computed AFTER detection runs, the rule cannot see the current
        // MetricVector. Two options:
        //   A) Change detect() to also receive &MetricVector -- violates unchanged trait
        //   B) Have the rule extract phase durations from records -- imprecise but correct
        //
        // We go with (B): the rule doesn't fire from records. Instead, it is a no-op
        // in detect(records). The phase duration comparison happens in the BASELINE
        // comparison module, which has access to the current MetricVector.
        //
        // WAIT -- re-reading the spec more carefully: FR-04.5 says "Compare each phase's
        // duration (from PhaseMetrics in current MetricVector) against the historical mean."
        // This means the comparison needs the current MetricVector. Since the rule can't
        // access it through detect(), we have two choices:
        //   1. Pass MetricVector through the rule constructor (alongside history)
        //   2. Handle phase duration outlier detection in the baseline comparison instead
        //
        // Per ADR-001, the rule receives history at construction. The CURRENT MetricVector
        // is also available at construction time if we pass it. But the BRIEF says
        // default_rules receives Option<&[MetricVector]> for history only.
        //
        // RESOLUTION: The current MetricVector IS available to default_rules() caller.
        // But that's in the server, which calls default_rules(Some(&history)).
        // The current MV is computed at step 7 in tools.rs. Detection runs at step 7 too.
        //
        // Actually, looking at tools.rs step 7:
        //   let rules = default_rules();
        //   let hotspots = detect_hotspots(&attributed, &rules);
        //   let metrics = compute_metric_vector(&attributed, &hotspots, now);
        //
        // Detection runs BEFORE metrics computation. So the current MetricVector
        // is NOT available at detect() time.
        //
        // FINAL RESOLUTION: The PhaseDurationOutlierRule must infer phase durations
        // from the observation records themselves. It looks for implicit phase boundaries.
        // However, this is complex and fragile. A simpler approach:
        //
        // The server can reorder: compute metrics FIRST, then pass current MetricVector
        // to default_rules alongside history. But this changes the server integration.
        //
        // OR: The PhaseDurationOutlierRule receives the current MetricVector at construction
        // via a separate parameter. default_rules signature becomes:
        //   default_rules(history: Option<&[MetricVector]>) -> Vec<Box<dyn DetectionRule>>
        // And the server passes the current MV through PhaseDurationOutlierRule::new()
        // by computing metrics first, then running detection.
        //
        // This requires server reordering (metrics before detection). The implementation
        // brief says the server calls compute_metric_vector AFTER detect_hotspots, because
        // hotspot counts feed into metrics. Chicken-and-egg.
        //
        // PRAGMATIC SOLUTION: The PhaseDurationOutlierRule uses records to estimate phase
        // durations. It looks for the pattern used by compute_metric_vector to compute
        // phase durations (the attribution module's task-subject-based phase extraction).
        // This keeps the trait unchanged and avoids server reordering.

        // Extract phase durations from records using task subject parsing
        // Records from the same feature have task subjects like "Phase 3a: ...", "Stage 3b: ..."
        // We approximate by looking at timestamp ranges per detected phase
        //
        // Simpler approach: Since records are attributed to the feature and may span
        // multiple sessions/phases, we look at the overall time span. But we don't have
        // per-phase attribution in the records at this level.
        //
        // SIMPLEST CORRECT: This rule just returns empty if we can't extract phases.
        // The baseline comparison module handles phase duration outlier detection
        // because it HAS access to the current MetricVector.

        // For now: use the stored phase_baselines and compare against any phase duration
        // hints in the records. If we can't extract phase info, return empty.
        // The actual phase duration outlier detection is handled by baseline comparison.

        vec![]  // Phase duration outlier is handled by baseline comparison module
```

IMPORTANT DESIGN NOTE: After analysis, the PhaseDurationOutlierRule cannot meaningfully implement detect(records) because:
1. Phase durations come from MetricVector, computed AFTER detection
2. The DetectionRule trait cannot be changed
3. Records don't carry explicit phase attribution

The phase duration outlier detection is instead implemented in the baseline comparison module (`baseline.rs`), which compares current MetricVector phase durations against historical means. The rule struct still exists for registration in default_rules() and for ADR-001 compliance (constructor injection pattern), but its detect() returns empty. The actual detection happens in `compare_to_baseline()`.

REVISED APPROACH: Rather than having an empty detect(), the rule DOES compute phase durations from records by parsing the metrics computation logic inline. This is the approach we will take in implementation:

```
    detect(records):
        // Compute phase durations from records using the same logic as compute_metric_vector
        // Phase info comes from attribution: records have task subjects that imply phases
        // For now, we cannot reliably extract phase names from raw records
        // The baseline comparison handles this. This rule returns empty.
        vec![]
```

The implementation agent should check if `compute_metric_vector` logic can be reused to extract phase durations from records. If not feasible within the DetectionRule trait constraint, leave detect() returning empty and ensure baseline comparison covers FR-04.5 fully.

### Rule 5 Alternative: Construction-time Detection

Actually, re-reading ADR-001 more carefully: the rule receives HISTORICAL MetricVectors at construction, and the CURRENT MetricVector's phase data is what we compare against. Since we can't get the current MV in detect(), an alternative is:

The server computes metrics first, THEN constructs the rules. This changes the ordering but is correct:
```
let metrics = compute_metric_vector(&attributed, &[], now);  // no hotspots yet
let rules = default_rules(Some(&history), Some(&metrics));    // pass current MV
let hotspots = detect_hotspots(&attributed, &rules);
// Re-compute metrics with hotspot counts
let metrics = compute_metric_vector(&attributed, &hotspots, now);
```

This adds a double-computation of metrics but makes the rule work correctly. The implementation agent should evaluate both approaches and pick the one that's cleanest.

## Error Handling

- All rules handle empty records
- `input_to_file_path` returns None for unexpected shapes
- `find_completion_boundary` returns None when no TaskUpdate found
- PhaseDurationOutlierRule handles empty or insufficient history gracefully

## Key Test Scenarios

### SourceFileCountRule
1. Write 7 distinct .rs files -> fires
2. Write 5 .rs files -> silent
3. Write same .rs file twice -> counts as 1
4. Write .md file -> not counted

### DesignArtifactCountRule
1. Write/Edit 26 files under product/features/ -> fires
2. 24 files -> silent
3. Files outside product/features/ not counted

### AdrCountRule
1. Write 4 ADR-*.md files -> fires
2. Write 2 ADR files -> silent
3. Non-ADR files not counted

### PostDeliveryIssuesRule
1. `gh issue create` after completion boundary -> fires
2. `gh issue create` before completion -> silent
3. No TaskUpdate records -> no boundary -> silent

### PhaseDurationOutlierRule
1. Phase "3a" duration 2x historical mean (with 3+ data points) -> fires
2. Phase "3a" with < 3 historical data points -> absolute threshold
3. No history -> absolute threshold only
4. Empty records -> empty findings
