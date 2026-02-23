# Agent Report: nxs-004-researcher

## Agent ID
nxs-004-researcher

## Task
Research problem space for nxs-004 (Core Traits & Domain Adapters) and produce SCOPE.md.

## Key Findings

### 1. Three Independent Crates Need Unification
- unimatrix-store: 85 tests, synchronous redb API with 8 tables
- unimatrix-vector: 85 tests, synchronous hnsw_rs wrapper with Arc<Store> dependency
- unimatrix-embed: 76 active tests, EmbeddingProvider trait already exists but is embed-specific
- No shared trait layer exists between them

### 2. Schema Evolution Contract is Well-Defined
- bincode v2 positional encoding means serde(default) does NOT handle missing fields
- Explicit scan-and-rewrite is the documented migration strategy (schema.rs comments)
- COUNTERS table exists and can host schema_version
- Fields must be append-only (after embedding_dim)

### 3. Security Fields are Pre-Approved
- PRODUCT-VISION.md explicitly names all 7 fields for nxs-004
- ROADMAP-SECURITY-RECOMMENDATIONS.md decision #1 is APPROVED
- SHA-256 content hash format aligns with embed pipeline's prepare_text separator

### 4. Async Pattern is Documented
- ASS-003 spike established Arc<T> + spawn_blocking pattern
- VectorIndex already takes Arc<Store> in its constructor
- All three crates are Send + Sync

### 5. EmbeddingProvider Trait Already Exists
- unimatrix-embed already defines a trait for embedding generation
- EmbedService in core should wrap this at entry-level (title+content) not raw text level

## Artifacts Produced
- `/workspaces/unimatrix/product/features/nxs-004/SCOPE.md`

## Open Questions for Human
1. Async wrappers in core (feature-gated) vs separate crate?
2. Should unimatrix-core re-export domain types from unimatrix-store?
3. Compute content_hash during migration? (Recommended: yes)
