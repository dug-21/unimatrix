# Pseudocode: detection-rules

**Wave**: 3 (depends on Wave 1 observation-record, Wave 2 domain-pack-registry)
**Crate**: `unimatrix-observe`
**Files modified**:
- `crates/unimatrix-observe/src/detection/mod.rs`
- `crates/unimatrix-observe/src/detection/agent.rs`
- `crates/unimatrix-observe/src/detection/friction.rs`
- `crates/unimatrix-observe/src/detection/session.rs`
- `crates/unimatrix-observe/src/detection/scope.rs`
- `crates/unimatrix-observe/src/extraction/recurring_friction.rs`
- `crates/unimatrix-observe/src/extraction/knowledge_gap.rs`
- `crates/unimatrix-observe/src/extraction/implicit_convention.rs`
- `crates/unimatrix-observe/src/extraction/file_dependency.rs`
- `crates/unimatrix-observe/src/extraction/dead_knowledge.rs`
- `crates/unimatrix-observe/src/session_metrics.rs`
- `crates/unimatrix-observe/src/types.rs`
- `crates/unimatrix-observe/tests/extraction_pipeline.rs`

## Purpose

Rewrite all 21 detection rules and all 5 extraction rules to replace `HookType` enum
match arms with string comparisons. Add mandatory `source_domain == "claude-code"`
guard as the first operation in every domain-specific rule's `detect()` method.
Add `domain_rules()` function to `detection/mod.rs`.

## detection/mod.rs Changes

### Remove HookType import

```
-- DELETE: use crate::types::{..., HookType, ...}
-- KEEP: use crate::types::{HotspotCategory, HotspotFinding, MetricVector, ObservationRecord}
```

### Updated test fixture helpers

All `make_pre`, `make_post`, `make_subagent_start`, `make_subagent_stop`,
`make_bash_with_input`, `make_record_in_session` in the `mod tests` block must be
updated to use `event_type` and `source_domain` (see observation-record.md for the
canonical helper pattern).

No more `hook: HookType::PreToolUse` — use `event_type: "PreToolUse".to_string()`,
`source_domain: "claude-code".to_string()`.

### find_completion_boundary helper (source_domain guard)

This helper is called by `PostCompletionWorkRule` and `PostDeliveryIssuesRule` with
pre-filtered slices. The helper itself does NOT need a source_domain guard because it
operates only on `tool` and `input` fields which are claude-code specific. The caller
is responsible for filtering before calling this helper (ADR-005).

No change to the function signature. But callers must pass the pre-filtered slice.

### New function: domain_rules

```
/// Return RuleEvaluator instances for all DSL rules registered in a domain pack.
/// Returns empty vec for the "claude-code" pack (its rules are Rust impls in default_rules()).
pub fn domain_rules(pack: &DomainPack) -> Vec<Box<dyn DetectionRule>>:
    pack.rules
        .iter()
        .map(|descriptor| {
            Box::new(RuleEvaluator::new(descriptor.clone())) as Box<dyn DetectionRule>
        })
        .collect()
```

Import at top of `mod.rs`:
```
use crate::domain::{DomainPack, RuleEvaluator};
```

## Mandatory source_domain Guard Pattern (ADR-005)

Every domain-specific `detect()` must begin with this preamble. The preamble creates a
filtered local slice; all subsequent logic operates only on this slice.

```
-- Standard preamble for ALL claude-code rules:
let records: Vec<&ObservationRecord> = records
    .iter()
    .filter(|r| r.source_domain == "claude-code")
    .collect();
-- All code below this line uses `records` (the filtered local), NOT the input slice
```

This is a spec-level contract (ADR-005). The gate-3a checklist verifies it.

## detection/agent.rs — 7 Rules

All rules remove `use crate::types::{..., HookType, ...}` and add the source_domain guard.

### Rule 1: ContextLoadRule

Old filter: `record.hook == HookType::PostToolUse`
New logic:
```
fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding>:
    -- source_domain guard (FIRST)
    let records: Vec<&ObservationRecord> = records.iter()
        .filter(|r| r.source_domain == "claude-code").collect()

    let mut sorted = records.clone()
    sorted.sort_by_key(|r| r.ts)

    let mut total_kb = 0.0
    let mut evidence = Vec::new()

    for record in sorted:
        -- OLD: if record.hook == HookType::PostToolUse
        if record.event_type == "PostToolUse":
            let tool = record.tool.as_deref().unwrap_or("")
            if tool == "Write" || tool == "Edit": break
            if tool == "Read":
                if let Some(size) = record.response_size: ...  -- unchanged
    ...  -- threshold comparison unchanged
```

### Rule 2: LifespanRule

