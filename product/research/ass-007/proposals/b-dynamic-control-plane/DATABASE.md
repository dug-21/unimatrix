# Proposal B: Dynamic Control Plane -- Database

## Entry Type Taxonomy

All entries share the same core schema. `type` distinguishes storage and export behavior.

| Type | Exported? | Renderer Target | Indexed? | Versioned? |
|------|-----------|----------------|----------|------------|
| `knowledge` | No | Runtime only | Vector + metadata | Correction chains |
| `convention` | No | Runtime only | Vector + metadata | Correction chains |
| `decision` | No | Runtime only | Vector + metadata | Correction chains |
| `pattern` | No | Runtime only | Vector + metadata | Correction chains |
| `retrospective` | No | Runtime only | Vector + metadata | Append-only |
| `protocol` | Yes | `protocols/{topic}.md` | Vector + metadata | Full version history |
| `role` | Yes | `agents/ndp/{topic}.md` | Vector + metadata | Full version history |
| `routing` | Yes | `protocols/agent-routing.md` (merged) | Metadata only | Full version history |
| `rule` | Yes | `rules/{topic}.md` | Vector + metadata | Full version history |
| `skill` | Yes | `skills/{topic}/SKILL.md` | Vector + metadata | Full version history |
| `constitutional` | Yes | `CLAUDE.md` (append block) | Metadata only | Full version history |
| `template` | No | Used by `unimatrix seed` | Metadata only | Immutable |

Exportable types get full version history (every mutation preserved). Knowledge types get correction chains (current + superseded, no intermediate snapshots).

## redb Table Layout

### Core Tables (shared with Proposal A baseline)

```
ENTRIES: TableDefinition<u64, &[u8]>
  key: entry_id
  value: bincode-serialized EntryRecord {
    content: String,
    topic: String,
    category: String,
    entry_type: EntryType,        // enum, not freeform
    status: Status,               // Active, Deprecated, PendingReview
    tags: Vec<String>,
    confidence: f32,
    usage_count: u32,
    created_at: u64,
    updated_at: u64,
    supersedes: Option<u64>,
    superseded_by: Option<u64>,
    correction_count: u32,
    version: u32,                 // NEW: monotonic version counter per entry
    checksum: [u8; 32],          // NEW: SHA-256 of rendered output (for export tracking)
  }

TOPIC_INDEX: TableDefinition<(u64, u64), ()>
  key: (topic_hash, entry_id)

CATEGORY_INDEX: TableDefinition<(u64, u64), ()>
  key: (category_hash, entry_id)

TAG_INDEX: MultimapTableDefinition<&str, u64>
  key: tag_string -> entry_ids

TIME_INDEX: TableDefinition<(u64, u64), ()>
  key: (timestamp, entry_id)

STATUS_INDEX: TableDefinition<(u8, u64), ()>
  key: (status_byte, entry_id)

TYPE_INDEX: TableDefinition<(u8, u64), ()>       // NEW
  key: (type_byte, entry_id)

VECTOR_MAP: TableDefinition<u64, u64>
  key: entry_id -> hnsw_data_id

COUNTERS: TableDefinition<&str, u64>
  keys: "next_entry_id", "export_generation"
```

### New Tables for Control Plane

```
VERSION_HISTORY: TableDefinition<(u64, u32), &[u8]>
  key: (entry_id, version_number)
  value: bincode-serialized VersionRecord {
    content: String,              // full content snapshot at this version
    changed_by: String,           // "cli", "mcp:context_correct", "auto:retro-cluster"
    reason: Option<String>,
    timestamp: u64,
    diff_from_previous: Option<String>,  // unified diff
  }
  // Only populated for exportable types (protocol, role, routing, rule, skill, constitutional)
  // Knowledge types use correction chains instead (cheaper, sufficient)

EXPORT_STATE: TableDefinition<&str, &[u8]>
  key: output_file_path (e.g., "protocols/implementation-protocol.md")
  value: bincode-serialized ExportRecord {
    source_entry_ids: Vec<u64>,   // which entries compose this file (1 for most, N for routing)
    last_export_generation: u64,
    last_export_checksum: [u8; 32],
    last_export_timestamp: u64,
  }
  // Enables: "what changed since last export?" and "which file does this entry render to?"

RETRO_CLUSTERS: TableDefinition<u64, &[u8]>
  key: cluster_id
  value: bincode-serialized RetroCluster {
    target_topic: String,         // protocol or role being critiqued
    entry_ids: Vec<u64>,          // retrospective entries in this cluster
    similarity_scores: Vec<f32>,  // pairwise similarity
    proposed_correction: Option<u64>,  // entry_id of proposed correction (if generated)
    status: ClusterStatus,        // Detected, ProposedCorrection, Applied, Dismissed
    detected_at: u64,
  }
  // Drives the automated correction loop
```

## How Process Entries Differ from Knowledge Entries

Process entries (protocol, role, routing, rule, skill, constitutional) have:

1. **Full version history** -- every mutation is a snapshot in VERSION_HISTORY. Knowledge entries only track correction chains (current + previous).
2. **Export tracking** -- EXPORT_STATE links entry to output file. Knowledge entries are never exported.
3. **Auto-correction gating** -- corrections to process entries can be queued for review. Knowledge corrections apply immediately.
4. **Rendering logic** -- each type has a renderer that transforms entry content into the target file format. Most are identity (content = file), but `routing` entries are merged into one file, and `constitutional` entries are injected between markers in CLAUDE.md.

## Storing Complex Structures as Entries

**Routing rules** -- one entry per template, assembled by export:
```
entry_id: 47, type: routing, topic: "planning-swarm"
content: "### Planning Swarm\n\n```\nCoordinator: ndp-scrum-master\nWave 1: ..."
tags: ["swarm-template", "planning"]
```

**Wave definitions** -- embedded in protocol entry content (not separate entries). A protocol is a single document, stored as a single entry. The rendering is the content verbatim.

**Gate conditions** -- embedded in protocol content. Gates are prose ("validator returns PASS or WARN"), not structured data. Keeping them in the protocol entry preserves readability.

## Retrospective Cluster Detection

Runs on `context_store` when `category = "retrospective"`:

```
1. Embed the new retrospective entry
2. Search existing retrospectives filtered by same topic:
   search_filter(embedding, k=10, filter=type_retro AND topic_match)
3. If >= threshold (default 3) results with similarity > 0.85:
   a. Create/update RETRO_CLUSTERS entry
   b. If cluster.status == Detected AND count >= auto_threshold:
      Generate correction proposal (LLM-at-write or template-based)
      Store as new entry with status=PendingReview
      Update cluster.proposed_correction
4. If require_review: queue for `unimatrix review`
   Else: auto-apply correction, trigger export if configured
```
