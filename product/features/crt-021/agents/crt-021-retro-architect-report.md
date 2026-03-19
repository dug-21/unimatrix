# crt-021 Retrospective: Architect Report

> Agent: crt-021-retro-architect (uni-architect)
> Feature: crt-021 (W1-1 Typed Relationship Graph)
> Date: 2026-03-19

---

## 1. Patterns

### 1a. Prior pattern search

Searched Unimatrix for graph patterns in `unimatrix-engine` and `unimatrix-server` domains before acting. Found:

- Entry #1607 (SupersessionGraph pattern) — active, confirmed STALE per stewardship review
- Entry #1560 (Arc<RwLock<T>> background-tick state cache) — active, covers TypedGraphState exactly

### 1b. Entry #1607 — Updated (DONE)

**Action**: `context_correct` on #1607 → new entry **#2476**

**Title**: TypedRelationGraph: typed edge weights, two-pass build with bootstrap exclusion, edges_of_type filter boundary

**What changed**: The old entry described `StableGraph<u64, ()>`, per-query rebuild inside `spawn_blocking`, and a crt-017 `EdgeKind` extension point — all replaced by crt-021. The correction covers:
- `TypedRelationGraph` struct (`StableGraph<u64, RelationEdge>` + `HashMap<u64, NodeIndex>`)
- `RelationType` enum (5 variants, string encoding rationale)
- `RelationEdge` struct (6 fields including `bootstrap_only`)
- `edges_of_type` as sole filter boundary (SR-01 invariant, grep-testable)
- Three-pass build: Pass 1 nodes, Pass 2a Supersedes from `entries.supersedes`, Pass 2b non-Supersedes from GRAPH_EDGES (skip `bootstrap_only=true` structurally), Pass 3 cycle check on Supersedes-only temp graph
- `graph_penalty` and `find_terminal_active` unchanged in behavior, now use `edges_of_type` exclusively
- Background-tick caller pattern (build once, swap under write lock)

**Tags updated**: removed `supersession`; added `crt-021`, `typed-edges`, `relation-edge`, `edges-of-type`, `typed-relation-graph`, `bootstrap-only`, `filter-boundary`

### 1c. Entry #1560 (Arc<RwLock tick-rebuild) — Skipped

**Reason**: Entry #1560 ("Background-tick state cache pattern: Arc<RwLock<T>> shared through ServiceLayer, sole writer is the tick") already covers the TypedGraphState pattern fully. ADR #2417 explicitly states: "Arc<RwLock<_>> tick-rebuild pattern (entry #1560) extended, not replaced." crt-021 adds no new structural wrinkle to the pattern — `TypedGraphState` is another instance of the same pattern, validated alongside `ConfidenceStateHandle` (crt-019) and `EffectivenessStateHandle` (crt-018b). No update needed.

---

## 2. Procedures

### 2a. Schema migration bootstrap procedure — Skipped

The v12→v13 migration introduces two new techniques: (1) `INSERT ... SELECT` from existing tables to bootstrap edge data, and (2) a window-function normalized weight formula (`COALESCE(CAST(count AS REAL) / NULLIF(MAX(count) OVER (), 0), 1.0)`).

**Reason for skip**: Entry #836 ("How to add a new table to the v6+ SQLite schema") already covers the general pattern including `INSERT OR IGNORE` idempotency and backfill from existing data. The window-function weight normalization is specific to the co-access count normalization problem; the test-plan agent already stored entry #2428 ("Migration test pattern: window function weight normalization with empty-table guard (R-06 pattern)"). Storing a third procedure entry for the same migration block would be redundant. The R-06 guard is now tested and documented in #2428.

### 2b. Build/test process — No changes needed

No new procedure patterns emerged from the build process that aren't already covered by existing entries.

---

## 3. ADR Status

### 3a. Entry #2417 (ADR-001 crt-021, typed edge weight model) — VALIDATED ACTIVE

All 2094 tests passed. All 21 acceptance criteria met. Entry #2417 is accurate and active. No correction needed.

### 3b. Entry #1602 (ADR-002 crt-014, per-query graph rebuild) — FLAGGED FOR SUPERSESSION

**Status**: Active — but STALE. This ADR explicitly describes per-query rebuild ("Use Option A: per-query rebuild... graph construction happens inside the existing `spawn_blocking` block in the search pipeline"), which crt-021 superseded via FR-22 (pre-built `TypedGraphState`, no per-query rebuild).

**Proposed replacement text**:

> ## ADR-002 (crt-014, superseded): Per-Query Graph Rebuild
>
> **Superseded by**: crt-021 implementation of FR-22, per entry #2417.
>
> ### Decision (superseded)
> Build supersession graph per-query inside `spawn_blocking`. No caching. Upgrade threshold: ~5,000 entries.
>
> ### What replaced it
> `TypedGraphState` holds a pre-built `TypedRelationGraph` rebuilt once per background tick (every ~15 min) via `Arc<RwLock<TypedGraphState>>`. The search hot path reads the pre-built graph under a short read lock — `build_typed_relation_graph` is never called on a search request. This eliminates spawn_blocking on the hot path entirely (crt-014 regression fix, see memory/lesson_crt014_per_query_store_scan.md). The per-query approach is no longer in use.

