# Risk-Based Test Strategy: crt-040

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Category data unavailable from `candidate_pairs` — AC-03 filter requires per-pair DB lookup or batch pre-fetch that is unresolved in architecture | High | High | Critical |
| R-02 | `write_nli_edge` delegation to `write_graph_edge` silently retags existing NLI edges if the refactor is incorrect | High | Med | High |
| R-03 | `impl Default` and serde backing function diverge on `supports_cosine_threshold` value — AC-10/AC-16 silent behavioral split (pattern #3817, lesson #4014) | High | Med | High |
| R-04 | `nli_post_store_k` removal masks a test regression if both add and remove are in the same commit and test counts are not verified separately | Med | Med | Medium |
| R-05 | `inferred_edge_count` metric silently understates inference activity — observability gap affects ops investigation when Supports edges are absent | Med | High | Medium |
| R-06 | Path C runs when `candidate_pairs` is empty — no-op is correct but the observability log must emit zero-count rather than skip, or monitoring will misread silence as absence of tick | Med | Med | Medium |
| R-07 | Path B + Path C collision on same `(source_id, target_id, Supports)` pair in same tick — second INSERT silently discarded; delivery may incorrectly treat `false` return from `write_graph_edge` as an error | Med | Med | Medium |
| R-08 | `existing_supports_pairs` pre-filter stale for intra-tick Path C writes — duplicate within Path C iteration possible if Phase 4 normalization to `(lo, hi)` is inconsistent | Med | Low | Low |
| R-09 | Cosine value NaN or Inf from HNSW boundary — `!weight.is_finite()` guard missing causes write of invalid weight to `graph_edges` | Med | Low | Low |
| R-10 | `informs_category_pairs` lookup requires category for each `(source_id, target_id)` — architecture defers this to `all_active` linear scan; O(n) per pair scales poorly at corpus size | Med | Med | Medium |
| R-11 | `nli_detection_tick.rs` file size — adding Path C write loop without extraction pushes file well beyond 500-line readability limit | Low | High | Medium |
| R-12 | Eval gate MRR regression — new `Supports` edges alter re-ranking signal and cause MRR < 0.2875 baseline | High | Low | Medium |
| R-13 | Config merge function not updated for `supports_cosine_threshold` — project-level config override is silently ignored | Med | Med | Medium |

---

## Risk-to-Scenario Mapping

### R-01: Category Data Gap in candidate_pairs (DESIGN GAP — MUST RESOLVE BEFORE DELIVERY)

**Severity**: High
**Likelihood**: High
**Impact**: AC-03 cannot be implemented. Path C will either crash on a missing category lookup, skip the filter entirely (writing false-positive Supports edges), or stall delivery while the approach is debated. This is the most critical unresolved design question.

**Root Cause**: `candidate_pairs` is typed `Vec<(u64, u64, f32)>` — only `(source_id, target_id, cosine)`. It contains no category data. The `informs_category_pairs` filter in AC-03 requires `source_category` and `target_category` for each pair. The architecture specifies using `all_active: Vec<EntryRecord>` (Phase 2 pre-fetch) as a linear scan to resolve categories per pair. This approach works functionally but has O(n) lookup complexity per candidate pair.

**Two viable resolution paths:**
1. **O(n) HashMap pre-build (preferred)**: After Phase 2 loads `all_active`, build a `HashMap<u64, &str>` mapping `entry_id → category`. Path C performs O(1) lookup per pair. Memory cost is one HashMap over already-loaded data. This should be specified explicitly.
2. **Per-pair DB lookup (rejected)**: One `SELECT category FROM entries WHERE id = ?` per candidate pair. Latency cost is O(candidates × round-trip). Violates the spirit of the no-new-HNSW-scan constraint and degrades hot-path performance.

**Test Scenarios**:
1. Unit test: `candidate_pairs` contains pair `(A, B, 0.70)` where A is `lesson-learned` and B is `decision` — assert `Supports` edge written (category lookup via `all_active` HashMap succeeds).
2. Unit test: `candidate_pairs` contains pair where one entry ID is absent from `all_active` (deprecated between Phase 2 and Path C) — assert no panic, no edge written, `warn!` emitted, loop continues.
3. Unit test: `candidate_pairs` contains pair with disallowed category combination (e.g., `decision` → `decision`) at cosine 0.80 — assert no edge written.
4. Integration test: Tick runs on a populated DB with cross-category pairs qualifying — assert `graph_edges` contains rows with `relation_type = 'Supports'` and `source = 'cosine_supports'`.

**Coverage Requirement**: The category resolution mechanism must be unit-tested with: qualifying pair present, missing entry (deprecated mid-tick), disallowed category pair above threshold. The HashMap build approach must be tested as the actual lookup path, not mocked away.

---

### R-02: write_nli_edge Refactor Silently Retags Existing Edges

**Severity**: High
**Likelihood**: Med
**Impact**: All existing `Informs` and NLI `Supports` edges in `graph_edges` have `source = 'nli'`. If `write_nli_edge` is refactored to delegate to `write_graph_edge` incorrectly (e.g., passing wrong `source` argument), existing callers continue to compile but write `source = 'cosine_supports'` instead of `'nli'`. GNN feature construction and `inferred_edge_count` silently break. Historical evidence: ADR-001 (entry #4025) identifies this as the primary correctness hazard.

**Test Scenarios**:
1. Unit test: call `write_nli_edge(...)` and assert the written `graph_edges` row has `source = 'nli'` and `created_by = 'nli'` — not `'cosine_supports'`.
2. Unit test: call `write_graph_edge(..., source = "cosine_supports")` and assert written row has `source = 'cosine_supports'` and `created_by = 'cosine_supports'`.
3. Regression: all existing Path A and Path B call sites compile against the unchanged `write_nli_edge` signature — verified by `cargo check`.
4. Integration test: after a tick with Path A active, query `graph_edges WHERE relation_type = 'Informs'` — assert all rows have `source = 'nli'`, none have `source = 'cosine_supports'`.

**Coverage Requirement**: The `write_nli_edge` → `write_graph_edge` delegation must be covered by a direct unit test that inspects the `source` column value. Compiler success alone is insufficient — the wrong string literal compiles.

---

### R-03: impl Default / Serde Default Divergence on supports_cosine_threshold

**Severity**: High
**Likelihood**: Med
**Impact**: `InferenceConfig::default().supports_cosine_threshold` returns a different value than deserialization of an empty config. Code paths using `InferenceConfig::default()` (tests, server bootstrap without config file) use one threshold; production deployments with config files use another. This is the exact failure mode from crt-038 gate-3b rework (lesson #4014, pattern #3817).

**Test Scenarios**:
1. Unit test (impl Default path): `assert_eq!(InferenceConfig::default().supports_cosine_threshold, 0.65_f32)`.
2. Unit test (serde path): `assert_eq!(toml::from_str::<InferenceConfig>("").unwrap().supports_cosine_threshold, 0.65_f32)`.
3. Unit test (backing function): `assert_eq!(default_supports_cosine_threshold(), 0.65_f32)`.
4. These three tests must be independent — not sharing the same deserialization call.

**Coverage Requirement**: Both the `impl Default` path and the serde deserialization path must be asserted in distinct tests. ADR-002 mandates using `supports_cosine_threshold: default_supports_cosine_threshold()` in the impl Default literal as the structural mitigation — the test is the verification.

---

### R-04: nli_post_store_k Removal Masking Test Regression

**Severity**: Med
**Likelihood**: Med
**Impact**: Adding `supports_cosine_threshold` and removing `nli_post_store_k` in the same commit makes it difficult to distinguish which change broke a test. Historical pattern: SR-06 flags this explicitly. ADR-002 notes the removal must be isolated in its own test verification step.

**Test Scenarios**:
1. AC-17: `grep -n "nli_post_store_k" crates/unimatrix-server/src/infra/config.rs` returns zero results — enforced as part of delivery verification.
2. AC-18: Deserializing TOML containing `nli_post_store_k = 5` into `InferenceConfig` succeeds without error — confirms `deny_unknown_fields` is not active.
3. Test suite must pass with `nli_post_store_k` removed before `supports_cosine_threshold` is added — or the commit sequence must be reviewable step-by-step.

**Coverage Requirement**: The removal is verified by both grep (static) and serde round-trip (runtime). The addition is verified by AC-09/AC-10/AC-16.

---

### R-05: inferred_edge_count Silent Undercount

**Severity**: Med
**Likelihood**: High
**Impact**: After crt-040, `inferred_edge_count` in `GraphCohesionMetrics` counts only `source = 'nli'` edges. Path C edges (`source = 'cosine_supports'`) are invisible in this metric. Operators seeing `inferred_edge_count = 0` with `supports_edge_count > 0` will be confused — the system appears to have written edges with no inference. Debugging will be impeded. This was flagged as SR-02 and is accepted as a follow-up, but the eval gate must explicitly use `supports_edge_count`.

**Test Scenarios**:
1. AC-15: Write a `source = 'cosine_supports'` edge; assert `inferred_edge_count` is unchanged (backward compat confirmed).
2. NFR-05: After one tick on a populated database, assert `supports_edge_count > 0` in `GraphCohesionMetrics`.
3. Document in code: add a comment at `inferred_edge_count` SQL query noting it counts only `source='nli'` and referencing the follow-up issue.

**Coverage Requirement**: The eval gate uses `supports_edge_count`, not `inferred_edge_count`. This must be verified — not assumed. The source-agnostic SQL for `supports_edge_count` is confirmed from architecture.

---

### R-06: Path C Observability When candidate_pairs Is Empty

**Severity**: Med
**Likelihood**: Med
**Impact**: When `candidate_pairs` is empty, Path C's loop is a no-op. If the observability log is inside the loop, it never fires. Monitoring systems that watch for the log line will see silence and misinterpret it as a missing Path C or a tick failure. ADR-003 requires a Path C observability log that emits even when zero edges are written.

**Test Scenarios**:
1. Unit test: call the tick with empty `candidate_pairs`; assert Path C observability log fires with `cosine_supports_candidates = 0` and `cosine_supports_edges_written = 0`.
2. Unit test: call the tick with qualifying pairs; assert log fires with correct candidate and written counts.
3. Verify log field names do not collide with Path A field names in structured tracing output.

**Coverage Requirement**: The observability log must be placed outside the write loop, fired unconditionally after Path C iteration. This must be specified and tested, not left to delivery judgment. Historical lesson #3723 identified missing tick completion logs as a tuning blindspot.

---

### R-07: Path B + Path C Same-Tick Collision False Error

**Severity**: Med
**Likelihood**: Med
**Impact**: When `nli_enabled=true`, Path B may attempt a `Supports` edge for the same `(source_id, target_id)` pair that Path C already wrote. `INSERT OR IGNORE` silently returns false. If delivery code treats `false` from `write_graph_edge` as an error condition (logs at `error!` or increments an error counter), every collision is misreported. The UNIQUE constraint is confirmed to be `(source_id, target_id, relation_type)` without `source` — so this scenario will occur in practice when NLI is enabled.

**Test Scenarios**:
1. Integration test (`nli_enabled=true`): set up a qualifying pair that both Path C and Path B would attempt; assert exactly one row in `graph_edges` for that pair; assert no error log emitted.
2. Unit test: call `write_graph_edge` twice for the same `(source_id, target_id, relation_type)` — assert second call returns `false`, no panic, no error log.
3. Assert Path C's write loop treats `false` return as a non-fatal no-op (continue, no warn).

**Coverage Requirement**: The `false` return from `write_graph_edge` for an already-existing pair must be tested as expected behavior and must NOT trigger a warn or error log. This distinction from genuine SQL errors (which do warn) must be explicit in the implementation.

---

### R-08: Intra-Tick Phase 4 Normalization Inconsistency

**Severity**: Med
**Likelihood**: Low
**Impact**: Phase 4 normalizes pairs to canonical `(lo, hi)` form. If this normalization is applied inconsistently (e.g., some pairs stored as `(hi, lo)`), the `existing_supports_pairs` pre-filter (which also uses canonical form) will miss duplicates. `INSERT OR IGNORE` is the backstop, but two rows with swapped `source_id`/`target_id` are different rows and both would be inserted.

**Test Scenarios**:
1. Unit test: verify Phase 4 normalization produces `source_id < target_id` for all pairs.
2. Unit test: call Path C with a pair `(A, B)` and `(B, A)` both in `candidate_pairs` above threshold — assert exactly one `Supports` edge written, not two.

**Coverage Requirement**: The canonical form invariant must be verified in a unit test, not assumed from Phase 4 code.

---

### R-09: NaN/Inf Cosine Guard Missing

**Severity**: Med
**Likelihood**: Low
**Impact**: HNSW cosine values are expected to be finite, but the guard is a safety requirement. A NaN weight written to `graph_edges.weight` (f32 SQLite REAL) produces undefined behavior in PPR traversal. Path A applies `!weight.is_finite()` — Path C must apply the same guard.

**Test Scenarios**:
1. Unit test: inject a pair with `cosine = f32::NAN` into `candidate_pairs`; assert no edge written, `warn!` emitted, loop continues.
2. Unit test: inject `cosine = f32::INFINITY`; same assertion.

**Coverage Requirement**: Guard must be present and tested before the threshold comparison. The guard is not optional — architecture mandates it explicitly in the error handling strategy section.

---

### R-10: all_active Linear Scan for Category Resolution

**Severity**: Med
**Likelihood**: Med
**Impact**: Path C needs `source_category` and `target_category` for each pair in `candidate_pairs`. The architecture specifies using `all_active: Vec<EntryRecord>` (Phase 2 pre-fetch). A linear scan of `all_active` for each pair is O(|all_active| × |candidate_pairs|). At moderate corpus size (thousands of active entries, up to 50 candidates per tick), this is O(50,000) comparisons per tick — on the hot path. A HashMap pre-build is the correct mitigation (see R-01).

**Test Scenarios**:
1. Performance test: verify that category resolution uses a `HashMap<u64, category>` built once from `all_active`, not a linear scan per pair.
2. Alternatively, code review gate: delivery agent must not submit a linear scan implementation.

**Coverage Requirement**: The HashMap pre-build approach must be mandated in the implementation brief, not left to delivery discretion. This is the architectural gap flagged in the spawn prompt.

---

### R-11: File Size — nli_detection_tick.rs Extraction

**Severity**: Low
**Likelihood**: High
**Impact**: `nli_detection_tick.rs` is already >2,000 lines. Path C adds another write loop plus observability log. If not extracted to a private helper function, the tick function body becomes unreadable and violates the 500-line rule (NFR-07). The architecture defers this decision to delivery, which creates risk of omission.

**Test Scenarios**:
1. Delivery gate: after implementation, count lines in the tick function body. If > 150 lines, extraction to `run_cosine_supports_path(...)` is required.
2. Code review: the Path C helper, if extracted, must be a private function in the same file, not a new module (module rename is deferred to Group 3).

**Coverage Requirement**: This is a code quality risk, not a functional risk. The delivery brief must include an explicit decision point for extraction.

---

### R-12: Eval Gate MRR Regression

**Severity**: High
**Likelihood**: Low
**Impact**: New `Supports` edges alter the co-access affinity and PPR signal, which feeds into the re-ranking formula (`0.85*similarity + 0.15*confidence + co-access boost`). If cosine Supports edges at threshold 0.65 introduce false-positive semantic connections, search re-ranking degrades and MRR falls below 0.2875. ASS-035 validated the threshold empirically, making regression unlikely, but the eval harness is the gate.

**Test Scenarios**:
1. AC-14: run `python product/research/ass-039/harness/run_eval.py`; assert MRR >= 0.2875.
2. Sanity check: run eval with empty `supports_edge_count` (pre-delivery) and confirm baseline MRR = 0.2875 — establishes that the harness itself is stable.

**Coverage Requirement**: The eval harness must run on the post-delivery system state with Path C active and at least one tick completed. Running it on an empty graph (no supports edges yet) does not validate AC-14.

---

### R-13: Config Merge Function Not Updated

**Severity**: Med
**Likelihood**: Med
**Impact**: `InferenceConfig` has a config merge function that propagates project-level overrides (following the `nli_informs_cosine_floor` pattern with f32 epsilon comparison). If `supports_cosine_threshold` is added to the struct but not to the merge function, a project-level config override is silently ignored. SPECIFICATION.md FR-08 specifies the merge function must be updated; delivery agents have historically missed merge function sites (lesson #4013 — spec names only a subset of sites).

**Test Scenarios**:
1. Unit test: create a project-level `InferenceConfig` with `supports_cosine_threshold = 0.70` and a base config with `0.65`; call the merge function; assert merged config has `0.70`.
2. Grep verification: confirm `supports_cosine_threshold` appears in the merge function body.

**Coverage Requirement**: The merge function test is required alongside the serde and impl Default tests. All three config paths (serde, impl Default, merge) must be independently verified.

---

## Integration Risks

### IR-01: Path A / Path C Candidate Set Overlap

Path A (Informs) uses a separate HNSW scan (`informs_metadata`) with its own cosine floor (`nli_informs_cosine_floor = 0.50`) and temporal ordering guard. Path C uses `candidate_pairs` from the NLI Supports candidate scan (`supports_candidate_threshold = 0.50` default). These two scans are independent. A pair above both thresholds could theoretically receive both an `Informs` edge (from Path A) and a `Supports` edge (from Path C). This is intended — the relation types are distinct. Verify: `UNIQUE(source_id, target_id, relation_type)` allows one row per `(pair, type)`, so `(A, B, Informs)` and `(A, B, Supports)` coexist correctly.

### IR-02: supports_candidate_threshold vs. supports_cosine_threshold Ordering

Path C applies its threshold filter (0.65) against pairs already filtered by `supports_candidate_threshold` (0.50 default). This means all Path C candidates have cosine >= 0.50. Path C then applies the finer >= 0.65 filter. If an operator raises `supports_candidate_threshold` above 0.65, Path C receives zero candidates regardless of threshold — it silently becomes a no-op. This is a misconfiguration risk, not a bug, but should be documented.

### IR-03: crt-041 Concurrent InferenceConfig Modification

crt-041 also touches `InferenceConfig` (per SCOPE.md cross-feature note). If crt-041 merges before crt-040, the impl Default struct literal will have changed. The delivery agent must rebase and verify both the `supports_cosine_threshold` entry in impl Default and the `nli_post_store_k` removal are applied to the latest struct literal, not a stale snapshot.

---

## Edge Cases

| Edge Case | Risk | Mitigation |
|-----------|------|------------|
| `candidate_pairs` is empty | Path C loop is no-op; observability log must still fire | Log placed outside the loop |
| All pairs below cosine threshold | Zero writes, budget not consumed | Log emits `cosine_supports_edges_written = 0` |
| All pairs above threshold but in disallowed category pairs | Zero writes due to category filter | Category filter test (AC-03) |
| `MAX_COSINE_SUPPORTS_PER_TICK = 50` exhausted in first tick | Remaining candidates skipped; next tick will cover them | Budget cap is correct behavior; no test needed beyond AC-12 |
| Entry deprecated between Phase 2 and Path C execution | `all_active` HashMap has no entry for that ID | `None` branch: continue, no write, no panic |
| `supports_cosine_threshold = 0.65` exactly (boundary) | Pair at exactly 0.65 must qualify (`>=` not `>`) | Boundary test: pair with cosine exactly 0.65 is written |
| `supports_cosine_threshold` at boundary values 0.0 or 1.0 | validate() must reject these (exclusive range) | AC-09 validation tests |
| Config file contains `nli_post_store_k` after removal | Serde must silently discard unknown field | AC-18 forward-compatibility test |
| Path C budget counter incremented on failed write (false return) | Budget artificially exhausted | Counter must only increment on `true` return from `write_graph_edge` |

---

## Security Risks

### Untrusted Input Assessment

**`supports_cosine_threshold` config field**: Loaded from operator-controlled `config.toml`. Value is an f32 validated by `InferenceConfig::validate()` against range `(0.0, 1.0)` exclusive. Blast radius of misconfiguration: threshold at 0.001 causes near-universal Supports edge writes (flooding the graph); threshold at 0.999 causes zero writes (silent feature disablement). The range validation is the security control. No injection risk — the value is used in a float comparison, not in SQL string composition.

**`informs_category_pairs` config field**: Reused as-is from Path A. Already validated in existing code. No new attack surface introduced by Path C.

**`metadata` JSON written to `graph_edges`**: Path C writes `{"cosine": <f32>}` constructed in Rust code from a typed f32. Not derived from untrusted user input. No injection risk.

**`EDGE_SOURCE_COSINE_SUPPORTS` string constant**: Hardcoded in Rust source, not configurable. No injection surface.

**DB write path**: `write_graph_edge` uses parameterized `sqlx` queries (`?1`, `?2`, etc.). No string interpolation into SQL. SQL injection is not possible via this path.

**Blast radius if Path C is misconfigured**: Worst case is graph flooding with false-positive Supports edges at a low threshold. This degrades PPR traversal quality and could inflate `supports_edge_count`. It does not expose data outside the system or allow privilege escalation. Recovery is graph compaction via `context_status(maintain=true)`.

---

## Failure Modes

| Failure | Expected Behavior | Test Coverage |
|---------|------------------|---------------|
| SQL error in `write_graph_edge` | `warn!` log, returns `false`, Path C loop continues | Unit test: inject SQL failure, assert no panic |
| Entry missing from `all_active` HashMap | `continue` to next pair, no edge written, no `unwrap` panic | Unit test: pair with unknown entry ID |
| `write_graph_edge` returns `false` (IGNORE'd by UNIQUE constraint) | Loop continues; not counted in budget; no log emission | Unit test: duplicate pair |
| NaN/Inf cosine from Phase 4 | Guard fires, `warn!`, continue | Unit test: NaN/Inf injected (R-09) |
| MRR regression after delivery | Eval harness fails at AC-14 gate; delivery does not merge | Eval gate is mandatory before PR merge |
| `supports_candidate_threshold` raised above `supports_cosine_threshold` | Path C gets zero candidates; `supports_edge_count` does not grow | Operator documentation; monitoring via `context_status` |
| `write_graph_edge` budget counter wrong (counts false returns) | Budget exhausted prematurely; subsequent qualifying pairs skipped | Unit test: 50 qualifying + 10 duplicate pairs; assert 50 rows written, not 40 |

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01: `write_nli_edge` hardcodes `'nli'`; generalization risks silently retagging edges | R-02 | Resolved by ADR-001: `write_graph_edge` sibling added; `write_nli_edge` unchanged. Unit test asserts `write_nli_edge` still writes `source='nli'`. |
| SR-02: `inferred_edge_count` silently incomplete for cosine Supports edges | R-05 | Accepted: eval gate uses `supports_edge_count` (source-agnostic). Follow-up issue filed. AC-15 confirms backward compat. |
| SR-03: `MAX_COSINE_SUPPORTS_PER_TICK` not operator-tunable | — | Accepted per ADR-004. TODO comment at constant site mandated. Not a delivery risk. |
| SR-04: UNIQUE constraint scope unverified at scope time | — | Resolved in architecture: DDL confirmed `UNIQUE(source_id, target_id, relation_type)` without `source`. INSERT OR IGNORE is correct dedup backstop. |
| SR-05: `informs_category_pairs` reuse couples Informs and Supports filter domains | — | Accepted. `supports_category_pairs` is a documented extension point for a follow-on feature. No crt-040 risk. |
| SR-06: `nli_post_store_k` removal bundled with feature risks masking regressions | R-04 | Mitigated by AC-17/AC-18 isolation tests and delivery sequencing guidance. |
| SR-07: Path C errors could break tick infallibility | — | Resolved in architecture error handling strategy: no `?`, no `unwrap`, all errors log `warn!` and continue. R-09 covers NaN guard specifically. |
| SR-08: Path C is downstream of Phase 4 — changes to Phase 4 candidate selection silently reduce Path C input | R-06 | Partially mitigated: observability log fires even when zero candidates. Full protection requires architecture documentation that Phase 4 threshold must remain <= Path C threshold. |
| SR-09: `existing_supports_pairs` pre-filter stale for intra-tick writes | R-07, R-08 | Resolved: Path C before Path B (ADR-003). INSERT OR IGNORE is authoritative dedup. Intra-tick Path C normalization to `(lo, hi)` prevents self-collision. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 1 (R-01) | 4 scenarios — category resolution mechanism must be specified before delivery begins |
| High | 3 (R-02, R-03, R-12) | 10 scenarios — write delegation correctness, dual-site config, eval gate |
| Medium | 7 (R-04–R-07, R-10, R-11, R-13) | 16 scenarios — removal isolation, observability, collision semantics, file size, config merge |
| Low | 2 (R-08, R-09) | 4 scenarios — normalization consistency, NaN/Inf guard |

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for `"lesson-learned failures gate rejection graph inference tick"` — found #3579 (gate-3b missing tests), #3723 (tick observability gap), #3668 (candidate stall)
- Queried: `/uni-knowledge-search` for `"risk pattern graph edge write source tagging"` — found #4025 (write_nli_edge pattern, directly informs R-02), #3884, #3889
- Queried: `/uni-knowledge-search` for `"InferenceConfig serde default impl Default dual site"` — found #3817, #4013, #4014 (all directly inform R-03, R-04, R-13)
- Queried: `/uni-knowledge-search` for `"category lookup candidate_pairs tick missing data"` — found #3659, #3668; no prior art for the category gap pattern specifically
- Queried: `/uni-knowledge-search` for `"tick infallible error propagation unwrap warn continue"` — found #3897 (infallible bidirectional tick pattern), #1542 (error semantics for tick loops)
- Stored: nothing novel — R-01 (category data gap in candidate_pairs) is feature-specific and not yet a cross-feature pattern. If it recurs in crt-041 or Group 4, store as a pattern then.
