# FINDINGS: ADR Dependency Tracking — `DependsOn` Graph Relationship

**Spike**: ass-055
**Date**: 2026-05-06
**Approach**: investigation
**Confidence**: validated (all answers grounded in codebase evidence with line references)

---

## Findings

### Q: Is `Prerequisite` the correct semantic mapping for `depends_on`, or does the direction convention mismatch require renaming or a new type?

**Answer**: `Prerequisite` is the correct semantic mapping. No rename is needed. The edge must be stored as A→B meaning "A is a prerequisite of B" (B depends on A). This direction is already baked into PPR and graph_expand behavior with passing tests.

**Evidence**: Nygard ADR `depends_on` means "the validity of this decision assumes that [linked decision] holds." `Prerequisite` encodes the same structural constraint: "A is a prerequisite of B" ↔ "B depends on A." The dependency validity coupling is identical. "Prerequisite" is more precise than "DependsOn" for a knowledge graph because it encodes a logical precondition, not a temporal task ordering.

PPR uses reverse-walk (transpose PPR). For edge A→B, seeding B causes mass to flow backward to A because A's outgoing neighbors include B (`graph_ppr.rs:34–39`). Applied to `Prerequisite`: store A→B. Seeding Decision B causes PPR mass to flow back to A — the decision B depends on. This is the desired retrieval behavior.

`test_prerequisite_incoming_direction` (`graph_ppr_tests.rs:344–356`) directly confirms this: edge A→B, seed B, A surfaces. `test_prerequisite_wrong_direction_does_not_propagate` (`graph_ppr_tests.rs:359–377`) confirms seeding A does not surface B.

graph_expand uses outgoing traversal from seed (`graph_expand.rs:123–136`). For edge A→B and seed B, outgoing from B does NOT reach A. The gap is accepted — PPR compensates via Phase 5 reverse-walk. Storing a reverse edge for graph_expand would create semantic ambiguity.

**Direction table**:

| Edge stored as | PPR seed B (dependent) surfaces A (prerequisite)? | graph_expand seed B surfaces A? |
|---|---|---|
| A→B (correct: A is prereq of B) | Yes (reverse walk) | No |
| B→A (wrong) | No | Yes |

**Recommendation**: Retain `Prerequisite`. Store A→B. No rename or new enum variant. Document the graph_expand gap in implementation notes.

---

### Q: Should `depends_on` linkage be stored as a pure graph edge (GRAPH_EDGES only) or as a dual-source field (`entries.depends_on` analogous to `entries.supersedes`)?

**Answer**: Pure GRAPH_EDGES-only edge, with `depends_on: Option<Vec<u64>>` added to both `context_store` and `context_correct`. No new MCP tool. No entries-table column. No schema migration.

**Evidence**: `Informs` was the first net-new agent-facing edge type after crt-021. It writes exclusively to GRAPH_EDGES via `write_graph_edge` in `nli_detection.rs:78–118`. The entries table was not modified. `AnalyticsWrite::GraphEdge` (`analytics.rs:188–196`) provides the fire-and-forget channel. This is the reference implementation pattern for Prerequisite edge writes.

The dual-source pattern in `build_typed_relation_graph` (Pass 2a from `entries.supersedes`, Pass 2b skipping GRAPH_EDGES Supersedes rows at `graph.rs:295`) exists because `entries.supersedes` predates crt-021. It is a legacy accommodation with no retrieval benefit to replicate. Option C (entries column) would require migration to v26, a NULL column on all non-decision entries, and a new dual-source skip.

**Write path evaluation**:

| Criterion | Option A+: context_store + context_correct params | Option B: context_relate tool | Option C: entries.depends_on |
|---|---|---|---|
| New entries | One call, dependency co-located with definition | Separate call required | One call (with migration) |
| Retrofit existing ADRs | context_correct creates new version | Single post-hoc call | Requires re-issuing content |
| Audit/attribution | Attributed to issuing agent via correction chain | Explicit separate attribution | Implicit |
| Schema migration | None | None | Required (v25→v26) |
| Correction chain | Edge not transferred; version bump is earned (acknowledging dependency IS a semantic update) | Edge not transferred | Field not transferred |
| Informs precedent alignment | Partial | Strong | None |
| Tool count | 12 (no new tool) | 13 | 12 |

The retrofit path creates a new entry version even when textual content is unchanged — but declaring a formal dependency is a meaningful semantic update to the entry's knowledge content, making the version bump defensible. Dependency declaration co-located with entry definition is ergonomically cleaner for agents.

**Recommendation**: Add `depends_on: Option<Vec<u64>>` to both `context_store` and `context_correct`. No new MCP tool. GRAPH_EDGES-only writes via `write_graph_edge` in `nli_detection.rs:78–118`. Schema stays at v25.

---

### Q: What happens to dependent decisions when their dependency is deprecated or superseded? Is cascading review notification possible and desirable?