**AWAITING HUMAN APPROVAL** before executing `context_correct` or `context_deprecate` on #1602. This is an ADR supersession; authority to proceed rests with the human reviewer.

---

## 4. Lessons

### 4a. Gate 3b working-tree failure — Stored as **#2477** (NEW)

**Title**: SM must commit all in-flight working tree diffs before invoking gate validators

**What happened**: Wave-3 `uni-rust-dev` removed deprecated `SupersessionGraph` shims in the working tree. SM committed only agent reports, not the `graph.rs` diff. Gate 3b checked committed HEAD, found shims still present, returned REWORKABLE FAIL. Fix: a single `git commit` of the already-clean working tree change.

**Why stored separately from #2463**: Entry #2463 ("Gate 3c: always verify committed code, not working tree state") covers the **validator** side — how validators should verify. Entry #2477 covers the **SM** side — committing all in-flight diffs before invoking validators. Both halves are needed; each has a distinct audience and distinct action.

### 4b. Compile cycles (146 cycles) — Updated entry #1269 → **#2478**

**Action**: `context_correct` on #1269, adding crt-021 recurrence data.

**What was added**: crt-021 produced 146 compile cycles (threshold 6, prior worst was col-022 at 106). Added as third recurrence data point. Added sixth mitigation: batch all structural renames in a single pass before any compilation on large multi-crate refactors. The 146-cycle count is partly attributable to the 8h session gap (lifespan outlier, not a design problem), but the per-agent full-workspace build pattern is the controllable root cause.

**Note on lifespan outlier (554min/556min)**: The two `uni-rust-dev` instances running 554min and 556min are a baseline anomaly caused by the 8h session gap while background agents were paused. The agents themselves did not run that long — they accumulated runtime while the session was suspended. This is not an agent design problem and no lesson is warranted.

### 4c. bash_for_search (378 Bash search calls) — Skipped

**Reason**: Entry #1371 ("Agents default to Bash for search instead of Grep/Glob tools — reinforcement needed in spawn prompts") already covers this exactly, with the same root cause (agents investigating unfamiliar code fall back to Bash grep/find). No new information; crt-021 is another recurrence of the same pattern. Adding a correction to #1371 with crt-021 data would add marginal value — the entry is already classified as a systemic pattern. Skipped.

### 4d. Zero-rework validated pattern — Not stored as new entry

**Observation**: crt-021 had zero rework sessions (`rework_session_count: 0`) and zero post-completion work (`post_completion_work: 0.0%`). The pre-built design session artifacts (IMPLEMENTATION-BRIEF, pseudocode, test-plan) eliminated scope drift.

**Reason not stored**: Entry #925 ("Clean first-pass delivery from precise scope decomposition and file-aligned parallelism") already covers this validated pattern from col-020b. Crt-021 is additional confirmation, not a new finding. The positive outcome is already recorded in Unimatrix entry #2475 (retrospective findings for crt-021).

---

## 5. Summary of Findings

| Item | Entry | Action | Status |
|------|-------|--------|--------|
| #1607 SupersessionGraph pattern (stale) | → #2476 | Corrected to TypedRelationGraph | DONE |
| #1560 Arc<RwLock tick-rebuild pattern | — | Already covers TypedGraphState | SKIPPED (adequate) |
| #2417 ADR-001 crt-021 (typed edge model) | — | Validated active | CONFIRMED |
| #1602 ADR-002 crt-014 (per-query rebuild) | — | Flagged for supersession | AWAITING APPROVAL |
| Schema migration bootstrap procedure | — | Covered by #836 + #2428 | SKIPPED (redundant) |
| Gate 3b SM working-tree lesson | #2477 | New lesson stored | DONE |
| Compile cycles lesson update | #1269 → #2478 | Corrected with crt-021 data | DONE |
| bash_for_search lesson | — | Already covered by #1371 | SKIPPED (adequate) |
| Zero-rework pattern | — | Already covered by #925, #2475 | SKIPPED (redundant) |

### ADR Requiring Human Decision

**Entry #1602 (ADR-002, crt-014: Per-Query Graph Rebuild)** is active but describes architecture that crt-021 explicitly replaced. Proposed action: deprecate #1602 with replacement text pointing to crt-021 / entry #2417. Awaiting human approval before executing.

### Open Question for Protocol

The gate-3b REWORKABLE FAIL from this feature (SM committed reports without the production diff) suggests the delivery protocol should include an explicit pre-gate step: "Run `git status --short`; commit all modified production files before invoking the gate validator." Currently the protocol does not call this out. Consider adding it to `.claude/protocols/uni/uni-delivery-protocol.md`.
