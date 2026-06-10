# crt-053 Implementation Brief — Active-Only PPR Expansion Seeds

**Feature ID**: crt-053
**GH Issue**: [#717](https://github.com/dug-21/unimatrix/issues/717)
**Status**: READY FOR DELIVERY (design locked 2026-06-10; alignment 6/6 PASS, zero variances)
**Phase**: Cortical (`crt`) — Learning & drift / search-quality correctness

> **BINDING DIRECTIVE — read before opening any code.** This is the most sensitive code in the
> system. The dimensions of this feature were researched (ass-073 #720, ass-074 #721) and decided
> by human judgment. They are **not** open for re-optimization. You **will** see adjacent things
> that look like gaps (injection-path penalty bypass, vnc-017 redirect ceiling, deprecated entries
> in Flexible results). **Do not close them.** Do exactly what is in scope — nothing more. If you
> believe something in scope is wrong, **stop and raise it; do not "fix" it.** The "no adlibs"
> directive is binding on delivery. This is a single-edit feature (C-01).

---

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-053/SCOPE.md |
| Scope Risk Assessment | product/features/crt-053/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/crt-053/architecture/ARCHITECTURE.md |
| ADR-001 (storage of seed predicate decision) | product/features/crt-053/architecture/ADR-001-active-only-ppr-seeds.md |
| Specification | product/features/crt-053/specification/SPECIFICATION.md |
| Risk-Test Strategy | product/features/crt-053/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-053/ALIGNMENT-REPORT.md |

---

## Goal

Restrict the PPR / `graph_expand` expansion seed set to **Active** entries only, so graph expansion
anchors exclusively on current knowledge. Today the expander seeds from the full Flexible candidate
pool (including Deprecated/superseded entries) and `graph_expand` applies no status filter, so a
deprecated entry can serve as a BFS seed and inject its neighbors at full weight (penalty `1.0`) —
the latent seed-side leak ass-074 (#721) identified. The fix is a single typed predicate on the
`seed_ids` build inside the `ppr_expander_enabled` branch; everything downstream of the BFS, and the
entire HNSW ranking path, is unchanged.

---

## Component Map

This feature touches **one** production file. There is no multi-component decomposition — the
Component Map below reflects the architecture's deliberately minimal surface.

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| `SearchService::search` seed filter (search.rs) | pseudocode/search-seed-filter.md | test-plan/search-seed-filter.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

> pseudocode and test-plan files are produced in Session 2 Stage 3a. Given the single-edit scope,
> expect one component file. The acceptance surface is the nan-018 fixture corpus (with the ass-073
> positive-edge revision) **or** the Python integration suite over raw `entries` JSON — the tester
> picks (OQ-1). No eval-harness metric gate is permitted (SR-01 / NFR-05).

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| How to prevent deprecated entries anchoring PPR expansion | Filter `seed_ids` to `e.status == Status::Active` inside the `ppr_expander_enabled` branch at the `seed_ids` build (~`search.rs:915`) | SCOPE.md "The change"; ARCHITECTURE "The Change" | architecture/ADR-001-active-only-ppr-seeds.md (Unimatrix #4917) |
| Predicate form | Typed enum comparison `== Status::Active`, never a string compare | FR-02; SR-02; ARCHITECTURE "Predicate Design" | architecture/ADR-001-active-only-ppr-seeds.md |
| Whether to also redirect/penalize injected entries | NO — no `find_terminal_active` on injected entries, no `penalty_map` extension. Seed filter is the sole mechanism | Locked Decision 4 | architecture/ADR-001-active-only-ppr-seeds.md |
| Penalty steepness / magnitude tuning | NO — not the lever (ass-073/ass-074); Q6/Q8 dropped entirely | Locked Decision 3 | architecture/ADR-001-active-only-ppr-seeds.md |
| Two-mode design (Flexible keep-visible / Strict evict) | UNCHANGED — deprecated still appears & is penalized in Flexible | Locked Decision 1; C-03; ADR-001 #481 | architecture/ADR-001-active-only-ppr-seeds.md |
| vnc-017 50-edge redirect ceiling | UNCHANGED — pre-existing, separately tracked; residual case knowingly accepted | Locked Decision 5 | architecture/ADR-001-active-only-ppr-seeds.md |
| Acceptance validation method | Behavior-based ID-level assertions only; NO eval-harness metric gate (P@5/MRR/soft-GT) | SR-01; NFR-05; R-01 | architecture/ADR-001-active-only-ppr-seeds.md |

### Locked Decisions (carried VERBATIM — do not reopen, do not "complete")

1. **Two modes stand.** Flexible (search) = penalize-but-keep-visible; Strict (briefing) = evict.
   Returning deprecated entries in search is **not an error**. Not in scope to change.
2. **The bar is "deprecated must not outweigh a comparable active," not "deprecated must be absent"
   (in Flexible).** The HNSW topology penalty already meets that bar on the ranking path; the
   active-only seed filter closes the one path (PPR injection) where it didn't. Sufficient by judgment.
3. **No steepness work.** ass-073 and ass-074 both confirmed penalty *magnitude* is not the lever.
   crt-053 is **not** a tuning feature. Q6/Q8 are dropped entirely.
4. **No injection-side redirect or penalty machinery.** Do **not** add `find_terminal_active`
   resolution to injected entries, and do **not** extend `penalty_map` to injected entries. The
   active-only seed filter is the chosen mechanism. The residual case it does not cover (a deprecated
   *neighbor* of an active seed reachable only via the vnc-017 >50-edge redirect ceiling) is
   **knowingly accepted**, not a bug to patch here.
5. **The vnc-017 redirect ceiling (50) is not this feature's problem.** Pre-existing,
   separately-tracked limitation. Leave it.

### Four-Issue Cluster Disposition (carried VERBATIM)

| Issue | Disposition |
|---|---|
| **#704** (deprecated surfacing / outranking active) | **Closes via this feature.** The active-only seed filter removes the one path (PPR injection at penalty 1.0) by which a deprecated entry could outrank a comparable active; the HNSW penalty + 6b terminal-active head injection already handle the ranking path. Close on this PR. |
| **#406** (multi-hop terminal-active injection "fails") | **Does NOT reproduce** in the eval graph rebuild (ass-073 #720) — the multi-hop redirect resolves correctly. Treat the failing test as a **test/snapshot-construction artifact to investigate**, not a retrieval fix. Do not "fix" retrieval for it. |
| **#585** (edge generation pulls deprecated into candidate scoring) | **Out of scope — separate concern.** It is about keeping the *edge graph* free of deprecated targets (write-time hygiene). Decide and track separately on #585; do not bundle it here. |
| **#405** (deprecated confidence flake) | **Split out (locked earlier).** Independent timing flake; not this feature. |

---

## Files to Create/Modify

| File | Change |
|------|--------|
| `crates/unimatrix-server/src/services/search.rs` | **The only production edit.** Add a `.filter(\|(e, _)\| e.status == Status::Active)` to the `seed_ids` build inside the `if self.ppr_expander_enabled` branch (~`:915`). Ensure `Status` is in scope (import if needed — import is the only permissible adjacent edit). |
| Test surface — nan-018 fixture corpus **or** Python integration suite | Add behavior-based acceptance tests (AC-01..AC-05) over raw `entries` JSON. Extend existing fixtures/helpers; do not create isolated scaffolding. Fixture may need the ass-073 positive-edge revision (OQ-1). |

**No new files, modules, structs, traits, config flags, or functions.** (ARCHITECTURE Component Breakdown.)

---

## Data Structures (all pre-existing — reused, none created)

| Symbol | Type / Signature | Source |
|--------|------------------|--------|
| `results_with_scores` | `Vec<(EntryRecord, f32)>` (post 6a/6b; mixed statuses) | local in `SearchService::search` |
| `EntryRecord.status` | `pub status: Status` | `unimatrix-store/src/schema.rs:57` |
| `Status` enum | `Active=0, Deprecated=1, Proposed=2, Quarantined=3` (`#[repr(u8)]`, `PartialEq`) | `unimatrix-store/src/schema.rs:8-15` |
| `seed_ids` (edit site) | `Vec<u64>` | `search.rs:~915`, inside `if self.ppr_expander_enabled` |
| `ppr_expander_enabled` | `bool` field on `SearchService` (guards the block at `:911`) | `SearchService` |

---

## Function Signatures (all unchanged)

| Function | Signature | Status |
|----------|-----------|--------|
| `graph_expand` | `fn(&TypedRelationGraph, &[u64], depth, max) -> HashSet<u64>` | **Unchanged.** Receives a narrower `seed_ids` slice; traversal direction (forward BFS over Outgoing positive edges), depth, ceilings untouched. |
| `SecurityGateway::is_quarantined` | `is_quarantined(&entry.status)` at `search.rs:950` | **Unchanged.** Per-expanded-entry security gate, separate from and downstream of the seed filter. NOT the seed filter — do not conflate or edit (R-11). |

### Reference implementation (from ADR-001 — the entire production delta)

```rust
// BEFORE (:915)
let seed_ids: Vec<u64> = results_with_scores.iter().map(|(e, _)| e.id).collect();

// AFTER (crt-053): seed graph_expand from ACTIVE entries only.
let seed_ids: Vec<u64> = results_with_scores
    .iter()
    .filter(|(e, _)| e.status == Status::Active)
    .map(|(e, _)| e.id)
    .collect();
```

---

## Constraints

- **C-01** — The active-only filter is the **only** production change. No other file/stage edited
  for status behavior. (Diff touches exactly one file and exactly the `seed_ids` build.)
- **C-02** — The `ppr_expander_enabled = false` path stays **bit-for-bit identical**. The filter is
  lexically inside the enabled branch and touches only the local `seed_ids` binding — structurally
  guaranteed, not disciplinary.
- **C-03** — Flexible/Strict mode semantics untouched (ADR-001 #481). Deprecated entries still
  appear and are still penalized in Flexible.
- **C-04** — Status tests assert ranking/presence outcomes, never penalty constants (crt-013 #703).
- **Anti-AC** — No test may assert deprecated entries are *absent* from Flexible (search) results.
  Such an assertion contradicts the two-mode design and MUST NOT be added.

### Delivery trip-wires (from RISK-TEST-STRATEGY R-01..R-12)

- **R-03 / scope creep (Critical):** the diff must touch only the `seed_ids` build. No
  `find_terminal_active` on injected entries, no `penalty_map` mutation, no new config flag, no
  edge-write change. Existing `graph_expand` write-only negative tests must remain UNCHANGED (the
  #4495 vnc-018 trip-wire — inverting them is the documented failure).
- **R-04 / vacuous pass (Critical):** AC-01/AC-05 each need a differential control arm — with the
  filter removed (or the deprecated seed forced active), the deprecated-only neighbor MUST reappear,
  proving absence is filter-caused, not unreachability (#4902).
- **R-02 / over-drop (Critical):** prove the filter RETAINS active seeds (6b terminal-active heads,
  HNSW actives), not just that it drops deprecated.
- **R-11 / quarantine gate:** do not edit `:950`. Seed predicate dropping Quarantined seeds is
  defense-in-depth, not a replacement for the enforcement check.
- **R-10 / #406:** if #406 reproduces in the delivery fixture, RAISE as a fixture-divergence signal
  vs ass-073's eval graph; do NOT patch retrieval.

---

## Dependencies

- `crates/unimatrix-server/src/services/search.rs` — sole production edit site (Phase 0 seed
  collection, inside `if self.ppr_expander_enabled`).
- `crates/unimatrix-store/src/schema.rs::Status` — the enum the predicate tests.
- `crates/unimatrix-engine/src/graph_expand.rs::graph_expand` — BFS consumer of `seed_ids`
  (unchanged).
- nan-018 fixture corpus (with ass-073 positive-edge revision) and/or the Python integration suite
  — acceptance verification surface.
- Research provenance (read-only): ass-073 (#720), ass-074 (#721).
- No new crates, no external services.

---

## NOT in Scope (explicit exclusions)

- **Injection-side redirect** — do NOT add `find_terminal_active` to injected entries.
- **Penalty machinery** — do NOT extend `penalty_map` to injected entries; do NOT change any
  penalty magnitude or steepness (Q6/Q8 dropped).
- **Deprecated absence in Flexible** — do NOT make deprecated entries absent from search results,
  and do NOT add a test asserting such absence (anti-AC).
- **Eval-harness gate** — no P@5/MRR or any eval-harness metric acceptance gate (SR-01; #500 /
  soft-GT P@5 trap).
- **vnc-017 50-edge redirect ceiling** — pre-existing, separately tracked. Residual case knowingly
  accepted.
- **#585 edge-graph hygiene** — separate concern, tracked separately.
- **#406 multi-hop injection** — does not reproduce; test/snapshot artifact to investigate, not a
  retrieval fix.
- **#405 deprecated confidence flake** — split out; independent timing flake.
- **Traversal-direction / mode / scoring changes** of any kind beyond narrowing the seed set.

---

## Alignment Status

**6/6 PASS — zero variances, zero scope additions** (ALIGNMENT-REPORT.md, 2026-06-10).

| Check | Status |
|-------|--------|
| Vision Alignment | PASS — advances self-learning intelligence (#4677); honors graceful-degradation, in-memory hot-path, capability-gate principles |
| Milestone Fit | PASS — Cortical search-quality correctness; no future-milestone capability built |
| Scope Gaps | PASS — single deliverable + 3 validation arms all addressed |
| Scope Additions | PASS — zero additions; every locked exclusion carried verbatim across all docs |
| Architecture Consistency | PASS — single edit site, off-path equivalence, enum predicate concordant across ARCHITECTURE / SPECIFICATION / RISK-TEST-STRATEGY |
| Risk Completeness | PASS — SR-01..SR-06 fully traced to R-01..R-12; vacuous-pass + direction-semantics traps covered with differential control arms |

No VARIANCE or FAIL findings require user approval. The one noted (not a variance) item:
**SCOPE-mandated simplification** — behavior-based validation only, no eval-harness gate, because
the platform cannot measure graph-relational search-heuristic effectiveness (SR-01, ass-074 #721,
Unimatrix #4888).

---

## Open Questions (RESOLVED in Stage 3a)

- **OQ-1 (RESOLVED — tester):** Neither the nan-018 fixture corpus nor the Python MCP suite can host
  AC-01/AC-05 — fixtures author only `superseded_by`→`Supersedes` edges (excluded from positive BFS)
  with an empty positive-edge slice, and the MCP layer cannot toggle the expander or author a positive
  deprecated→neighbor edge. **Chosen surface: the Rust full-pipeline harness
  `crates/unimatrix-server/tests/pipeline_e2e.rs`** (live `SearchService::search`, positive edges via
  `TestHarness::insert_graph_edge` + `rebuild_typed_graph`, supports the R-04 control arm). Requires
  one cumulative **test-support-only** extension: an expander-enabling `TestHarness` constructor
  variant (current `new()` wires `ppr_expander_enabled = false`). Test-support only — does NOT touch
  C-01. Control-arm form: forcing the deprecated seed Active is recommended (no second code path).
- **OQ-2 (RESOLVED — pseudocode):** `results_with_scores` **is** the sole seed source for
  `graph_expand` inside the enabled branch (verified live: `seed_ids`@915 from it alone, `graph_expand`
  receives exactly `&seed_ids`, `in_pool`@929 derives from it). R-09 cleared. Edit-site line numbers
  hold exactly (911 branch, 915 build). Refinement: `Status` is already imported (search.rs:10) — the
  diff is the filter clause **alone**, no import edit; C-01 is tighter than the brief stated.