Old match: `match record.hook { HookType::SubagentStart => ..., HookType::SubagentStop => ..., _ => {} }`
New logic:
```
fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding>:
    let records: Vec<&ObservationRecord> = records.iter()
        .filter(|r| r.source_domain == "claude-code").collect()

    for record in &records:
        -- OLD: match record.hook { HookType::SubagentStart => ..., HookType::SubagentStop => ... }
        if record.event_type == "SubagentStart":
            let agent_type = record.tool.as_deref().unwrap_or("unknown")
            starts.entry(&record.session_id).or_default().push((record.ts, agent_type))
        else if record.event_type == "SubagentStop":
            stops.entry(&record.session_id).or_default().push(record.ts)
    ...  -- pairing and duration logic unchanged
```

### Rule 3: FileBreadthRule

No HookType dependency currently — only checks `record.tool`. BUT it does not filter
on event_type currently, so it counts file accesses from both Pre and Post events.
After Wave 1, the logic is unchanged EXCEPT for the mandatory preamble guard:

```
fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding>:
    let records: Vec<&ObservationRecord> = records.iter()
        .filter(|r| r.source_domain == "claude-code").collect()
    -- Remaining logic: unchanged (operates on record.tool and record.input)
```

### Rule 4: RereadRateRule

Same as FileBreadthRule — no HookType dependency, just needs the source_domain guard:
```
fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding>:
    let records: Vec<&ObservationRecord> = records.iter()
        .filter(|r| r.source_domain == "claude-code").collect()
    -- Remaining logic: unchanged
```

### Rule 5: MutationSpreadRule

Same pattern as FileBreadthRule:
```
fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding>:
    let records: Vec<&ObservationRecord> = records.iter()
        .filter(|r| r.source_domain == "claude-code").collect()
    -- Remaining logic: unchanged
```

### Rule 6: CompileCyclesRule

Old filter: `record.hook == HookType::PreToolUse`
```
fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding>:
    let records: Vec<&ObservationRecord> = records.iter()
        .filter(|r| r.source_domain == "claude-code").collect()

    for record in &records:
        -- OLD: if record.tool.as_deref() == Some("Bash") && record.hook == HookType::PreToolUse
        if record.tool.as_deref() == Some("Bash") && record.event_type == "PreToolUse":
            ...  -- compile command detection unchanged
```

### Rule 7: EditBloatRule

Old filter: `record.hook == HookType::PostToolUse`
```
fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding>:
    let records: Vec<&ObservationRecord> = records.iter()
        .filter(|r| r.source_domain == "claude-code").collect()

    for record in &records:
        -- OLD: record.tool.as_deref() == Some("Edit") && record.hook == HookType::PostToolUse
        let is_edit_post = record.tool.as_deref() == Some("Edit")
            && record.event_type == "PostToolUse"
        ...  -- threshold logic unchanged
```

## detection/friction.rs — 4 Rules

### Rule 1: PermissionRetriesRule

Old match: `match record.hook { HookType::PreToolUse => ..., HookType::PostToolUse => ..., _ => {} }`
```
fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding>:
    let records: Vec<&ObservationRecord> = records.iter()
        .filter(|r| r.source_domain == "claude-code").collect()

    for record in &records:
        if let Some(tool) = &record.tool:
            -- OLD: match record.hook
            if record.event_type == "PreToolUse":
                *pre_counts.entry(tool.clone()).or_default() += 1
                ...
            else if record.event_type == "PostToolUse":
                *post_counts.entry(tool.clone()).or_default() += 1
    ...  -- threshold logic unchanged
```

### Rule 2: SleepWorkaroundsRule

No HookType dependency currently — just add source_domain guard:
```
fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding>:
    let records: Vec<&ObservationRecord> = records.iter()
        .filter(|r| r.source_domain == "claude-code").collect()
    -- Remaining logic: unchanged
```

### Rule 3: SearchViaBashRule

Old filter: `record.hook == HookType::PreToolUse`
```
fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding>:
    let records: Vec<&ObservationRecord> = records.iter()
        .filter(|r| r.source_domain == "claude-code").collect()

    for record in &records:
        -- OLD: if record.tool.as_deref() == Some("Bash") && record.hook == HookType::PreToolUse
        if record.tool.as_deref() == Some("Bash") && record.event_type == "PreToolUse":
            ...  -- search command detection unchanged
```

### Rule 4: OutputParsingStruggleRule

Old filter: `record.hook == HookType::PreToolUse`
```
fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding>:
    let records: Vec<&ObservationRecord> = records.iter()
        .filter(|r| r.source_domain == "claude-code").collect()

    let mut sorted = records.clone()
    sorted.sort_by_key(|r| r.ts)

    for record in &sorted:
        -- OLD: if record.tool.as_deref() == Some("Bash") && record.hook == HookType::PreToolUse
        if record.tool.as_deref() == Some("Bash") && record.event_type == "PreToolUse":
            ...  -- piped command detection unchanged
```

