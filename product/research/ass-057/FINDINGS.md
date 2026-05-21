# FINDINGS: ASS-057 — Unimatrix as a Research Domain Substrate: Fit Analysis and Value-Add Assessment

**Spike**: ass-057
**Date**: 2026-05-14
**Approach**: investigation (synthesis of four parallel tracks A/B/C/D)
**Confidence**: validated (all findings grounded in codebase evidence with file/line references)

---

## Fitness Verdict (Lead)

**ALIGNED WITH EXTENSIONS**

The research domain can be served with 15 new RelationType variants (~60 lines of code), one consolidated `context_graph` MCP tool, a unified edge write path on existing tools, and zero schema changes. The vector-first vs. graph-first inversion is not an architectural conflict — the two retrieval paradigms are orthogonal code paths sharing storage.

**Scope update (ASS-055 integration + SDLC co-delivery)**: This work is not research-domain-only. Six of the new edge types have direct SDLC semantic value (`Advances`, `Motivates`, `Refutes`, `About`, `DerivedFrom`, plus `Prerequisite`/depends_on whose write path was designed in ASS-055). The graph traversal tools are consolidated into a single `context_graph` tool to prevent tool sprawl. The edge write mechanism is a unified `edges` parameter on existing `context_store` and `context_correct` tools — no new write tool. Full rationale in Sections 2b and 5.

---

## 1. Entity Category Mapping Table

All 8 required research categories (plus optional Insight) are expressible using the existing config-driven category model with zero code changes.

**How the model works**: `[knowledge] categories` in `config.toml` fully controls the allowlist. `CategoryAllowlist` enforces it at every `context_store` call. The 5 INITIAL_CATEGORIES are defaults that operators replace entirely. `boosted_categories` and `adaptive_categories` are subsets — no hardcoded category references anywhere in the scoring pipeline.

### Mapping table

| Required category | `config.toml` name | What goes in `content` | Schema gaps |
|---|---|---|---|
| **Goal** | `goal` | Goal statement, scope, success criteria. `topic` field serves as goal domain. | None |
| **Source** | `source` | Stub: title, URL/DOI, publication metadata, 1–2 sentence abstract. Substance in Reader/cache. | None |
| **Finding** | `finding` | Researcher's triage interpretation of one Source. Full prose. | None |
| **Claim** | `claim` | Single atomic proposition, 1–3 sentences, self-contained and falsifiable. | None |
| **Entity** | `entity` | Canonical name, aliases, brief description. `tags` carry type signal (`["tool", "paper", "person"]`). | None |
| **Thesis** | `thesis` | Proposition text. Status lifecycle requires workaround — see below. | **Status lifecycle gap** |
| **POC** | `poc` | Stub: description, git branch pointer, hypothesis. Substance lives on branch. | None |
| **Deliverable** | `deliverable` | Stub: description, filesystem/URL pointer, format. Substance lives on disk. | None |
| **Insight** (optional) | `insight` | Cross-item synthesis prose. | None |

**Stub-with-pointer pattern**: Convention, not schema requirement. `EntryRecord.content` is `String` — accepts URLs, file paths, abstracts. A short stub produces a meaningful vector for semantic retrieval. No schema changes needed.

**Thesis status lifecycle gap** (the one meaningful schema gap): The research domain requires `proposed → supported → refuted → abandoned`. Unimatrix has `Active(0)`, `Deprecated(1)`, `Proposed(2)`, `Quarantined(3)`. `Proposed` maps to proposed; `Active` is a weak proxy for supported; `Deprecated` carries semantic distortion for refuted/abandoned; `refuted` has no Status equivalent at all.

Critical structural finding: `EntryRecord` has no `metadata` field (confirmed by exhaustive inspection of `schema.rs:48-102`). The workaround is storing thesis status in `tags` using a namespaced tag (e.g., `"thesis-status:refuted"`) — searchable via `context_lookup` tags filter, but unindexed and typo-prone. The clean resolution is a `metadata: Option<serde_json::Value>` column on `entries` — an architectural decision, not an MVI blocker.

### Schema gaps summary

| Property needed | Present? | Gap severity |
|---|---|---|
| title, content, category, tags, status, version chain, created_by, confidence, feature_cycle | Yes | None to marginal |
| Research-domain thesis status (proposed/supported/refuted/abandoned) | No | **Gap**: tags convention is workaround; metadata column migration resolves cleanly |
| URL/pointer field | Covered by content convention | None |

---

## 2. Edge Type Gap Table

**GRAPH_EDGES schema** (confirmed `migration.rs:340-352`): columns include `source_id`, `target_id`, `relation_type TEXT`, `weight REAL DEFAULT 1.0`, `created_at`, `created_by`, `source`, `bootstrap_only`, `metadata TEXT`. UNIQUE constraint: `(source_id, target_id, relation_type)`. Indexes: `idx_graph_edges_source_id`, `idx_graph_edges_target_id`, `idx_graph_edges_relation_type` (all single-column). No composite indexes.

**Current RelationType** (6 variants): `Supersedes`, `Contradicts`, `Supports`, `CoAccess`, `Prerequisite`, `Informs`.

**Extension cost per new variant**: ~4 lines in `graph.rs`: enum body, `as_str()`, `from_str()`. Missing `from_str()` arm causes R-10 guard in `build_typed_relation_graph` to silently drop rows — critical update site. No schema migration.

**PPR behavior**: Reverse-walk transpose PPR. For edge A→B, seeding B causes mass to flow back to A. Current positive types: Supports, CoAccess, Prerequisite, Informs (hardcoded `graph_ppr.rs:168-187`). Contradicts and Supersedes excluded.

**graph_expand behavior**: BFS over Direction::Outgoing from seed set. Same 4 positive types (hardcoded `graph_expand.rs:62`). Supersedes and Contradicts excluded.

### Gap table (14 edges)

