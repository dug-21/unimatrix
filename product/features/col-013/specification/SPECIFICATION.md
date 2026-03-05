# Specification: col-013 Extraction Rule Engine

## Functional Requirements

### FR-01: ExtractionRule Trait

**FR-01.1**: Define `ExtractionRule` trait in `unimatrix-observe::extraction`:
```rust
pub trait ExtractionRule: Send {
    fn name(&self) -> &str;
    fn evaluate(&self, observations: &[ObservationRecord], store: &Store) -> Vec<ProposedEntry>;
}
```

**FR-01.2**: Define `ProposedEntry` struct:
```rust
pub struct ProposedEntry {
    pub title: String,
    pub content: String,
    pub category: String,
    pub topic: String,
    pub tags: Vec<String>,
    pub source_rule: String,        // rule name that produced this
    pub source_features: Vec<String>, // feature cycles that contributed
    pub extraction_confidence: f64, // rule's confidence in this extraction [0.0, 1.0]
}
```

**FR-01.3**: Provide `default_extraction_rules() -> Vec<Box<dyn ExtractionRule>>` returning all 5 rules (mirrors `default_rules()` pattern from detection).

### FR-02: Knowledge Gap Rule

**FR-02.1**: Scan observations for `context_search` tool calls (hook=PreToolUse, tool="mcp__unimatrix__context_search") with zero results (response_size=0 or response contains "No results" in snippet).

**FR-02.2**: Group zero-result searches by query pattern (normalized: lowercase, trimmed).

**FR-02.3**: If the same query pattern appears across 2+ distinct feature cycles, produce a ProposedEntry:
- category: "gap"
- title: "Knowledge gap: {query pattern}"
- content: "Agents searched for '{query}' across features [{features}] with no results. This topic may need explicit documentation."
- extraction_confidence: min(0.8, 0.4 + 0.1 * feature_count)
- tags: ["auto-extracted", "knowledge-gap"]

### FR-03: Implicit Convention Rule

**FR-03.1**: Scan observations for file access patterns (Read, Write, Edit tool calls) extracting file paths from input.

**FR-03.2**: Identify path patterns that appear in 100% of observed features (e.g., every feature reads `CLAUDE.md`, every feature writes to `product/features/`).

**FR-03.3**: Require minimum 3 features observed to establish a convention (avoids false positives from small sample sizes).

**FR-03.4**: Produce a ProposedEntry:
- category: "convention"
- title: "Convention: {pattern description}"
- content: Description of the pattern and its consistency
- extraction_confidence: min(0.9, 0.5 + 0.05 * feature_count)
- tags: ["auto-extracted", "implicit-convention"]

### FR-04: Dead Knowledge Rule

**FR-04.1**: Query the Store for entries that were accessed (access_count > 0, or presence in co-access pairs) during earlier features but have not been accessed in the most recent 5 features.

**FR-04.2**: "Access during earlier features" is determined by comparing the entry's `last_accessed_at` timestamp against feature session timestamps from the SESSIONS table.

**FR-04.3**: Produce a ProposedEntry with deprecation signal:
- category: "lesson-learned"
- title: "Possible dead knowledge: {entry title}"
- content: "Entry '{title}' (ID: {id}) was accessed in features [{earlier}] but has not been accessed in the last 5 features [{recent}]. Consider deprecating."
- extraction_confidence: 0.5 (moderate -- could be context-dependent dormancy)
- tags: ["auto-extracted", "dead-knowledge", "deprecation-signal"]

### FR-05: Recurring Friction Rule

**FR-05.1**: Run the existing `DetectionRule`s against observations from each feature.

**FR-05.2**: Identify hotspot rule_names that fire in 3+ distinct features.

**FR-05.3**: Produce a ProposedEntry:
- category: "lesson-learned"
- title: "Recurring friction: {rule_name}"
- content: Description of the recurring hotspot, affected features, and aggregate severity
- extraction_confidence: min(0.85, 0.5 + 0.1 * feature_count)
- tags: ["auto-extracted", "recurring-friction"]

### FR-06: File Dependency Rule

**FR-06.1**: Scan observations for consistent read-before-edit chains: within the same session, a Read of file A consistently followed by a Write/Edit of file B (within a configurable time window, default 60 seconds).

**FR-06.2**: A dependency chain must appear in 3+ distinct features.

**FR-06.3**: Produce a ProposedEntry:
- category: "pattern"
- title: "File dependency: {file_a} -> {file_b}"
- content: Description of the read-before-edit pattern and its consistency
- extraction_confidence: min(0.8, 0.4 + 0.1 * feature_count)
- tags: ["auto-extracted", "file-dependency"]

