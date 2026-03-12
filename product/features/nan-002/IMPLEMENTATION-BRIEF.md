# nan-002: Knowledge Import -- Implementation Brief

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/nan-002/SCOPE.md |
| Scope Risk Assessment | product/features/nan-002/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/nan-002/architecture/ARCHITECTURE.md |
| Specification | product/features/nan-002/specification/SPECIFICATION.md |
| Risk & Test Strategy | product/features/nan-002/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/nan-002/ALIGNMENT-REPORT.md |
| ADR-001 Shared Format Types | product/features/nan-002/architecture/ADR-001-shared-format-types.md |
| ADR-002 Direct SQL Insert | product/features/nan-002/architecture/ADR-002-direct-sql-insert.md |
| ADR-003 Force Flag Safety | product/features/nan-002/architecture/ADR-003-force-flag-safety.md |
| ADR-004 Embedding After Commit | product/features/nan-002/architecture/ADR-004-embedding-after-commit.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| cli-registration | pseudocode/cli-registration.md | test-plan/cli-registration.md |
| format-types | pseudocode/format-types.md | test-plan/format-types.md |
| import-pipeline | pseudocode/import-pipeline.md | test-plan/import-pipeline.md |
| embedding-reconstruction | pseudocode/embedding-reconstruction.md | test-plan/embedding-reconstruction.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Provide a `unimatrix-server import --input <path>` CLI subcommand that restores a Unimatrix knowledge base from a nan-001 JSONL export dump, preserving all learned signals (confidence, helpful/unhelpful counts, co-access pairs, correction chains), and re-embeds all entries with the current ONNX model to guarantee vector consistency. This completes the backup/restore cycle required for the Platform Hardening milestone, enabling cross-project knowledge transfer and multi-repo deployment resilience.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Format contract coupling | Shared `format.rs` module with typed deserialization structs; export keeps Value-based serialization | SR-08, SR-09 | architecture/ADR-001-shared-format-types.md |
| Data insertion method | Direct SQL INSERT via `store.lock_conn()`, not Store API (must preserve original IDs, timestamps, confidence, hashes) | Prior art: Unimatrix #336, #344 | architecture/ADR-002-direct-sql-insert.md |
| Destructive --force safety | Stderr warning with dropped entry count, no interactive confirmation prompt; CI/CD scriptable | SR-04 | architecture/ADR-003-force-flag-safety.md |
| Embedding placement | Re-embed after DB commit, not inside transaction; bounds transaction duration, partial restore (DB without vectors) is still useful | SR-01 | architecture/ADR-004-embedding-after-commit.md |
| Schema version access | Read from counters table after `Store::open()` (same approach as export), not from `CURRENT_SCHEMA_VERSION` constant | Architecture open question 1 | N/A |
| Counter overwrite strategy | `INSERT OR REPLACE INTO counters` to handle auto-initialized counters from `Store::open()` | ADR-002 | architecture/ADR-002-direct-sql-insert.md |

## Files to Create/Modify

| Path | Action | Description |
|------|--------|-------------|
| `crates/unimatrix-server/src/format.rs` | Create | Shared typed deserialization structs for JSONL format_version 1 (ExportHeader, ExportRow enum, per-table row structs) |
| `crates/unimatrix-server/src/import.rs` | Create | Full import pipeline: header validation, pre-flight, JSONL ingestion, hash validation, re-embedding, vector persistence, audit provenance |
| `crates/unimatrix-server/src/lib.rs` | Modify | Register `pub mod format;` and `pub mod import;` |
| `crates/unimatrix-server/src/main.rs` | Modify | Add `Command::Import` variant with `--input`, `--skip-hash-validation`, `--force` args; add match arm dispatching to `import::run_import()` |

## Data Structures

### ExportHeader
```rust
pub struct ExportHeader {
    pub _header: bool,
    pub schema_version: i64,
    pub exported_at: i64,
    pub entry_count: i64,
    pub format_version: i64,
}
```

### ExportRow (serde tagged enum)
```rust
#[serde(tag = "_table")]
pub enum ExportRow {
    #[serde(rename = "counters")]
    Counter(CounterRow),
    #[serde(rename = "entries")]
    Entry(EntryRow),
    #[serde(rename = "entry_tags")]
    EntryTag(EntryTagRow),
    #[serde(rename = "co_access")]
    CoAccess(CoAccessRow),
    #[serde(rename = "feature_entries")]
    FeatureEntry(FeatureEntryRow),
    #[serde(rename = "outcome_index")]
    OutcomeIndex(OutcomeIndexRow),
    #[serde(rename = "agent_registry")]
    AgentRegistry(AgentRegistryRow),
    #[serde(rename = "audit_log")]
    AuditLog(AuditLogRow),
}
```

