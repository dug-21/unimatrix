# Gate 3b Report: crt-029

> Gate: 3b (Code Review)
> Date: 2026-03-27
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | PASS | All phases implemented correctly; sync VectorIndex use is a documented, approved deviation |
| Architecture compliance | PASS | Component boundaries, ADRs, and integration points all satisfied |
| Interface implementation | PASS | All signatures match; pub(crate) promotions present |
| Test case alignment | PASS | All required test scenarios present; background.rs integration tests deferred to 3c as planned |
| Code quality | PASS | Builds clean; no stubs/todo/unwrap in non-test code; nli_detection_tick.rs is 773 lines (≤ 800) |
| Security | PASS | No hardcoded secrets; no path traversal; no command injection; no panicking deserialization |
| Knowledge stewardship | PASS | All four delivery agent reports have proper Queried/Stored blocks |
| C-13 (no Contradicts) | PASS | grep returns only comments/docs — zero live Contradicts writes |
| C-14/R-09 (rayon/tokio boundary) | PASS | Rayon closure body is sync-only; .await is outside the closure on the rayon_pool.spawn() future |
| AC-06c (cap before embedding) | PASS | select_source_candidates caps in Phase 3; get_embedding called only in Phase 4 on capped list |
| C-01 (no spawn_blocking) | PASS | grep returns only module doc comment — zero spawn_blocking calls |
| C-08 (file ≤ 800 lines) | PASS | 773 lines |
| cargo build | PASS | Finished with 0 errors |
| cargo clippy (new files) | PASS | No warnings from crt-029 files; pre-existing warnings are in unimatrix-engine/unimatrix-observe |
| cargo audit | WARN | cargo-audit not installed in environment; cannot verify |

---

## Detailed Findings

### Check 1: Pseudocode Fidelity

**Status**: PASS

**Evidence**:

All eight phases of `run_graph_inference_tick` are implemented in the order specified by the pseudocode:
- Phase 1 (guard): `nli_handle.get_provider().await` → silent return on Err. Matches pseudocode exactly.
- Phase 2 (data fetch): `query_by_status(Active)`, `query_entries_without_edges()`, `query_existing_supports_pairs()`. Degraded-mode fallbacks (empty sets) match pseudocode error handling table.
- Phase 3 (source candidate cap): `select_source_candidates()` called before any embedding lookup. Cap invariant enforced.
- Phase 4 (HNSW expansion): Calls `vector_index.get_embedding()` and `vector_index.search()` synchronously. **Notable deviation**: the pseudocode specified async wrapper calls (`.await`); the implementation calls the synchronous `VectorIndex` methods directly. The delivery agent documented this in their report (entry #3663): `VectorIndex.search` and `get_embedding` are sync lock-guarded methods on the concrete `unimatrix_vector::VectorIndex` type (not the `AsyncVectorIndex` async wrappers). The comment at line 106 explicitly documents this: "VectorIndex::search and get_embedding are synchronous (internal RwLock, no Tokio I/O)." This is correct and safe — calling sync methods from the tokio thread does not block the executor for the short O(N) scan, and avoids spurious `spawn_blocking` overhead. The architecture comment at ARCHITECTURE.md §Integration Points cites the async wrappers but the actual `VectorIndex` type in scope is the sync variant. PASS.
- Phase 5 (priority sort): Three-tier sort implemented with `b_cross.cmp(&a_cross)` → `b_iso.cmp(&a_iso)` → `b_sim.partial_cmp(a_sim)`. Matches pseudocode comparator exactly.
- Phase 6 (text fetch): `get_content_via_write_pool()` per pair; skip-on-error with debug log. Matches pseudocode.
- Phase 7 (rayon dispatch): Single `rayon_pool.spawn(move || { ... }).await` with owned data. Matches pseudocode W1-2 pattern exactly.
- Phase 8 (write): `write_inferred_edges_with_cap()` called with `supports_edge_threshold` (not `nli_entailment_threshold`). C-06 respected.

`select_source_candidates` and `write_inferred_edges_with_cap` match their pseudocode signatures and body logic.

The `_existing_edge_set` parameter of `select_source_candidates` is received but not consumed (prefixed `_`). The pseudocode explicitly noted this as acceptable: "If implementation judges the cross-category computation too expensive over the full edge set, the parameter may be ignored." Cross-category priority is applied in Phase 5. PASS.

### Check 2: Architecture Compliance

**Status**: PASS

**Evidence**:

- **ADR-001** (new module `nli_detection_tick.rs`): Module exists at `crates/unimatrix-server/src/services/nli_detection_tick.rs`. Declared as `pub(crate) mod nli_detection_tick;` in `services/mod.rs`.
- **ADR-002** (`write_inferred_edges_with_cap` as named variant): Function exists as a standalone `async fn` with no `contradiction_threshold` parameter. Does not reuse `write_edges_with_cap`.
- **ADR-003** (source-candidate bound derived from `max_graph_inference_per_tick`): `select_source_candidates` takes `max_sources: usize` which is passed `config.max_graph_inference_per_tick`. No separate config field added.
- **ADR-004** (`query_existing_supports_pairs` as separate helper): Method exists in `read.rs` returning `HashSet<(u64, u64)>` with normalized pairs.
- **Component boundaries**: All new code lives in the correct files per architecture.
- **SR-07 struct literal trap**: `InferenceConfig::default()` is a struct literal that explicitly includes all four new fields. All test usages in the codebase use `..InferenceConfig::default()` tail syntax. Build succeeds, confirming no bare struct literal misses.
- **File placement**: The new module split is in `services/nli_detection_tick.rs` as required (not merged into `nli_detection.rs`).

### Check 3: Interface Implementation

**Status**: PASS

**Evidence**:

| Interface | Pseudocode Signature | Implemented Signature | Match |
|-----------|---------------------|----------------------|-------|
| `run_graph_inference_tick` | `pub async fn(store, nli_handle, vector_index, rayon_pool, config)` | Matches exactly | PASS |
| `select_source_candidates` | `fn(all_active: &[EntryRecord], existing_edge_set: &HashSet<(u64,u64)>, isolated_ids: &HashSet<u64>, max_sources: usize) -> Vec<u64>` | Matches exactly | PASS |
| `write_inferred_edges_with_cap` | `async fn(store, pairs, nli_scores, supports_threshold: f32, max_edges: usize) -> usize` | Matches exactly; no `contradiction_threshold` | PASS |
| `query_entries_without_edges` | `async fn(&self) -> Result<Vec<u64>>` | Matches; uses `read_pool()` | PASS |
| `query_existing_supports_pairs` | `async fn(&self) -> Result<HashSet<(u64, u64)>>` | Matches; uses `read_pool()`; normalises to `(min, max)` | PASS |
| `write_nli_edge` | `pub(crate) async fn(...)` | Promoted to `pub(crate)` | PASS |
| `format_nli_metadata` | `pub(crate) fn(...)` | Promoted to `pub(crate)` | PASS |
| `current_timestamp_secs` | `pub(crate) fn(...)` | Promoted to `pub(crate)` | PASS |

Four new `InferenceConfig` fields with correct types, defaults, serde attributes, `Default` impl entries, `validate()` guards, and merge function entries — all verified.

The `validate()` chose Option B (new `GraphInferenceThresholdInvariantViolated` error variant) for the cross-field invariant, with fields named `candidate` and `edge`. This is cleaner than Option A and compiles correctly.

### Check 4: Test Case Alignment

**Status**: PASS

**Evidence**:

**`nli_detection_tick.rs` tests** (17 tests):
- `select_source_candidates`: 8 tests covering cap enforcement, cap larger than entries, empty input, max_sources=0, isolated priority, created_at ordering, combined priority, all-isolated edge case.
- `write_inferred_edges_with_cap`: 7 tests covering cap enforcement (AC-11), strict-threshold (AC-09), no-Contradicts (AC-10a/R-01), zero eligible, exact-cap-count, idempotency (AC-16), edge source/bootstrap_only (AC-13).
- `run_graph_inference_tick`: 1 test covering the NLI-not-ready no-op (AC-05).
- Edge cases: 3 tests (empty entry set, single entry, pair normalization).

**`config.rs` tests** (17+ new tests):
- AC-01/AC-17: defaults and TOML deserialization (3 tests).
- AC-02: cross-field invariant — equal, candidate>edge, candidate<edge (3 tests).
- AC-03: individual range boundaries (5 tests).
- AC-04: `max_graph_inference_per_tick` bounds (3 tests).
- AC-04b: `graph_inference_k` bounds (3 tests).

**`read.rs` store tests** (13 tests):
- `query_entries_without_edges`: 6 tests (empty, no edges, with edges, partial coverage, bootstrap-only ignored, deprecated excluded).
- `query_existing_supports_pairs`: 7 tests (empty, supports only, bootstrap excluded, excludes-contradicts, mixed bootstrap, normalisation).

**Background.rs unit tests**: No new unit tests for the call site. The test plan explicitly acknowledges this is acceptable; observable verification is through infra-001 integration tests (Stage 3c). The static grep gates (ordering, guard, module declaration) are verified by this gate review.

**Minor gap (WARN)**: `test_write_inferred_edges_edge_source_nli` (AC-13) checks `bootstrap_only = false` and `relation_type = "Supports"` but does not assert `source = 'nli'` despite the test name claiming it does. The `write_nli_edge` SQL hardcodes `source = 'nli'` so the behaviour is correct; the test assertion is incomplete. This is a test completeness WARN, not a functional defect.

### Check 5: Code Quality

**Status**: PASS

**Evidence**:

- `cargo build --workspace 2>&1 | tail -3`: `Finished 'dev' profile [unoptimized + debuginfo] target(s) in 0.19s` — zero errors.
- No `todo!()`, `unimplemented!()`, `TODO`, or `FIXME` in any of the six new/modified files (grep confirmed).
- No `.unwrap()` in non-test production code. The single `unwrap_or(Ordering::Equal)` at line 185 is a safe fallback, not a panic risk.
- `nli_detection_tick.rs`: 773 lines (C-08 limit is 800). PASS.
- `read.rs`: 2407 lines — pre-existing condition explicitly excluded from crt-029 scope (architecture doc: "No split of the existing file is in scope for crt-029").
- Clippy on `unimatrix-server` and `unimatrix-store`: the failing clippy checks (`collapsible_if`, `manual_pattern_char_comparison`, etc.) are all in pre-existing code (`unimatrix-engine`, `unimatrix-observe`, `unimatrix-server` pre-existing paths). Zero clippy diagnostics point to any crt-029-introduced file (`nli_detection_tick.rs`, the new store methods, or the config/background additions).

### Check 6: Security

**Status**: PASS

**Evidence**:

- No hardcoded secrets or API keys. NLI model path resolution is via config; no credentials embedded.
- No path traversal: no file path operations in the new code.
- No command injection: no shell invocations.
- Input validation: `InferenceConfig::validate()` validates all four new fields with range and cross-field checks before any tick runs. `query_entries_without_edges` and `query_existing_supports_pairs` use parameterized `sqlx::query` (not string concatenation). `write_nli_edge` uses bound parameters (`?1..?6`) — no SQL injection risk.
- Serialization safety: `format_nli_metadata` uses `serde_json::to_string` per the function comment ("Uses `serde_json::to_string` to prevent SQL injection via string concatenation"). Malformed NLI scores produce a valid JSON fallback.
- No panicking paths in non-test production code (only `debug_assert_eq!` in `write_inferred_edges_with_cap` which is a no-op in release builds).

### Check 7: Critical Check C-13 (No Contradicts)

**Status**: PASS

**Evidence**:

```
grep -n 'Contradicts' nli_detection_tick.rs
```

All five matches are in comments and doc strings:
- Line 13: module doc comment
- Line 44: function doc comment
- Line 587: test name comment
- Line 608: `assert_ne!` argument string in test (asserting there are NO Contradicts edges)
- Line 609: test assertion message

Zero live code references to "Contradicts" as a write target. `write_inferred_edges_with_cap` only ever passes `"Supports"` as the `relation_type` argument to `write_nli_edge`.

### Check 8: Critical Check C-14/R-09 (Rayon/Tokio Boundary — Independent Review)

**Status**: PASS

**Evidence** (independent reviewer, not the author of the closure):

The `rayon_pool.spawn(move || { ... }).await` block at lines 233–242:

```rust
let nli_result = rayon_pool
    .spawn(move || {
        // SYNC-ONLY CLOSURE — no .await, no Handle::current()
        let pairs_ref: Vec<(&str, &str)> = nli_pairs
            .iter()
            .map(|(q, p)| (q.as_str(), p.as_str()))
            .collect();
        provider_clone.score_batch(&pairs_ref)
    })
    .await;
```

The closure body (lines 235–240) contains:
1. `nli_pairs.iter().map(...).collect()` — pure synchronous iterator operations on owned `Vec<(String, String)>` data that was cloned before the spawn.
2. `provider_clone.score_batch(&pairs_ref)` — synchronous method call on `Arc<dyn CrossEncoderProvider>`. `score_batch` is a sync method (takes `&[(...)...]`, returns `Result<Vec<NliScores>>`).

The `.await` on line 242 is **outside** the closure, on the `Future` returned by `rayon_pool.spawn(...)`. This is the tokio-thread await on the rayon completion channel, not an await inside the rayon thread.

`grep -n 'Handle::current'` returns only two comment lines (8 and 17 in the module header, and 235 in a comment). Zero live code references.

**C-14 PASS. The rayon closure is synchronous CPU-bound only.**

### Check 9: Critical Check AC-06c (Cap Before Embedding)

**Status**: PASS

**Evidence**:

Phase 3 (lines 93–103) calls `select_source_candidates()` which operates entirely on `&[EntryRecord]` metadata (id, category, created_at fields). No embedding access occurs.

Phase 4 (lines 108–163) begins `for source_id in &source_candidates` — iterating the already-capped list from Phase 3. `vector_index.get_embedding(*source_id)` is called only inside this loop.

The invariant is enforced by code structure: `source_candidates` is produced by Phase 3 and consumed by Phase 4 with no opportunity for embedding calls to precede the cap. The comment at line 92 documents: "Invariant: source_candidates.len() <= config.max_graph_inference_per_tick (ADR-003)."

**AC-06c PASS.**

### Check 10: Critical Check C-01 (No spawn_blocking)

**Status**: PASS

```
grep -n 'spawn_blocking' nli_detection_tick.rs
```

Single match: line 8 in module doc comment. Zero live code references to `spawn_blocking`.

### Check 11: cargo audit

**Status**: WARN

`cargo-audit` is not installed in the build environment. CVE check could not be performed. This is an environment limitation, not a code defect. Pre-existing dependencies are unchanged by crt-029 (NFR-03 — no new crate dependencies). The risk of an undetected CVE from new dependencies is zero since no new dependencies were added.

---

## Rework Required

None. All checks pass.

---

## Warnings (Non-Blocking)

| Warning | Location | Detail |
|---------|----------|--------|
| WARN-01: AC-13 test assertion incomplete | `nli_detection_tick.rs:675–689` | `test_write_inferred_edges_edge_source_nli` asserts `bootstrap_only = false` but does not assert `source = 'nli'` despite the test name claiming it does. Behaviour is correct (SQL hardcodes `'nli'`); the test assertion is incomplete. |
| WARN-02: `cargo audit` not installed | Build environment | CVE scan not possible. No new dependencies added, so risk is minimal. |
| WARN-03: `read.rs` 2407 lines | `unimatrix-store/src/read.rs` | Pre-existing; explicitly excluded from crt-029 scope in architecture doc. Not introduced by this feature. |
| WARN-04: No background.rs unit tests for AC-14 | `background.rs` | Deferred to infra-001 integration suite (Stage 3c) per test plan. Acceptable; test plan documents this explicitly. |

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_search` for gate-3b validation patterns and rayon/tokio boundary patterns — returned entries confirming the W1-2 constraint (entry #3653), VectorIndex sync/async distinction (entry #3663), and C-14/R-09 independent reviewer requirement. All findings incorporated.
- Stored: nothing novel to store — no new systemic gate patterns found; this gate ran cleanly on the first iteration with no rework failures.
