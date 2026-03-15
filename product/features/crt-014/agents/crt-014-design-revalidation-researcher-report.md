# crt-014 Design Revalidation Report
## Agent: crt-014-design-revalidation-researcher

**Date**: 2026-03-15
**Scope**: Post-crt-018b design artifact validation — verifying that codebase references in crt-014 design documents still match the current state of the worktree at `/workspaces/unimatrix-crt-014` (branch `feature/crt-014`).

---

## Summary

crt-018b (Effectiveness-Driven Retrieval) significantly reorganized `search.rs` by adding a pre-search effectiveness snapshot block (~40 lines) before Step 0, introducing `EffectivenessStateHandle` and `EffectivenessSnapshot` fields to `SearchService`, and inserting `utility_delta()` into both sort closures. This caused all line-number references in SCOPE.md to become stale. The functional logic (penalty_map construction, single-hop comment, penalty application) is still present and working, just at different line numbers. The import path for `DEPRECATED_PENALTY`/`SUPERSEDED_PENALTY` also changed — they are no longer imported from `unimatrix_engine::confidence` but from `crate::confidence` (a re-export). All structural design decisions (what to build, where, how) remain valid. `thiserror` is not yet present in `unimatrix-engine/Cargo.toml` and must be added.

**Overall Verdict: Design is valid — implementation approach is sound. Line-number references in SCOPE.md Background Research section are stale but non-blocking. One dependency gap (`thiserror`) identified.**

---

## Check 1: `confidence.rs` — DEPRECATED_PENALTY and SUPERSEDED_PENALTY

**Claim (SCOPE.md lines 58–63):**
> `DEPRECATED_PENALTY = 0.7` and `SUPERSEDED_PENALTY = 0.5` at `confidence.rs:52–57`, imported in `search.rs:15`.

**Finding: VALID constants, STALE line numbers and import path.**

- `DEPRECATED_PENALTY: f64 = 0.7` exists at **line 60** of `unimatrix-engine/src/confidence.rs` (not lines 52–57).
- `SUPERSEDED_PENALTY: f64 = 0.5` exists at **line 65** (not lines 52–57).
- The actual lines 52–57 contain `COLD_START_ALPHA`, `COLD_START_BETA`, and `PROVENANCE_BOOST` (added by crt-019 and col-010b between design artifact creation and now).
- Import in `search.rs` is **`use crate::confidence::{DEPRECATED_PENALTY, SUPERSEDED_PENALTY, ...}`** at **line 18** (not line 15). This imports via `crate::confidence` which is a re-export of `unimatrix_engine::confidence` declared in `unimatrix-server/src/lib.rs:18`.
- The constants' values (0.7 and 0.5) and their roles are exactly as described.
- The four associated tests (`deprecated_penalty_value`, `superseded_penalty_value`, `superseded_penalty_harsher_than_deprecated`, `penalties_independent_of_confidence_formula`) still exist at **lines 891–920** of `confidence.rs` (design artifacts reference "lines 720–752"). The test names are correct; only the line numbers are stale.

**Impact:** None on implementation. The import path is `crate::confidence::{DEPRECATED_PENALTY, SUPERSEDED_PENALTY}` (not `unimatrix_engine::confidence::`) — this matters when writing the removal diff. The ARCHITECTURE.md Component 4 step 1 says to remove from `use crate::confidence::{..., DEPRECATED_PENALTY, SUPERSEDED_PENALTY, ...}` which is already correct.

---

## Check 2: `search.rs` structure post-crt-018b

### 2a. What crt-018b added

crt-018b inserted:
1. A new `SearchService` field: `effectiveness_state: EffectivenessStateHandle` and `cached_snapshot: Arc<Mutex<EffectivenessSnapshot>>` (lines 93–99).
2. A new free function `utility_delta(category: Option<EffectivenessCategory>) -> f64` (lines 109–117).
3. A new import block (lines 13–15): `use unimatrix_engine::effectiveness::{EffectivenessCategory, SETTLED_BOOST, UTILITY_BOOST, UTILITY_PENALTY}` and `use crate::services::effectiveness::{EffectivenessSnapshot, EffectivenessStateHandle}`.
4. A ~30-line effectiveness snapshot block at the very top of `search()` (lines 163–194), executed before Step 0, that clones `categories: HashMap<u64, EffectivenessCategory>`.
5. `utility_delta` calls inside both sort closures (Step 7 at lines 359–368, Step 8 co-access re-sort at lines 430–435).
6. `utility_delta` in the final `ScoredEntry` construction (Step 11, line 460–463).
7. A large new test block (lines 681–1044) covering `utility_delta` and the snapshot locking behavior.

