# Proposal C: Database Design

## redb Table Layout

Core tables are shared with baseline design. Proposal C adds outcome and usage tracking tables.

### Core Tables (same as baseline)

```
ENTRIES:         entry_id: u64 -> bincode<EntryMetadata + content>
TOPIC_INDEX:     (topic_hash: u64, entry_id: u64) -> ()
CATEGORY_INDEX:  (category_hash: u64, entry_id: u64) -> ()
TAG_INDEX:       MultimapTable<&str, u64>           // tag -> entry_ids
TIME_INDEX:      (created_at_ms: u64, entry_id: u64) -> ()
STATUS_INDEX:    (status_u8: u8, entry_id: u64) -> ()
VECTOR_MAP:      entry_id: u64 -> hnsw_data_id: u64
COUNTERS:        &str -> u64                        // "next_entry_id", etc.
```

### Proposal C Additions: Usage + Outcome Tables

```
USAGE_LOG:       (entry_id: u64, accessed_at_ms: u64) -> bincode<UsageRecord>
FEATURE_ENTRIES: MultimapTable<&str, u64>           // feature_id -> entry_ids used
OUTCOME_INDEX:   (feature_hash: u64, entry_id: u64) -> ()  // feature -> outcome entries
```

### Entry Metadata Schema

```rust
struct EntryMetadata {
    id: u64,
    content: String,
    topic: String,
    category: String,          // "convention", "decision", "outcome", "process-proposal", "process"
    tags: Vec<String>,
    status: EntryStatus,       // Active, Deprecated, PendingReview
    source: Option<String>,    // agent ID or "human"
    confidence: f32,
    created_at: u64,
    updated_at: u64,
    supersedes: Option<u64>,
    superseded_by: Option<u64>,
    correction_count: u32,
    // Proposal C additions:
    feature_id: Option<String>,    // which feature this entry relates to
    usage_count: u32,              // times retrieved in tool calls
    helpful_count: u32,            // times marked helpful via reflexion
    last_used_at: Option<u64>,
}

enum EntryStatus {
    Active = 0,
    Deprecated = 1,
    PendingReview = 2,         // process proposals awaiting human review
}

struct UsageRecord {
    agent_role: Option<String>,
    feature_id: Option<String>,
    tool: String,              // "search", "lookup", "briefing"
    helpful: Option<bool>,     // from reflexion callback
}
```

## Usage Tracking Flow

When any retrieval tool returns entries:

1. For each returned entry, write to `USAGE_LOG`: `(entry_id, now_ms) -> UsageRecord`
2. Increment `usage_count` on the entry in `ENTRIES`
3. If `feature_id` is known (from briefing context or tag), write to `FEATURE_ENTRIES`: `feature_id -> entry_id`

When reflexion data arrives (via `context_store` with `tags: ["reflexion:helpful"]` or `["reflexion:unhelpful"]`):

4. Update `helpful_count` on the referenced entry
5. Update the `UsageRecord.helpful` field

All writes are batched into the same redb write transaction as the tool response -- no separate write path.

## Outcome-to-Retrospective Pipeline

### Step 1: Outcome Accumulation

During a feature lifecycle, agents store outcome entries:
```
ENTRIES table gets outcome entries with category="outcome", feature_id="nxs-012"
OUTCOME_INDEX maps (hash("nxs-012"), entry_id) for fast retrieval
FEATURE_ENTRIES maps "nxs-012" -> [entry_ids of all entries used during this feature]
```

### Step 2: Retrospective Aggregation

When `context_retrospective(feature: "nxs-012")` is called:

```
1. OUTCOME_INDEX range scan: all entries where feature_hash = hash("nxs-012")
   -> Collect: completion data, quality signals, blocker reports

2. FEATURE_ENTRIES lookup: all entry_ids used during nxs-012
   -> For each: check USAGE_LOG for helpful/unhelpful counts
   -> Compute: retrieval efficiency = helpful / total retrieved

3. If compare_with provided: repeat steps 1-2 for comparison features
   -> Compute deltas: duration change, quality change, efficiency change

4. Gap detection:
   - Entries searched for but not found (from outcome entries tagged "outcome:blocker")
   - Entries retrieved but marked unhelpful (from reflexion data in USAGE_LOG)
   - Categories with no entries but high search frequency
```

### Step 3: Proposal Generation

```
For each detected gap:
  1. Create new entry:
     category: "process-proposal"
     status: PendingReview
     content: structured proposal with evidence
     tags: ["process", "evidence:{feature_count}", gap-type tag]
     supersedes: None (new proposal)

  2. Insert into ENTRIES, index in STATUS_INDEX under PendingReview
  3. Return proposal IDs in retrospective response
```

### Step 4: Human Approval

```
Approve (via CLI or context_correct):
  1. Create new entry: category="process", status=Active, supersedes=proposal_id
  2. Update proposal entry: superseded_by=new_id, status=Deprecated
  3. New entry is now retrievable via context_briefing and context_lookup

Reject (via CLI or context_deprecate):
  1. Update proposal: status=Deprecated, reason stored
  2. The rejection reason itself is data -- future proposals on the same topic
     can check for prior rejections to avoid re-proposing
```

## Confidence Formula (Extended)

```
confidence = base_confidence
    * usage_factor(usage_count)              // Wilson score lower bound
    * freshness_factor(days_since_last_use)  // exponential decay, 90-day half-life
    * correction_penalty(correction_count)   // 0.8^correction_count
    * helpfulness_factor(helpful_count, usage_count)  // Proposal C addition
```

The `helpfulness_factor` is: if `usage_count > 3`, apply `helpful_count / usage_count` as a multiplier (clamped to 0.5-1.0). Entries that are frequently retrieved but rarely helpful decay faster. This requires no ML -- just arithmetic on tracked counters.

## Storage Estimates

Additional overhead per entry: ~40 bytes (feature_id, usage_count, helpful_count, last_used_at).
Usage log: ~64 bytes per access record. At 100K entries with avg 5 accesses each = ~30MB.
Feature-entries multimap: negligible (feature_id string + u64 per link).

Total additional storage for Proposal C over baseline: ~35MB at 100K entry scale. Well within acceptable bounds.
