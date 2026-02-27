# col-001: Outcome Tracking

## Problem Statement

Unimatrix accumulates knowledge (conventions, decisions, patterns) and tracks how that knowledge is used (access counts, helpfulness votes, co-access patterns). But it has no structured way to record **what happened** as a result of using that knowledge -- whether a gate passed, whether a feature shipped, whether a process worked or failed.

Today, the "outcome" category exists in the category allowlist (`categories.rs` line 9) but nothing in the system treats outcome entries differently from any other entry. An agent can store `category: "outcome"` with freeform content and arbitrary tags, but there is no:

1. **Indexing** -- No way to efficiently query "all outcomes for feature crt-004" or "all gate results across features" without full-text scanning.
2. **Tag structure** -- No convention for how outcome tags should be formatted. Agents could use `gate:3a`, `gate-3a`, `gate_3a`, or `third-gate-alpha` and the system cannot distinguish them.
3. **Aggregation** -- No way to compute "what percentage of gate-3a results are pass vs fail" without loading and parsing every outcome entry.

This matters because Milestone 5's subsequent features depend on structured outcome data. col-002 (Retrospective Pipeline) needs to aggregate outcomes across features to detect process patterns. col-003 (Process Proposal Workflow) needs to reference outcome evidence when proposing improvements. col-004 (Feature Lifecycle) needs gate status tracking. All three are blocked unless outcome data is queryable and structured.

Beyond feature work, Unimatrix supports other workflow types -- bug fixes, incidents, operational tasks, process improvements. Outcomes from these workflows are equally valuable for process analysis. The outcome tracking model must accommodate all workflow types, not just features. The required `type` structured tag enables cross-workflow aggregation without requiring changes to the existing `feature_cycle` field.

The FEATURE_ENTRIES multimap (crt-001) links features to *retrieved* entries. But outcomes are *stored* entries with a different lifecycle -- they are written at the end of a workflow step, not retrieved during one. The feature-cycle-to-outcome relationship needs its own index.

## Goals

1. **OUTCOME_INDEX table** -- Add a new redb table that indexes outcome entries by feature cycle. Key design: `(feature_cycle, entry_id) -> ()` enables O(1) lookup of all outcomes for a given feature cycle, without scanning the entire ENTRIES table. This is the 13th table. The `feature_cycle` field already exists on `EntryRecord` and `NewEntry` — no schema change needed. Non-feature workflows (bug fixes, incidents) use `feature_cycle` as a general-purpose workflow identifier (e.g., `bug-42`, `incident-2026-02-25`), while the required `type` tag distinguishes workflow types for aggregation.

2. **Structured outcome tags** -- Define and enforce a tag schema for outcome entries. Tags follow a `key:value` format with a fixed set of recognized keys (`type`, `gate`, `phase`, `result`, `agent`, `wave`). The `type` tag is **required** for all outcome entries. The system validates structured tags on `context_store` when `category: "outcome"` and rejects malformed tags. Non-structured tags are still permitted alongside structured ones.

3. **Outcome tag parsing and validation** -- Parse structured tags into typed fields at write time. `gate:3a` parses to `OutcomeTag::Gate("3a")`, `result:pass` parses to `OutcomeTag::Result(OutcomeResult::Pass)`, `type:feature` parses to `OutcomeTag::Type(WorkflowType::Feature)`. Reject unknown structured tag keys with a clear error. This parsing happens in the server crate (validation layer), not in the store crate.

4. **OUTCOME_INDEX population on context_store** -- When a `context_store` call creates an entry with `category: "outcome"`, extract the feature cycle from the entry's `feature_cycle` field and insert into OUTCOME_INDEX. This is inline (not fire-and-forget) because outcome indexing is part of the write transaction's correctness, not a side effect.

5. **Outcome querying via context_lookup** -- Extend `context_lookup` to support outcome-specific filtering: `category: "outcome"` combined with structured tag filters (e.g., `tags: ["type:feature", "gate:3a", "result:pass"]`). No new tool -- outcome queries use the existing lookup tool with the existing tag intersection logic, which already handles `key:value` format tags as strings. The `type` tag enables cross-workflow aggregation (e.g., "all feature outcomes") without needing to enumerate feature cycle prefixes.

6. **Outcome aggregation in context_status** -- Extend `StatusReport` with outcome statistics: total outcome entries, outcomes by workflow type, outcomes by result (pass/fail/rework), outcomes by gate. This gives col-002 the aggregated view it needs without requiring that tool to scan entries directly.

7. **Feature-cycle-outcome linkage** -- Populate OUTCOME_INDEX with the `feature_cycle` field from the stored entry. If `feature_cycle` is empty, the outcome is not indexed (orphan outcomes are valid entries but not linked to a workflow). This ensures the index only contains workflow-relevant outcomes.