### 2b. penalty_map construction location

**Claim (SCOPE.md line 65–66):** "penalty_map built at `search.rs:190–211`"

**Finding: STALE line numbers.**

The `penalty_map` construction is now at:
- Declaration: `let mut penalty_map: HashMap<u64, f64> = HashMap::new();` — **line 274**
- `SUPERSEDED_PENALTY` insertion — **line 288**
- `DEPRECATED_PENALTY` insertion — **line 290**
- The block spans approximately lines 264–296.

The logic is structurally identical to what the design documents describe — the crt-014 implementation plan targets this exact block for replacement.

### 2c. Single-hop successor injection comment

**Claim (SCOPE.md line 67):** "Single-hop only (ADR-003, AC-06)" comment at `search.rs:251`

**Finding: STALE line number, comment still present.**

The comment `// Single-hop only (ADR-003, AC-06)` exists at **line 332** (not line 251). The surrounding logic (check `successor.status != Status::Active` and `successor.superseded_by.is_some()` before injecting) is exactly as described in the design. The full injection block spans approximately lines 298–343.

### 2d. Penalty application call sites (rerank_score calls)

**Claim (SCOPE.md line 68):** "Penalty applied multiplicatively during sort at lines ~278, ~343, ~369"

**Finding: STALE line numbers — there are now more call sites than originally documented.**

The penalty is applied at:
- Step 7 sort closure (lines 348–372): `final_a = base_a * penalty_a` at line 367, `final_b = base_b * penalty_b` at line 368.
- Step 8 co-access re-sort (lines 415–439): penalty applied at lines 434–435.
- Step 11 final_score construction: line 463.

crt-018b added **two extra penalty application sites**: the co-access re-sort (Step 8) now applies penalty too (it did before, but now it also includes `utility_delta`), and the Step 11 final_score (newly explicit). The Step 8 sort formula is `(base + delta + boost + prov) * penalty` — this is relevant to crt-014's implementation because the pseudocode in IMPLEMENTATION-BRIEF.md only shows the Step 6a/6b/7 pattern. The Step 8 co-access re-sort also reads from `penalty_map` and must be compatible with crt-014's graph-derived penalties. Since `penalty_map` is the shared mutable structure, crt-014's replacement of its population logic will automatically affect Step 8 as well — no additional changes needed beyond what the design specifies.

### 2e. New imports at the top of search.rs relevant to crt-014

The existing import at line 18 that crt-014 must modify:
```rust
use crate::confidence::{DEPRECATED_PENALTY, SUPERSEDED_PENALTY, cosine_similarity, rerank_score};
```
After crt-014, `DEPRECATED_PENALTY` and `SUPERSEDED_PENALTY` are removed; `cosine_similarity` and `rerank_score` stay.

The new crt-014 import to add:
```rust
use unimatrix_engine::graph::{build_supersession_graph, graph_penalty, find_terminal_active, GraphError, FALLBACK_PENALTY};
```
This is consistent with what ARCHITECTURE.md Component 4 step 2 specifies.

### 2f. crt-018b `EffectivenessSnapshot` and lock ordering

**Finding: NEW CONTEXT — implementation must be aware of lock ordering constraints.**

crt-018b established a strict lock ordering protocol (documented in search.rs comments and in `services/effectiveness.rs`):
- `effectiveness_state` read lock must be acquired and **dropped** before `cached_snapshot` mutex is acquired.
- Comment: "LOCK ORDERING (R-01): acquire read lock, read generation, DROP guard, then acquire cached_snapshot mutex. Never hold both guards simultaneously."

crt-014 inserts a `Store::query(QueryFilter::default())` call via `spawn_blocking` before Step 6a. This is pure I/O (not a lock), so it does not interact with the effectiveness lock ordering. However, the implementation agent should be aware that the `spawn_blocking` for graph construction will execute after the effectiveness snapshot block (lines 163–194) has already completed and returned. No conflict.

---

## Check 3: `schema.rs` — supersedes/superseded_by fields

