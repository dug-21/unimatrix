# crt-013: Retrieval Calibration

## Problem Statement

The retrieval pipeline has three issues that degrade result quality and operational scalability:

1. **Episodic augmentation vs co-access double-counting (#50):** Three mechanisms consume CO_ACCESS data — MicroLoRA adaptation (pre-HNSW embedding adjustment), co-access boost (post-ranking +0.03 additive), and episodic augmentation (no-op stub). The first two already double-count co-access signal at different pipeline stages. The stub, if activated, would triple-count. Additionally, `co_access_affinity()` (W_COAC=0.08) is defined in `confidence.rs:239` but never called in production — it was designed as a query-time confidence component but the search pipeline uses `compute_search_boost()` instead. The relationship between these mechanisms is unclear and undocumented.

2. **crt-010 status penalty effectiveness (#118):** crt-010 introduced multiplicative penalties (DEPRECATED_PENALTY=0.7, SUPERSEDED_PENALTY=0.5) applied post-rerank in `search.rs:278-280`. These replaced the negligible ~0.008 additive penalty from base_score. However, no validation has been performed to confirm the penalties are effective in practice — specifically whether deprecated entries still outrank active entries in realistic scenarios, and whether the UDS strict-mode filter correctly excludes all deprecated/superseded entries from silent injection.

3. **Briefing neighbor count hardcode (#17):** `BriefingService` hardcodes `k: 3` at `services/briefing.rs:228` for semantic search candidates. The token budget constrains output anyway, but k=3 limits recall — the feature-tag boost and co-access anchoring only sort within those 3 candidates. Making this configurable enables tuning as the knowledge base grows.

4. **Status scan scalability (#17):** `StatusService` performs `SELECT * FROM entries` at `status.rs:136-144`, loading all entries into memory to compute correction chain metrics, trust source distribution, and security metrics. This is O(n) on total entries. With SQL aggregation queries, most of these can be O(1) reads from pre-computed counters or GROUP BY queries without full deserialization.

## Goals

1. **Resolve co-access signal overlap** — Decide the architectural relationship between MicroLoRA adaptation, co-access boost, episodic augmentation stub, and `co_access_affinity()`. Remove or consolidate redundant mechanisms. Document the surviving architecture in an ADR.

2. **Validate crt-010 status penalties** — Write integration tests that prove deprecated entries rank below active entries at comparable similarity, and that UDS strict mode excludes deprecated/superseded entries. If penalties are insufficient, adjust constants.

3. **Make briefing neighbor count configurable** — Replace the hardcoded `k: 3` with a configurable parameter (default 3) that can be tuned without code changes.

4. **Optimize status scan** — Replace the full entries table scan with SQL aggregation queries for correction chain metrics, trust source distribution, and security metrics.

## Non-Goals

- **No changes to the 6-factor confidence formula.** The stored formula (W_BASE through W_TRUST, sum=0.92) is unchanged.
- **No changes to HNSW vector index structure.** Vector index pruning was addressed by crt-010 compaction.
- **No new MCP tools or parameters** beyond making k configurable internally.
- **No schema migrations.** All required tables and columns exist.
- **No changes to co-access pair recording.** CO_ACCESS storage and UsageDedup are unmodified; only the consumption/boosting side is in scope.
- **No changes to MicroLoRA adaptation logic.** The embedding-level co-access mechanism (crt-006) is not modified — only the post-ranking mechanisms are evaluated.
- **No marker injection fix (#17 item 3).** The `[KNOWLEDGE DATA]` marker escaping issue is a separate concern.

## Background Research

### Co-Access Signal Architecture (Current State)

Four mechanisms consume CO_ACCESS data at different pipeline stages:

| Mechanism | Stage | Location | Max Effect | Status |
|-----------|-------|----------|------------|--------|
| MicroLoRA adaptation | Pre-HNSW | `unimatrix-adapt/src/service.rs` | Embedding shift | Production |
| Co-access boost | Post-rerank | `unimatrix-engine/src/coaccess.rs` | +0.03 additive | Production |
| Co-access affinity | Query-time confidence | `unimatrix-engine/src/confidence.rs:239` | +0.08 to confidence | **Dead code** |
| Episodic augmentation | Post-search | `unimatrix-adapt/src/episodic.rs` | +0.02 (designed) | **No-op stub** |

**Finding 1:** `co_access_affinity()` is defined with weight W_COAC=0.08 but is never called in the search pipeline. It was designed in crt-004 as a query-time addition to stored confidence, but the search pipeline instead uses the additive `compute_search_boost()`. The 0.08 weight reservation in the confidence formula is effectively wasted.

**Finding 2:** MicroLoRA (embedding-level) and co-access boost (score-level) both consume CO_ACCESS data. This is double-counting by design — the original intent (crt-006 PR #49, issue #50) was to evaluate empirically whether MicroLoRA makes the scalar boost redundant (Option D: ship, then evaluate).

**Finding 3:** Episodic augmentation is a no-op stub (crt-006 SR-04). Its designed max_boost of 0.02 would overlap with the 0.03 co-access boost, adding a third layer of CO_ACCESS-derived scoring.

### crt-010 Status Penalties (Current State)

Implemented in `search.rs` and `confidence.rs`:

- **Flexible mode (MCP):** `final_score = (rerank_score + co_access_boost + provenance) * penalty` where penalty is 0.7 (deprecated) or 0.5 (superseded)
- **Strict mode (UDS):** Hard filter drops non-Active and superseded entries entirely
- **Co-access exclusion:** Deprecated IDs collected at `search.rs:305-309`, passed to `compute_search_boost()`, excluded from both anchor and partner roles in `coaccess.rs:133-135,153-155`

**Quantitative validation needed:** With the 0.7 penalty, does a deprecated entry with similarity=0.90 still outrank an active entry with similarity=0.88?
- Active: `(0.85*0.88 + 0.15*0.65) * 1.0 = 0.845`
- Deprecated: `(0.85*0.90 + 0.15*0.59) * 0.7 = (0.854) * 0.7 = 0.598`

The multiplicative penalty appears effective — 0.598 << 0.845. But edge cases need testing: what about deprecated entries with very high similarity (0.99) vs active entries with moderate similarity (0.70)?

### Briefing k=3 Hardcode

At `services/briefing.rs:228`, `k: 3` limits semantic candidates before token-budget trimming. The co-access anchors (line 236) boost relevant entries within those 3, but with a growing knowledge base, fetching only 3 candidates limits recall. A configurable parameter (e.g., via `BriefingConfig` or similar) would allow tuning.

### Status Scan Performance

`status.rs:136-144` loads all entries to compute:
- Correction chain metrics (supersedes/superseded_by counts): lines 158-164
- Trust source distribution: lines 166-171
- Security metrics (entries without attribution): lines 172-174
- Active entries list for lambda computation: lines 175-179

All of these can be replaced with targeted SQL queries:
- `SELECT COUNT(*) FROM entries WHERE supersedes IS NOT NULL`
- `SELECT trust_source, COUNT(*) FROM entries GROUP BY trust_source`
- `SELECT COUNT(*) FROM entries WHERE created_by = ''`
- Active entries still needed for lambda, but can use `WHERE status = 'Active'`

## Scope

### Component 1: Co-Access Signal Consolidation

**Crate:** `unimatrix-engine`, `unimatrix-adapt`

**Decision:** Resolve the four-mechanism overlap. Recommended approach based on #50 analysis:

1. **Keep MicroLoRA adaptation** — embedding-level co-access is the most principled mechanism (operates in embedding space, integrated with ONNX model). No change.
2. **Keep co-access boost** — `compute_search_boost()` in `coaccess.rs` provides a lightweight, interpretable post-ranking signal. Effective at +0.03 max.
3. **Remove episodic augmentation stub** — delete `crates/unimatrix-adapt/src/episodic.rs`. The stub adds no value and if activated would triple-count CO_ACCESS. If a distinct signal emerges later, reintroduce it.
4. **Remove `co_access_affinity()`** — dead code in `confidence.rs:239-257`. Never called in production. The W_COAC=0.08 weight reservation was superseded by the boost approach. Remove the function and redistribute the 0.08 weight or leave as documented headroom.

**ADR needed:** Document the surviving two-mechanism architecture (MicroLoRA pre-HNSW + scalar boost post-rerank) and why the other two were removed.

**Design decision: W_COAC weight disposition.** Options:
- A) Delete W_COAC constant, keep stored weights at 0.92 (current behavior, no formula change)
- B) Redistribute 0.08 across existing 6 factors (formula change, requires recalibration)
- C) Keep W_COAC constant as documented headroom for future use

Option A is recommended — the 0.08 was never used in stored confidence, so removing the constant and its tests is a pure dead-code cleanup with zero behavioral change.

### Component 2: Status Penalty Validation

**Crate:** `unimatrix-server` (integration tests)

Write integration tests covering:
- **Flexible mode:** Deprecated entry (sim=0.90) ranks below active entry (sim=0.88) after penalty
- **Flexible mode:** Superseded entry (sim=0.95) ranks below active entry (sim=0.85) after penalty
- **Strict mode:** Deprecated and superseded entries excluded from UDS results entirely
- **Co-access exclusion:** Deprecated entries do not receive or provide co-access boost
- **Edge case:** Query where only deprecated entries match — results still returned (degraded, not empty)
- **Edge case:** Superseded entry with Active successor — successor injected, superseded entry penalized

If any test reveals the penalties are insufficient, adjust DEPRECATED_PENALTY and SUPERSEDED_PENALTY constants.

### Component 3: Configurable Briefing Neighbor Count

**Crate:** `unimatrix-server`

**File:** `services/briefing.rs`

- Add a `semantic_k` field to `BriefingService` (or a config struct passed at construction)
- Default value: 3 (preserves current behavior)
- Wire the parameter through `ServiceSearchParams::k` at line 228
- Make configurable via server config (environment variable or config file)

### Component 4: Status Scan Optimization

**Crate:** `unimatrix-server`, `unimatrix-store`

**File:** `services/status.rs:130-180`

Replace the full entries table scan with targeted SQL queries:

1. **Correction chain metrics:** `SELECT COUNT(*) FROM entries WHERE supersedes IS NOT NULL` and `SELECT COUNT(*) FROM entries WHERE superseded_by IS NOT NULL` and `SELECT SUM(correction_count) FROM entries`
2. **Trust source distribution:** `SELECT COALESCE(NULLIF(trust_source, ''), '(none)'), COUNT(*) FROM entries GROUP BY 1`
3. **Security metrics:** `SELECT COUNT(*) FROM entries WHERE created_by = ''`
4. **Active entries for lambda:** Keep targeted query `SELECT ... FROM entries WHERE status = 'Active'` (needed for coherence dimensions, but much smaller result set)
5. **Total correction count:** SQL SUM instead of Rust-side iteration

Add these as new `Store` methods or extend existing query infrastructure.

## Acceptance Criteria

- AC-01: Episodic augmentation stub (`episodic.rs`) removed from `unimatrix-adapt`. Module declaration removed from `lib.rs`. All references cleaned up.
- AC-02: `co_access_affinity()` function removed from `confidence.rs`. W_COAC constant disposition decided and implemented per ADR. Tests for removed function removed.
- AC-03: ADR documenting the two-mechanism co-access architecture (MicroLoRA + boost) stored in Unimatrix.
- AC-04: Integration test proves deprecated entry (sim=0.90) ranks below active entry (sim=0.88) in Flexible mode.
- AC-05: Integration test proves superseded entry excluded in Strict mode.
- AC-06: Integration test proves deprecated entries excluded from co-access boost.
- AC-07: Integration test proves deprecated-only query still returns results in Flexible mode.
- AC-08: Briefing `k` parameter configurable with default 3. Existing behavior unchanged when unconfigured.
- AC-09: Status scan no longer performs `SELECT * FROM entries`. Correction chain metrics, trust source distribution, and security metrics computed via SQL aggregation.
- AC-10: `context_status` returns identical results before and after optimization (verified by comparison test).
- AC-11: All existing tests pass with no regressions.

## Affected Crates

| Crate | Changes |
|-------|---------|
| `unimatrix-engine` | Remove `co_access_affinity()`, W_COAC disposition, remove associated tests |
| `unimatrix-adapt` | Remove `episodic.rs` module and references |
| `unimatrix-server` | Briefing k configuration, status scan optimization, status penalty integration tests |
| `unimatrix-store` | New SQL aggregation query methods for status scan |

## Design Decisions Needed

1. **W_COAC weight disposition:** Remove constant (Option A), redistribute (Option B), or keep as headroom (Option C). Recommendation: Option A.
2. **Briefing k configuration mechanism:** Environment variable, server config struct, or BriefingService constructor parameter. Recommendation: Server config struct with env var override.
3. **Status scan: new Store methods vs inline SQL.** New Store methods are cleaner but add API surface. Recommendation: New Store methods — consistent with service-layer abstraction.

## Constraints

- **Depends on crt-011** for correct confidence data. Session count dedup must be landed first.
- **No schema migrations.** All work uses existing tables and columns.
- **Backward-compatible.** Default briefing k=3 preserves current behavior. Status scan optimization is internal.
- **Extend existing test infrastructure.** Use `TestServiceContext` and existing integration test patterns.

## Open Questions

1. **Should we also remove the episodic augmentation test scaffolding in `unimatrix-adapt`?** The tests are trivial (no-op stub assertions). Removing them is clean; keeping them serves no purpose. Recommendation: remove.
2. **Is the 0.03 co-access boost cap still appropriate after MicroLoRA handles embedding-level co-access?** May warrant empirical evaluation in col-015 (Wave 4 validation). Out of scope here — flag for future.
3. **Should status scan optimization be a separate Store method per metric, or a single `compute_status_aggregates()` batch query?** Trade-off: granularity vs round-trip count. Recommendation: single method returning a struct.

## References

- GH #50: Episodic augmentation vs co-access boost overlap
- GH #118: Status signals underweighted in search retrieval (ass-015 research)
- GH #17: Status scan optimization, briefing k, marker injection
- `crates/unimatrix-engine/src/coaccess.rs`: Co-access boost computation
- `crates/unimatrix-engine/src/confidence.rs`: Confidence formula, W_COAC, penalties
- `crates/unimatrix-adapt/src/episodic.rs`: Episodic augmentation stub
- `crates/unimatrix-server/src/services/search.rs`: Search pipeline with penalties
- `crates/unimatrix-server/src/services/briefing.rs:228`: k=3 hardcode
- `crates/unimatrix-server/src/services/status.rs:136-144`: Full table scan
- `product/features/crt-010/SCOPE.md`: Status-aware retrieval scope

## Tracking

https://github.com/dug-21/unimatrix/issues/156

- GitHub Issues: #50, #118, #17