## Non-Goals

- **No new MCP tool.** Outcomes are stored via `context_store` and queried via `context_lookup` and `context_status`. No `context_outcome` tool.
- **No outcome analysis or pattern detection.** That is col-002 (Retrospective Pipeline). col-001 provides the structured data; col-002 provides the intelligence.
- **No process proposals from outcomes.** That is col-003 (Process Proposal Workflow).
- **No gate status tracking UI.** That is col-004 (Feature Lifecycle) and mtx-003 (Feature Drilldown).
- **No changes to the confidence formula.** Outcome entries are knowledge artifacts, not a confidence signal. They are subject to the same confidence evolution as any entry.
- **No retroactive indexing of existing outcome entries.** If any outcome entries were stored before col-001, they will not be retroactively indexed. A migration could be added later if needed, but the expected count of pre-existing outcomes is zero or negligible.
- **No outcome entry templates or auto-generation.** Agents are responsible for storing outcomes with the correct category, tags, and feature_cycle. col-001 validates and indexes what they provide.
- **No changes to EntryRecord schema.** Outcome data is indexed via OUTCOME_INDEX and TAG_INDEX, not via new fields on EntryRecord. No schema migration.
- **No workflow_type field on the index or EntryRecord.** Workflow type is conveyed via the required `type` structured tag (e.g., `type:feature`, `type:bugfix`), not a separate index or field. TAG_INDEX handles the querying.
- **No rename of `feature_cycle` field.** The existing `feature_cycle` field on `EntryRecord` and `NewEntry` is reused as-is for all workflow types. Renaming would require schema migration and risk bincode deserialization issues. Non-feature workflows simply use `feature_cycle` with their own ID conventions (e.g., `bug-42`).

## Background Research

### Existing Infrastructure

**Category allowlist:** "outcome" is already a valid category (`categories.rs` INITIAL_CATEGORIES). Agents can already store entries with `category: "outcome"`. No allowlist change needed.

**TAG_INDEX (nxs-001):** Multimap `&str -> set of u64`. Tags are stored as plain strings. `key:value` tags like `gate:3a` are valid tag strings and work with the existing tag intersection query. No TAG_INDEX change needed.

**FEATURE_ENTRIES (crt-001):** Multimap `&str -> set of u64`. Links feature IDs to entries that were *retrieved* during that feature's work. This tracks "what knowledge did this feature consume." OUTCOME_INDEX tracks "what results did this feature produce." The two are complementary.

**context_store (vnc-002):** Creates entries with category validation, content scanning, near-duplicate detection, and embedding generation. The `feature_cycle` field exists on `NewEntry` and `EntryRecord` but is not exposed to MCP callers via `StoreParams`. This field needs to be exposed as an MCP parameter (keeping the existing name), enabling agents to associate outcomes (and any entry) with a feature cycle or workflow.

**context_lookup (vnc-002):** Deterministic query with category, topic, tags, status, and limit filters. Already supports tag intersection. Outcome queries work by combining `category: "outcome"` with tags like `["gate:3a", "result:pass"]`. No tool parameter changes needed for basic queries.

**context_status (vnc-003):** Health metrics including entry counts, category distribution, topic distribution, age distribution. Outcome statistics would be a new section in the StatusReport.

**Store::open (db.rs):** Currently opens 12 tables in a write transaction during initialization. OUTCOME_INDEX would be the 13th.

### Tag Schema Design

Structured tags use `key:value` format. Recognized keys:

| Key | Values | Required | Example |
|-----|--------|----------|---------|
| `type` | `feature`, `bugfix`, `incident`, `process` | **yes** | `type:feature` |
| `gate` | any non-empty string | no | `gate:3a` |
| `phase` | `research`, `design`, `implementation`, `testing`, `validation` | no | `phase:implementation` |
| `result` | `pass`, `fail`, `rework`, `skip` | no | `result:pass` |
| `agent` | any agent ID | no | `agent:col-001-agent-1-architect` |
| `wave` | integer | no | `wave:2` |

The `type` tag is **required** for all outcome entries. This enables cross-workflow aggregation without parsing workflow ID prefixes -- e.g., `context_lookup(category: "outcome", tags: ["type:feature", "result:pass"])` returns all passing feature outcomes in one query.

The `gate` tag accepts any non-empty string rather than a hardcoded set, to avoid coupling to protocol specifics. The protocol defines gates `1, 1b, 2a, 2a+, 2b, 2c, 3a, 3b, 3c` but new gate identifiers should not require code changes.

Tags that do not contain `:` are treated as plain tags (backward compatible). Tags with `:` where the key is not in the recognized set are rejected with a validation error. This prevents typos from silently creating unqueryable outcomes.

### Scale Considerations