### FR-07: Quality Gate Pipeline

**FR-07.1**: Rate limit check: maintain an in-memory counter of extractions in the current clock hour. Reject if count >= 10. Counter resets each hour.

**FR-07.2**: Content validation: reject if title length < 10 chars, content length < 20 chars, or category not in the category allowlist.

**FR-07.3**: Cross-feature validation: reject if `source_features.len() < minimum` where minimum depends on rule (2 for knowledge-gap, 3 for convention/friction/dependency, 5 for dead-knowledge).

**FR-07.4**: Confidence floor: reject if `extraction_confidence < 0.2`.

**FR-07.5**: Near-duplicate check: embed the proposed entry's title+content, search HNSW for top-1 neighbor. Reject if cosine similarity >= 0.92 (reuse existing dedup threshold from context_store).

**FR-07.6**: Point-of-insertion contradiction check: call `check_entry_contradiction()` (ADR-006). Reject if a contradiction is detected (conflict_score > 0.0 at default sensitivity).

**FR-07.7**: Return structured rejection reasons for logging/diagnostics.

### FR-08: Auto-Entry Storage

**FR-08.1**: Entries passing the quality gate are stored via the same `StoreService` path as `context_store`, with:
- `trust_source: "auto"`
- `created_by: "system:extraction:{rule_name}"`
- `feature_cycle`: the most recent feature cycle from source observations
- `status: Active`
- `confidence`: computed by `compute_confidence()` (which will use the new "auto" trust_score)

**FR-08.2**: Provenance metadata stored in tags: `["auto-extracted", "rule:{rule_name}", "source-features:{comma-separated}"]`.

### FR-09: Background Maintenance Tick

**FR-09.1**: Launch a single `tokio::spawn` task during server startup that runs `tokio::time::interval(Duration::from_secs(900))` (15 minutes).

**FR-09.2**: Each tick executes maintenance operations (relocated from `StatusService::run_maintenance()`):
1. Co-access stale pair cleanup (>30 day pairs)
2. Confidence refresh (batch 100 stale entries)
3. HNSW graph compaction (if stale_ratio > 10%)
4. Session GC (timed-out sessions)
5. Observation retention cleanup (>90 days)

**FR-09.3**: Each tick runs the extraction pipeline (FR-01 through FR-08) on observations accumulated since the last watermark.

**FR-09.4**: Track tick metadata: `last_run` (epoch seconds), `duration_ms`, `maintenance_items` (count), `extractions_proposed`, `extractions_accepted`, `extractions_rejected`.

**FR-09.5**: Log tick start and completion at INFO level. Log individual errors at WARN level. Log if 2x interval passes without a successful tick at WARN level.

### FR-10: context_status Changes

**FR-10.1**: The `maintain` parameter on `ContextStatusParams` is silently ignored. The parameter remains in the struct for backward compatibility but has no effect.

**FR-10.2**: `StatusReport` gains new fields:
- `last_maintenance_run: Option<u64>` -- epoch seconds of last successful maintenance tick
- `next_maintenance_scheduled: Option<u64>` -- epoch seconds of next scheduled tick
- `extraction_stats: Option<ExtractionStats>` -- extraction pipeline statistics
- `coherence_by_source: HashMap<String, f64>` -- per-trust_source lambda breakdown

**FR-10.3**: `ExtractionStats` struct:
```rust
pub struct ExtractionStats {
    pub entries_extracted_total: u64,
    pub entries_rejected_total: u64,
    pub last_extraction_run: Option<u64>,
    pub rules_fired: HashMap<String, u64>,  // rule_name -> count of entries produced
}
```

### FR-11: CRT Refactors

**FR-11.1** (crt-002): In `unimatrix-engine/src/confidence.rs`, add `"auto" => 0.35` to the `trust_score()` match arm. Place between "agent" (0.5) and the catch-all (0.3).

**FR-11.2** (crt-003): Extract `check_entry_contradiction()` from `scan_contradictions()` in `unimatrix-server/src/infra/contradiction.rs`. The batch function should be refactored to call the single-entry function internally (reducing code duplication).

**FR-11.3** (crt-005): Add `coherence_by_source: HashMap<String, f64>` computation to `StatusService::compute_status()`. Group active entries by `trust_source`, compute lambda dimensions for each group, report per-source coherence.

**FR-11.4** (crt-005): Relocate the body of `StatusService::run_maintenance()` to a standalone `maintenance_tick()` function callable from the background loop. `run_maintenance()` becomes a thin wrapper (or is removed) once the background tick is the sole maintenance path.

