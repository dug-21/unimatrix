# FINDINGS: ADR Dependency Tracking — `DependsOn` Graph Relationship

**Spike**: ass-055
**Date**: 2026-05-06
**Approach**: investigation
**Confidence**: validated (all answers grounded in codebase evidence with line references)

---

## Findings

### Q1: Is `Prerequisite` the correct semantic mapping for `depends_on`, or does the direction convention mismatch require renaming or a new type?

**Answer**: `Prerequisite` is the correct semantic mapping. No rename is needed. The edge must be stored as **A→B meaning "A is a prerequisite of B" (B depends on A)**. This direction is already baked into PPR and graph_expand behavior with passing tests.

**Evidence — naming semantics**: Nygard ADR `depends_on` means "the validity of this decision assumes that [linked decision] holds." `Prerequisite` encodes the same structural constraint: "A is a prerequisite of B" ↔ "B depends on A." The dependency validity coupling is identical. "Prerequisite" is more precise than "DependsOn" for a knowledge graph because it encodes a logical precondition, not a temporal task ordering.

**Evidence — PPR direction (critical)**: PPR uses **reverse-walk** (transpose PPR). For edge A→B, seeding B causes mass to flow backward to A because A's outgoing neighbors include B (`graph_ppr.rs:34–39`, comment at line 37 is explicit). Applied to `Prerequisite`: store A→B (A is prerequisite of B). Seeding Decision B → PPR flows mass backward to A (the decision B depends on). This is the desired retrieval behavior.

The PPR test `test_prerequisite_incoming_direction` (`graph_ppr_tests.rs:344–356`) directly confirms this:
```
// A=1 is prerequisite of B=2: edge A→B via Prerequisite. Seed: B.
let graph = make_graph_with_edges(&[(1, 2, RelationType::Prerequisite, 1.0)]);
// A must surface as prerequisite of B
assert!(result.get(&1).copied().unwrap_or(0.0) > 0.0);
```

And `test_prerequisite_wrong_direction_does_not_propagate` (`graph_ppr_tests.rs:359–377`) confirms seeding A does NOT surface B (no forward propagation).

**Evidence — graph_expand direction (the asymmetry)**: graph_expand uses **outgoing traversal from seed** (`graph_expand.rs:123–136`). For edge A→B and seed B, outgoing from B does NOT reach A. The behavioral contract at `graph_expand.rs:20–25` makes this explicit: "Given seed B and edge C→B (Incoming to B), entry C is NOT reachable."

The test `test_graph_expand_prerequisite_surfaces_neighbor` (`graph_expand_tests.rs:124–134`) uses seed `[1]` and edge `1→2` and surfaces `2` — confirming graph_expand only works in the forward direction (seed the prerequisite, surface what depends on it).

**Direction table**:

| Edge stored as | PPR seed B (dependent) surfaces A (prerequisite)? | graph_expand seed B surfaces A? |
|---|---|---|
| A→B (correct: A is prereq of B) | Yes (reverse walk) | No |
| B→A (wrong) | No | Yes |

The graph_expand gap means Phase 0 candidate widening does not discover what B depends on when B is the query anchor. PPR (Phase 5, alpha=0.85, iterations=20) compensates. Accept the asymmetry — storing a reverse edge for graph_expand would create semantic ambiguity (both directions would read as mutual dependency).

**Recommendation**: Retain `Prerequisite`. Store A→B. No rename or new type. Document the graph_expand gap in implementation notes.

---

### Q2: Should `depends_on` linkage be stored as a pure graph edge (GRAPH_EDGES only) or as a dual-source field (`entries.depends_on` analogous to `entries.supersedes`)?

**Answer**: Pure GRAPH_EDGES-only edge, written via a new **`context_relate` MCP tool** (Option B). No `entries.depends_on` column. No schema migration.

**Evidence — Informs precedent**: `Informs` was the first net-new agent-facing edge type after crt-021. It writes exclusively to GRAPH_EDGES via `write_graph_edge` in `crates/unimatrix-server/src/services/nli_detection.rs:78–118`. The entries table was not modified. The `AnalyticsWrite::GraphEdge` variant (`analytics.rs:188–196`) provides a fire-and-forget channel. This is the reference pattern.