At Unimatrix's expected scale, a feature cycle produces 5-20 outcome entries (one per gate, per wave, per agent report). With 50 completed features, that is 250-1000 outcome entries total. OUTCOME_INDEX storage: 16 bytes per entry (8 bytes feature hash key + 8 bytes entry_id) = ~16KB at 1000 entries. Negligible.

### Feature Cycle as Key

The roadmap specifies `feature_hash` as the OUTCOME_INDEX key. Using a hash rather than a string has one advantage (fixed 8-byte key) and one disadvantage (hash collisions, loss of readability in debugging). Given Unimatrix's scale (<1000 workflows), collisions are negligible. However, the existing FEATURE_ENTRIES table uses `&str` keys (the feature ID string directly). For consistency and debuggability, OUTCOME_INDEX should use the same pattern: `(&str, u64) -> ()` where the string is the feature cycle value (e.g., `col-001`, `bug-42`, `incident-2026-02-25`). This matches TOPIC_INDEX and CATEGORY_INDEX key patterns.

**Decision: Use `(&str, u64) -> ()` for OUTCOME_INDEX, consistent with existing index patterns.** The roadmap's `feature_hash` is reinterpreted as the feature_cycle string, not a numeric hash. The existing `feature_cycle` field is reused without renaming.

### Feature Cycle Naming Conventions

The `feature_cycle` field is used for all workflow types, not just features. Naming conventions by type:

| Workflow Type | ID Pattern | Example |
|---------------|-----------|---------|
| Feature | `{phase}-{NNN}` | `col-001`, `crt-004` |
| Bug fix | `bug-{NNN}` | `bug-42` |
| Incident | `incident-{YYYY-MM-DD}` | `incident-2026-02-25` |
| Process | `process-{name}` | `process-gate-review` |

These are conventions, not enforced at the schema level. The OUTCOME_INDEX key is an opaque string. The `type` structured tag (not the feature_cycle value) is used for workflow type aggregation.

## Proposed Approach

### 1. OUTCOME_INDEX Table

Add to `schema.rs`:
```rust
pub const OUTCOME_INDEX: TableDefinition<(&str, u64), ()> =
    TableDefinition::new("outcome_index");
```

Add to `Store::open` table initialization. This is the 13th table. Schema version remains 2 (no EntryRecord field changes, just a new table).

### 2. Structured Tag Validation

Add an `outcome_tags` module in the server crate with:
- `OutcomeTagKey` enum: `Type`, `Gate`, `Phase`, `Result`, `Agent`, `Wave`
- `WorkflowType` enum: `Feature`, `Bugfix`, `Incident`, `Process`
- `OutcomeResult` enum: `Pass`, `Fail`, `Rework`, `Skip`
- `parse_structured_tag(tag: &str) -> Result<OutcomeTag, ServerError>` -- parses `key:value` format
- `validate_outcome_tags(tags: &[String]) -> Result<(), ServerError>` -- validates all tags when `category == "outcome"`, enforces `type` tag presence

Validation is called in the `context_store` tool handler after category validation, only when `category == "outcome"`.

### 3. context_store Extension

When `category == "outcome"` in `context_store`:
1. Validate structured tags via `validate_outcome_tags` (includes `type` tag required check)
2. After entry insertion, if `feature_cycle` is non-empty, insert `(feature_cycle, entry_id)` into OUTCOME_INDEX within the same write transaction

The `feature_cycle` parameter needs to be exposed in `StoreParams` (the field already exists on `NewEntry` and `EntryRecord` but is not settable via MCP).

### 4. context_status Extension

Add to `StatusReport`:
- `total_outcomes: u64`
- `outcomes_by_type: BTreeMap<String, u64>` -- e.g., `{"feature": 15, "bugfix": 3}`
- `outcomes_by_result: BTreeMap<String, u64>` -- e.g., `{"pass": 12, "fail": 3, "rework": 2}`
- `outcomes_by_feature_cycle: Vec<(String, u64)>` -- top feature cycles by outcome count

Computing outcome stats requires scanning OUTCOME_INDEX (feature cycle count) and TAG_INDEX intersected with category "outcome" (result and type breakdowns).

### 5. StoreParams Extension

Add `feature_cycle: Option<String>` to `StoreParams`. This allows agents to associate outcome entries (and any entry) with a feature cycle or workflow. The field maps directly to `NewEntry.feature_cycle` — no rename needed.

## Acceptance Criteria

