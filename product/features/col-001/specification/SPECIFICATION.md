# Specification: col-001 Outcome Tracking

## Objective

Add structured outcome tracking to Unimatrix so that outcome entries are validated, indexed by feature cycle, and aggregated in status reports. This provides the structured data foundation that col-002 (Retrospective Pipeline), col-003 (Process Proposals), and col-004 (Feature Lifecycle) depend on.

## Functional Requirements

### FR-01: OUTCOME_INDEX Table Creation
The store crate defines `OUTCOME_INDEX` as a `TableDefinition<(&str, u64), ()>` and creates it during `Store::open` alongside existing tables. The string key is the `feature_cycle` value.

### FR-02: Structured Tag Parsing
The server crate parses tags containing `:` into key-value pairs. Recognized keys: `type`, `gate`, `phase`, `result`, `agent`, `wave`. Tags without `:` are treated as plain tags.

### FR-03: Tag Key Validation for Outcomes
When `category == "outcome"`, tags with `:` are validated against the recognized key set. Tags with an unrecognized key (e.g., `foo:bar`) are rejected with a descriptive error.

### FR-04: Required `type` Tag
Outcome entries must include a `type` tag. Validation rejects outcome entries without a `type` tag. Valid type values: `feature`, `bugfix`, `incident`, `process`.

### FR-05: `result` Tag Value Validation
When a `result` tag is present on an outcome entry, its value must be one of: `pass`, `fail`, `rework`, `skip`.

### FR-06: Open `gate` Tag Values
The `gate` tag accepts any non-empty string value. No hardcoded gate identifier set.

### FR-07: `phase` Tag Value Validation
When a `phase` tag is present on an outcome entry, its value must be one of: `research`, `design`, `implementation`, `testing`, `validation`.

### FR-08: `agent` and `wave` Tag Format
The `agent` tag accepts any non-empty string (agent ID). The `wave` tag accepts any string that parses as a non-negative integer.

### FR-09: StoreParams `feature_cycle` Parameter
`StoreParams` includes `feature_cycle: Option<String>`. When present, it maps to `NewEntry.feature_cycle`. When absent, `NewEntry.feature_cycle` defaults to empty string.

### FR-10: OUTCOME_INDEX Population on Store
When `context_store` creates an outcome entry with non-empty `feature_cycle`, the server inserts `(feature_cycle, entry_id)` into OUTCOME_INDEX within the same write transaction.

### FR-11: Orphan Outcome Handling
Outcome entries with empty `feature_cycle` are stored successfully. They are NOT indexed in OUTCOME_INDEX. A warning note is included in the response.

### FR-12: Outcome Querying via context_lookup
`context_lookup` with `category: "outcome"` and structured tags (e.g., `["type:feature", "gate:3a"]`) returns matching entries via the existing TAG_INDEX intersection logic. No new tool or parameter.

### FR-13: Outcome Statistics in context_status
`StatusReport` includes: `total_outcomes` (count), `outcomes_by_type` (map), `outcomes_by_result` (map), `outcomes_by_feature_cycle` (top cycles by count).

### FR-14: Non-Outcome Entry Isolation
Tags on non-outcome entries are not validated against the structured key set, regardless of whether they contain `:`. Structured tag validation fires ONLY for `category == "outcome"`.

## Non-Functional Requirements

### NFR-01: Backward Compatibility
All new parameters are `Option<T>`. Existing tool calls without `feature_cycle` continue to work without modification. Existing `StoreParams` JSON without the field deserializes correctly.

### NFR-02: Schema Stability
Schema version remains 2. No EntryRecord field changes. No migration needed for existing databases.

### NFR-03: Performance
OUTCOME_INDEX population adds one table insert to the write transaction (negligible at <1000 outcomes). Status report outcome scanning adds proportional to outcome entry count (negligible at expected scale).

### NFR-04: Safety
`#![forbid(unsafe_code)]` maintained across all crates. No new crate dependencies.

### NFR-05: Transaction Atomicity
OUTCOME_INDEX insert is part of the same write transaction as entry creation. If either fails, both roll back.

## Acceptance Criteria

| AC-ID | Description | Verification Method | Source |
|-------|-------------|--------------------|----|
| AC-01 | OUTCOME_INDEX table exists with `(&str, u64) -> ()` schema | test | SCOPE |
| AC-02 | OUTCOME_INDEX created during Store::open (13 tables total) | test | SCOPE |
| AC-03 | Structured tags follow `key:value` format with recognized keys | test | SCOPE |
| AC-04 | context_store with category "outcome" rejects unknown `key:value` keys | test | SCOPE |
| AC-05 | Tags without `:` pass through as plain tags | test | SCOPE |
| AC-06 | `type` tag required for outcome entries | test | SCOPE |
| AC-07 | `type` values validated: feature, bugfix, incident, process | test | SCOPE |
| AC-08 | `result` values validated: pass, fail, rework, skip | test | SCOPE |
| AC-09 | `gate` accepts any non-empty string | test | SCOPE |
| AC-10 | Outcome with non-empty feature_cycle indexed in OUTCOME_INDEX | test | SCOPE |
| AC-11 | Outcome with empty feature_cycle stored but NOT indexed | test | SCOPE |
| AC-12 | StoreParams includes feature_cycle: Option<String> | test | SCOPE |
| AC-13 | context_lookup with outcome tags returns matching entries | test | SCOPE |
| AC-14 | context_status includes outcome stats | test | SCOPE |
| AC-15 | OUTCOME_INDEX population is transactional (same commit) | test | SCOPE |
| AC-16 | Tag parsing/validation in server crate only | grep | SCOPE |
| AC-17 | Existing tests pass, no regressions | test | SCOPE |
| AC-18 | Unit tests for tag parsing, validation, OUTCOME_INDEX | test | SCOPE |
| AC-19 | Integration tests for store+lookup+status outcome flow | test | SCOPE |
| AC-20 | `#![forbid(unsafe_code)]`, no new dependencies | grep | SCOPE |
| AC-21 | Schema version remains 2 | test | SCOPE |

