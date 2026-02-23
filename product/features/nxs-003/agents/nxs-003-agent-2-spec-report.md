# Agent Report: nxs-003-agent-2-spec

**Agent**: uni-specification
**Agent ID**: nxs-003-agent-2-spec
**Feature**: nxs-003 (Embedding Pipeline)
**Output**: `product/features/nxs-003/specification/SPECIFICATION.md`

## What Was Produced

SPECIFICATION.md covering:
- 13 functional requirements (FR-01 through FR-13)
- 6 non-functional requirements (NFR-01 through NFR-06)
- 19 acceptance criteria (AC-01 through AC-19, all traced from SCOPE.md)
- Domain models with entity definitions, relationships, and ubiquitous language
- 6 user workflows
- Constraints, dependencies, and explicit NOT-in-scope list
- 3 open questions for architect
- 11 key specification decisions

## Self-Check Results

- [x] SPECIFICATION.md covers all acceptance criteria from SCOPE.md (AC-01 through AC-19)
- [x] Every functional requirement is testable
- [x] Non-functional requirements include measurable targets
- [x] Domain Models section defines key terms (12 entries in ubiquitous language)
- [x] NOT in scope section is explicit (11 exclusions)
- [x] Output file is in `product/features/nxs-003/specification/` only
- [x] No placeholder or TBD sections

## Key Decisions Made

1. **Standalone crate**: unimatrix-embed has no dependency on unimatrix-store or unimatrix-vector. Integration happens at the caller level (vnc-001/vnc-002). This was not explicit in SCOPE.md but follows from the crate's role as a pure embedding primitive.

2. **Mutex for thread safety**: Specified `Mutex<Session>` as the mechanism to satisfy `Send + Sync` given ort's `&mut self` inference requirement. SCOPE.md mentioned "Mutex or serialized access" — specification commits to Mutex.

3. **No rayon**: Excluded batch-level parallelism (rayon) that ruvector uses. At 384-d and batch size 32, pooling is negligible vs inference time. Keeps dependencies minimal.

4. **thiserror 2.0**: First workspace crate to use thiserror rather than manual impls. Flagged as OQ-1 for architect awareness.

5. **Empty string is valid input**: Specified that `embed("")` returns a valid embedding (via CLS/SEP tokens), not an error. Simplifies caller logic.

6. **EmbedError::Tokenizer wraps String, not tokenizers::Error directly**: The tokenizers crate error type may not implement Send/Sync or may be opaque. Wrapping as String is safer and matches the ruvector pattern.

## Open Questions for Architect

- **OQ-1**: thiserror 2.0 vs manual Error impls (consistency with nxs-001/nxs-002)
- **OQ-2**: Exact ort feature flags needed (download-binaries, half, etc.)
- **OQ-3**: MockProvider determinism strategy (hash-based approach details)
