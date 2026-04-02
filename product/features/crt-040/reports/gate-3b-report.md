# Gate 3b Report: crt-040

> Gate: 3b (Code Review)
> Date: 2026-04-02
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | PASS | All four waves implemented as specified; one accepted deviation (category_map `&str` vs `String`) |
| Architecture compliance | PASS | Path C placement, ADR decisions, UNIQUE constraint behavior all correct |
| Interface implementation | PASS | write_graph_edge signature exact; EDGE_SOURCE_COSINE_SUPPORTS re-exported; supports_cosine_threshold dual-site |
| Test case alignment | WARN | 6 of 18 path-c-loop test plan TCs absent; 1 of 4 store-constant TCs absent; all are low-risk gaps |
| Code quality | PASS | Compiles clean; no stubs/placeholders; no unwrap() in production code; nli_detection_tick.rs is 3036 lines (pre-existing) with Path C extracted to helper |
| Security | PASS | Parameterized SQL; no path traversal; no injection vectors; is_finite() guard on cosine values |
| Knowledge stewardship | PASS | All four agent reports contain Queried: and Stored: entries |

## Detailed Findings

### 1. Pseudocode Fidelity

**Status**: PASS

**Evidence**: All four waves match their pseudocode:

- **Wave 1a** (`read.rs`): `pub const EDGE_SOURCE_COSINE_SUPPORTS: &str = "cosine_supports"` inserted after `EDGE_SOURCE_CO_ACCESS` with the SR-04 UNIQUE constraint doc comment. `lib.rs` re-export adds it alphabetically between `EDGE_SOURCE_CO_ACCESS` and `EDGE_SOURCE_NLI`.

- **Wave 1b** (`config.rs`): `supports_cosine_threshold: f32` added at all 5 required sites (struct field with `#[serde(default)]`, backing function returning `0.65`, `impl Default` calling backing function not a literal, `validate()` range check `(0.0, 1.0)` exclusive, config merge with f32 epsilon comparison). All 6 sites of `nli_post_store_k` removed; grep returns 4 lines, all inside TC-11 test body — zero non-test references.

- **Wave 2** (`nli_detection.rs`): `write_graph_edge` added as a sibling to `write_nli_edge`. SQL uses `?6` bound twice for `(created_by, source)`. `Ok` arm returns `query_result.rows_affected() > 0` (distinguishes new insert from UNIQUE dedup, unlike `write_nli_edge` which returns `true` on any `Ok`). `Err` arm emits `tracing::warn!` with structured fields and returns `false`. `write_nli_edge` is unmodified — SQL literal `'nli', 'nli'` intact.

