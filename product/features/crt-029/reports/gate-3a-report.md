# Gate 3a Report: crt-029

> Gate: 3a (Design Review) — RETRY after rework
> Date: 2026-03-27
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | Component boundaries, file split, interfaces match architecture |
| Specification coverage | PASS | All FR/NFR/AC addressed in pseudocode |
| Risk coverage | PASS | All 13 risks mapped to test scenarios; critical risks R-01/R-09 explicitly covered |
| Interface consistency | PASS | Shared types and signatures consistent across all pseudocode files |
| C-13: no contradiction_threshold in write_inferred_edges_with_cap | PASS | Signature confirmed correct; pseudocode explicitly discards contradiction score |
| C-14 / R-09: Phase 7 rayon closure sync-only | PASS | Closure annotated sync-only; test plan includes independent validator requirement |
| AC-06c: Phase 3 cap before Phase 4 embeddings | PASS | Phase ordering confirmed correct in pseudocode and OVERVIEW data flow |
| AC-18†: grep gate for 52 InferenceConfig struct literals | PASS | Grep gate present in both pseudocode (inference-config.md) and test plan |
| Wave ordering | PASS | inference-config + store-query-helpers + promotions Wave 1; nli-detection-tick Wave 2; background-call-site Wave 3 |
| Knowledge stewardship compliance | PASS | Architect report now has `## Knowledge Stewardship` section with Queried: and Stored: entries (#3656–#3659) |
| Architect report stale note (C-13) | WARN | Report point 5 references contradiction_threshold parameter superseded by C-13 — architecture document is authoritative; rework did not touch this note |

---

## Detailed Findings

### Check 1: Architecture Alignment

**Status**: PASS

**Evidence**:
- `pseudocode/OVERVIEW.md` lists exactly the four components specified in ARCHITECTURE.md: inference-config (`infra/config.rs`), store-query-helpers (`unimatrix-store/src/read.rs`), nli-detection-tick (`services/nli_detection_tick.rs`), background-call-site (`background.rs`).
- Component boundaries match: `nli_detection_tick.rs` is a new module (mandatory split per ADR-001 / ARCHITECTURE.md §Component 3); module declaration and `pub(crate)` promotions are correctly scoped to inference-config component.
- Technology choices consistent: all queries use `read_pool()` / `write_pool_server()` per C-02; no new crate dependencies (NFR-03); no schema migration (NFR-04).
- ADR-001 through ADR-004 (Unimatrix entries #3656–#3659) are referenced and their decisions are faithfully reflected in all pseudocode files.
- The ARCHITECTURE.md `run_graph_inference_tick` signature in the integration surface table (`pub async fn run_graph_inference_tick(store, nli_handle, vector_index, rayon_pool, config)`) exactly matches the pseudocode nli-detection-tick.md Function 1 signature.

---

### Check 2: Specification Coverage

**Status**: PASS

**Evidence**:
- **FR-01** (run_graph_inference_tick): nli-detection-tick.md implements all 14 sub-requirements including guard (Phase 1), active-only query (Phase 2), source-candidate selection before get_embedding (Phase 3), HNSW expansion with `graph_inference_k` (Phase 4), deduplication (Phase 4 `pair_key`), pre-filter (Phase 4 `existing_supports_pairs.contains`), truncation (Phase 5), single rayon dispatch (Phase 7), Supports-only write (Phase 8), and debug logging.
- **FR-02** (four new InferenceConfig fields): inference-config.md documents all four fields with correct types, defaults, and `#[serde(default)]` annotations.
- **FR-03** (validate() extensions): inference-config.md sections 4a–4e implement all five guards: candidate threshold range, edge threshold range, cross-field invariant (strict `>=` reject predicate per AC-02), max_graph_inference_per_tick range, graph_inference_k range.
- **FR-04** (priority ordering): OVERVIEW.md data flow and nli-detection-tick.md Phase 5 implement the three-tier sort (cross-category, isolated, similarity descending). Phase 3 `select_source_candidates` provides isolated-first ordering for source selection.
- **FR-05** (query_entries_without_edges): store-query-helpers.md implements the exact SQL from the specification with `status = 0` and `bootstrap_only = 0` filter.
- **FR-06** (background call site): background-call-site.md implements the call after `maybe_run_bootstrap_promotion` gated on `inference_config.nli_enabled`.
- **FR-07** (edge write conventions): nli-detection-tick.md write_inferred_edges_with_cap calls `write_nli_edge` with `EDGE_SOURCE_NLI`; `INSERT OR IGNORE` and `bootstrap_only = false` are handled inside `write_nli_edge` (existing function, per ARCHITECTURE.md).
- **FR-08** (Supports-only write): nli-detection-tick.md explicitly discards contradiction score. No `contradiction_threshold` parameter.
- **FR-09** (per-tick edge cap): write_inferred_edges_with_cap has `IF edges_written >= max_edges { break }` guard.
- **FR-10** (source-candidate bound before embedding): Phase 3 runs on metadata only; Phase 4 calls `get_embedding` only for the capped list.
- **NFR-01–07**: Addressed in comments and structure. Single rayon dispatch (NFR-02, W1-2); no new crates (NFR-03); no schema migration (NFR-04); file split in new module (NFR-05); single-batch dispatch per tick minimises contention (NFR-06); targeted SQL query (NFR-07).
- **C-13, C-14**: Both explicitly documented in the module header docblock of nli-detection-tick.md.
- **AC-16 (idempotency)**: Covered by Phase 4 pre-filter + `INSERT OR IGNORE` backstop; test scenario present.
- **AC-18†** (52 InferenceConfig struct literals): grep gate in inference-config.md and test plan.

No out-of-scope features are implemented. No functional requirement is unaddressed.

---

### Check 3: Risk Coverage

**Status**: PASS

**Evidence**: All 13 risks from RISK-TEST-STRATEGY.md have corresponding test scenarios and acceptance criteria:

| Risk | Coverage in Test Plans |
|------|----------------------|
| R-01 (Contradicts from tick) | Grep gate + `test_write_inferred_edges_supports_only_no_contradicts` (nli-detection-tick.md) |
| R-02 (unbounded get_embedding) | `test_select_source_candidates_cap_enforced` + AC-06c unit test (nli-detection-tick.md) |
| R-03 (threshold boundary) | 5 validation unit tests in inference-config.md covering AC-02/AC-03 |
| R-04 (pre-filter scan) | 6 unit tests for `query_entries_without_edges` + 6 for `query_existing_supports_pairs` (store-query-helpers.md) |
| R-05 (rayon pool starvation) | `test_write_inferred_edges_with_cap_cap_enforced` + AC-08 single-dispatch test (nli-detection-tick.md) |
| R-06 (pool ambiguity) | Grep gate in store-query-helpers.md; human resolution required for ADR conflict |
| R-07 (struct literal trap) | Grep gate in inference-config.md; `cargo check` gate |
| R-08 (cap logic inlining) | `write_inferred_edges_with_cap` extracted as standalone function; AC-11 test |
| R-09 (tokio handle in rayon) | Grep gates + independent code review requirement (OVERVIEW.md, nli-detection-tick.md) |
| R-10 (W1-2 spawn_blocking) | Grep gate: `grep -n 'spawn_blocking'` on nli_detection_tick.rs |
| R-11 (pub(crate) promotions) | Compile gate; grep gate in background-call-site.md |
| R-12 (priority ordering) | `test_select_source_candidates_priority_ordering_combined` (nli-detection-tick.md) |
| R-13 (stale pre-filter HashSet) | `test_tick_idempotency` (nli-detection-tick.md) |

Risk priorities are reflected in test plan emphasis: R-09 (Critical) receives explicit independent reviewer requirement; R-02 (Critical) receives dedicated cap-enforcement unit test for `select_source_candidates`; R-01 receives multiple verification vectors (grep + behavioral unit test + integration test).

SR-06 (R-06) remains an unresolved human decision per RISK-TEST-STRATEGY.md — this is documented and not a test plan gap.

---

### Check 4: Interface Consistency

**Status**: PASS

**Evidence**: All interfaces are consistent across OVERVIEW.md, per-component pseudocode files, and the ARCHITECTURE.md integration surface table.

Key interface consistency points:
- `write_inferred_edges_with_cap` signature: `async fn(store: &Store, pairs: &[(u64, u64)], nli_scores: &[NliScores], supports_threshold: f32, max_edges: usize) -> usize` — matches in nli-detection-tick.md Function 3, OVERVIEW.md integration surface, and ARCHITECTURE.md integration surface.
- `select_source_candidates` signature: `fn(all_active: &[EntryRecord], existing_edge_set: &HashSet<(u64, u64)>, isolated_ids: &HashSet<u64>, max_sources: usize) -> Vec<u64>` — consistent across OVERVIEW.md and nli-detection-tick.md Function 2.
- `HashSet<(u64, u64)>` normalization contract: both store-query-helpers.md (method 2 notes) and nli-detection-tick.md Phase 4 use `(min(a,b), max(a,b))` canonical form.
- `query_entries_without_edges` returns `Vec<u64>` (not `HashSet`); caller converts — consistent between store-query-helpers.md and nli-detection-tick.md Phase 2.
- `pub(crate)` promotions listed consistently in OVERVIEW.md integration surface and inference-config.md section 6.
- Background call site parameter mapping in background-call-site.md matches the `run_graph_inference_tick` signature exactly.

No contradictions found between component pseudocode files for shared types or function signatures.

---

### Check 5: C-13 — write_inferred_edges_with_cap has no contradiction_threshold and writes no Contradicts edges

**Status**: PASS

**Evidence** (nli-detection-tick.md Function 3):
- Signature: `async fn write_inferred_edges_with_cap(store, pairs, nli_scores, supports_threshold, max_edges) -> usize` — no `contradiction_threshold` parameter is present.
- Explicit comment: "Supports-ONLY: No `contradiction_threshold` parameter. No `Contradicts` writes. The `contradiction` score in `NliScores` is not read."
- Pseudocode body: "Evaluate ONLY entailment — contradiction score is DISCARDED (C-13)."
- `write_nli_edge` call: `relation_type = "Supports"` — hard-coded string; no Contradicts path exists.
- Module header docblock: explicitly documents C-13 / AC-10a constraint.
- ARCHITECTURE.md integration surface table also explicitly documents the `write_inferred_edges_with_cap` signature as "Supports-only; no `contradiction_threshold` (C-13)".

---

### Check 6: C-14 / R-09 — Phase 7 rayon closure sync-only with independent validator requirement

**Status**: PASS

**Evidence** (nli-detection-tick.md Phase 7):
- Closure body contains only: pair reference construction (sync) and `provider_clone.score_batch(&pairs_ref)` (synchronous function call).
- Comment on closure body: "SYNC-ONLY CLOSURE BODY — no .await, no Handle::current()".
- `.await` is positioned outside the closure body — on the tokio thread waiting for the rayon future.
- Header docblock: explicitly lists "PROHIBITED inside any `rayon_pool.spawn()` closure" items.
- Pre-merge grep gates documented in pseudocode: `Handle::current`, `spawn_blocking`, `Contradicts` all checked.
- Test plan OVERVIEW.md: "the reviewer MUST NOT be the same agent or person who wrote the closure (C-14 requirement)."
- Test plan nli-detection-tick.md: explicit Independent Code Review Requirement section specifying the Stage 3c tester serves as independent reviewer.

---

### Check 7: AC-06c — Phase 3 cap before Phase 4 get_embedding

**Status**: PASS

**Evidence**:
- OVERVIEW.md data flow: Phase 3 (`select_source_candidates`) precedes Phase 4 (`VectorIndex::get_embedding`) in the explicit sequence.
- nli-detection-tick.md Phase 3 comment: "AC-06c / R-02 CRITICAL CONSTRAINT: select_source_candidates runs on metadata only (IDs + category strings). No get_embedding call occurs in this phase."
- Phase 4 begins only after Phase 3 completion; first `get_embedding` call occurs in Phase 4 after Phase 3 result is in hand.
- `select_source_candidates` pseudocode operates only on `&[EntryRecord]` (id + category), `HashSet<(u64,u64)>`, `HashSet<u64>` — no vector index dependency.
- Test plan nli-detection-tick.md: `test_select_source_candidates_cap_enforced` — 200 entries, cap=10, asserts returned Vec length is exactly 10.
- AC-06c test scenario in pseudocode: "seed 50 active entries, config.max_graph_inference_per_tick = 5, assert get_embedding called <= 5 times."

---

### Check 8: AC-18† — Test plan includes grep gate for 52 InferenceConfig struct literals

**Status**: PASS

**Evidence**:
- inference-config.md pseudocode: includes the C-11 pre-merge check bash command with "Current count: 52 occurrences."
- Test plan inference-config.md: dedicated "Pre-Merge Grep Gate (R-07 / AC-18†)" section with bash command and expected behavior.
- Test plan OVERVIEW.md mandatory pre-merge grep gates: `grep -rn 'InferenceConfig {' crates/unimatrix-server/src/` listed.
- Assertions summary (test plan inference-config.md): "R-07/AC-18† | Grep gate (shell) | 52+ occurrences updated, `cargo check` passes."

---

### Check 9: Wave Ordering

**Status**: PASS

**Evidence** (pseudocode/OVERVIEW.md Sequencing Constraints):
1. **Wave 1 (no deps)**: `pub mod nli_detection_tick;` in mod.rs + `pub(crate)` promotions in nli_detection.rs (must be done first to expose symbols). `InferenceConfig` fields (inference-config component) can be added in any wave.
2. **Wave 2**: `nli_detection_tick.rs` (depends on pub(crate) promotions being in place; compilation will fail otherwise — desired catch).
3. **Wave 3**: `background.rs` call site (depends on tick module compiling).

Store-query-helpers has no dependencies on other components; can be Wave 1.

The wave structure correctly places the high-risk compile-catch (pub(crate) promotions) as the first-wave gate, ensuring the earliest possible failure signal for R-11.

---

### Check 10: Knowledge Stewardship Compliance

**Status**: PASS

**Evidence**:
- `agents/crt-029-agent-1-architect-report.md` (architect — active-storage agent): `## Knowledge Stewardship` section present (lines 67–78) with:
  - `Queried:` entries: `context_search(query: 'NLI tick graph inference', category: 'pattern')` → found #3653, #3655; `context_search(query: 'InferenceConfig validation patterns')` → found #2730; `context_briefing` for duties/conventions.
  - `Stored:` entries: ADR-001 → #3656, ADR-002 → #3657, ADR-003 → #3658, ADR-004 → #3659 — all four ADRs explicitly listed with Unimatrix entry IDs.
- `agents/crt-029-agent-3-risk-report.md` (risk-strategist): `## Knowledge Stewardship` section present with `Queried:` and `Stored:` entries. PASS.
- `agents/crt-029-agent-1-pseudocode-report.md` (pseudocode agent — read-only): `## Knowledge Stewardship` section present with `Queried:` entries and explicit "Deviations from established patterns: none" justification. PASS.
- `agents/crt-029-agent-2-testplan-report.md` (test plan agent — read-only): `## Knowledge Stewardship` section present with `Queried:` and `Stored:` entries (entry #3660 stored). PASS.

All four agent reports satisfy stewardship requirements. The rework has resolved the gate-blocking defect from the previous run.

---

### Check 11: Architect Report Stale Note on C-13 (WARN)

**Status**: WARN

**Evidence** (`agents/crt-029-agent-1-architect-report.md`, Key Decisions point 5):
> "5. **Contradiction threshold floor (SR-01)** — `write_inferred_edges_with_cap` takes `contradiction_threshold` as an explicit parameter; always passed as `config.nli_contradiction_threshold`. Never lower than the post-store path."

This note describes the pre-C-13 design (where an explicit `contradiction_threshold` parameter was passed). The final architecture (ARCHITECTURE.md) supersedes this with C-13: `write_inferred_edges_with_cap` has NO `contradiction_threshold` parameter and writes NO Contradicts edges. The rework that added the Knowledge Stewardship section did not correct this note. However:

- ARCHITECTURE.md is the authoritative design document and correctly reflects C-13.
- All pseudocode correctly implements C-13 (no contradiction_threshold, no Contradicts write path).
- The stale note in the report is inconsistent with the architecture document the same agent produced.

**Severity**: WARN only — not gate-blocking. The architecture document is correct and authoritative; delivery agents read ARCHITECTURE.md, not the agent report's Key Decisions list. The note may cause confusion during Gate 3b code review but does not block design phase.

---

## Knowledge Stewardship

- Queried: existing Unimatrix entries via context_search — entries #3655, #3656–#3659, #2730, #2800, #3591, #3631, #3653 referenced across agent reports and confirmed present. No new queries required; all relevant entries visible from previous gate run and rework artifacts.
- Stored: nothing novel to store — the architect-report-missing-stewardship pattern is already a known validation finding type; the retry itself resolved the issue cleanly with no new cross-feature systemic pattern emerging.