### EntryRow (26 columns -- ground truth from `crates/unimatrix-store/src/db.rs` DDL)

CRITICAL: The Architecture's EntryRow is correct. The Specification's FR-06 list is WRONG (it includes `allowed_topics`, `allowed_categories`, `target_ids` which belong to `agent_registry`, not `entries`). Implementation agents MUST use this list:

```rust
pub struct EntryRow {
    pub id: i64,
    pub title: String,
    pub content: String,
    pub topic: String,
    pub category: String,
    pub source: String,                       // NOT in Spec FR-06 (Spec error)
    pub status: i64,
    pub confidence: f64,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_accessed_at: i64,
    pub access_count: i64,
    pub supersedes: Option<i64>,
    pub superseded_by: Option<i64>,
    pub correction_count: i64,                // NOT in Spec FR-06 (Spec error)
    pub embedding_dim: i64,                   // NOT in Spec FR-06 (Spec error)
    pub created_by: String,
    pub modified_by: String,
    pub content_hash: String,
    pub previous_hash: String,
    pub version: i64,
    pub feature_cycle: String,
    pub trust_source: String,
    pub helpful_count: i64,
    pub unhelpful_count: i64,
    pub pre_quarantine_status: Option<i64>,
}
```

### FeatureEntryRow
NOTE: The DDL column is `feature_id` (not `feature_cycle`). The export JSON key is also `feature_id`. The Architecture's struct incorrectly names this `feature_cycle`.
```rust
pub struct FeatureEntryRow {
    pub feature_id: String,   // DDL: feature_id TEXT NOT NULL
    pub entry_id: i64,
}
```

### OutcomeIndexRow
```rust
pub struct OutcomeIndexRow {
    pub feature_cycle: String,  // DDL: feature_cycle TEXT NOT NULL
    pub entry_id: i64,
}
```

### CounterRow
```rust
pub struct CounterRow {
    pub name: String,
    pub value: i64,
}
```

### EntryTagRow
```rust
pub struct EntryTagRow {
    pub entry_id: i64,
    pub tag: String,
}
```

### CoAccessRow
```rust
pub struct CoAccessRow {
    pub entry_id_a: i64,
    pub entry_id_b: i64,
    pub count: i64,
    pub last_updated: i64,
}
```

### AgentRegistryRow
```rust
pub struct AgentRegistryRow {
    pub agent_id: String,
    pub trust_level: i64,
    pub capabilities: String,          // JSON-in-TEXT, e.g. '["admin","read"]'
    pub allowed_topics: Option<String>, // JSON-in-TEXT, nullable
    pub allowed_categories: Option<String>, // JSON-in-TEXT, nullable
    pub enrolled_at: i64,
    pub last_seen_at: i64,
    pub active: i64,
}
```

### AuditLogRow
```rust
pub struct AuditLogRow {
    pub event_id: i64,
    pub timestamp: i64,
    pub session_id: String,
    pub agent_id: String,
    pub operation: String,
    pub target_ids: String,   // JSON-in-TEXT, e.g. '[]'
    pub outcome: i64,
    pub detail: String,
}
```

## Function Signatures

### Primary Entry Point
```rust
// crates/unimatrix-server/src/import.rs
pub fn run_import(
    project_dir: Option<&Path>,
    input: &Path,
    skip_hash_validation: bool,
    force: bool,
) -> Result<(), Box<dyn std::error::Error>>;
```

### CLI Registration
```rust
// crates/unimatrix-server/src/main.rs (added to Command enum)
Import {
    #[arg(short, long)]
    input: PathBuf,
    #[arg(long)]
    skip_hash_validation: bool,
    #[arg(long)]
    force: bool,
}
```