**Evidence — supersedes dual-source complexity**: The dual-source pattern in `build_typed_relation_graph` (Pass 2a authoritative from `entries.supersedes`, Pass 2b skips GRAPH_EDGES Supersedes rows, `graph.rs:258–296`) exists because `entries.supersedes` predates crt-021. It is a legacy accommodation. Pass 2b explicitly skips GRAPH_EDGES Supersedes rows at `graph.rs:295` to avoid duplicates. Replicating this for `depends_on` would add another special-case in the graph builder with no retrieval benefit.

**Evidence — schema migration cost**: Current schema is v25 (`migration.rs:21`). Adding `entries.depends_on` (Option C) requires migration to v26, a NULL column on all non-decision entries, and a new dual-source skip in `build_typed_relation_graph`.

**Write path option evaluation**:

| Criterion | Option A: context_store param | Option B: context_relate tool | Option C: entries.depends_on |
|---|---|---|---|
| Retrofit existing ADRs | Requires re-issuing content | Single call, no re-issue | Requires re-issuing content |
| Audit/attribution | Implicit | Explicit with rationale field | Implicit |
| Schema migration | None | None | Required (v25→v26) |
| Correction chain | Edge not transferred | Edge not transferred | Field not transferred |
| Informs precedent alignment | Partial | Strong | None |
| Tool count | 12 | 13 | 12 |

**Recommendation**: Option B — `context_relate(source_id, target_id, relation: "Prerequisite", rationale: str)`. Retrofit without re-issuing content is the decisive ergonomic advantage. The `write_graph_edge` helper in `nli_detection.rs:78–118` is the direct implementation reference (validate params → require Write cap → call `write_graph_edge` with `relation_type = "Prerequisite"` → log to audit).

---

### Q3: What happens to dependent decisions when their dependency is deprecated or superseded? Is cascading review notification possible and desirable?

**Answer**: No auto-transfer. A surface-only notification is feasible and desirable. Full edge-transfer rule is risky and not recommended.

**Evidence — correction chain does not touch GRAPH_EDGES**: `correct_entry` in `write_ext.rs:410–602` performs: deprecate original, insert correction with `supersedes=Some(original_id)`, insert tags, insert vector mapping, increment status counters. Zero GRAPH_EDGES insert or update in the entire transaction. Verified exhaustively.

**Evidence — deprecate does not touch GRAPH_EDGES**: `context_deprecate` sets `status=Deprecated` only. The background tick compaction (`background.rs:496`) deletes orphaned edges where endpoints have been removed from the entries table — but deprecated entries remain in the table, so their Prerequisite edges persist indefinitely.