| Edge | Direction | Current RelationType | Gap | PPR behavior | Edge properties | Metadata JSON verdict |
|---|---|---|---|---|---|---|
| `advances` | Finding/Thesis/POC/Insight/Deliverable → Goal *(research)*; Feature/Pattern/Decision → Goal *(SDLC)* | **None — new** | New `Advances` variant | Seed=Goal: PPR reverse-walks to all advancing entries. Highly desirable. Add `Advances` to PPR positive types (~8 lines). | `contribution_kind` | Display only. **Metadata sufficient.** |
| `cites` | Finding → Source | **None — new** | New `Cites` variant | **Exclude from PPR** — Source stubs add noise. | None | None needed. |
| `asserts` | Finding → Claim | **None — new** | New `Asserts` variant | Seed=Claim: PPR surfaces Finding. Desirable. | None | None needed. |
| `mentions` | Finding/Thesis/Claim/Insight/Goal → Entity | **None — new** | New `Mentions` variant | Seed=Entity: PPR reverse-walks to all mentioning entries. Highly desirable for Q4. | `count`, `salience` | Display only. **Metadata sufficient.** |
| `supports` | Claim → Thesis | **Supports — reuse** | None | In PPR positive types. Seed=Thesis: PPR surfaces Claims. Desirable. | `strength` | **Map to `weight` column directly** — zero JSON parsing. |
| `refutes` | Claim → Thesis *(research)*; Lesson → Pattern *(SDLC — "this lesson shows this pattern fails")* | **None — new** | New `Refutes` variant | Add to PPR positive types (OQ-A-2). Seed=Thesis/Pattern: surfaces refuting Claims/Lessons. | `strength` | **Map to `weight` column directly**. |
| `contradicts` | Claim ↔ Claim (symmetric) | **Contradicts — reuse** | **Symmetry storage gap — see below.** Currently excluded from PPR; add for research domain (~8 lines). | `confidence` (float/NULL), `human_confirmed` (bool) | **Metadata sufficient.** `confidence` alternatively to `weight`. |
| `depends_on` | Thesis → Thesis *(research)*; Decision → Decision *(SDLC — ADR dependency, "B's validity assumes A holds")* | **Prerequisite — reuse** | **Write path not yet implemented.** ASS-055 fully designed this: direction A→B correct for PPR; zero engine changes needed; write path is `edges` parameter on context_store/context_correct. See Section 2b. | In PPR positive types. Seed=B: PPR surfaces A. Desirable. | None | None needed. |
| `tests` | POC → Thesis | **None — new** | New `Tests` variant | Add to PPR positive types. Seed=Thesis: surfaces testing POCs. | None | None needed. |
| `derived_from` | Insight → Finding/Claim/Thesis *(research)*; Pattern → Feature *(SDLC — "this pattern originated from these features")* | **None — new** | New `DerivedFrom` variant | Seed=Insight/Pattern: PPR surfaces derivation sources. | None | None needed. |
| `motivates` | Insight → Thesis *(research)*; Lesson → Decision *(SDLC — "this lesson is why this ADR was written")* | **None — new** | New `Motivates` variant | Add to PPR positive types (~8 lines). Seed=Thesis/Decision: surfaces motivating Insights/Lessons. | None | None needed. |
| `about` | Thesis → Entity *(research)*; Decision/Pattern → Concept *(SDLC — "this ADR governs this technology")* | **None — new** | New `About` variant | Seed=Entity/Concept: PPR surfaces all Theses/Decisions about it. Desirable for Q4. | None | None needed. |
| `supersedes` | * → * | **Supersedes — reuse** | GRAPH_EDGES Supersedes rows skipped in Pass 2b (`graph.rs:295`); in-memory graph derives topology from `entries.supersedes` only. `revision_reason` in edge metadata accessible via direct SQL only (OQ-5). | Excluded from PPR. Handled by dedicated supersession functions. | `revision_reason` | Display only. **Metadata sufficient** — but inaccessible via graph traversal layer. |
| `related_to` | * → * (soft fallback) | **None — new** | New `RelatedTo` variant | Include in PPR as weak signal (analogous to Informs). Store bidirectionally for symmetric use. | `note` | Display only. **Metadata sufficient.** |

**Summary**: 4 of 14 research edges reuse existing variants. 10 new research variants required. Additionally, 5 existing research edge types have direct SDLC semantic value (`Advances`, `Motivates`, `Refutes`, `About`, `DerivedFrom`) — no additional variants needed, same enum entries serve both domains. Total new enum variants: 10. No schema migration. ~40 lines total in `graph.rs`.

### `contradicts` symmetry resolution (explicit)

**Resolution: store bidirectionally (Option a).** Store `(A, B, Contradicts)` and `(B, A, Contradicts)` atomically. UNIQUE constraint treats `(A,B)` and `(B,A)` as distinct tuples — both allowed. `INSERT OR IGNORE` on re-assertion is idempotent per direction.

**Rationale**: Option b (OR clause) prevents index use and adds permanent query complexity. Option c (canonical ordering) has the same query disadvantage with an additional write-time invariant burden. Bidirectional storage is the established Unimatrix pattern — CoAccess is stored bidirectionally (`migration.rs:632-665`); Informs likewise (`migration.rs:710-759`).

**Action on existing call sites**: The existing `query_contradicts_edges_for_entry` (`read.rs:1529-1532`) uses `WHERE target_id = ?1` — asymmetric, misses edges FROM an entry. Once both directions are stored, update call sites to use `WHERE source_id = ?1` and simplify.

---

## 2b. SDLC Edge Set and Unified Write Path (ASS-055 Integration)

### Why this is in scope

ASS-057 was scoped as a research domain spike. However, six of the new RelationType variants have direct SDLC semantic value — they are not research-specific. Delivering the graph capability without the SDLC write paths would leave the most strategically important edges (Goal traceability, ADR dependency, reasoning chains) permanently out of reach for the home domain. ASS-055 (2026-05-06) fully designed the write path for `depends_on`/Prerequisite. This section integrates that design into the combined scope.

### SDLC-specific edge set requiring write paths

| Edge | SDLC direction | SDLC semantic | Existing enum variant? |
|---|---|---|---|
| `depends_on` | Decision → Decision | "B's validity assumes A holds" (Nygard ADR dependency) | Yes — `Prerequisite` |
| `advances` | Feature/Pattern/Decision → Goal | "This feature advances this strategic goal" | No — `Advances` (new, shared with research) |
| `motivates` | Lesson → Decision | "This lesson is why this ADR was written" | No — `Motivates` (new, shared with research) |
| `refutes` | Lesson → Pattern | "This lesson shows this pattern fails" | No — `Refutes` (new, shared with research) |
| `about` | Decision/Pattern → Concept | "This ADR governs this technology/concept" | No — `About` (new, shared with research) |
| `derived_from` | Pattern → Feature | "This pattern originated from these features" | No — `DerivedFrom` (new, shared with research) |