### FR-12: Type Migration

**FR-12.1**: Move `ObservationRecord`, `HookType`, `ParsedSession`, `ObservationStats` from `unimatrix-observe::types` to `unimatrix-core`.

**FR-12.2**: Add `pub use` re-exports in `unimatrix-observe::types` and `unimatrix-observe::lib.rs` so existing consumers see no change.

**FR-12.3**: Add `serde_json` dependency to `unimatrix-core` Cargo.toml (required by `ObservationRecord.input: Option<serde_json::Value>`).

**FR-12.4**: Update imports in all affected files (~14 files across unimatrix-observe and unimatrix-server).

## Non-Functional Requirements

**NFR-01**: Background tick must not block MCP request handling. All CPU-bound work (store access, embedding, compaction) runs via `spawn_blocking`.

**NFR-02**: Extraction tick must complete within 30 seconds under normal conditions (< 10,000 new observations). If exceeded, log a warning and continue.

**NFR-03**: Memory usage for extraction state (watermark, rate limit counter, tick metadata) must be < 1KB.

**NFR-04**: All existing tests (1025+ unit, 174+ integration) must continue to pass after CRT refactors and type migration.

## Domain Model

### New Types (in unimatrix-observe::extraction)

| Type | Description |
|------|-------------|
| `ExtractionRule` (trait) | Interface for knowledge extraction rules |
| `ProposedEntry` | Output of an extraction rule before quality gate |
| `QualityGateResult` | Accept or Reject with structured reason |
| `ExtractionStats` | Aggregate statistics for status reporting |
| `ExtractionContext` | Shared state for extraction pipeline (watermark, rate counter) |

### Modified Types

| Type | Change |
|------|--------|
| `StatusReport` | +last_maintenance_run, +next_maintenance_scheduled, +extraction_stats, +coherence_by_source |
| `unimatrix-core` | +ObservationRecord, +HookType, +ParsedSession, +ObservationStats |

## Acceptance Criteria Summary

| AC | Requirement | Verifiable By |
|----|-------------|---------------|
| AC-01 | ExtractionRule trait defined | Compilation + custom rule test |
| AC-02 | KnowledgeGapRule produces entries from zero-result searches across 2+ features | Unit test with synthetic observations |
| AC-03 | ImplicitConventionRule produces entries from 100%-consistent patterns across 3+ features | Unit test with synthetic observations |
| AC-04 | DeadKnowledgeRule produces deprecation signals for access-cliff entries | Unit test with synthetic entries/observations |
| AC-05 | RecurringFrictionRule produces entries from hotspots in 3+ features | Unit test with synthetic observations |
| AC-06 | FileDependencyRule produces entries from read-before-edit chains in 3+ features | Unit test with synthetic observations |
| AC-07 | Quality gate rejects near-duplicates (cosine >= 0.92) | Integration test with embedding |
| AC-08 | Quality gate rejects contradictions | Integration test with contradiction check |
| AC-09 | Quality gate enforces rate limit (10/hour) | Unit test with counter |
| AC-10 | Quality gate enforces cross-feature validation | Unit test per rule minimum |
| AC-11 | Quality gate enforces confidence floor (< 0.2) | Unit test |
| AC-12 | Auto-extracted entries have trust_source="auto" and provenance tags | Integration test |
| AC-13 | Background tick starts automatically at server startup | Integration test (tick fires) |
| AC-14 | Maintenance tick performs all 5 maintenance operations | Integration test |
| AC-15 | Extraction pipeline triggers on each tick | Integration test |
| AC-16 | context_status reports maintenance status, ignores maintain param | Unit test |
| AC-17 | trust_score("auto") returns 0.35 | Unit test |
| AC-18 | check_entry_contradiction() extracted and usable | Unit test |
| AC-19 | StatusReport includes coherence_by_source | Unit test |
| AC-20 | All existing tests pass (no regressions) | CI / cargo test --workspace |
| AC-21 | Extraction rules have unit tests | Test count verification |
| AC-22 | Quality gate has tests for each rejection path | Test count verification |

## Constraints

- No new schema migration (auto-extracted entries use existing entries table)
- No new MCP tools (extraction is internal, not user-facing)
- No new CLI commands
- unimatrix-observe Cargo.toml gains `unimatrix-store` dependency
- unimatrix-core Cargo.toml gains `serde_json` dependency
- Estimated ~600 lines new code + ~175 lines refactored (75 CRT + 100 type migration imports)