**Consequence of non-transfer**: When ADR-A (#100) is superseded by ADR-A' (#200), Prerequisite edge 100→B remains. Entry #200 has no Prerequisite edge to B. PPR seeded on B surfaces #100 (deprecated, penalized at CLEAN_REPLACEMENT_PENALTY=0.40) but does NOT surface #200 via the Prerequisite channel. The Supersedes chain will surface #200 as the terminal active node via `find_terminal_active`, but not as a dependency relationship.

**Edge transfer analysis**: "When A→A' via Supersedes, copy all Prerequisite edges from A to A'" is risky. If A' meaningfully changes the decision, copied edges may be semantically incorrect. The safer approach is explicit re-assertion by agents.

**Surface-only notification**: Add `stale_dependency_edges` count to `context_status` output — a JOIN query against GRAPH_EDGES and entries: count rows where `relation_type='Prerequisite'` and the source entry's status is Deprecated. This follows the existing graph metrics pattern in `read.rs:1003–1080`. When nonzero, agents are prompted to review and re-assert via `context_relate` on the successor entry.

**Detection rule**: Add a `DependencyOnDeprecated` `DetectionRule` impl in `unimatrix-observe/src/detection/` for `context_cycle_review`. Fires when GRAPH_EDGES contains a Prerequisite edge pointing to or from a recently deprecated entry in the current feature cycle.

**Recommendation**: No auto-transfer. Add `stale_dependency_edges` count to `context_status`. Add `DependencyOnDeprecated` detection rule to `context_cycle_review`. Surface-only, agents re-assert explicitly.

---

### Q4: How does the dependency edge participate in PPR, `graph_expand`, and `context_briefing`? Is the PPR reverse-walk direction correct?

**Answer**: PPR is correct and works with zero changes. graph_expand covers the forward direction only (gap accepted). `context_briefing` works with zero changes. `context_cycle_review` needs one new detection rule.

**Evidence — PPR (alpha=0.85, iterations=20, ppr_max_expand=50)**: With A→B stored (A is prereq of B):
- Seed B → reverse-walk surfaces A. Correct — "show me what this decision depends on."
- PPR test `test_prerequisite_incoming_direction` (`graph_ppr_tests.rs:344–356`) already validates this.
- `positive_out_degree_weight` in `graph_ppr.rs:168–187` already includes `RelationType::Prerequisite` in the denominator normalization.
- Zero engine changes needed. Prerequisite edges feed directly into Phase 5 PPR expansion once written.

**Evidence — graph_expand**: Seed B, edge A→B: outgoing from B finds no Prerequisite edges → A not surfaced (confirmed by direction contract `graph_expand.rs:20–25`). Seed A, edge A→B: outgoing from A finds B → B surfaces. Graph expand covers the forward direction only. PPR compensates for the backward direction. No change needed.

**Evidence — context_briefing**: `context_briefing` (`tools.rs:1084–1161`) delegates to `IndexBriefingService` which runs HNSW + PPR. When Decision B appears in the HNSW result set or is session-seeded, B enters the PPR seed set and A surfaces via reverse-walk. Design-phase briefings for features touching B will automatically pull in A with zero changes.

**Evidence — context_cycle_review**: The `DetectionRule` trait (`unimatrix-observe/src/detection/mod.rs:15`) is the extension point. A new rule checks whether any Prerequisite edge in the current cycle's entries points to a deprecated/superseded source. This is a pure observation — no new tool.

**Expected retrieval improvement**: Once Prerequisite edges exist, any query anchored on a dependent Decision B will retrieve its prerequisite chain via PPR — without relying on co-access patterns that only develop after repeated co-retrieval. Dependency chains are immediately visible from first write, even for ADRs that have never been co-accessed.

**Recommendation**: PPR and context_briefing require zero changes. Accept graph_expand gap. Add one detection rule in observe crate.

---

### Q5: Security and capability model

**Answer**: Write capability is necessary but not sufficient. Add source-entry ownership validation and a confidence floor guard. No Admin requirement.

**Evidence — current Write gate**: `require_cap(&ctx.agent_id, Capability::Write)` at `tools.rs:607`. All enrolled agents receive Write by default in permissive mode (`registry.rs:223`). Any Write-capable agent could call `context_relate(source_id=B_theirs, target_id=A_authoritative)` and store B→A.

**Evidence — PPR spoofing path**: With edge B→A stored (B claims to be prerequisite of A), seeding A in any query causes mass to flow: B accumulates from A's score via B's outgoing edge to A. So when A is seeded, B surfaces. This IS the inflation path: store B→A (B is prereq of A, even if semantically nonsensical), seed A in a query, B surfaces.

**Mitigation 1 — source ownership**: Validate that the calling agent's `agent_id` matches `entries.created_by` for the `source_id` entry. An agent can only assert a dependency FROM entries they created. This prevents agent X from claiming "random-entry B is a prerequisite of authoritative ADR-A" when they did not create B.

**Mitigation 2 — confidence floor on source**: Require `source_entry.confidence >= threshold` (e.g., 0.1) before accepting a Prerequisite edge FROM it. This prevents a zero-confidence throwaway from piggybacking on a high-confidence ADR. Threshold configurable.

Cross-author dependencies (Agent B asserts "my Decision depends on System ADR-A") remain valid — the source ownership constraint only gates the `source_id` direction. The target can be any entry.

**Recommendation**: Write capability gate (existing). Add source ownership validation (calling agent must match `entries.created_by` for source_id). Add confidence floor check on source entry. No Admin requirement for standard dependency links.

---

### Q6: Blast radius assessment

**Answer**: Minimal. ~5 files change, ~7 files benefit for free. No schema migration. Rough effort: 2–3 engineering days.

**Files that must change**:

| File | Change | Approx lines |
|---|---|---|
| `crates/unimatrix-server/src/mcp/tools.rs` | Add `context_relate` tool handler + `RelateParams` struct | ~100 |
| `crates/unimatrix-observe/src/detection/` | Add `DependencyOnDeprecated` detection rule | ~40 |
| `crates/unimatrix-store/src/read.rs` | Add `stale_dependency_edges` count to status query | ~20 |
| `crates/unimatrix-engine/src/graph.rs` | Update comment at line 77 removing "no write path exists in crt-021" | ~2 |
| `crates/unimatrix-server/src/mcp/response/` | Add `context_relate` success formatter | ~20 |

**Files that benefit with zero changes**:

| File | Why it benefits for free |
|---|---|
| `graph_ppr.rs` | Already handles Prerequisite in `positive_out_degree_weight` (line 179) and iteration loop (line 112) |
| `graph_expand.rs` | Already handles Prerequisite in BFS (line 133) |
| `graph.rs` (`build_typed_relation_graph`) | Pass 2b already accepts any valid `RelationType` from GRAPH_EDGES — Prerequisite rows load automatically |
| `nli_detection.rs` | `write_graph_edge` is directly callable by `context_relate` handler |
| `graph_ppr_tests.rs` | `test_prerequisite_incoming_direction` and `test_prerequisite_wrong_direction_does_not_propagate` already validate PPR behavior — no new PPR tests needed |
| `graph_expand_tests.rs` | `test_graph_expand_prerequisite_surfaces_neighbor` already validates forward direction |
| `analytics.rs` | `AnalyticsWrite::GraphEdge` variant already handles Prerequisite writes via the fire-and-forget channel |

**Schema migration**: None. GRAPH_EDGES schema (`migration.rs:338–349`) has `relation_type TEXT NOT NULL` with `UNIQUE(source_id, target_id, relation_type)`. Writing `relation_type='Prerequisite'` is valid immediately. Schema stays at v25.

**Tests to add**:

| Test | Purpose |
|---|---|
| `test_context_relate_requires_write_capability` | Gate enforcement |
| `test_context_relate_source_ownership_enforced` | Anti-spoofing |
| `test_context_relate_writes_prerequisite_edge` | DB persistence |
| `test_context_relate_idempotent` | UNIQUE constraint tolerates re-call |
| `test_stale_dependency_count_nonzero_when_source_deprecated` | Status output |
| `test_dependency_on_deprecated_detection_rule_fires` | Observe crate |

**Estimated effort**: 2–3 engineering days.

---

## Unanswered Questions

None. All six Goal questions answered with direct codebase evidence.

---

## Out-of-Scope Discoveries

1. **graph_expand bidirectionality for Prerequisite**: The gap between PPR (discovers dependency backward) and graph_expand (forward only) could be closed by storing reverse edges at write time, mirroring how CoAccess edges are stored bidirectionally (migration.rs v19→v20 Statement A). Low-effort additive change if graph_expand gap proves significant in practice. Not pursued here.

2. **Automated dependency detection from entry content**: Phase 4b already detects Informs relationships via HNSW cosine + category pair filters without agent assertion. A similar structural detection pass on `decision`-to-`decision` entry pairs with high cosine and shared topic vocabulary could auto-suggest Prerequisite candidates. Flagged as a W3 opportunity in the NLI/structural detection pipeline.

3. **Dependency subgraph for ISO 42001 governance export**: Once Prerequisite edges exist, a simple GRAPH_EDGES JOIN query materializes the decision dependency graph for enterprise audit export. This is the load-bearing data structure for the goal→decision→outcome audit graph described in the enterprise vision. Zero additional engine work once write path exists.

---

## Recommendations Summary

- **Q1 — Semantic fit and direction**: Retain `Prerequisite`. Store A→B (A is prerequisite of B). PPR direction is already correct and tested. Accept graph_expand gap (PPR compensates). No rename or new enum variant.
- **Q2 — Write path**: Implement `context_relate` as a 13th MCP tool. GRAPH_EDGES-only, no entries-table column, no schema migration (stays at v25). Use `write_graph_edge` from `nli_detection.rs` as the direct implementation reference.
- **Q3 — Correction chain**: No edge auto-transfer. Add `stale_dependency_edges` count to `context_status`. Add `DependencyOnDeprecated` detection rule to `context_cycle_review`. Agents re-assert explicitly on successor entries.
- **Q4 — Surfacing**: PPR and context_briefing work with zero engine changes once write path exists. graph_expand covers forward direction only (acceptable). Add one detection rule in observe crate.
- **Q5 — Security**: Write capability is necessary. Add source-entry ownership validation (calling agent must match `created_by` of source_id entry). Add confidence floor on source entry. No Admin requirement.
- **Q6 — Blast radius**: ~5 files change, ~7 benefit for free. No schema migration. 2–3 engineering days.
- **Overall**: Go, Wave 2. Zero retrieval engine changes, no schema migration, existing tests already validate core PPR behavior, write path follows established `write_graph_edge` pattern. Dependency is the only named relationship in the vision not yet modeled. Risk is low, value is high.
