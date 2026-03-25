# Component 1: RetrospectiveReport Struct Extensions

**File**: `crates/unimatrix-observe/src/types.rs`
**Action**: Modify — add new fields to `RetrospectiveReport` and `FeatureKnowledgeReuse`;
            add new structs `PhaseStats`, `ToolDistribution`, `GateResult`, `EntryRef`.

---

## Purpose

Extend the `RetrospectiveReport` struct with five new optional fields that carry the new
data produced by Components 2 and 3. Extend `FeatureKnowledgeReuse` with five new fields
for the knowledge split fix (GH#320). Define all new types that the other components depend on.

This is a pure type-definition component — no logic, no DB access, no computation.

---

## New Structs to Add

### `ToolDistribution`

```
struct ToolDistribution {
    read:    u64,
    execute: u64,
    write:   u64,
    search:  u64,
}
derives: Debug, Clone, Serialize, Deserialize, Default
serde: #[serde(default)] on all fields (absent keys default to 0)
```

### `GateResult`

```
enum GateResult {
    Pass,
    Fail,
    Rework,
    Unknown,
}
derives: Debug, Clone, PartialEq, Eq, Serialize, Deserialize
serde: default variant is Unknown
```

`#[serde(default)]` on the enum itself so missing JSON field deserializes to `Unknown`.
Implement `Default for GateResult { fn default() -> Self { GateResult::Unknown } }`.

### `PhaseStats`

```
struct PhaseStats {
    phase:             String,
    pass_number:       u32,         // 1-indexed: which pass this row represents
    pass_count:        u32,         // total passes for this phase name in the cycle
    duration_secs:     u64,
    session_count:     usize,       // distinct session_ids in the phase window
    record_count:      usize,       // total observations in the phase window
    agents:            Vec<String>, // deduplicated agent names, first-seen order
    tool_distribution: ToolDistribution,
    knowledge_served:  u64,
    knowledge_stored:  u64,
    gate_result:       GateResult,
    gate_outcome_text: Option<String>,  // raw outcome string from cycle_phase_end
    hotspot_ids:       Vec<String>,     // populated by formatter, empty from computation
}
derives: Debug, Clone, Serialize, Deserialize
serde:
  - hotspot_ids: #[serde(default, skip_serializing_if = "Vec::is_empty")]
  - gate_outcome_text: #[serde(default, skip_serializing_if = "Option::is_none")]
  - all other fields: required (no default)
```

NOTE: The SPECIFICATION §Domain Models also includes `pass_breakdown: Vec<(u64, u64)>` on
`PhaseStats`. The IMPLEMENTATION-BRIEF does not include this field. The ARCHITECTURE.md does
not include it either. Do NOT add `pass_breakdown` unless the spawn prompt explicitly adds it.
This is an open gap — flag to the implementation agent.

### `EntryRef`

```
struct EntryRef {
    id:            u64,
    title:         String,
    feature_cycle: String,   // source feature that stored this entry
    category:      String,
    serve_count:   u64,      // times served during this cycle's sessions
}
derives: Debug, Clone, Serialize, Deserialize
```

FIELD NAME NOTE: SPECIFICATION §Domain Models uses `source_cycle: String`.
ARCHITECTURE.md and IMPLEMENTATION-BRIEF use `feature_cycle: String`.
Use `feature_cycle` to match the architecture. If spec wins: this is an open gap.
Flag to implementation agent: choose one name and use consistently in all three files.

---

## Modifications to Existing Structs

### `RetrospectiveReport` — five new fields

Insert after the existing `phase_narrative: Option<PhaseNarrative>` field.

```
// New fields — col-026

#[serde(default, skip_serializing_if = "Option::is_none")]
pub goal: Option<String>,

#[serde(default, skip_serializing_if = "Option::is_none")]
pub cycle_type: Option<String>,

#[serde(default, skip_serializing_if = "Option::is_none")]
pub attribution_path: Option<String>,

#[serde(default, skip_serializing_if = "Option::is_none")]
pub is_in_progress: Option<bool>,     // ADR-001: NEVER plain bool

#[serde(default, skip_serializing_if = "Option::is_none")]
pub phase_stats: Option<Vec<PhaseStats>>,
```

### `FeatureKnowledgeReuse` — five new fields

Insert after the existing `category_gaps: Vec<String>` field.
All use `#[serde(default)]`.

```
#[serde(default)]
pub total_served: u64,                              // = delivery_count alias (same value)

#[serde(default)]
pub total_stored: u64,                              // entries created in this cycle

#[serde(default)]
pub cross_feature_reuse: u64,                       // entries from prior cycles served

#[serde(default)]
pub intra_cycle_reuse: u64,                         // entries stored THIS cycle and served

#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub top_cross_feature_entries: Vec<EntryRef>,       // top-5 by serve_count
```

NOTE on `total_served`: this is the same count as `delivery_count` but named for clarity in
the Knowledge Reuse section. The computation sets both: `total_served = delivery_count`.
Implementation agent may derive one from the other rather than duplicating logic.

---

## Modifications to `build_report()` in `report.rs`

`build_report()` constructs a `RetrospectiveReport` literal. When the five new fields are
added to the struct, the literal becomes incomplete → compile error. Add:

```
In build_report() RetrospectiveReport { ... } literal, add:
    goal: None,
    cycle_type: None,
    attribution_path: None,
    is_in_progress: None,
    phase_stats: None,
```

This change is in `crates/unimatrix-observe/src/report.rs`, not types.rs. It is required
immediately when the struct gains new fields.

---

## Test Updates Required in This File

All existing tests that construct `RetrospectiveReport` explicitly (not via `build_report()`)
must add the new five fields. Search for `RetrospectiveReport {` in the types.rs test module —
two occurrences at lines ~515 and ~562 in the current file. Add:

```
goal: None,
cycle_type: None,
attribution_path: None,
is_in_progress: None,
phase_stats: None,
```

All existing tests that construct `FeatureKnowledgeReuse` explicitly must add new fields.
Search for `FeatureKnowledgeReuse {` in the types.rs test module — lines ~458 and ~585. Add:

```
total_served: 0,
total_stored: 0,
cross_feature_reuse: 0,
intra_cycle_reuse: 0,
top_cross_feature_entries: vec![],
```

### New tests to add

**T-RE-01**: `test_phase_stats_serde_roundtrip`
- Construct a `PhaseStats` with all fields populated
- Serialize to JSON, deserialize back
- Assert all fields match

**T-RE-02**: `test_gate_result_default`
- Deserialize JSON without `gate_result` key → assert `GateResult::Unknown`

**T-RE-03**: `test_entry_ref_serde_roundtrip`
- Construct `EntryRef { id: 42, title: "...", feature_cycle: "col-023", category: "decision", serve_count: 5 }`
- Serialize, deserialize, assert field equality

**T-RE-04**: `test_retrospective_report_new_fields_omitted_when_none`
- Construct `RetrospectiveReport` with all five new fields = None
- Serialize to JSON
- Assert none of: "goal", "cycle_type", "attribution_path", "is_in_progress", "phase_stats" appear in JSON

**T-RE-05**: `test_retrospective_report_new_fields_roundtrip`
- Construct with: `goal = Some("test goal")`, `cycle_type = Some("Design")`,
  `attribution_path = Some("cycle_events-first (primary)")`,
  `is_in_progress = Some(true)`, `phase_stats = Some(vec![...])`
- Serialize, deserialize, assert all fields present with correct values

**T-RE-06**: `test_is_in_progress_none_key_absent`
- `is_in_progress = None` → JSON must not contain "is_in_progress" key at all (not null)
- Deserialize JSON without "is_in_progress" key → `is_in_progress = None` (not `Some(false)`)

**T-RE-07**: `test_feature_knowledge_reuse_new_fields_default_zero`
- Deserialize old JSON without new fields → all new fields default to 0 / empty vec
- Assert: total_served=0, total_stored=0, cross_feature_reuse=0, intra_cycle_reuse=0,
  top_cross_feature_entries=[]

**T-RE-08**: `test_pre_col026_json_backward_compat`
- Use the pre-col-026 JSON format (no new fields)
- Deserialize into `RetrospectiveReport`
- Assert: no panic, all new fields are None/0/empty

---

## Error Handling

This component defines types only. No error handling required here.
All error paths are in the components that compute values for these fields.

---

## Key Constraints

- `is_in_progress: Option<bool>` ONLY — ADR-001. Plain `bool` is prohibited.
- All five `RetrospectiveReport` new fields: `#[serde(default, skip_serializing_if = "Option::is_none")]`
- All five `FeatureKnowledgeReuse` new fields: `#[serde(default)]`
- `top_cross_feature_entries`: additionally `skip_serializing_if = "Vec::is_empty"`
- `GateResult` must implement `Default` returning `Unknown`
- `ToolDistribution` must implement `Default` returning all-zero