**Claim (SCOPE.md line 70–74):** "`supersedes: Option<u64>` and `superseded_by: Option<u64>` at `schema.rs:67–69`"

**Finding: VALID — correct values and correct lines.**

`unimatrix-store/src/schema.rs`:
- Line 67: `pub supersedes: Option<u64>,`
- Line 69: `pub superseded_by: Option<u64>,`

Both have `#[serde(default)]` decorators (lines 66 and 68 respectively). The type, optionality, and line numbers are exactly as documented. No changes from crt-018b.

---

## Check 4: `unimatrix-engine/Cargo.toml` — petgraph and thiserror

**Claim (SCOPE.md / IMPLEMENTATION-BRIEF.md):** petgraph not yet present; thiserror should be verified before adding.

**Finding: VALID for petgraph — NEW CONTEXT gap for thiserror.**

Current `unimatrix-engine/Cargo.toml` dependencies:
```toml
unimatrix-core, unimatrix-store, serde, serde_json, sha2, dirs, tracing, nix
```

- `petgraph`: **not present** — correct, crt-014 adds it.
- `thiserror`: **not present** in `unimatrix-engine/Cargo.toml`.

The IMPLEMENTATION-BRIEF.md Dependencies table notes: "verify present in engine Cargo.toml before adding." Since `thiserror` is absent, it **must be added** to `unimatrix-engine/Cargo.toml` as part of crt-014's Cargo.toml modification. The workspace Cargo.toml should be checked to see if `thiserror` is a workspace dependency (it is used in `unimatrix-store` and `unimatrix-server`). The implementation agent must add it; the design document correctly flagged this as a check item.

**Action required by implementation:** Add both `petgraph` and `thiserror` to `unimatrix-engine/Cargo.toml`.

---

## Check 5: `unimatrix-engine/src/lib.rs` — current public modules

**Claim (IMPLEMENTATION-BRIEF.md):** crt-014 adds `pub mod graph;` to `lib.rs`.

**Finding: VALID — no conflict, correct insertion point.**

Current `lib.rs` modules (lines 17–27):
```rust
pub mod auth;
pub mod coaccess;
pub mod confidence;
pub mod effectiveness;   // NEW — added by crt-018b
pub mod event_queue;
pub mod project;
pub mod transport;
pub mod wire;
```

crt-018b added `pub mod effectiveness;` to `lib.rs`. This is a new module that was not present when the crt-014 design was written. It does not conflict with crt-014's planned `pub mod graph;` addition. The insertion is straightforward — add `pub mod graph;` to the existing list.

---

## Check 6: Overall implementation approach validity

**Finding: VALID — crt-018b additions do not invalidate the implementation approach.**

The pseudocode pattern in IMPLEMENTATION-BRIEF.md (graph construction before Step 6a, replacement of penalty_map insertion, replacement of single-hop injection) maps cleanly onto the current `search.rs` structure. Specifically:

1. **Graph construction placement**: The design calls for inserting graph construction "before Step 6a." In the current file, Step 6a begins at line 264. The graph construction `spawn_blocking` call goes between the end of Step 5 (line 248) and the start of Step 6a (line 264). The effectiveness snapshot block (lines 163–194) runs earlier, before Step 0, and does not interfere.

2. **Penalty marking replacement**: The design's pseudocode exactly matches lines 276–295 of the current file. The `if use_fallback { FALLBACK_PENALTY } else { graph_penalty(...) }` substitution is straightforward.

3. **Successor injection replacement**: The design's multi-hop pseudocode maps onto the current lines 298–343 block. The `find_terminal_active` call replaces `entry.superseded_by` as the terminal ID source.

