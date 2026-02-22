# Agent Report: nxs-001-researcher

## Agent ID
nxs-001-researcher

## Task
Explore the problem space for nxs-001 (Embedded Storage Engine) and produce SCOPE.md.

## Key Findings

### Prior Research Coverage
Three completed spikes (ASS-001, ASS-003, ASS-007) provide high-confidence answers to nearly all design questions. This is not a greenfield exploration -- the storage engine design has been iterated through spike research and interface specification work.

### Design Decisions Already Made (High Confidence)
- Single redb file with 8 named tables (from ASS-007 Proposal A DATABASE.md)
- serde + bincode serialization for EntryRecord (from ASS-003 D2)
- Compound tuple keys for index tables (from ASS-003 D2)
- MultimapTable for TAG_INDEX (from ASS-003 D2)
- VECTOR_MAP bridge table pattern (from ASS-001 D1 + ASS-007)
- Sync API with spawn_blocking for async contexts (from ASS-003 D2)
- 64-128 MB cache, compact on shutdown (from ASS-003 D2)

### Table Layout Evolution
The spike research shows a minor evolution between ASS-003 and ASS-007:
- ASS-003 proposed PHASE_INDEX; ASS-007 replaced it with TOPIC_INDEX and CATEGORY_INDEX (broader, more generic)
- ASS-007 Proposal A uses `(&str, u64)` for topic/category indexes; Proposal C uses `(u64_hash, u64)` for the same. I chose string keys for human readability and scan capability.
- VECTOR_MAP value type: ASS-001 uses `usize`, ASS-007 Proposal A uses `usize`, but redb needs fixed-width types. Standardized on `u64` in redb, cast at boundary.

### Schema Evolution Strategy
The `#[serde(default)]` approach is well-validated by the product vision ("designed from day 1 so that evolution requires zero migrations"). All Proposal C additions (feature_id, usage_count, helpful_count, last_used_at) can be added later as `#[serde(default)]` fields without any migration.

## Open Questions Surfaced
1. bincode v1 vs v2 -- affects serialization API and default handling
2. String key lifetime management in the redb API
3. Combined query API design (composed vs monolithic)
4. Entry update index management (caller vs internal)

## Artifacts Produced
- `/workspaces/unimatrix/product/features/nxs-001/SCOPE.md`

## Self-Check Results
- [x] SCOPE.md has all required sections (Problem, Goals, Non-Goals, ACs, Constraints)
- [x] Every acceptance criterion has an AC-ID (AC-01 through AC-16)
- [x] Non-Goals are explicit and reference specific future features
- [x] Constraints section includes real technical constraints
- [x] Open Questions section captures genuine unknowns
- [x] Background Research is based on actual spike research reading
- [x] SCOPE.md written to product/features/nxs-001/SCOPE.md
