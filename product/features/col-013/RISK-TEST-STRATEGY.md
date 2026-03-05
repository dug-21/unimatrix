# Risk & Test Strategy: col-013 Extraction Rule Engine

## Risk Register

### R-01: Extraction Rules Produce Low-Quality Entries (SR-09)
- **Severity**: High
- **Likelihood**: Medium
- **Impact**: Degrades search/briefing quality for all agents
- **Mitigation**: Quality gate pipeline with 6 checks. trust_score 0.35 naturally ranks auto-entries lower. Cross-feature validation prevents single-observation entries.
- **Test**: Integration tests for each quality gate check. End-to-end test verifying auto-extracted entry ranks below human-authored entry in search results.
- **Residual risk**: Low after quality gate. Monitor in first 3-5 feature cycles.

### R-02: Background Tick Fails Silently (SR-07)
- **Severity**: Medium
- **Likelihood**: Medium
- **Impact**: Maintenance stops running, knowledge base degrades over time. No error visible to users.
- **Mitigation**: `last_maintenance_run` in StatusReport. INFO-level logging on tick start/completion. WARN if 2x interval passes without tick.
- **Test**: Unit test that tick_metadata updates on each run. Integration test that context_status reports last_maintenance_run.
- **Residual risk**: Low. Monitoring via context_status.

### R-03: CRT Refactors Introduce Regressions (SR-06)
- **Severity**: High
- **Likelihood**: Low
- **Impact**: Confidence scoring, contradiction detection, or status reporting breaks for all entries.
- **Mitigation**: Each refactor is isolated and testable. Existing test suites cover the affected code paths.
- **Test**: Dedicated tests for each CRT change. Full `cargo test --workspace` as gate.
- **Residual risk**: Very low. Changes are 5-100 lines each.

### R-04: Observation Table Query Performance (SR-02)
- **Severity**: Medium
- **Likelihood**: Medium
- **Impact**: Extraction tick takes too long, potentially blocking subsequent ticks.
- **Mitigation**: Watermark pattern (ADR-004) ensures O(new_rows). 90-day retention bounds table size. 30-second timeout on extraction tick.
- **Test**: Performance test with synthetic 10K observation rows to verify extraction completes within timeout.
- **Residual risk**: Low with watermark pattern.

### R-05: SQLite Write Contention During Extraction (SR-08)
- **Severity**: Medium
- **Likelihood**: Medium
- **Impact**: SQLITE_BUSY errors when extraction writes entries concurrently with MCP tool calls.
- **Mitigation**: Same spawn_blocking + store locking pattern as existing writes. Extraction batches writes (all entries from one tick stored in sequence, not parallel). WAL mode enables concurrent reads.
- **Test**: Concurrent write test: extraction write + MCP store call simultaneously.
- **Residual risk**: Low. Single-user server bounds concurrency.

### R-06: Type Migration Breaks Imports (SR-04)
- **Severity**: Low
- **Likelihood**: Low
- **Impact**: Compilation failure in affected crates.
- **Mitigation**: Re-exports from unimatrix-observe preserve backward compatibility. Mechanical change with no logic differences.
- **Test**: `cargo check --workspace` after migration. No test changes needed.
- **Residual risk**: Negligible.

### R-07: Rate Limit Reset on Server Restart
- **Severity**: Low
- **Likelihood**: High (restarts are normal)
- **Impact**: Burst of up to 10 extractions immediately after restart.
- **Mitigation**: Acceptable by design. The quality gate still applies. 10 entries is a small burst. The watermark also resets, so a full scan occurs once, but extraction confidence thresholds and cross-feature validation prevent flooding.
- **Test**: Verify rate limit resets correctly and re-engages after restart.
- **Residual risk**: Accepted. Not worth persisting rate limit state.

## Scope Risk Traceability

