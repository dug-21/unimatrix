# C2: Supersession Candidate Injection — Test Plan

## Location
`crates/unimatrix-server/src/services/search.rs` (integration tests)

## Tests

### T-SI-01: Active successor injected when deprecated entry matched (AC-04)
- Insert: Deprecated entry X (superseded_by=Y), Active entry Y (different embedding)
- Query matches X's embedding closely
- Expected: Y appears in results via injection with its own cosine similarity

### T-SI-02: Injected successor similarity is from cosine, not inherited (AC-05)
- Insert: Deprecated entry X (sim=0.95 to query), Active successor Y (sim=0.40 to query)
- Expected: Y's score reflects 0.40-range similarity, not X's 0.95

### T-SI-03: Single-hop limit — no transitive chains (AC-06, ADR-003)
- Insert: Deprecated A (superseded_by=B), Active B (superseded_by=C), Active C
- Query matches A
- Expected: B is NOT injected (B has superseded_by), C is NOT injected (two hops)

### T-SI-04: Dangling superseded_by reference (AC-07, FR-2.7)
- Insert: Deprecated entry with superseded_by=99999 (non-existent)
- Search completes successfully
- Expected: no injection, no error, no panic

### T-SI-05: Successor is Deprecated — skip injection
- Insert: Deprecated X (superseded_by=Y), Deprecated Y
- Expected: Y not injected (not Active)

### T-SI-06: Successor is Quarantined — skip injection
- Insert: Deprecated X (superseded_by=Y), Quarantined Y
- Expected: Y not injected

### T-SI-07: Successor already in results — no duplicate injection
- Insert: Deprecated X (superseded_by=Y), Active Y with embedding similar to query
- Both X and Y match HNSW query
- Expected: Y appears once, not duplicated

### T-SI-08: Multiple deprecated entries superseded by same Active — single injection
- Insert: Deprecated A (superseded_by=Z), Deprecated B (superseded_by=Z), Active Z
- Both A and B match query
- Expected: Z injected once

### T-SI-09: Self-referential supersession — no infinite loop
- Insert: Deprecated X (superseded_by=X)
- Expected: no injection (X is in existing_ids), no loop

### T-SI-10: Explicit status=Deprecated disables injection (AC-14b, FR-6.2)
- Insert: Deprecated X (superseded_by=Y), Active Y
- Search with filters.status = Some(Status::Deprecated)
- Expected: X returned, Y NOT injected

### T-SI-11: Successor with no stored embedding — skip injection gracefully
- Insert: Deprecated X (superseded_by=Y), Active Y (no embedding in VECTOR_MAP)
- Expected: Y not injected, search completes normally

## Risk Coverage

| Risk | Scenarios | Tests |
|------|-----------|-------|
| R-01 (get_embedding API) | Embedding retrieval for successor | T-SI-01, T-SI-11 |
| R-05 (dangling references) | Non-existent, deprecated, quarantined, self-ref | T-SI-04..06, T-SI-09 |
| R-07 (explicit status + injection) | Disabled when status=Deprecated | T-SI-10 |
| AC-04 | Injection works | T-SI-01 |
| AC-05 | Own cosine similarity | T-SI-02 |
| AC-06 | Single-hop | T-SI-03 |
| AC-07 | Dangling reference | T-SI-04 |
| AC-14b | Injection disabled | T-SI-10 |