Note: The existing `sorted` variable in `OutputParsingStruggleRule` uses the full
records slice. After adding the preamble, `sorted` must use the filtered slice.

## detection/session.rs — 5 Rules

All session rules need the source_domain preamble. The existing HookType comparisons
convert as follows:

- `HookType::PreToolUse` → `event_type == "PreToolUse"`
- `HookType::PostToolUse` → `event_type == "PostToolUse"`
- `HookType::SubagentStart` → `event_type == "SubagentStart"`
- `HookType::SubagentStop` → `event_type == "SubagentStop"`

Standard preamble for all 5 rules:
```
let records: Vec<&ObservationRecord> = records.iter()
    .filter(|r| r.source_domain == "claude-code").collect()
```

`PostCompletionWorkRule` and related rules that call `find_completion_boundary` must
pass the pre-filtered slice to that helper (the helper receives `&[&ObservationRecord]`
or the implementor adjusts the helper signature to accept `&[&ObservationRecord]`).

Implementor note: `find_completion_boundary` currently accepts `&[ObservationRecord]`
(owned slice). After the source_domain guard, callers have `Vec<&ObservationRecord>`.
Two options:
1. Change `find_completion_boundary` to accept `&[&ObservationRecord]`
2. Keep the signature, pass only the filtered records (clone or collect owned)

Option 1 is preferred to avoid cloning. Adjust the helper signature accordingly.
If the change would cascade to too many callsites, use a type alias or wrapper.

## detection/scope.rs — 5 Rules

Same source_domain preamble pattern. Scope rules operate on session-level data
(file counts, artifact counts, phase durations). Add the preamble; all other
logic is unchanged.

`PhaseDurationOutlierRule` is constructor-injected with history data
(`Option<&[MetricVector]>`). The source_domain preamble is still needed in `detect()`
to ensure phase timing is computed only from claude-code records.

## extraction/ — 5 Extraction Rules

The extraction rules are in `unimatrix-observe/src/extraction/`. They contain
`HookType` match arms. All must be updated to string comparisons with
`source_domain == "claude-code"` guards.

Pattern for all extraction rules:
```
-- Replace all: r.hook == HookType::X
-- With:        r.event_type == "X" && r.source_domain == "claude-code"

-- Or with pre-filter at the start of the function:
let records: Vec<&ObservationRecord> = records.iter()
    .filter(|r| r.source_domain == "claude-code").collect()
```

The extraction rules (`recurring_friction.rs`, `knowledge_gap.rs`,
`implicit_convention.rs`, `file_dependency.rs`, `dead_knowledge.rs`) should use the
same pre-filter preamble for consistency with the detection rules.

## session_metrics.rs Changes

Update any field references from `record.hook` to `record.event_type` and
`record.source_domain`. Apply `source_domain == "claude-code"` guard where relevant.

## tests/extraction_pipeline.rs Changes (Wave 4 partial)

All `ObservationRecord` construction sites must be updated to supply both `event_type`
and `source_domain`. This is formally a Wave 4 task but logically belongs here for
understanding: every record constructed for a claude-code test must have
`source_domain: "claude-code".to_string()`. Any test record without this will
silently pass through the source_domain guard with no findings (R-03).

Static verification: `grep -r 'source_domain: ""' unimatrix-observe/tests/` must
return zero matches after Wave 4.

## Error Handling

All `detect()` functions remain infallible — they return `Vec<HotspotFinding>` (empty
on no findings). Panics within rule implementations must not crash the server; they are
caught at the `spawn_blocking` boundary (FM-03). No new error variants in this component.

## Key Test Scenarios

1. **R-01 cross-domain isolation**: for each of the 21 rules, supply a mixed slice with
   `source_domain = "claude-code"` records that should fire AND `source_domain = "unknown"`
   records that resemble the trigger pattern. Assert zero findings from unknown records.

2. **R-02 backward compatibility**: for each rule, supply the same trigger fixture as
   before the refactor (now with `event_type = "PreToolUse"` etc.) and assert the same
   finding is produced.

3. **EC-06 empty slice**: all 21 rules return `vec![]` without panicking on empty input.

4. **AC-02 test count non-regression**: `cargo test -p unimatrix-observe` rule test
   count does not decrease from baseline.

5. **R-03 test fixture audit**: every `ObservationRecord` construction in test code
   supplies a non-empty `source_domain`.

6. **domain_rules()**: supply a `DomainPack` with two `RuleDescriptor` entries; assert
   `domain_rules(&pack).len() == 2` and the returned rules implement `DetectionRule`.

7. **default_rules() still returns 21 rules**: `default_rules(None).len() == 21`.

8. **detect_hotspots() integration**: combine `default_rules() + domain_rules(sre_pack)`
   for a mixed session; claude-code rules fire only on claude-code events; sre rules fire
   only on sre events.