4. **utility_delta interaction**: crt-018b added `utility_delta` calls inside both sort closures and Step 11. These apply **additively before the penalty multiplication** — `(base + delta + prov) * penalty`. Since crt-014 only changes how `penalty_map` is populated (not how it's consumed), there is **zero interaction** between the utility_delta additions and crt-014's graph penalty changes. The sort formula structure remains compatible.

5. **"Files to Create / Modify" table in IMPLEMENTATION-BRIEF.md**: All five entries remain accurate:
   - `crates/unimatrix-engine/src/graph.rs` — CREATE: correct
   - `crates/unimatrix-engine/src/lib.rs` — MODIFY: correct (add `pub mod graph;` alongside new `pub mod effectiveness;`)
   - `crates/unimatrix-engine/Cargo.toml` — MODIFY: correct (add petgraph **and** thiserror)
   - `crates/unimatrix-engine/src/confidence.rs` — MODIFY: correct (constants still at lines 60, 65; 4 tests still present at lines 891–920)
   - `crates/unimatrix-server/src/services/search.rs` — MODIFY: correct

---

## Stale References (Complete List)

| Document | Claim | Actual |
|----------|-------|--------|
| SCOPE.md Background Research | `confidence.rs` lines 52–57 for penalty constants | Lines 60 and 65 |
| SCOPE.md Background Research | Imported in `search.rs:15` | Line 18 (`use crate::confidence::`) |
| SCOPE.md Background Research | `penalty_map` at `search.rs:190–211` | Lines 264–296 |
| SCOPE.md Background Research | Injection at `search.rs:219–259`; comment at line 251 | Lines 298–343; comment at line 332 |
| SCOPE.md Background Research | Penalty applied at lines ~278, ~343, ~369 | Lines 367–368 (Step 7), 434–435 (Step 8), 463 (Step 11) |
| SCOPE.md Background Research | Penalty tests at `confidence.rs:720–752` | Lines 891–920 |
| SCOPE.md Background Research | Search tests at `search.rs:450–571` | Lines 540–678 (original T-SP tests); crt-018b added additional tests lines 681–1044 |

---

## New Context Items (What Changed in crt-018b That Implementation Must Know)

1. **`effectiveness` module in `unimatrix-engine`**: `pub mod effectiveness;` now exists in `lib.rs`. The effectiveness module is at `crates/unimatrix-engine/src/effectiveness/mod.rs` (a subdirectory module). `pub mod graph;` insertion should go alphabetically or at end of the list — no conflict either way.

2. **`thiserror` is missing from `unimatrix-engine/Cargo.toml`**: Must be added. Check the workspace `Cargo.toml` to determine if it is a workspace-level dependency (likely yes, given other crates use it).

3. **`search.rs` import line 18 uses `crate::confidence::`** not `unimatrix_engine::confidence::`: The removal must target `crate::confidence::{DEPRECATED_PENALTY, SUPERSEDED_PENALTY, cosine_similarity, rerank_score}` and split it into: keep `cosine_similarity` and `rerank_score` in the existing `crate::confidence` import, and add the new `unimatrix_engine::graph::` import separately.

4. **Step 8 co-access re-sort applies penalty too**: `penalty_map` is read in the Step 8 sort closure (lines 432–433), not just Step 7. This was present before crt-018b but was not explicitly called out in the design's Step 8 description. crt-014's graph penalties will automatically apply to Step 8 re-sorting since they populate the same `penalty_map`. No additional change needed.

5. **Test block expansion in search.rs**: crt-018b added 363 lines of new tests (lines 681–1044) covering utility_delta and snapshot behavior. The 8 original T-SP-01..T-SP-08 tests still exist (lines 540–678) and still reference `DEPRECATED_PENALTY` and `SUPERSEDED_PENALTY`. The crt-014 implementation agent must update these 8 tests plus the 4 confidence.rs tests (lines 891–920). The crt-018b tests (lines 681–1044) do NOT reference penalty constants and do not need modification.

6. **`EffectivenessSnapshot::new_shared()` call in `SearchService::new()`**: The constructor now takes an additional `effectiveness_state: EffectivenessStateHandle` parameter (line 127) and internally creates `cached_snapshot: EffectivenessSnapshot::new_shared()` (line 139). crt-014 does not modify `SearchService::new()` so this is informational only.

---

## Overall Verdict

**Design is valid. Implementation approach is sound. No design artifacts need modification.**

The stale line numbers in SCOPE.md Background Research are informational citations, not executable contracts. They do not affect any acceptance criteria, ADR decisions, or implementation pseudocode. All of those remain accurate.

The one actionable gap: `thiserror` must be added to `unimatrix-engine/Cargo.toml` alongside `petgraph`. This was already flagged as a "verify before adding" step in IMPLEMENTATION-BRIEF.md — the verification answer is: it is not present, so add it.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for crt-018b search.rs modifications and effectiveness module -- no results (feature too recent)
- Stored: nothing novel to store -- findings are feature-specific line-number drift from crt-018b insertion; not a generalizable pattern beyond "crt-018b added ~40 lines before Step 0 and ~363 test lines to search.rs"