- AC-01: `OUTCOME_INDEX` redb table exists with `(&str, u64) -> ()` schema, where the string key is the `feature_cycle` value
- AC-02: `OUTCOME_INDEX` is created during `Store::open` alongside the other 12 tables (13 total)
- AC-03: Structured outcome tags follow `key:value` format with recognized keys: `type`, `gate`, `phase`, `result`, `agent`, `wave`
- AC-04: `context_store` with `category: "outcome"` validates all `key:value` tags against the recognized key set, rejecting unknown keys
- AC-05: Tags without `:` are treated as plain tags and are not validated against the structured key set
- AC-06: `type` tag is **required** for outcome entries; validation rejects outcome entries without a `type` tag
- AC-07: `type` tag values are validated against the set `{feature, bugfix, incident, process}`
- AC-08: `result` tag values are validated against the set `{pass, fail, rework, skip}`
- AC-09: `gate` tag values accept any non-empty string (not coupled to protocol-specific gate identifiers)
- AC-10: When `context_store` creates an outcome entry with non-empty `feature_cycle`, the entry is indexed in OUTCOME_INDEX
- AC-11: Outcome entries with empty `feature_cycle` are stored successfully but not indexed in OUTCOME_INDEX
- AC-12: `StoreParams` includes `feature_cycle: Option<String>` parameter, mapped to `NewEntry.feature_cycle`
- AC-13: `context_lookup` with `category: "outcome"` and structured tags (e.g., `["type:feature", "gate:3a", "result:pass"]`) returns matching outcome entries via existing tag intersection
- AC-14: `context_status` includes outcome statistics: `total_outcomes`, `outcomes_by_type`, `outcomes_by_result`, `outcomes_by_feature_cycle`
- AC-15: OUTCOME_INDEX population is part of the write transaction (not fire-and-forget), ensuring consistency
- AC-16: All structured tag parsing and validation is in the server crate, not the store crate
- AC-17: Existing tests pass with no regressions (12 -> 13 tables in Store::open, StoreParams extension is backward compatible via Option)
- AC-18: Unit tests cover: structured tag parsing, validation acceptance and rejection, `type` tag required enforcement, OUTCOME_INDEX writes and reads, outcome stats computation
- AC-19: Integration tests cover: storing outcome entries via context_store with feature_cycle, querying via context_lookup with outcome tags, outcome stats in context_status
- AC-20: `#![forbid(unsafe_code)]`, no new crate dependencies beyond existing workspace
- AC-21: Schema version remains 2 (no EntryRecord field changes, no field renames)

## Constraints

- **No new crate dependencies.** All functionality uses existing workspace crates (redb, bincode, serde, tokio).
- **`#![forbid(unsafe_code)]`**, edition 2024, MSRV 1.89.
- **Object-safe traits.** Any trait extensions must maintain object safety.
- **Store crate remains domain-agnostic.** OUTCOME_INDEX is a structural index (feature_cycle+entry pairs), not outcome-specific logic. Tag validation and parsing live in the server crate.
- **Backward compatible.** All new parameters are `Option<T>`. Existing tool calls without `feature_cycle` continue to work. Existing entries are unaffected.
- **Test infrastructure is cumulative.** Build on existing test fixtures in unimatrix-store and unimatrix-server.
- **OUTCOME_INDEX must be created in Store::open.** Follow the same pattern as other tables.
- **No EntryRecord schema change.** The `feature_cycle` field already exists on EntryRecord. OUTCOME_INDEX is a secondary index, like TOPIC_INDEX or CATEGORY_INDEX.
- **Consistent key patterns.** OUTCOME_INDEX uses `(&str, u64) -> ()` like TOPIC_INDEX and CATEGORY_INDEX.
- **Outcome tag validation is server-only.** The store crate treats tags as opaque strings. The server crate adds semantic validation for outcome entries.

## Resolved Decisions

1. **`feature_cycle` is reused without renaming.** The existing `feature_cycle` field on `EntryRecord` and `NewEntry` is used as-is for all workflow types (features, bug fixes, incidents, process improvements). Renaming to `workflow_id` was considered but rejected — it would touch all crates and risk bincode deserialization issues (SR-01). Non-feature workflows simply populate `feature_cycle` with their own ID conventions (e.g., `bug-42`).

2. **Workflow type conveyed via required `type` tag, not a separate field.** The `type` structured tag (`type:feature`, `type:bugfix`, etc.) enables cross-workflow aggregation via existing TAG_INDEX — no new index needed. This was chosen over a `workflow_type` field because it composes with existing lookup infrastructure and avoids schema-level additions.

3. **`gate` tag values are open strings.** Any non-empty string is accepted, to avoid coupling to protocol-specific gate identifiers. Format validation only (non-empty).

4. **`feature_cycle` is optional.** Orphan outcomes (no feature_cycle link) are valid entries but not indexed in OUTCOME_INDEX. A warning is included in the tool response when storing an outcome without `feature_cycle`.

## Open Questions

1. **Should existing entries be retroactively indexed if their category is "outcome"?** Proposed as non-goal. If there are pre-existing outcome entries (unlikely), a one-time migration scan could be added. Recommendation: defer unless there is evidence of existing outcomes.

## Tracking

https://github.com/dug-21/unimatrix/issues/40