### Key Existing APIs Consumed
```rust
// unimatrix-store
Store::open(path: &Path) -> Result<Arc<Store>>
Store::lock_conn(&self) -> MutexGuard<'_, Connection>
compute_content_hash(title: &str, content: &str) -> String

// unimatrix-embed
OnnxProvider::new(config: EmbedConfig) -> Result<Self>
embed_entries(provider: &dyn EmbeddingProvider, entries: &[(String, String)], separator: &str) -> Result<Vec<Vec<f32>>>

// unimatrix-vector
VectorIndex::new(store: Arc<Store>, config: VectorConfig) -> Result<Self>
VectorIndex::insert(&self, entry_id: u64, embedding: &[f32]) -> Result<()>
VectorIndex::dump(&self, dir: &Path) -> Result<()>

// unimatrix-engine
project::ensure_data_directory(project_dir: Option<&Path>, socket_dir: Option<&Path>) -> Result<ProjectPaths>
```

## Pipeline Stages

```
[1] Open input file
[2] Parse & validate header (format_version == 1, schema_version <= current)
[3] Pre-flight: Store::open(), check empty DB or --force, PID file warning
[4] If --force on non-empty DB: drop all data from 8 tables + vector_map, log warning
[5] BEGIN IMMEDIATE transaction
[6] Ingest JSONL line-by-line: deserialize via format::ExportRow, route to per-table INSERT
[7] Hash validation (unless --skip-hash-validation): content hash + chain integrity
[8] COMMIT transaction
[9] Initialize OnnxProvider, batch-embed entries (64/batch), build VectorIndex
[10] VectorIndex::dump() to persist HNSW index
[11] Record import provenance in audit_log
[12] Print summary to stderr
```

## Constraints

1. **Schema version 11**: Validate header schema_version <= current (read from counters after Store::open). No older-schema import migration.
2. **Single-threaded, synchronous**: No tokio runtime. Matches hook/export pattern.
3. **ONNX model required**: ~80MB download on first use. Clear error messaging if unavailable.
4. **Direct SQL, not Store API**: Must preserve original IDs, timestamps, confidence, hashes, counters. Parameterized queries only (no string interpolation).
5. **Foreign key enforcement**: PRAGMA foreign_keys = ON. Dependency-ordered JSONL satisfies FK constraints.
6. **Database exclusivity**: Warn via PID file check if server is running. Do not block.
7. **Vector persistence**: Save HNSW index to `vector/` directory after building.
8. **Format stability**: Only format_version 1 supported.
9. **Memory bounded**: Line-by-line JSONL reading. Embedding batch size 64. Incremental HNSW construction.
10. **Atomic DB writes**: Single transaction wraps all inserts. Any failure = full rollback.

## Dependencies

### Internal Crates
| Crate | Usage |
|-------|-------|
| `unimatrix-store` | Store::open, lock_conn, compute_content_hash |
| `unimatrix-embed` | OnnxProvider::new, embed_entries |
| `unimatrix-vector` | VectorIndex::new, insert, dump |
| `unimatrix-engine` | project::ensure_data_directory |

### External Crates (all existing in workspace)
| Crate | Usage |
|-------|-------|
| `serde` / `serde_json` | JSONL deserialization |
| `clap` | CLI argument parsing |
| `rusqlite` | Direct SQL INSERT via params![] |

## NOT in Scope

- Merge/append mode (full restore only)
- Incremental/resumable import
- MCP tool exposure (CLI-only)
- Stdin input (`--input -`)
- Remote/network import
- Format version negotiation
- Schema migration of imported data
- Adaptation state (MicroLoRA weights)
- Operational data (sessions, observations, injection_log, query_log)
- Decompression of input
- Interactive confirmation prompts for --force
- --skip-embedding dry-run mode

## Alignment Status

**Overall: PASS with one VARIANCE requiring implementation attention.**

The vision guardian identified one variance between Architecture and Specification:

- **VARIANCE (RESOLVED)**: Architecture `format::EntryRow` lists `source`, `correction_count`, `embedding_dim` as entry columns. Specification FR-06 lists `allowed_topics`, `allowed_categories`, `target_ids` instead. Both claim 26 columns. **Ground truth from `crates/unimatrix-store/src/db.rs` DDL confirms the Architecture is correct.** The entries table has `source`, `correction_count`, `embedding_dim`. The Specification's FR-06 erroneously included agent_registry columns. Implementation agents MUST use the Architecture's column list (verified against DDL above).

- **ADDITIONAL FINDING**: Architecture's `FeatureEntryRow` struct uses field name `feature_cycle`, but the actual DDL column and export JSON key are `feature_id`. Implementation agents MUST use `feature_id` for the `feature_entries` table.

All other alignment checks passed: vision alignment, milestone fit, scope completeness, risk coverage.