**Answer**: No auto-transfer. A surface-only notification is feasible and desirable. Full edge-transfer rule is risky and not recommended.

**Evidence**: `correct_entry` in `write_ext.rs:410–602` performs zero GRAPH_EDGES insert or update in its entire transaction — verified exhaustively. `context_deprecate` sets `status=Deprecated` only. Background tick compaction (`background.rs:496`) deletes orphaned edges where endpoints are removed from entries, but deprecated entries remain in the table, so Prerequisite edges persist indefinitely after deprecation.

Consequence of non-transfer: when ADR-A (#100) is superseded by ADR-A' (#200), Prerequisite edge 100→B remains. PPR seeded on B surfaces #100 (deprecated, penalized at CLEAN_REPLACEMENT_PENALTY=0.40) but does NOT surface #200 via the Prerequisite channel.

Edge auto-transfer is risky: if A' meaningfully changes the decision, copied edges may be semantically incorrect.

Surface-only notification: add `stale_dependency_edges` count to `context_status` — a JOIN against GRAPH_EDGES and entries counting rows where `relation_type='Prerequisite'` and source entry status is Deprecated. Follows the existing graph metrics pattern in `read.rs:1003–1080`. Add a `DependencyOnDeprecated` `DetectionRule` impl in `unimatrix-observe/src/detection/` for `context_cycle_review`.

**Recommendation**: No edge auto-transfer. Add `stale_dependency_edges` count to `context_status`. Add `DependencyOnDeprecated` detection rule to `context_cycle_review`. Agents re-assert explicitly on successor entries.

---

### Q: How does the dependency edge participate in PPR, `graph_expand`, and `context_briefing`? Is the PPR reverse-walk direction correct for this relationship?

**Answer**: PPR is correct and works with zero changes. graph_expand covers the forward direction only (gap accepted). `context_briefing` works with zero changes. `context_cycle_review` needs one new detection rule.

**Evidence**:

PPR (alpha=0.85, iterations=20, ppr_max_expand=50): `positive_out_degree_weight` in `graph_ppr.rs:168–187` already includes `RelationType::Prerequisite` in the denominator normalization. Zero engine changes needed.

graph_expand: Confirmed by `graph_expand.rs:20–25` — seed B, edge A→B, outgoing from B finds no Prerequisite edges, A not surfaced. PPR compensates for the backward direction.

`context_briefing` (`tools.rs:1084–1161`) delegates to `IndexBriefingService` running HNSW + PPR. When Decision B appears in the HNSW result set or is session-seeded, B enters the PPR seed set and A surfaces via reverse-walk. Design-phase briefings for features touching B automatically pull in A with zero changes.

`context_cycle_review`: The `DetectionRule` trait (`unimatrix-observe/src/detection/mod.rs:15`) is the extension point. A new rule checks whether any Prerequisite edge in the current cycle's entries points to a deprecated/superseded source.

**Expected retrieval improvement**: Dependency chains become immediately visible from first write, without waiting for co-access patterns to develop through repeated co-retrieval.

**Recommendation**: PPR and `context_briefing` require zero changes. Accept graph_expand gap. Add one detection rule in observe crate.

---

### Q: What capability is required to write a `Prerequisite` edge? Is there a PPR spoofing risk?

**Answer**: Write capability is necessary but not sufficient. Add source-entry ownership validation and a confidence floor guard. No Admin requirement.

**Evidence**: Current Write gate at `tools.rs:607`. All enrolled agents receive Write by default in permissive mode (`registry.rs:223`). Any Write-capable agent could store B→A with edge direction B claiming to be a prerequisite of authoritative ADR-A — PPR seeded on A then surfaces B, inflating B's apparent relevance.

Mitigation 1 — source ownership: validate that the calling agent's `agent_id` matches `entries.created_by` for the `source_id` entry. An agent can only assert a dependency FROM entries they created.

Mitigation 2 — confidence floor on source: require `source_entry.confidence >= threshold` (e.g., 0.1) before accepting a Prerequisite edge. Prevents zero-confidence throwaways from piggybacking on high-confidence ADRs. Threshold configurable.

Cross-author dependencies (Agent B asserts "my Decision depends on System ADR-A") remain valid — source ownership only gates the source_id direction. The target can be any entry.

**Recommendation**: Write capability gate (existing). Add source ownership validation. Add confidence floor check on source entry. No Admin requirement.

---

### Q: What is the blast radius? Files changed, files benefiting for free, schema migration required, estimated effort?

**Answer**: Minimal. Five files change, seven benefit for free. No schema migration. Rough effort: 2–3 engineering days.

**Files that must change**:

| File | Change | Approx lines |
|---|---|---|
| `crates/unimatrix-server/src/mcp/tools.rs` | Add `depends_on` param to `context_store` and `context_correct` handlers; write Prerequisite edges after entry write | ~60 |
| `crates/unimatrix-observe/src/detection/` | Add `DependencyOnDeprecated` detection rule | ~40 |
| `crates/unimatrix-store/src/read.rs` | Add `stale_dependency_edges` count to status query | ~20 |
| `crates/unimatrix-engine/src/graph.rs` | Remove "no write path exists in crt-021" comment at line 77 | ~2 |

**Files that benefit with zero changes**:

| File | Why |
|---|---|
| `graph_ppr.rs` | Already handles Prerequisite in `positive_out_degree_weight` (line 179) and iteration loop (line 112) |
| `graph_expand.rs` | Already handles Prerequisite in BFS (line 133) |
| `graph.rs` (`build_typed_relation_graph`) | Pass 2b accepts any valid `RelationType` from GRAPH_EDGES — Prerequisite rows load automatically |
| `nli_detection.rs` | `write_graph_edge` is directly callable by `context_relate` handler |
| `graph_ppr_tests.rs` | `test_prerequisite_incoming_direction` and `test_prerequisite_wrong_direction_does_not_propagate` already validate PPR behavior |
| `graph_expand_tests.rs` | `test_graph_expand_prerequisite_surfaces_neighbor` already validates forward direction |
| `analytics.rs` | `AnalyticsWrite::GraphEdge` variant already handles Prerequisite writes via fire-and-forget channel |

**Schema migration**: None. GRAPH_EDGES schema (`migration.rs:338–349`) has `relation_type TEXT NOT NULL`. Writing `relation_type='Prerequisite'` is valid immediately. Schema stays at v25.

**Tests to add**:

| Test | Purpose |
|---|---|
| `test_context_relate_requires_write_capability` | Gate enforcement |
| `test_context_relate_source_ownership_enforced` | Anti-spoofing |
| `test_context_relate_writes_prerequisite_edge` | DB persistence |
| `test_context_relate_idempotent` | UNIQUE constraint tolerates re-call |
| `test_stale_dependency_count_nonzero_when_source_deprecated` | Status output |
| `test_dependency_on_deprecated_detection_rule_fires` | Observe crate |

**Recommendation**: Proceed. Five files, six new tests, no schema migration. Implementation reference is `write_graph_edge` in `nli_detection.rs:78–118`.

---

## Unanswered Questions

None. All six Goal questions answered with direct codebase evidence.

---

## Out-of-Scope Discoveries

1. **graph_expand bidirectionality for Prerequisite**: The gap between PPR (discovers dependency backward) and graph_expand (forward only) could be closed by storing reverse edges at write time, mirroring how CoAccess edges are stored bidirectionally (migration.rs v19→v20 Statement A). Low-effort additive change if the gap proves significant in practice. Not pursued here.

2. **Automated dependency detection from entry content**: Phase 4b already detects Informs relationships via HNSW cosine + category pair filters without agent assertion. A similar structural detection pass on `decision`-to-`decision` entry pairs with high cosine and shared topic vocabulary could auto-suggest Prerequisite candidates. W3 opportunity in the NLI/structural detection pipeline.

3. **Dependency subgraph for ISO 42001 governance export**: Once Prerequisite edges exist, a simple GRAPH_EDGES JOIN query materializes the decision dependency graph for enterprise audit export. This is the load-bearing data structure for the goal→decision→outcome audit graph described in the enterprise vision. Zero additional engine work once the write path exists.

---

## Recommendations Summary

- **Q1 — Semantic fit and direction**: Retain `Prerequisite`. Store A→B (A is prerequisite of B). PPR direction is already correct and tested. Accept graph_expand gap (PPR compensates). No rename or new enum variant.
- **Q2 — Write path**: Add `depends_on: Option<Vec<u64>>` to `context_store` and `context_correct`. No new MCP tool (stays at 12). GRAPH_EDGES-only, no schema migration (stays at v25). Use `write_graph_edge` from `nli_detection.rs:78–118` as the implementation reference.
- **Q3 — Correction chain**: No edge auto-transfer. Add `stale_dependency_edges` count to `context_status`. Add `DependencyOnDeprecated` detection rule to `context_cycle_review`. Agents re-assert explicitly on successor entries.
- **Q4 — Surfacing**: PPR and `context_briefing` work with zero engine changes once write path exists. graph_expand covers forward direction only (acceptable). Add one detection rule in observe crate.
- **Q5 — Security**: Write capability required. Add source-entry ownership validation (calling agent must match `created_by` of source_id). Add confidence floor on source entry. No Admin requirement.
- **Q6 — Blast radius**: Five files change, seven benefit for free. No schema migration. 2–3 engineering days.
- **Overall**: **Go, Wave 2**. Zero retrieval engine changes, no schema migration, existing tests already validate core PPR behavior, write path follows the established `write_graph_edge` pattern. `Prerequisite` is the only named relationship in the vision not yet modeled. Risk is low, value is high.
