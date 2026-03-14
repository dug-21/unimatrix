# crt-019 Retrospective: Architect Report

> Agent: crt-019-retro-architect (uni-architect)
> Mode: retrospective
> Date: 2026-03-14

---

## 1. Patterns

### New entries stored

**#1493 — ConfidenceStateHandle sharing pattern: Arc\<RwLock\<T\>\> cloned through ServiceLayer to multiple services** (pattern)
- Tags: arc-rwlock, confidence-state, crt-019, service-layer, shared-state, unimatrix-server
- What: Single Arc<RwLock<ConfidenceState>> created in ConfidenceService, cloned via Arc::clone into SearchService (reader), StatusService (writer), UsageService (reader). Writer holds write lock for a short critical section covering all four fields atomically. Readers clone scalar(s) and drop before any await point.
- Why new: Search found #316 (ServiceLayer extraction pattern) and #734 (graceful RwLock fallback) — neither covers the multi-service Arc clone wiring or the four-field consistency rationale.

**#1494 — Snapshot-before-spawn pattern: read RwLock guard, clone scalars, drop guard before spawn_blocking** (pattern)
- Tags: async-rust, closure-capture, crt-019, rwlock, snapshot-before-spawn, spawn-blocking, unimatrix-server
- What: Before spawn_blocking, acquire read lock in a scoped let block, clone the needed f64 values, drop the guard, then capture only the cloned scalars in the closure. Contrasts with the Gate 3b bug where hardcoded literals were captured instead.
- Why new: Existing #731 (batched fire-and-forget), #1367 (spawn_blocking_with_timeout) do not document the scalar-snapshot-before-closure pattern or the anti-pattern it prevents.

**#1498 — flat_map repeat + dedup-before-multiply for weighted access increment in UsageService** (pattern)
- Tags: access-weight, crt-019, dedup-before-multiply, flat-map, usage-service, unimatrix-server, weighted-increment
- What: After UsageDedup filter, expand each passing ID with flat_map + repeat(access_weight) to produce a weighted list. Store applies access_count += 1 per list entry. Dedup must fire before expansion to prevent dedup from collapsing the doubled entries. access_weight default must be 1 (never 0).
- Why new: Search found no existing entry covering this technique.

### Existing entry validated

**#1480 — Parameter-passing over shared state when promoting engine constants to runtime values** (pattern)
- Assessment: STILL ACCURATE. The final implementation matches exactly: SEARCH_SIMILARITY_WEIGHT removed from engine, rerank_score gains confidence_weight: f64 parameter, callers in search.rs pass the runtime value read from ConfidenceStateHandle. Engine remains stateless. No drift from what was stored during design.
- No update needed.

### Skipped

- **Arc<RwLock> poison recovery** — already covered by #734 (Graceful RwLock Fallback). crt-019 applies the same `unwrap_or_else(|e| e.into_inner())` pattern without novelty.
- **Pure computation engine module pattern** — #1042 already covers this. crt-019 reinforces it but adds nothing new.

---

## 2. Procedures

### Schema version change
None. NFR-05 confirmed schema stays at v12. No migration procedure needed.

### Test infrastructure changes
Four new integration tests added across existing suites — cumulative extension, not new scaffolding:
- `test_empirical_prior_flows_to_stored_confidence` → test_lifecycle.py
- `test_context_get_implicit_helpful_vote` → test_tools.py
- `test_context_lookup_doubled_access_count` → test_tools.py
- `test_search_uses_adaptive_confidence_weight` → test_confidence.py

No new procedure entry warranted — this follows the established "extend existing fixtures" convention.

### flat_map repeat technique
Stored as pattern #1498 (above) rather than a procedure — it is a code pattern at a single call site, not a multi-step reusable technique.

---

## 3. ADR Validation

All four feature ADRs validated against final implementation (Gate 3b + 3c reports):

| ADR | Core Decision | Validation Status | Notes |
|-----|--------------|-------------------|-------|
| ADR-001 | Adaptive blend in Arc\<RwLock\<ConfidenceState\>\> at ServiceLayer, not engine crate | VALIDATED | ConfidenceStateHandle wired through ServiceLayer to all three services. SEARCH_SIMILARITY_WEIGHT confirmed absent. |
| ADR-002 | Bayesian prior cold-start: alpha0=3.0, beta0=3.0; empirical update requires MINIMUM_VOTED_POPULATION >= 10 | VALIDATED | MINIMUM_VOTED_POPULATION = 10 at status.rs line 31. Boundary tests at n=9 and n=10 pass. Cold-start clamp [0.5, 50.0] confirmed (after Gate 3a SPEC fix). |
| ADR-003 | base_score(Status::Proposed, "auto") returns 0.5 regardless of trust_source | VALIDATED | `auto_proposed_base_score_unchanged` test passes. Active/auto = 0.35, all other Active = 0.5. |
| ADR-004 | context_lookup sets access_weight=2; context_get sets access_weight=1 (default) | VALIDATED | tools.rs line 470 sets access_weight: 2 for lookup; line 619 uses default 1 for get. flat_map repeat confirmed in usage.rs. |

No ADRs flagged for supersession. All four remain active and accurate.

---

## 4. Lessons

### New entries stored

**#1495 — Wired-but-unused field anti-pattern: struct wiring does not imply behavioral wiring at use site** (lesson-learned)
- Tags: anti-pattern, code-review, crt-019, gate-failure, placeholder, struct-wiring, wired-but-unused
- Root cause of Gate 3b rework iteration 1, two instances: (1) status.rs discarded all four computed values with `let _ = (...)` + TODO comment; (2) UsageService had ConfidenceStateHandle wired into struct but closure captured 3.0, 3.0 literals. Compile succeeds in both cases.
- Detection rule: For every struct holding a handle, verify all method bodies read the field at the right site; check closure capture lists for literals that should be runtime reads.
- Not covered by any existing entry — checked pattern and lesson-learned categories.