| Scope Risk | Architecture Mitigation | Test Coverage |
|------------|------------------------|---------------|
| SR-01 (tick starvation) | ADR-003: async coordinator, spawn_blocking for work | Integration test: tick fires while spawn_blocking tasks are running |
| SR-02 (observation growth) | ADR-004: watermark pattern, O(new_rows) | Performance test with synthetic data |
| SR-03 (quality gate cost) | ADR-005: cheap checks first, rate limit caps at 10/hour | Unit tests for pipeline ordering |
| SR-04 (type migration) | ADR-002: re-exports for backward compatibility | cargo check --workspace |
| SR-05 (crate coupling) | ADR-001: documented trade-off, detection rules untouched | Compilation verification |
| SR-06 (CRT regressions) | Isolated changes, existing tests | Dedicated unit tests per refactor |
| SR-07 (silent maintenance) | ADR-003: last_maintenance_run, logging, 2x interval warning | StatusReport field test |
| SR-08 (write contention) | Same locking pattern as existing | Concurrent write test |
| SR-09 (low-quality entries) | Quality gate, trust_score 0.35, cross-feature validation | End-to-end extraction quality test |
| SR-10 (store coupling) | Pragmatic: start with &Store, abstract later | Compilation verification |

## Test Strategy

### Unit Tests (in unimatrix-observe::extraction)

| Test | What it verifies | Priority |
|------|-----------------|----------|
| ExtractionRule trait compliance | Custom rule compiles and runs | P0 |
| KnowledgeGapRule with synthetic data | Produces gap entries from 2+ feature zero-result searches | P0 |
| KnowledgeGapRule single feature | Produces nothing (cross-feature gate) | P0 |
| ImplicitConventionRule 100% consistency | Produces convention from universal pattern | P0 |
| ImplicitConventionRule partial consistency | Produces nothing (not 100%) | P0 |
| ImplicitConventionRule min features | Needs 3+ features | P0 |
| DeadKnowledgeRule access cliff | Detects entries dormant for 5 features | P0 |
| DeadKnowledgeRule still accessed | Produces nothing | P0 |
| RecurringFrictionRule 3+ features | Produces lesson-learned from recurring hotspots | P0 |
| RecurringFrictionRule 2 features | Produces nothing | P0 |
| FileDependencyRule chains | Detects read-before-edit in 3+ features | P0 |
| FileDependencyRule no pattern | Produces nothing | P0 |
| quality_gate rate limit | Rejects after 10/hour | P0 |
| quality_gate content validation | Rejects short titles/content | P0 |
| quality_gate cross-feature validation | Rejects per rule minimum | P0 |
| quality_gate confidence floor | Rejects < 0.2 | P0 |
| default_extraction_rules | Returns 5 rules | P0 |

### Unit Tests (CRT refactors)

| Test | What it verifies | Priority |
|------|-----------------|----------|
| trust_score("auto") == 0.35 | New trust_source weight | P0 |
| trust_score existing values unchanged | No regression on "human", "system", "agent" | P0 |
| check_entry_contradiction opposing | Detects contradiction for conflicting content | P0 |
| check_entry_contradiction compatible | Returns None for non-conflicting content | P0 |
| coherence_by_source computation | Groups entries by trust_source and reports lambda | P1 |

### Integration Tests (unimatrix-server)

| Test | What it verifies | Priority |
|------|-----------------|----------|
| Background tick fires | Tick metadata updates after interval | P0 |
| Maintenance runs in background | Confidence refresh, co-access cleanup via tick | P1 |
| Extraction pipeline end-to-end | Observations -> extraction -> quality gate -> stored entry | P0 |
| context_status reports maintenance info | last_maintenance_run, extraction_stats present | P1 |
| maintain=true silently ignored | No error, no maintenance triggered | P0 |
| Near-duplicate quality gate | Embedding-based rejection | P1 |
| Contradiction quality gate | check_entry_contradiction rejection | P1 |
| Auto-entry trust_source and tags | Stored entry has correct metadata | P0 |

### Regression Tests

| Test | What it verifies | Priority |
|------|-----------------|----------|
| cargo test --workspace | All existing 1025+ unit, 174+ integration tests pass | P0 |
| Type migration imports | All crates compile after ObservationRecord move | P0 |
| Detection rules unchanged | 21 detection rules produce same results | P0 |

## Test Infrastructure

- Extraction rule unit tests use synthetic `ObservationRecord` vectors (same pattern as detection rule tests)
- Quality gate tests that need embedding use the `test-support` feature flag (mock embedding adapter)
- Background tick tests use `tokio::time::pause()` to advance time without waiting
- Store tests reuse existing `tempfile`-based test database pattern
- No new test fixtures or scaffolding needed beyond synthetic data constructors