The five new variants (`Advances`, `Motivates`, `Refutes`, `About`, `DerivedFrom`) are shared between SDLC and research. No SDLC-exclusive variants are needed — the research domain edge taxonomy covers the SDLC cases through semantic reuse.

### Unified write path: `edges` parameter on existing tools

**ASS-055 designed** a `depends_on: Option<Vec<u64>>` parameter on `context_store` and `context_correct` for the Prerequisite write path (no new tool, no schema migration, 2–3 days). The generalization of that design to all typed edges:

```
context_store(
  title, content, category, topic, tags, ...
  edges: [
    { type: "depends_on",  target_id: 123 },
    { type: "advances",    target_id: 456 },
    { type: "motivates",   target_id: 789 },
  ]
)
```

Same mechanism on `context_correct` — declaring relationships is a meaningful semantic update to the entry, making the version bump defensible (per ASS-055 Q2 rationale).

**Security model** (from ASS-055 Q5, fully validated):
- Existing `Capability::Write` gate (already enforced)
- Source ownership validation: calling agent's `agent_id` must match `created_by` of the source entry. An agent can only assert edges FROM entries they created. Cross-author targets are valid — only source ownership is gated.
- Confidence floor on source: `source_entry.confidence >= threshold` (configurable, e.g., 0.1). Prevents zero-confidence throwaways from piggybacking on high-confidence ADRs via PPR mass flow.

**Write mechanics**: Each `edges` element calls `write_graph_edge` (the `nli_detection.rs:78–118` reference implementation). GRAPH_EDGES-only storage. `AnalyticsWrite::GraphEdge` fire-and-forget channel — no schema migration.

**Tool count impact**: Zero. `edges` is a new parameter on `context_store` and `context_correct`. No new tools required for the write path.

### SDLC notification surfaces (ASS-055 Q3)

Two additions to the existing monitoring infrastructure:

**`stale_dependency_edges` in `context_status`**: A JOIN on GRAPH_EDGES + entries counting rows where `relation_type='Prerequisite'` and the source entry status is `Deprecated`. Follows the existing graph metrics pattern in `read.rs:1003–1080`. Surfaces "N decisions depend on a deprecated entry — review recommended." ~20 lines.

**`DependencyOnDeprecated` detection rule in `context_cycle_review`**: A `DetectionRule` impl in `unimatrix-observe/src/detection/` that fires when any Prerequisite/depends_on edge in the current cycle's entries points to a deprecated or superseded source entry. ~40 lines.