**#1496 — Numeric constant mismatch across SPEC / pseudocode / test-plan causes latent test-vs-implementation conflict** (lesson-learned)
- Tags: constant-mismatch, crt-019, design-phase, document-alignment, gate-failure, numeric-constant, spec-pseudocode-test
- Root cause of Gate 3a rework iteration 1: FR-09 clamp said [0.5, 20.0], pseudocode used [0.5, 50.0], test plan asserted <= 20.0. Three agents, three documents, no cross-check.
- Related to #723 and #1204 but distinct: those cover architecture/spec and pseudocode/edge-case alignment; this covers numeric literal consistency across all three document types simultaneously.
- Prevention: define constants once in SPEC by name; Gate 3a adds a constant consistency check.

**#1497 — Private formula copy in service file creates silent divergence risk from engine crate** (lesson-learned)
- Tags: code-review, crt-019, divergence-risk, engine-crate, formula-copy, private-function, unimatrix-server
- From Gate 3c non-blocking WARN: `adaptive_confidence_weight_local` in status.rs duplicates `unimatrix_engine::confidence::adaptive_confidence_weight`. No divergence at ship time but future formula changes to the engine will not propagate.
- Rule: Service files must call engine functions directly, never copy formula implementations. Detection: grep for `_local` / `_internal` suffixed functions mirroring engine names.
- Not covered by existing entries.

### Skipped

- **permission_retries lesson** — settings.json allowlist already well-documented; adding another lesson would duplicate existing process guidance without new content.
- **compile_cycles lesson** — #1269 and #1165 already cover "High Compile Cycles Signal Need for Targeted Test Invocations." crt-019's 126 cycles vs threshold 6 is consistent with the known pattern; no new generalizable finding.
- **bash_for_search lesson** — #1371 already covers "Agents default to Bash for search instead of Grep/Glob tools." crt-019 confirms this recurs (494 bash searches, 24.9% of bash calls) but adds no new root cause analysis.

---

## 5. Retrospective Findings

### Hotspot analysis — generalizable findings

**edit_bloat_ratio outlier (0.50 vs mean 0.06):**
The 5598 KB edit bloat with a 0.50 ratio is the most anomalous metric. Likely cause: the confidence formula refactor required updating function signatures at many call sites across 103 files, and agents may have re-written files with large intermediate content during the cold restart cycles. This is partially inherent to a broad formula refactor (mutation_spread is structural, not process waste) but the ratio suggests agents over-generated intermediate content rather than making targeted edits. No new lesson stored — existing edit discipline conventions apply.

**mutation_spread (103 files) and file_breadth (158 files):**
These are structurally inherent to crt-019's scope: replacing `compute_confidence`, `base_score`, and `rerank_score` signatures propagates to every test file and every call site across the workspace. Not a process problem. A future feature with similar broad signature refactoring should budget for this volume explicitly in SCOPE.md.

**context_load outlier (11248 KB vs mean 1096 KB):**
Consistent with the three cold restart events (480-min, 105-min, 135-min gaps). Each cold restart triggered re-reading 52, 6, and 2 previously-accessed files respectively. The 225 KB loaded before first write is the high initial context load from reading all design artifacts. Process recommendation already in retrospective: this is a session management issue (permission_retries compounding with long gaps), not a new knowledge finding.

**session_timeout (8.0h and 2.2h gaps):**
Two sessions separated by 8 hours. The cold restart at 480 minutes (8 hours) generated 52 re-reads — significant context reconstruction overhead. The per-agent lifespan warning (uni-tester at 52 min, threshold 45) is borderline and expected for a feature with 103-file mutation scope.

### Recommendation actions

| Recommendation | Action Taken | Entry |
|----------------|--------------|-------|
| permission_retries: add cargo commands to settings.json allowlist | Not stored (process config, not knowledge) | — |
| sleep_workarounds: use run_in_background | Not stored (existing convention) | — |
| compile_cycles: targeted cargo test invocations | Existing #1269, #1165 cover this | Skipped (duplicate) |
| bash_for_search: use Grep/Glob tools | Existing #1371 covers this | Skipped (duplicate) |

### Pre-existing retrospective entry

**#1492 — Retrospective findings: crt-019** (lesson-learned, stored by automated retrospective)
- This entry was stored by the Phase 1b retrospective process before this report. It captures aggregate hotspot counts and tags.
- The current report provides the detailed analysis that #1492 summarizes. No conflict.

---

## Unimatrix Entries Produced

| ID | Title | Category |
|----|-------|----------|
| #1493 | ConfidenceStateHandle sharing pattern: Arc\<RwLock\<T\>\> cloned through ServiceLayer to multiple services | pattern |
| #1494 | Snapshot-before-spawn pattern: read RwLock guard, clone scalars, drop guard before spawn_blocking | pattern |
| #1495 | Wired-but-unused field anti-pattern: struct wiring does not imply behavioral wiring at use site | lesson-learned |
| #1496 | Numeric constant mismatch across SPEC / pseudocode / test-plan causes latent test-vs-implementation conflict | lesson-learned |
| #1497 | Private formula copy in service file creates silent divergence risk from engine crate | lesson-learned |
| #1498 | flat_map repeat + dedup-before-multiply for weighted access increment in UsageService | pattern |
