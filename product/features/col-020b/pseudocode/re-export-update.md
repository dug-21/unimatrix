# C7: Re-export Update

## Purpose

Update the re-export in `unimatrix-observe/src/lib.rs` when `KnowledgeReuse` is renamed to `FeatureKnowledgeReuse`, and update all import sites across the workspace.

## File: `crates/unimatrix-observe/src/lib.rs`

### Change: Update re-export (line 33)

```
// Before (line 33 in the pub use types::{...} block):
    KnowledgeReuse,

// After:
    FeatureKnowledgeReuse,
```

Full updated re-export block:
```
pub use types::{
    AttributionMetadata, BaselineComparison, BaselineEntry, BaselineSet, BaselineStatus,
    EntryAnalysis, EvidenceCluster, EvidenceRecord, HookType, HotspotCategory, HotspotFinding,
    HotspotNarrative, FeatureKnowledgeReuse, MetricVector, ObservationRecord, ObservationStats,
    ParsedSession, PhaseMetrics, Recommendation, RetrospectiveReport, SessionSummary, Severity,
    UniversalMetrics,
};
```

Note: `FeatureKnowledgeReuse` replaces `KnowledgeReuse` in alphabetical position. It should be placed between `EvidenceRecord` and `MetricVector` to maintain alphabetical ordering.

## File: `crates/unimatrix-server/src/mcp/knowledge_reuse.rs`

### Change: Update import (line 12)

```
// Before:
use unimatrix_observe::KnowledgeReuse;

// After:
use unimatrix_observe::FeatureKnowledgeReuse;
```

## File: `crates/unimatrix-server/src/mcp/tools.rs`

### Change: Update return type reference

The function `compute_knowledge_reuse_for_sessions` (line 1622) references `unimatrix_observe::KnowledgeReuse` in its return type. This changes to `unimatrix_observe::FeatureKnowledgeReuse` (covered in C6 pseudocode).

### Verification: grep for stale references

Before finalizing, run:
```
grep -rn "KnowledgeReuse" crates/
```

All hits must be either:
- In test assertions referencing old JSON field names (acceptable)
- In doc comments explaining the rename (acceptable)
- In serde(alias) annotations (expected)

Any `use ... KnowledgeReuse` or type annotation `KnowledgeReuse` (not in a string literal) is a compilation error and must be updated.

## Error Handling

No error paths. This is a compile-time change. Any missed rename is caught by `cargo build`.

## Key Test Scenarios

1. `cargo build --workspace` succeeds after all renames
2. No remaining `use ... KnowledgeReuse` imports (grep verification)