No edge auto-transfer when a dependency target is superseded — ASS-055 validated this as risky (if A' meaningfully changes the decision, copied edges may be semantically incorrect). Agents re-assert explicitly on the successor entry.

### Enterprise audit graph (ASS-055 OSD-3)

Once `depends_on` (Prerequisite) edges exist and `context_graph` provides traversal:

```
context_graph(mode="subgraph", seed_ids=[goal-id], edge_types=["advances", "depends_on"], max_depth=4)
→ goal → advancing features/decisions → decision dependency chains
```

This materializes the `goal → decision → outcome` audit graph described in the enterprise vision. ASS-055 explicitly flagged this: "the load-bearing data structure for the ISO 42001 governance audit graph." Zero additional engine work beyond the write path and traversal tool.

### ASS-055 blast radius (confirmed)

| File | Change | Lines |
|---|---|---|
| `tools.rs` | Add `edges` parameter to `context_store` and `context_correct` handlers | ~60 |
| `unimatrix-observe/src/detection/` | Add `DependencyOnDeprecated` detection rule | ~40 |
| `read.rs` | Add `stale_dependency_edges` count to status query | ~20 |
| `graph.rs` | Remove "no write path exists" comment at line 77 | ~2 |

Seven files benefit with zero changes (PPR, graph_expand, build_typed_relation_graph, write_graph_edge, analytics, and existing PPR tests — all already handle `Prerequisite`).

**Effort**: 2–3 engineering days for the core write path. The detection rule and status surface add 0.5 days each.

---

## 3. Traversal API Feasibility Table

### Tool consolidation: `context_graph`

The eight traversal APIs identified in ASS-057 scope are consolidated into a **single `context_graph` MCP tool** with a `mode` parameter. This is the correct decomposition by retrieval intent — graph navigation is one intent, not eight. Precedent: `context_cycle` uses `type="start"|"stop"|"phase"` for the same reason.

```
context_graph(mode="neighbors",  id, edge_types, direction, depth, resolve_supersessions)
context_graph(mode="subgraph",   seed_ids, edge_types, direction, max_depth, max_nodes, resolve_supersessions)
context_graph(mode="inverse",    category, missing_edge_types, limit)
context_graph(mode="chain",      id, direction)          — supersession chain
context_graph(mode="current",    id)                     — follow superseded_by to terminal
context_graph(mode="path",       from_id, to_id, edge_types, max_depth)
context_graph(mode="filter",     category, where, edge_filters, limit)
```

**Net tool count impact**: +1 tool (context_graph). Total tools after this work: 13. `context_batch_write` is out of recommended scope — see OSC-6.

All modes are implemented under a single `context_graph` tool (see Section 2b for consolidation rationale).

| Mode | Classification | Storage change needed | resolve_supersessions impact | Key notes |
|---|---|---|---|---|
| `neighbors` | context_graph, existing storage | None | Store-layer helper ~20 lines | New BFS variant: caller-supplied edge types + bidirectional traversal |
| `subgraph` | context_graph, in-memory BFS | None for topology; add `metadata: Option<String>` to RelationEdge for edge properties (trivial) | Per-hop deprecated-node substitution | Returns `(Vec<EntryRecord>, Vec<EdgeRecord>)`. 200-node cap. ~290KB JSON at max. |
| `inverse` | context_graph, SQL antijoin | None at 3k; composite `(target_id, relation_type)` index strongly recommended | N/A | 1–3ms at 3k without composite index |
| `chain` | context_graph, recursive CTE on entries fields | None; add index on `entries.supersedes`/`superseded_by` in same migration | This IS the supersession query | `find_terminal_active` exists in graph.rs but is not MCP-exposed and returns only terminal |
| `current` | context_graph, single recursive CTE | None | This IS the supersession query | ~30 lines wrapping recursive CTE |
| `path` | context_graph, petgraph BFS in-memory | None | Per-hop deprecated-node substitution | petgraph algo already linked (`graph.rs:21`); `node_index: HashMap<u64, NodeIndex>` provides O(1) lookup; sub-ms at 3k nodes/10k edges |
| `filter` | context_graph, correlated SQL subquery | None at 3k; composite `(source_id, relation_type)` index recommended | N/A | ~4,500 operations at 3k entries; well under 10ms |

### Q9 antijoin query plan

```sql
SELECT e.id, e.title, e.topic, e.confidence, e.created_at
FROM entries e
LEFT JOIN graph_edges g
    ON e.id = g.target_id AND g.relation_type = 'cites'
WHERE e.category = 'source' AND e.status = 0 AND g.target_id IS NULL
LIMIT 100;
```

Without composite index: outer scan bounded by `idx_entries_category`. 300 Source rows × ~17 operations = ~5,100 total. **Estimated latency: 1–3ms.** Bounded index scan, not a full table scan. With composite `(target_id, relation_type)` index: sub-millisecond. Application-side filtering via N+1 MCP calls is not viable (301 MCP calls vs. 1 SQL query).

---

## 4. Supersession Semantics Gap

**Current state**: Supersession stored on entries table as `entries.supersedes: Option<u64>` and `entries.superseded_by: Option<u64>` (`schema.rs:67-69`). `context_correct` does NOT write a GRAPH_EDGES Supersedes row (confirmed: `store_correct.rs` — zero GRAPH_EDGES INSERTs; `write_ext.rs:400-602` — 8 steps, no GRAPH_EDGES write). In-memory graph derives Supersedes topology from `entries.supersedes` in Pass 2a, explicitly skipping GRAPH_EDGES Supersedes rows in Pass 2b (`graph.rs:294-296`).

Current traversal: no `resolve_supersessions` parameter on any tool. `context_lookup` defaults `WHERE status = 0`. No multi-hop supersession chain is MCP-queryable today.

**What resolve_supersessions=true requires**: Store-layer helper function (~20 lines): at each traversal hop, if `entry.status == Deprecated` and `resolve_supersessions == true`, follow `entry.superseded_by` iteratively (safety cap 50) until non-deprecated terminal found, substitute terminal for hop destination. Threading into 3 tools: ~30 lines per tool.

**resolve_supersessions=false (audit mode)**: Return edges as stored including deprecated endpoints. This is the natural default without the parameter — zero additional work.

**Concrete cost**:
- Storage: no schema change. `entries.superseded_by` already exists. No index (confirmed `db.rs:572-583`). Add `idx_entries_superseded_by` in traversal migration.
- Code: Store-layer helper + threading into traversal tools. **Total: 1–2 engineering days once traversal tools exist.**
- Recommendation: implement from day one, not as later addition. Retrofit cost exceeds build-in cost.

**Supersession chain CTE**: SQLite recursive CTE on `entries.supersedes`/`entries.superseded_by` is correct and sufficient. Safety cap at 50 hops prevents infinite loops. Chains are short in practice (< 10).

---

## 5. Architecture Fitness Verdict

### ALIGNED WITH EXTENSIONS

The research domain use case is "Unimatrix configured for a domain, extended with graph traversal tools" — not "a graph database with semantic search bolted on." The gap is tool surface area. The verdict is unambiguous.

### Vector-first vs. graph-first inversion — explicit address

The inversion is real as a product positioning question but is not an architectural conflict.

`context_search` is unconditionally vector-first: embedding computation always runs, HNSW search always runs, all downstream PPR/expansion phases depend on the HNSW seed set (`search.rs:551-666` — no bypass flag). The write path is equally mandatory about embeddings (`store_ops.rs:121`). These facts are correct.

However, graph traversal tools and vector-first search are **orthogonal code paths sharing storage**. New MCP tools (`context_neighbors`, `context_subgraph`, `context_path`, etc.) read from GRAPH_EDGES or the in-memory `TypedRelationGraph` directly, without touching the embedding adapter or HNSW index. `build_typed_relation_graph` has no vector dependency. `graph_expand` and `personalized_pagerank` are pure functions on `&TypedRelationGraph`. A research workflow can use only graph traversal tools and never call `context_search` — this is architecturally valid and fully supported.

**The thin-shell scenario**:
- Embeddings computed at write time for stub entries (no `no_embed` flag — hardcoded `store_ops.rs:121`). URL stubs get noise vectors. Degrades HNSW quality on the same instance but does not break graph traversal.
- Confidence scoring with zero helpfulness votes: Bayesian prior `(3.0, 3.0)` returns 0.5 neutral — graceful degradation, no crash (`confidence.rs:224`).
- PPR never fires unless `context_search` is called. No negative side effect.

**Five rationale factors**:
1. Category taxonomy is fully config-driven today — all 9 research categories, boosted_categories, adaptive_categories, informs_category_pairs all configurable.
2. Graph infrastructure handles core requirements — GRAPH_EDGES is string-typed for relation_type (no migration for new edge types); in-memory TypedRelationGraph is available for synchronous BFS; write_graph_edge provides the write path.
3. Critical gap is tools, not architecture — 12 required traversal patterns (Q1–Q12) require 6–8 new MCP tools; these are pure graph operations backed by existing storage.
4. Confidence scoring degrades gracefully — no crash, no invalid state.
5. Intelligence pipeline adapts with config — S2 Informs, Goal-conditioned learning, NLI on/off all configurable today.

### Hardcoded items requiring code (not config)

| Hardcoded behavior | Severity | Fix |
|---|---|---|
| Embedding mandatory at write time (`store_ops.rs:121`) | Manageable — stubs get noise vectors; graph traversal unaffected | Optional `no_embed_categories` flag (2–3 days; not MVI) |
| HNSW mandatory in context_search | Non-issue — graph tools bypass context_search entirely | No action needed for MVI |
| PPR positive edge types (4, hardcoded `graph_ppr.rs:168-187`) | Manageable — ~8 lines per new type | Add Advances, Motivates (~16 lines) |
| NLI scans all categories (no contradicts_category_pairs) | Manageable with code | Add `contradicts_category_pairs` config field (~20 lines) |
| context_cycle anchored to feature_cycle string | Design-level | Goal ID as pseudo-feature-cycle string is the working workaround; cycle_anchor_category is a future config extension |

---

## 6. Value-Add Opportunity Table

| Capability | Requirement Assumption | Current State | Research Fit | Workflow Gain | Extension Needed | Verdict |
|---|---|---|---|---|---|---|
| **NLI Contradiction Detection on Claims** | Model emits Contradicts edges; Unimatrix stores | Infrastructure exists (W1-4); C-13/AC-10a gate hard-discards contradiction scores at `nli_detection_tick.rs:716`; heuristic scan writes cache only, not GRAPH_EDGES; **zero Contradicts edges ever written in production** | **Strong** — Claims are propositional SNLI-format with genuine factual contradictions common in empirical CS research | Automatic Contradicts edges as Claims are stored; Q5 traversal works without model re-querying full corpus | ~3–5 call site changes: add category filter, restore `write_nli_edge("Contradicts")` call, remove C-13 for `claim` category only | **High** |
| **Evidence-Driven Confidence Scoring on Theses** | Manual status lifecycle | Six-factor composite in `confidence.rs`; no edge-count inputs; formula takes only `EntryRecord` | **Strong** — supports/refutes edge ratio is well-defined Bayesian evidence; helpfulness formula directly repurposable | Automatic thesis confidence tracking replacing manual status management; quantitative signal for promotion/abandonment | 1 store query per Thesis at maintenance tick; 1 new weight field in ConfidenceParams | **High** |
| **S2 Structural Vocabulary Informs Edges** | All edges declared explicitly | Fully implemented; domain-neutral; no-op if `s2_vocabulary` empty | **Strong** — research vocabulary directly configurable | **Zero-code-change** automatic Informs edges between entries mentioning the same research terms | Config only: populate `s2_vocabulary` | **High (zero code change)** |
| **Goal-Conditioned Behavioral Learning (crt-046)** | No learning or adaptation mentioned | **COMPLETE in production** (PR #511, schema v22); goal_clusters table; behavioral Informs edge emission; briefing blending | **Direct fit** — research Goals are precisely the session anchor crt-046 was designed for | Goal-conditioned briefing from session 2 onward; behavioral Informs edges feeding PPR from session 1 close; compounding value | **None** — use `context_cycle(type="start", topic="goal-NNN", goal="<goal text>")` as-is | **High (zero code change, invisible to requirements)** |
| **PPR Graph Traversal for Serendipitous Discovery** | Traversal is explicit and query-driven only | Fully implemented (crt-042/044/045 COMPLETE); reverse-walk propagates relevance through typed edges | **Strong** — research graph edges form correct propagation structure; Claims supporting queried Theses surface via PPR when semantically distant | Related work discovery without explicit query; "Claims supporting your Thesis you haven't seen" surfaces naturally | Add `Advances` to PPR positive types (~10 lines); no algorithm changes | **High (cold-start limited: sessions 1–4 = HNSW only; increasing value as graph populates)** |
| **S1 Tag Co-occurrence Informs Edges** | All edges declared explicitly | Fully implemented; bidirectional Informs on ≥3 shared tags; threshold hardcoded in SQL | **Moderate** — works if Claims are consistently tagged; `≥3` threshold may be too high | Free Informs edges between co-tagged Claims | Possibly lower HAVING ≥3 threshold via new config param | **Medium** |
| **S8 Search Co-retrieval CoAccess Edges** | All edges declared explicitly | Fully implemented; watermark-based incremental; daily cadence compatible | **Moderate** — CoAccess edges weak (weight 0.25) but feed PPR at no cost | Supplementary co-relevance signal for PPR | None | **Medium** |

### Strategic observation: two asymmetric value opportunities

**S2 Vocabulary Informs (zero effort, immediate high gain)**: Populate `s2_vocabulary` with domain terms. Informs edges appear automatically between entries mentioning the same terms. Zero code changes. The clearest expression of the "configured not rebuilt" vision.

**Goal-Conditioned Behavioral Learning (zero effort, compounding gain)**: crt-046 is fully implemented. Call `context_cycle` with `goal` parameter and `context_cycle_review` at session close. The requirements document does not mention this capability at all — it is invisible to the external spec but delivers genuine differential value from session 2 onward. A raw graph storage layer cannot provide this.

**context_cycle workaround (tension resolved)**: Use `context_cycle(type="start", topic="goal-{id}", goal="<goal text>")` as-is. Goal ID as topic string is the supported workaround today. `cycle_anchor_category` is a future config extension — it does not block any MVI functionality.

**For roadmap positioning**: lead with S2 and Goal-conditioned learning in the first research domain deployment — they demonstrate Unimatrix intelligence with zero code changes. NLI category-filtered contradiction detection and evidence-driven Thesis confidence are high value but require code changes — position them as Wave 3 Phase 2 enhancements.

---

## 7. Roadmap Positioning Recommendation

### Wave placement: Wave 3

Wave 2 is fully committed. Research domain support is Wave 3 scope.

**Gate conditions**:
1. Wave 2 shipped
2. ASS-055 write path design confirmed as the implementation reference for all typed edge writes (already designed — no additional research needed)
3. External repository confirms requirements are stable after reviewing these findings
4. W1-1 crt-021 typed graph stable (already complete)

### Phase 1 — Write Path + SDLC Edges (critical path, both domains)

**This ships first.** Without a write path for typed edges, the graph stays sparse and `context_graph` traversal returns thin results for both SDLC and research.

| Item | Value delivered | Effort |
|---|---|---|
| `edges` parameter on `context_store` + `context_correct` | Unified write path for all typed edges across SDLC and research | 2–3 days |
| 10 RelationType enum variants | Enum coverage for full taxonomy | 1 day |
| `stale_dependency_edges` in `context_status` | ADR dependency health visibility | 0.5 days |
| `DependencyOnDeprecated` detection rule | Cycle review notification when ADR dependency is stale | 0.5 days |
| Bidirectional Contradicts insert | Symmetric edge storage follows CoAccess precedent | 0.5 days |
| Config changes (goal category, s2_vocabulary, informs_category_pairs) | Research domain immediately usable; SDLC Goal traceability enabled | 0.5 days |
| **Phase 1 total** | **SDLC: ADR dependency + Goal traceability + reasoning chains. Research: all edge types writable.** | **~5–6 days** |

Phase 1 unblocks both domains immediately. SDLC agents can begin declaring `depends_on`, `advances`, `motivates`, `refutes` edges from day one. Research workflow can declare all 14 edge types. The graph populates. Phase 2 tools then have a real graph to traverse.

### Phase 2 — context_graph Tool (traversal, both domains)

**Depends on Phase 1 populating the graph.** Consolidated into one tool with mode parameter — net +1 tool (13 total).

| Item | Patterns served | Effort |
|---|---|---|
| `context_graph` modes: neighbors + subgraph | Research Q1, Q2, Q4, Q7, Q8; SDLC: dependency chain navigation, Goal evidence | 8–10 days |
| `context_graph` modes: chain + current | Research Q6 (supersession chain); SDLC: ADR version chain | 2–3 days |
| `context_graph` mode: inverse | Research Q9 (Sources with no incoming cites); SDLC: gap detection queries | 2–3 days |
| Composite GRAPH_EDGES indexes + supersedes/superseded_by indexes | Query performance at scale | 1 day |
| Add `Advances`/`Motivates` to PPR positive types | PPR serendipitous discovery through research + SDLC goal graph | 1 day |
| **Phase 2 total** | **Research: Q1, Q2, Q4, Q6, Q7, Q8, Q9. SDLC: dependency nav, goal traceability, gap detection.** | **~14–17 days** |

### Phase 3 — Intelligence + Extended Queries (deferred)

| Item | Value | Effort |
|---|---|---|
| `context_graph` modes: path + filter | Research Q10, Q11; SDLC: advanced graph queries | 6–8 days |
| NLI `contradicts_category_pairs` for Claims | Automatic contradiction detection for research Claims | 1–2 days |
| Evidence-driven Thesis/Pattern confidence | Confidence driven by supports/refutes edge counts | 3–5 days |
| **Phase 3 total** | | **~10–15 additional days** |

### Effort categorization

| Category | Total effort |
|---|---|
| (a) Config only | 0.5 days |
| (b) Write path — `edges` param on existing tools (ASS-055 design) | 2–3 days |
| (c) New enum variants (10 RelationType) | 1 day |
| (d) Monitoring additions (stale_dependency_edges, detection rule) | 1 day |
| (e) `context_graph` tool — 1 tool, 7 modes | ~14–17 days |
| (f) Intelligence extensions (NLI scoping, evidence confidence) | ~4–7 days |
| (g) Storage schema changes | **0 days** |
| (h) Core architecture changes | **0 days** |
| **Phase 1 total (write path — ships first)** | **~5–6 days** |
| **Phase 1+2 total (write path + traversal)** | **~20–24 days** |
| **Full implementation (all three phases)** | **~30–39 days** |

### What the research domain gets today without any changes

- `context_store` for all 9 categories (fully configurable)
- `context_get` and `context_lookup` for direct retrieval and category-filtered browsing
- Semantic discovery via `context_search`
- S2 vocabulary Informs edges (immediate, zero code, just config)
- Goal-conditioned briefing via `context_cycle` + crt-046 (in production, zero code)
- Behavioral Informs edge emission from co-access patterns (feeds PPR, zero code)

---

## 8. `research-domain.toml` Sketch

```toml
# research-domain.toml — Unimatrix configuration for autonomous research workflow
#
# [CONFIGURABLE TODAY]   — works with current codebase
# [NEEDS CODE]           — requires code change before effective
# [NOT YET CONFIGURABLE] — hardcoded; no config mechanism exists

[profile]
# Freshness-heavy scoring appropriate for time-sensitive research corpora.
# [CONFIGURABLE TODAY]
preset = "empirical"

[knowledge]
# All 8 required research categories (+ optional Insight).
# For pure-research instance: replace INITIAL_CATEGORIES entirely.
# [CONFIGURABLE TODAY]
categories = [
  "goal", "source", "finding", "claim",
  "entity", "thesis", "poc", "deliverable", "insight",
]

# Goal and Thesis are primary retrieval targets.
# [CONFIGURABLE TODAY]
boosted_categories = ["goal", "thesis"]

# Thesis has a lifecycle; confidence decay adjustment enabled.
# [CONFIGURABLE TODAY]
adaptive_categories = ["thesis"]

# One-week half-life for freshness scoring on rapidly-evolving research corpora.
# [CONFIGURABLE TODAY]
freshness_half_life_hours = 168.0

[server]
# Communicates entity taxonomy to agents using this Unimatrix instance.
# [CONFIGURABLE TODAY]
instructions = """
You are the knowledge engine for an autonomous research workflow.
Entity taxonomy:
  Goal       — anchor; work-defining entity
  Source     — external item stub (URL/DOI; substance in Reader)
  Finding    — researcher's interpretation of one Source
  Claim      — atomic proposition extracted from a Finding
  Entity     — normalized named thing (tool, paper, technique, person, concept)
  Thesis     — researcher's own proposition with status lifecycle
  POC        — experiment stub (substance on git branch)
  Deliverable — output stub (substance on disk)
  Insight    — cross-item synthesis (optional)

Thesis status lifecycle: store as tag "thesis-status:proposed|supported|refuted|abandoned".
Prefer context_neighbors / context_subgraph for structured graph queries.
Use context_search for serendipitous discovery.
Anchor sessions with context_cycle(type="start", topic="goal-{id}", goal="<goal text>").
"""

[inference]
# Enable NLI — Claims are the use case this model was built for.
# Requires ~85MB model download.
# [CONFIGURABLE TODAY]
nli_enabled                 = true
nli_model_name              = "minilm2-q8"
nli_contradiction_threshold = 0.70
nli_entailment_threshold    = 0.65
# NOTE: Without contradicts_category_pairs [NEEDS CODE below],
# enable NLI only on pure-research instances to avoid cross-category noise.

# Informs edge detection for research-domain category pairs.
# [CONFIGURABLE TODAY]
informs_category_pairs = [
  ["finding", "thesis"],
  ["claim",   "thesis"],
  ["finding", "claim" ],
  ["claim",   "claim" ],
]

# S2: entries mentioning these terms get automatic bidirectional Informs edges.
# Zero code change — populate for target research domain.
# [CONFIGURABLE TODAY]
s2_vocabulary = [
  "hypothesis", "evidence", "methodology", "replication",
  "benchmark", "baseline", "ablation", "dataset",
  "experiment", "evaluation", "citation", "empirical",
]

# PPR expansion in context_search.
# [CONFIGURABLE TODAY]
ppr_expander_enabled     = true
expansion_depth          = 2
max_expansion_candidates = 150

# =============================================================================
# [graph] — NOT A CURRENT CONFIG SECTION; design-artifact for future extension
# =============================================================================

# 10 new RelationType variants for the research domain.
# [NEEDS CODE: add to RelationType enum in graph.rs; ~40 lines]
# edge_types = [
#   "Advances", "Cites", "Asserts", "Mentions", "Refutes",
#   "Tests", "DerivedFrom", "Motivates", "About", "RelatedTo",
# ]

# PPR positive edges for research domain.
# Advances and Motivates add serendipitous Goal-driven discovery.
# [NEEDS CODE: modify graph_ppr.rs:168-187; ~16 lines]
# ppr_positive_edge_types = [
#   "Supports", "CoAccess", "Prerequisite", "Informs",  # existing four
#   "Advances", "Motivates",                             # new for research domain
# ]

# Symmetric edges: store both directions at write time.
# Follows CoAccess bidirectional-insert precedent (migration.rs:632-665).
# [NEEDS CODE: bidirectional insert in write handler]
# symmetric_edge_types = ["Contradicts", "RelatedTo"]

# NLI contradiction detection scoped to Claim-to-Claim pairs only.
# Removes C-13/AC-10a constraint for 'claim' category specifically.
# [NEEDS CODE: add contradicts_category_pairs to InferenceConfig; ~20 lines]
# contradicts_category_pairs = [["claim", "claim"]]

# Suppress embedding for stub entry categories (URL, git-path stubs).
# [NEEDS CODE: no_embed_categories flag in store_ops.rs:121; ~2-3 days]
# no_embed_categories = ["source", "poc", "deliverable"]

# =============================================================================
# [cycle] — NOT A CURRENT CONFIG SECTION; design-artifact for future extension
# =============================================================================

# context_cycle is hardcoded to feature_cycle (SDLC concept).
# Working workaround: use Goal ID as pseudo-feature-cycle string ("goal-{id}").
# This works today with crt-046 in production:
#   context_cycle(type="start", topic="goal-001", goal="<goal text>")
# [NOT YET CONFIGURABLE — future config extension]
# cycle_anchor_category = "goal"
```

### What the toml can configure vs. cannot

| Behavior | Status |
|---|---|
| All 8–9 research categories | Configurable today |
| boosted_categories, adaptive_categories, freshness_half_life_hours | Configurable today |
| Confidence preset | Configurable today |
| informs_category_pairs | Configurable today |
| S2 structural vocabulary | Configurable today |
| NLI enabled/disabled and thresholds | Configurable today |
| PPR expander enabled and depth | Configurable today |
| Briefing instructions | Configurable today |
| 10 new RelationType variants | Needs code (graph.rs enum) |
| PPR positive edge types | Needs code (graph_ppr.rs hardcoded) |
| Symmetric edge bidirectional insert | Needs code (write-path handler) |
| NLI contradiction scoped to Claims only | Needs code (contradicts_category_pairs config field) |
| Embedding suppression for stub categories | Needs code (no_embed flag in store_ops.rs) |
| graph_expand positive edge types | Needs code (graph_expand.rs hardcoded) |
| cycle_anchor_category | Not yet configurable (workaround: Goal ID as topic string) |
| `context_graph` tool (7 modes) | Missing — new MCP tool required |

---

## Unanswered Questions

**UQ-1 — Mixed SDLC + research instance isolation**: If SDLC and research share a Unimatrix instance (same repo hash), NLI scans SDLC entries for contradictions with research Claims. Cross-domain noise is mitigated by `informs_category_pairs` configuration but not eliminated. Clean solution: separate instances with separate repo hashes.

**UQ-2 — as-of timestamp support (Phase 3+)**: Full as-of support requires adding `deprecated_at` column to entries (deprecation is a status field change today, not a timestamp column) plus equivalent for GRAPH_EDGES. Estimated 5–7 days of schema migration. Correctly deferred per requirements document.

**UQ-3 — Thesis metadata column decision**: A `metadata: Option<serde_json::Value>` column on `entries` cleanly resolves the Thesis lifecycle gap and benefits all future domain deployments. Deferred until Wave 3 planning.

**UQ-4 — revision_reason accessibility**: `revision_reason` on Supersedes GRAPH_EDGES rows is invisible to all graph traversal logic (Pass 2b skips Supersedes rows). Accessible only via direct SQL. If `revision_reason` must be surfaced during supersession chain traversal, it needs to be stored in `entries.tags` by convention or the supersession model must be reworked to load Supersedes edge metadata.

---

## Out-of-Scope Discoveries

**OSC-1 — Convergent-citation S9 edge source**: Two Findings citing the same Source via `cites` edges is a strong structural signal of relevance — analogous to S1 shared tags but at the edge level. Implementation follows S1 pattern in `graph_enrichment_tick.rs` (~50 lines). Dependency: `Cites` must be a stored RelationType. Flag for a spike after Cites edges confirmed written.

**OSC-2 — Thesis-specific ConfidenceParams preset**: A `research` preset with `w_usage = 0, w_fresh = 0, w_corr = 0, w_base = 0, w_help = 0.7 (evidence), w_trust = 0.3` would give Theses evidence-weighted confidence without formula changes — only `ConfidenceParams` configuration. Should appear in the Wave 3 Phase 2 design.

**OSC-3 — informs_category_pairs as research inference configurator**: Pairs `["claim", "thesis"]`, `["finding", "claim"]`, `["finding", "thesis"]` in `informs_category_pairs` enable Path A cosine scan to generate Informs edges across research categories automatically. Pure config — no code required. Include in MVI config.

**OSC-4 — NLI suppression interaction with research Claims**: `suppress_contradicts` (search pipeline) suppresses entries with Contradicts edges pointing to high-scoring results. If category-filtered NLI writes Contradicts edges between Claims, `suppress_contradicts` will run on Claim results. This is desirable behavior (contradictory Claims suppressed in direct search, surfaced via PPR), but the interaction should be explicitly documented when enabling NLI for Claims.

**OSC-5 — PPR cold-start mitigation via S2**: For sessions 1–4 when PPR provides no graph-based serendipity, S2 Informs edges (immediate, config-only) partially compensate by providing automatic structural connections between entries mentioning the same vocabulary. S2 effectively accelerates PPR warm-up without requiring graph density.

**OSC-6 — context_batch_write (future consideration)**: The research domain requirements document assumed atomic bulk writes of 20–50 entries + 50–200 edges in a single transaction. Evaluated and removed from scope for the following reasons: (1) the stated use case (one paper per session → one Finding + 5–10 Claims) is well-served by sequential individual `context_store` calls using the `edges` parameter; (2) the partial-write failure mode is the same failure mode SDLC accepts today and handles through re-runs and human review; (3) the HNSW atomicity problem (DB commits but HNSW insertion can fail mid-batch, desynchronizing the vector index) has no clean resolution without significant architectural work; (4) cost is 7–10 days with an open production blocker. Revisit only if a deployed research workflow demonstrates at scale that sequential writes produce unacceptable partial-state failures. If implemented: requires a new synchronous write_pool path bypassing the analytics queue, same `Capability::Write` gate as `context_store`, max-batch-size limit enforced in MCP handler, and an explicit HNSW atomicity design decision (partial state + tick reconciliation vs. rollback mechanism).

---

## Recommendations Summary

- **Entity categories**: Use `config.toml [knowledge] categories` for all 9 research categories — zero code changes. Stub-with-pointer convention works as-is. For Thesis status lifecycle, use `tags` convention (`"thesis-status:refuted"`); `metadata` column migration resolves cleanly long-term (UQ-4).

- **Edge types**: 4 of 14 research edges reuse existing RelationType variants. 10 new variants needed — ~40 lines in `graph.rs`, no schema migration. 5 of the 10 new variants (`Advances`, `Motivates`, `Refutes`, `About`, `DerivedFrom`) serve SDLC and research identically — no duplication. Store `contradicts` bidirectionally (Option a), following CoAccess precedent. Map `strength` on supports/refutes to `weight` column; all other edge properties to `metadata` JSON.

- **SDLC edge write path (ASS-055 integration — ships first)**: Add `edges: Option<Vec<{type, target_id}>>` parameter to `context_store` and `context_correct`. No new tool. No schema migration. Security: source ownership validation (caller must own source entry) + confidence floor on source. Follows ASS-055 design exactly, generalized from `depends_on`-only to all typed edges. **This is Phase 1 and the critical path item** — without it, both SDLC and research graphs stay sparse and traversal is useless. 2–3 days.

- **ASS-055 notification surfaces**: Add `stale_dependency_edges` count to `context_status` (~20 lines) and `DependencyOnDeprecated` detection rule to `context_cycle_review` (~40 lines). 1 day total.

- **Tool consolidation**: Eight originally-identified traversal APIs collapse to one `context_graph` tool with a `mode` parameter. Net tool count: 12 → 13. Precedent: `context_cycle` uses `type=` for the same reason.

- **context_graph traversal (Phase 2)**: neighbors, subgraph, inverse, chain, current, path, filter modes. All backed by existing GRAPH_EDGES storage and in-memory TypedRelationGraph. Zero schema changes. Implement `resolve_supersessions` from day one. Phase 2 total: ~14–17 days.

- **Q9 antijoin**: Confirmed feasible — 1–3ms at 3k entities via SQL LEFT JOIN antijoin. Composite `(target_id, relation_type)` index reduces to sub-ms and is strongly recommended.

- **Architecture fitness**: **ALIGNED WITH EXTENSIONS.** Vector-first and graph-first are orthogonal code paths. The gap is write path (Phase 1) + one traversal tool (Phase 2). No schema migrations, no core architecture changes required.

- **NLI for Claims**: HIGH VALUE. Phase 3. Claims are the use case NLI was built for — zero Contradicts edges in production due to SDLC corpus mismatch. 3–5 call site changes to enable category-filtered NLI.

- **Thesis/Pattern confidence (evidence-driven)**: HIGH VALUE. Phase 3. Bayesian helpfulness formula directly repurposable for supports/refutes ratios. Serves both domains.

- **S2 Vocabulary Informs (zero code)**: HIGH VALUE. Immediate. Populate `s2_vocabulary` with domain terms; Informs edges appear automatically. Lead with this in first research domain deployment.

- **Goal-Conditioned Learning (zero code)**: HIGH VALUE. Immediate. crt-046 fully implemented. Use `context_cycle` with `goal` parameter. Compounding value from session 2 onward. Works for SDLC Goal entries too.

- **PPR Serendipitous Discovery**: HIGH VALUE (cold-start limited). Add `Advances` to PPR positive types (~10 lines). Value grows as graph populates from write path declarations.

- **Roadmap position**: Wave 3. Phase 1 (write path) is ~5–6 days and ships first. Phase 2 (context_graph tool) is ~14–17 days. Phase 3 (intelligence extensions) is ~17–25 days. Gate on Wave 2 completion. Zero schema migrations across all phases. **The write path (Phase 1) delivers immediate value to SDLC independent of the research domain adoption decision.**

- **Enterprise audit graph**: Once Phase 1 (`depends_on` write path) and Phase 2 (`context_graph` subgraph mode) ship, `goal → decision → outcome` traversal is a single query. Load-bearing for ISO 42001 governance audit export. Zero additional work beyond the combined scope.

- **research-domain.toml**: All current config delivers immediate value (categories, S2, NLI on/off, informs_category_pairs, boosted_categories). The `[graph]` and `[cycle]` sections are design-artifacts for future config extensibility — all items in those sections require code changes.