## Domain Models

### Structured Outcome Tags

```
OutcomeTagKey := Type | Gate | Phase | Result | Agent | Wave

WorkflowType := Feature | Bugfix | Incident | Process
  - Serialized: "feature", "bugfix", "incident", "process"

OutcomeResult := Pass | Fail | Rework | Skip
  - Serialized: "pass", "fail", "rework", "skip"

PhaseValue := Research | Design | Implementation | Testing | Validation
  - Serialized: "research", "design", "implementation", "testing", "validation"
```

### Tag Parsing Rules

1. If tag does not contain `:` → plain tag, no validation
2. Split on first `:` → (key, value)
3. If category is NOT "outcome" → skip validation, store as-is
4. If category IS "outcome":
   - key must be in recognized set {type, gate, phase, result, agent, wave}
   - key-specific value validation applies (see FR-04 through FR-08)
   - unknown key → error

### OUTCOME_INDEX

```
Key: (feature_cycle: &str, entry_id: u64)
Value: ()
Usage: prefix scan on feature_cycle string returns all outcome entry IDs for that workflow
```

### StatusReport Extension

```
total_outcomes: u64           // count of entries with category "outcome"
outcomes_by_type: Vec<(String, u64)>   // e.g., [("feature", 15), ("bugfix", 3)]
outcomes_by_result: Vec<(String, u64)> // e.g., [("pass", 12), ("fail", 3)]
outcomes_by_feature_cycle: Vec<(String, u64)> // top cycles by count
```

## User Workflows

### Workflow 1: Agent Stores a Gate Outcome

```
Agent → context_store(
  content: "Gate 3a passed for col-001. All pseudocode and test plans validated.",
  topic: "col-001",
  category: "outcome",
  tags: ["type:feature", "gate:3a", "result:pass", "phase:implementation"],
  feature_cycle: "col-001",
  agent_id: "col-001-validator"
)
→ Tag validation passes (all keys recognized, type present)
→ Entry stored with ID N
→ OUTCOME_INDEX: ("col-001", N) inserted
→ Response: entry stored, ID N
```

### Workflow 2: Agent Stores Outcome Without feature_cycle

```
Agent → context_store(
  content: "Process review completed.",
  topic: "process-review",
  category: "outcome",
  tags: ["type:process", "result:pass"]
)
→ Tag validation passes
→ Entry stored with ID N
→ OUTCOME_INDEX: NOT populated (feature_cycle empty)
→ Response: entry stored, ID N, warning: "outcome not linked to a workflow"
```

### Workflow 3: Agent Queries Outcomes

```
Agent → context_lookup(
  category: "outcome",
  tags: ["type:feature", "gate:3a", "result:pass"]
)
→ TAG_INDEX intersection: type:feature ∩ gate:3a ∩ result:pass
→ Filtered by CATEGORY_INDEX: outcome
→ Returns matching outcome entries
```

### Workflow 4: Status Report with Outcomes

```
Admin → context_status()
→ Standard status report
→ Plus: Outcome Statistics section
  - Total outcomes: 18
  - By type: feature (15), bugfix (3)
  - By result: pass (12), fail (3), rework (2), skip (1)
  - Top cycles: col-001 (8), crt-004 (5), bug-42 (3)
```

### Workflow 5: Agent Uses Bad Tag Key

```
Agent → context_store(
  category: "outcome",
  tags: ["type:feature", "severity:high"]
)
→ validate_outcome_tags rejects "severity:high" — unknown key
→ Error: "Unknown structured tag key 'severity'. Recognized keys: type, gate, phase, result, agent, wave"
→ Entry NOT stored
```

## Constraints

- **No new crate dependencies.** All functionality uses existing workspace crates.
- **`#![forbid(unsafe_code)]`**, edition 2024, MSRV 1.89.
- **Object-safe traits.** Any trait extensions maintain object safety.
- **Store crate domain-agnostic.** OUTCOME_INDEX is structural; tag validation is server-only.
- **Backward compatible.** All new parameters are `Option<T>`.
- **Test infrastructure cumulative.** Build on existing fixtures.
- **OUTCOME_INDEX created in Store::open.** Same pattern as other tables.
- **Consistent key patterns.** OUTCOME_INDEX uses `(&str, u64) -> ()`.

## Dependencies

- `unimatrix-store` — OUTCOME_INDEX table definition, Store::open extension
- `unimatrix-server` — outcome_tags module, tool handler extensions, StatusReport extension
- `redb` — TableDefinition, write transactions (existing dependency)
- `serde`, `schemars` — StoreParams field derivation (existing dependency)

## NOT in Scope

- No new MCP tool (no `context_outcome`)
- No outcome analysis or pattern detection (col-002)
- No process proposals (col-003)
- No gate status tracking UI (col-004, mtx-003)
- No confidence formula changes
- No retroactive indexing of pre-existing outcome entries
- No outcome templates or auto-generation
- No workflow_type field on EntryRecord or index
- No rename of `feature_cycle` field
- No schema migration
