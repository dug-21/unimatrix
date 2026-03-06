# C1: RetrievalMode + SearchService Status Logic — Test Plan

## Location
`crates/unimatrix-server/src/services/search.rs` (unit and integration tests)

## Unit Tests

### T-RM-01: RetrievalMode default is Flexible (ADR-001, SR-09)
- `RetrievalMode::default() == RetrievalMode::Flexible`

### T-RM-02: ServiceSearchParams default retrieval_mode is Flexible
- Construct ServiceSearchParams with all fields, verify retrieval_mode defaults

## Integration Tests (SearchService pipeline)

### T-SS-01: Strict mode drops deprecated entries (AC-01)
- Insert: Active entry (sim high), Deprecated entry (sim higher)
- Search with RetrievalMode::Strict
- Expected: only Active entry in results, Deprecated excluded

### T-SS-02: Strict mode drops superseded entries (AC-01)
- Insert: Active entry A, Active entry B with superseded_by = C
- Search with Strict mode
- Expected: entry B excluded (has superseded_by), entry A returned

### T-SS-03: Flexible mode applies DEPRECATED_PENALTY (AC-02)
- Insert: Active entry (sim=0.88), Deprecated entry (sim=0.90)
- Search with Flexible mode
- Expected: Active ranks higher (0.88 * 1.0 > 0.90 * 0.7 after rerank)

### T-SS-04: Flexible mode applies SUPERSEDED_PENALTY (AC-03)
- Insert: Deprecated entry (no supersession, sim=0.90), Superseded entry (sim=0.90)
- Search with Flexible mode
- Expected: Deprecated (0.7x) ranks above Superseded (0.5x)

### T-SS-05: Strict mode empty results — no fallback (AC-10, FR-1.5)
- Insert: only Deprecated entries for query topic
- Search with Strict mode
- Expected: empty results, no panic, no fallback to Flexible

### T-SS-06: Explicit status filter bypasses penalties (AC-14, FR-6.2)
- Insert: Deprecated entry
- Search with Flexible mode, filters.status = Some(Status::Deprecated)
- Expected: Deprecated entry returned at full score (no 0.7x penalty)

### T-SS-07: Quarantined still excluded in both modes (existing behavior)
- Insert: Quarantined entry
- Search with Flexible mode
- Expected: Quarantined entry not in results (unchanged behavior)

### T-SS-08: All results deprecated in Flexible mode — all returned with penalties
- Insert: multiple Deprecated entries only
- Search with Flexible mode
- Expected: all returned with penalty applied, not empty

### T-SS-09: Penalty applied to re-ranked score, not similarity
- Insert entries with known similarity and confidence
- Verify final_score = rerank_score(sim, conf) * penalty

## Risk Coverage

| Risk | Scenarios | Tests |
|------|-----------|-------|
| R-02 (penalty ranking) | Active vs Deprecated, Deprecated vs Superseded | T-SS-03, T-SS-04 |
| R-03 (strict empty) | All deprecated | T-SS-05, T-SS-08 |
| R-12 (default Flexible) | Default mode behavior | T-RM-01, T-RM-02 |
| AC-01 | Strict filtering | T-SS-01, T-SS-02 |
| AC-02 | Flexible penalty ranking | T-SS-03 |
| AC-03 | Superseded vs deprecated ranking | T-SS-04 |
| AC-10 | Empty strict results | T-SS-05 |
| AC-14 | Explicit status bypass | T-SS-06 |