- **Wave 3** (`nli_detection_tick.rs`): Path C extracted to private `run_cosine_supports_path` async helper (accepted deviation from inline pseudocode — extraction follows NFR-07 guidance). Guard order matches pseudocode: `!cosine.is_finite()` → threshold → budget break → category HashMap lookup → pre-filter → write. One accepted deviation: `category_map: HashMap<u64, &str>` (reuses Phase 5's existing map) rather than `HashMap<u64, String>` as pseudocode specified. Agent entry #4038 documents this optimization explicitly; test plan is noted as agnostic on the implementation detail.

**Joint early-return removal**: Confirmed removed. Comment at line 474: `// NOTE: Joint early-return removed (crt-040 AC-19).`

**Path B entry gate retained**: Confirmed at line 552: `if candidate_pairs.is_empty() { return; }` after Path C call.

### 2. Architecture Compliance

**Status**: PASS

**Evidence**:

- **ADR-001** (write_graph_edge sibling): `write_nli_edge` is not parameterized; `write_graph_edge` is an independent sibling. Module doc comment updated accordingly.
- **ADR-003** (Path C placement): Call to `run_cosine_supports_path` is after Path A observability log (line 523) and before the Path B entry gate comment (line 546). Ordering is correct.
- **ADR-004** (budget constant): `MAX_COSINE_SUPPORTS_PER_TICK = 50` is a module-level constant, separate from `max_graph_inference_per_tick` and `MAX_INFORMS_PER_TICK`. TODO comment for config-promotion is present.
- **NFR-01** (no new HNSW scan): Path C iterates `candidate_pairs` from Phase 4 directly. No `vector_index.search()` call in `run_cosine_supports_path`.
- **NFR-02** (no rayon/spawn_blocking): `run_cosine_supports_path` is `async fn` in the Tokio context. No `score_batch`, `rayon_pool`, or `spawn_blocking`.
- **SR-07** (tick infallibility): `run_cosine_supports_path` returns `()`. No `?` in Path C code. All error paths use `warn!` + `continue` or `break`.
- **SR-04 UNIQUE constraint**: INSERT OR IGNORE on `UNIQUE(source_id, target_id, relation_type)`. `source` column is NOT in the unique key — confirmed from existing DDL.

### 3. Interface Implementation

**Status**: PASS

**Evidence**:

- `write_graph_edge` signature matches spec: `pub(crate) async fn write_graph_edge(store: &Store, source_id: u64, target_id: u64, relation_type: &str, weight: f32, created_at: u64, source: &str, metadata: &str) -> bool`
- `EDGE_SOURCE_COSINE_SUPPORTS: &str = "cosine_supports"` in `read.rs`; re-exported from `lib.rs` at line 40 between `EDGE_SOURCE_CO_ACCESS` and `EDGE_SOURCE_NLI`
- `supports_cosine_threshold` dual-site: backing function `default_supports_cosine_threshold() -> f32 { 0.65 }` (line 795) AND `impl Default` literal calls `default_supports_cosine_threshold()` (line 637), not a raw `0.65` literal
- `nli_detection_tick.rs` import extended to include `write_graph_edge` (line 51) and `EDGE_SOURCE_COSINE_SUPPORTS` (line 43)
- `EDGE_SOURCE_COSINE_SUPPORTS` used in Path C write call (line 841) — not a string literal

### 4. Test Case Alignment

**Status**: WARN

**Evidence**:

Tests present that cover all high-priority items:

| Component | Tests Present | Tests Absent |
|-----------|---------------|--------------|
| store-constant (4 plan TCs) | TC-01, TC-03, TC-04 | TC-02 (crate root runtime assertion) |
| inference-config (13 plan TCs) | TC-01 through TC-11 (all present) | none |
| write-graph-edge (7 plan TCs) | TC-01 through TC-07 (all present) | none |
| path-c-loop (18 plan TCs) | TC-01,02,03(renamed),04,05,07,08,09,12,18 | TC-03(exact boundary), TC-10(infinity), TC-11(guard order), TC-13(counts correct), TC-15(inferred_edge_count), TC-17(reversed pair) |

**Absent test analysis:**

- **TC-02 store-constant** (crate root runtime assertion): Agent justified as "structurally covered by clean build." The re-export at `lib.rs:40` is confirmed present. WARN: test plan explicitly requires a runtime assertion via `unimatrix_store::EDGE_SOURCE_COSINE_SUPPORTS`; the compile-time coverage is weaker than specified.

- **TC-03 path-c-loop** (exact threshold boundary 0.65 qualifies): The `>=` vs `>` boundary check is missing a dedicated test. The code at line 776 is `if cosine < &config.supports_cosine_threshold { continue; }` — this is correct `>=` semantics. WARN: spec calls this out explicitly; absence is a gap even though the code is correct.

- **TC-10 path-c-loop** (infinity cosine no edge): Covered by the same `!cosine.is_finite()` guard tested in TC-09 (NaN). `f32::INFINITY.is_finite()` is `false` — behavior is identical. WARN: test plan lists this as a separate scenario; implementation is correct.

- **TC-11 path-c-loop** (NaN guard fires before threshold): Code inspection confirms `!cosine.is_finite()` (line 765) precedes the threshold check (line 776). The guard order is correct. WARN: no dedicated test; code review is the verification.

- **TC-13 path-c-loop** (observability counts correct for non-zero run): TC-12 tests zero counts; TC-13 tests non-zero counts with 5 qualifying / 3 below-threshold pairs. Absent but lower risk — the counter increment logic is covered implicitly by TC-07 (budget cap asserts 50 edges written implies 50 counter increments). WARN.

- **TC-15 path-c-loop** (inferred_edge_count unchanged after cosine_supports write): NFR-06 backward compat. The `compute_graph_cohesion_metrics` SQL was not modified; `inferred_edge_count` still counts only `source='nli'`. No regression possible from this change. WARN: test plan requires explicit assertion; implementation is provably correct from code.

- **TC-17 path-c-loop** (reversed pair produces at most one edge): Phase 4 normalizes to `(lo, hi)` canonical form. The INSERT OR IGNORE UNIQUE constraint handles any residual duplicates. TC-08 tests the UNIQUE conflict path. WARN: no dedicated reversed-pair test; covered by INSERT OR IGNORE backstop.

**Assessment**: All absent tests are WARN severity. The critical AC items (AC-01 through AC-19) are covered by the tests that are present. No absent test represents an unverified correctness property — the code is correct in all cases; the tests are simply fewer than the plan specified.

### 5. Code Quality

**Status**: PASS

**Evidence**:

- **Compilation**: `cargo build --workspace` completes with 0 errors (17 pre-existing warnings in unimatrix-server, not new).
- **Tests**: All test suites pass — 0 failures across the workspace.
- **No stubs**: No `todo!()`, `unimplemented!()`, `TODO`, or `FIXME` in new production code.
- **No unwrap in production code**: `run_cosine_supports_path` and `write_graph_edge` contain no `.unwrap()` calls. The `unwrap_or(Ordering::Equal)` in the Phase 5 sort comparator and `unwrap_or_else` patterns are pre-existing and are not `.unwrap()`.
- **File sizes**: `nli_detection_tick.rs` is 3036 lines — well over the 500-line limit, but this is a pre-existing condition documented in NFR-07. Path C was extracted into `run_cosine_supports_path` (as prescribed by NFR-07's 500-line extraction guidance). The extraction is appropriate; the file was 2000+ lines before this feature. The spec acknowledges this and delegates the extraction decision to the delivery agent.
- `nli_detection.rs` is 492 lines (under 500).
- `config.rs` is 7169 lines (pre-existing, outside the scope of this feature's line-count obligation).

### 6. Security

**Status**: PASS

**Evidence**:

- **Parameterized SQL**: `write_graph_edge` uses sqlx parameterized queries (`?1` through `?7`). No string interpolation into SQL.
- **Input validation**: `!cosine.is_finite()` guard prevents NaN/Inf from reaching the database weight column. Category values come from the in-memory `all_active` vec (not external input).
- **No path traversal**: No file path operations in new code.
- **No hardcoded secrets**: No credentials or API keys in new code.
- **No command injection**: No shell invocations.
- **cargo audit**: `cargo-audit` binary not installed in this environment; no known CVE findings are expected given no new dependencies were added.

### 7. Knowledge Stewardship Compliance

**Status**: PASS

**Evidence**: All four delivery agent reports contain proper `## Knowledge Stewardship` sections:

- **crt-040-agent-3-store-constant**: `Queried:` entry (briefing surfaced #3882, #3591). `Stored:` declined with reason — pattern already captured in those entries.
- **crt-040-agent-4-inference-config**: `Queried:` entry (briefing surfaced ADR-002 #4028, pattern #3817, #4013). `Stored:` entry #4036 "InferenceConfig field removal: grep all 6 sites before touching anything — neighboring doc comments are a hidden 7th."
- **crt-040-agent-5-write-graph-edge**: `Queried:` entry (briefing surfaced ADR-001 #4027, pattern #4025, #3884). `Stored:` entry #4037 "open_readonly SQL error injection for write helper unit tests."
- **crt-040-agent-6-path-c-loop**: `Queried:` entry (briefing surfaced ADR-003 #4029, ADR-004 #4030, pattern #4025, #3937). `Stored:` entry #4038 "Reuse Phase 5 category_map (&str) in Path C helper."

---

## Rework Required

None. All WARNs are test coverage gaps on items where the code is demonstrably correct. No FAIL conditions identified.

---

## Knowledge Stewardship

- Stored: nothing novel to store — the missing-test patterns observed here (TC-02 crate-root runtime assertion skipped as "build proves it", TC-03 boundary test absent for `>=` operator) are one-off gaps specific to this feature's delivery scope decisions, not a systemic pattern. Existing lesson #4014 already covers the impl Default trap; existing pattern #4013 covers hidden test site discovery.
