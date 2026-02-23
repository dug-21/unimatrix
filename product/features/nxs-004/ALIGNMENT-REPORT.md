# Alignment Report: nxs-004

> Reviewed: 2026-02-23
> Artifacts reviewed:
>   - product/features/nxs-004/architecture/ARCHITECTURE.md
>   - product/features/nxs-004/specification/SPECIFICATION.md
>   - product/features/nxs-004/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Security fields, migration, traits all match PRODUCT-VISION.md nxs-004 description |
| Milestone Fit | PASS | Completes M1 Foundation; no M2+ capabilities included |
| Scope Gaps | PASS | All 22 acceptance criteria addressed in source documents |
| Scope Additions | WARN | compact() excluded from EntryStore trait (ADR-006). Justified but worth noting. |
| Architecture Consistency | PASS | 10 components, clear dependency ordering, consistent with existing crate patterns |
| Risk Completeness | PASS | 12 risks covering migration, schema, traits, async, backward compatibility |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | compact() excluded from EntryStore | ADR-006 explains: requires &mut self, incompatible with object safety. vnc-001 calls compact() directly on Store during shutdown. Acceptable -- shutdown is infrastructure, not business logic. |
| Simplification | EmbedService uses fixed separator | Architecture specifies separator is hardcoded to ": " in the adapter (matching current unimatrix-embed behavior). SCOPE.md did not specify configurable separators. Acceptable. |

No scope gaps detected. No scope additions detected.

## Variances Requiring Approval

None. All source documents align with vision and scope. The compact() exclusion from the trait is a justified simplification documented in ADR-006.

## Detailed Findings

### Vision Alignment

**PRODUCT-VISION.md nxs-004 description states:**
> "Storage traits (EntryStore, VectorStore, IndexStore) in core crate. Domain adapter pattern -- implementations in domain modules. `spawn_blocking` with `Arc<Database>` for async. Security schema: Add 7 fields to EntryRecord -- `created_by`, `modified_by`, `content_hash` (SHA-256), `previous_hash`, `version` (u32), `feature_cycle`, `trust_source`. Implement scan-and-rewrite migration capability."

**Architecture/Specification deliver:**
- Three core traits (EntryStore, VectorStore, EmbedService) -- aligns. Note: vision says "IndexStore" but SCOPE.md refined this to "VectorStore" and "EmbedService" which is more precise. The vision's "IndexStore" was a placeholder name.
- Domain adapter pattern (StoreAdapter, VectorAdapter, EmbedAdapter) -- aligns.
- spawn_blocking async wrappers -- aligns.
- All 7 security fields with specified types -- aligns exactly.
- SHA-256 content hash -- aligns.
- Scan-and-rewrite migration with schema_version counter -- aligns.

**PRODUCT-VISION.md Security Cross-Cutting Concerns states:**
> "M1 (nxs-004): Schema fields: created_by, modified_by, content_hash, previous_hash, version, feature_cycle, trust_source. Scan-and-rewrite migration capability."

Source documents deliver exactly this. No excess, no deficit.

### Milestone Fit

nxs-004 is the last feature in Milestone 1 (Foundation / Nexus Phase). The source documents:
- Do NOT include MCP server components (M2)
- Do NOT include agent registry or audit log (M2)
- Do NOT include usage tracking (M4)
- Do NOT include workflow orchestration (M5)

The feature stays firmly within M1 boundaries. The trait abstractions enable M2 but do not implement it.

### Architecture Review

**Component breakdown** (10 components) is well-structured:
- C1-C5 in unimatrix-core (traits, error, re-exports, adapters, async wrappers)
- C6-C9 in unimatrix-store (schema, hash, write logic, migration)
- C10 crate setup

**ADR quality**: 6 ADRs, each with Context/Decision/Consequences format. Decisions are well-reasoned with alternatives considered. ADR-006 (object safety) correctly identifies the compact() tension and resolves it pragmatically.

**Integration surface table**: Complete with 28 method signatures across all three traits. This prevents implementation agents from inventing names.

**Implementation order**: Clear dependency chain from crate setup through schema changes to traits to adapters.

### Specification Review

**Functional requirements**: 9 FRs covering traits, schema, hash, version, migration, adapters, async, error, re-exports. Each is testable.

**Non-functional requirements**: 7 NFRs covering backward compatibility, object safety, thread safety, migration performance, unsafe prohibition, hash determinism, migration atomicity. All measurable.

**Acceptance criteria**: All 22 AC-IDs from SCOPE.md present with verification methods. Good traceability.

**Domain models**: EntryRecord documented with all 24 fields. NewEntry documented with all 10 fields. CoreError and Schema Version model documented.

**Note on update_status and version**: Specification states update_status does NOT increment version. This is a good decision -- status changes are metadata transitions, not content modifications. The version field tracks content evolution only.

### Risk Strategy Review

**Risk coverage**: 12 risks, 38 test scenarios. Risks are specific to nxs-004 (not generic platitudes).

**Critical risks well-identified**:
- R-01 (migration corruption) -- the highest-impact risk, thoroughly scenario-mapped
- R-04 (legacy deserialization) -- correctly identified as a gate: if this fails, migration is dead
- R-12 (existing tests) -- correctly identified as highest-likelihood risk

**Security risks assessed**: 4 security risks (SR-01 through SR-04). Correctly identifies that content_hash is engine-computed (not spoofable) and trust_source enforcement is vnc-001's job.

**Test priority order**: Sensible -- existing tests first (gate), then migration, then new features. Matches the implementation dependency order.

**Integration risks**: 4 IR items covering cross-crate dependencies, circular deps, counter interaction, and hash format alignment. All legitimate concerns.
