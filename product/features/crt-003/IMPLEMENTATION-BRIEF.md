# Implementation Brief: crt-003 Contradiction Detection

## Feature Summary

crt-003 adds contradiction detection, entry quarantine, and embedding consistency checks to Unimatrix. It spans 4 crates, introduces 1 new server module, extends the Status enum with a Quarantined variant, and adds a new MCP tool (context_quarantine).

## Source Documents

| Document | Path |
|----------|------|
| SCOPE.md | product/features/crt-003/SCOPE.md |
| Scope Risk Assessment | product/features/crt-003/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/crt-003/architecture/ARCHITECTURE.md |
| ADR-001 | product/features/crt-003/architecture/ADR-001-quarantined-base-score.md |
| ADR-002 | product/features/crt-003/architecture/ADR-002-reembed-for-scanning.md |
| ADR-003 | product/features/crt-003/architecture/ADR-003-conflict-heuristic-design.md |
| Specification | product/features/crt-003/specification/SPECIFICATION.md |
| Risk Strategy | product/features/crt-003/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-003/ALIGNMENT-REPORT.md |

## Component Map

| Component | Description | Crate | Pseudocode | Test Plan |
|-----------|-------------|-------|-----------|-----------|
| C1: status-extension | Add Quarantined variant to Status enum | unimatrix-store | pseudocode/status-extension.md | test-plan/status-extension.md |
| C2: retrieval-filtering | Exclude quarantined entries from search/lookup/briefing | unimatrix-server | pseudocode/retrieval-filtering.md | test-plan/retrieval-filtering.md |
| C3: quarantine-tool | context_quarantine MCP tool with quarantine/restore actions | unimatrix-server | pseudocode/quarantine-tool.md | test-plan/quarantine-tool.md |
| C4: contradiction-detection | Contradiction scanning and conflict heuristic module | unimatrix-server | pseudocode/contradiction-detection.md | test-plan/contradiction-detection.md |
| C5: status-report-extension | Extend StatusReport, StatusParams, and response formatting | unimatrix-server | pseudocode/status-report-extension.md | test-plan/status-report-extension.md |

## Implementation Order

```
C1 (status-extension)
 |
 +---> C2 (retrieval-filtering) --- parallel with ---> C4 (contradiction-detection)
 |
 +---> C3 (quarantine-tool) -- depends on C1, C2
 |
 +---> C5 (status-report-extension) -- depends on C1, C4
```

**Phase 1**: C1 (Status enum changes -- cross-crate, must be first)
**Phase 2**: C2 + C4 in parallel (no dependency between them)
**Phase 3**: C3 (depends on C1, C2)
**Phase 4**: C5 (depends on C1, C4, integrates everything)

## Key Design Decisions

1. **Quarantined base_score = 0.1** (ADR-001): Lower than Deprecated (0.2) because quarantine implies active suspicion, not just staleness.

2. **Re-embed from text for scanning** (ADR-002): Avoids complex hnsw_rs PointId mapping. Re-embedding simultaneously serves contradiction detection and embedding consistency checking.

3. **Multi-signal conflict heuristic with tunable threshold** (ADR-003): Three weighted signals (negation 0.6, incompatible directives 0.3, opposing sentiment 0.1) with configurable sensitivity parameter (default 0.5).

4. **Contradiction scanning defaults ON in context_status**: context_status is a batch diagnostic tool, not called on every request. Always scanning is appropriate.

5. **Embedding consistency check is opt-in**: `check_embeddings` parameter on context_status, default false. Re-embedding all entries is expensive.

6. **No automated quarantine**: Manual Admin-only action. Prevents DoS vector where an attacker triggers false positives to quarantine legitimate entries.

7. **Only Active entries can be quarantined**: Deprecated and Proposed entries cannot be quarantined. Simplifies the state machine.

## Cross-Crate Impact

| Crate | Changes |
|-------|---------|
| unimatrix-store | Status enum: add Quarantined variant, TryFrom, Display, status_counter_key |
| unimatrix-core | No changes (Status re-exported from store, traits unchanged) |
| unimatrix-vector | No changes (quarantined entries remain in HNSW) |
| unimatrix-embed | No changes (used for re-embedding during scans) |
| unimatrix-server | New module (contradiction.rs), modified tools.rs, response.rs, confidence.rs, validation.rs, server.rs |

## Risk Hotspots (Test First)

1. **R-02: Quarantine status leak** (C2) -- most critical risk. Test that quarantined entries are excluded from all retrieval paths.
2. **R-01: Exhaustive match regression** (C1) -- cross-crate impact. Verify every match site.
3. **R-03: Counter desync** (C3) -- verify counter arithmetic after quarantine/restore cycles.
4. **R-04/R-05: Heuristic accuracy** (C4) -- verify true positive and false positive cases.
5. **R-12: context_correct on quarantined entry** (C2) -- verify rejection.

## Constants and Thresholds

| Constant | Value | Location | Purpose |
|----------|-------|----------|---------|
| `SIMILARITY_THRESHOLD` | 0.85 | contradiction.rs | Minimum similarity to consider pair for conflict check |
| `DEFAULT_CONFLICT_SENSITIVITY` | 0.5 | contradiction.rs | Default sensitivity for conflict heuristic |
| `NEIGHBORS_PER_ENTRY` | 10 | contradiction.rs | Max neighbors to check per entry |
| `EMBEDDING_CONSISTENCY_THRESHOLD` | 0.99 | contradiction.rs | Minimum self-match similarity for consistency |
| `QUARANTINED_BASE_SCORE` | 0.1 | confidence.rs | base_score for Quarantined status |
| `NEGATION_WEIGHT` | 0.6 | contradiction.rs | Weight for negation opposition signal |
| `DIRECTIVE_WEIGHT` | 0.3 | contradiction.rs | Weight for incompatible directives signal |
| `SENTIMENT_WEIGHT` | 0.1 | contradiction.rs | Weight for opposing sentiment signal |
