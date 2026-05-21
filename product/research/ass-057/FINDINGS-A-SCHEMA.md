# FINDINGS: ASS-057 Track A — Entity Taxonomy & Storage Schema

**Spike**: ass-057 (Track A)
**Date**: 2026-05-14
**Approach**: investigation
**Confidence**: validated (all answers grounded in codebase evidence with file and line references)

---

## 1. Entity Category Mapping

### How the category model works today

**Config loading** (`crates/unimatrix-server/src/infra/config.rs`, `crates/unimatrix-server/src/infra/categories/mod.rs`):

- `INITIAL_CATEGORIES` (line 15, `categories/mod.rs`) defines 5 hardcoded baseline categories: `lesson-learned`, `decision`, `convention`, `pattern`, `procedure`.
- `KnowledgeConfig.categories` (`config.rs:155`) is fully configurable via `[knowledge] categories` in `config.toml`. The default is the 5 INITIAL_CATEGORIES but operators replace this list entirely — there is no hardcoded enforcement of the default 5 at validation time.
- `CategoryAllowlist` (`categories/mod.rs:33`) enforces the allowlist at ingest. Every `context_store` call runs `allowlist.validate(&params.category)` (confirmed at `tools.rs:615`). Invalid categories return `ServerError::InvalidCategory`.
- New categories can also be registered at runtime via `add_category()` — domain packs call this at startup (`categories/mod.rs:100`). Runtime additions are always "pinned" (never adaptive).
- `boosted_categories` (`config.rs:158-159`) is a subset of `categories`. It drives the provenance boost in search re-ranking: entries whose `category` is in the boosted set receive `PROVENANCE_BOOST` as the raw provenance signal (`search.rs:1190-1191`). This is purely additive at scoring time — no schema change required.
- `informs_category_pairs` (`config.rs:555`) is a list of `[lhs, rhs]` pairs controlling which category-to-category transitions can produce automatic `Informs` edges via NLI tick. This is where domain-specific relationship inference is configured.

**Implication**: All 9 research entity categories can be added to `[knowledge] categories` in `config.toml` with zero code changes. The category model is genuinely config-driven as claimed.

---

### Mapping table

| Required category | `config.toml` category name | What goes in `content` | Schema gaps |
|---|---|---|---|
| **Goal** | `goal` | The goal statement: scope, success criteria, current status summary. Structured prose or YAML block. | None. `topic` serves as the goal domain. `tags` carry related entities. |
| **Source** | `source` | Stub entry: title, URL/DOI, publication metadata, 1–2 sentence abstract. Substance (full text) lives in Reader/cache outside Unimatrix. | None. Stub-with-pointer is a valid convention today — see stub analysis below. |
| **Finding** | `finding` | Researcher's triage: summary of what this source means for the current research question. Full prose. | None. |
| **Claim** | `claim` | Single atomic proposition, 1–3 sentences. Should be self-contained and falsifiable. | None. |
| **Entity** | `entity` | Normalized entry for the named thing: canonical name, aliases, brief description. | None. `tags` can carry type signal (e.g., `["tool", "paper", "person"]`). |
| **Thesis** | `thesis` | The proposition text. Status lifecycle (proposed/supported/refuted/abandoned) lives in a metadata convention — see Thesis lifecycle analysis below. | Status lifecycle gap — see below. |
| **POC** | `poc` | Stub entry: description of experiment, git branch pointer, hypothesis being tested, setup instructions summary. Substance (runnable code) lives on a branch. | None. Same stub-with-pointer pattern as Source. |
| **Deliverable** | `deliverable` | Stub entry: description of output, filesystem/URL pointer, format. Substance lives on disk. | None. Same stub-with-pointer pattern. |
| **Insight** (optional) | `insight` | Cross-item synthesis prose: what patterns emerged, which theses are supported or challenged. | None. |

---

### Stub-with-pointer pattern analysis

The stub pattern (Source, POC, Deliverable carry pointers; substance lives elsewhere) is a **convention**, not a schema requirement. `EntryRecord.content` (`schema.rs:52`) is `String` — it accepts any text including a URL or file path. There is no URL-type column, no schema enforcement that content is a URL, and no validation against pointer format.

The convention works as-is because:
1. `content` is the primary payload fed to the HNSW embedding pipeline. A short stub (title + URL + abstract) produces a meaningful vector for semantic retrieval.
2. The full document content is not required for graph traversal — the stub is sufficient for edge storage and query.
3. Unimatrix's intelligence pipeline (NLI, confidence scoring, PPR) operates on the stub content, not the external document. For Source entries, NLI scoring would run on the abstract/summary, which is appropriate.

No schema changes are needed to support stub entries.

---

### Thesis status lifecycle analysis

**The mismatch**: The research domain requires `thesis.status` to cycle through `proposed → supported → refuted → abandoned`. Unimatrix entries have `status: Status` with four values: `Active (0)`, `Deprecated (1)`, `Proposed (2)`, `Quarantined (3)` (`schema.rs:10-15`).

**Overlap and conflict**:
- `Proposed` maps cleanly to `thesis.status = proposed`. Direct reuse.
- `Active` is the nearest match for `thesis.status = supported` — but `Active` is a generic lifecycle state for all categories, not a domain-specific epistemic state.
- `Deprecated` maps to `abandoned`, but `deprecated` carries the connotation of "superseded by a better version," not "this thesis was examined and rejected." Semantic distortion is present.
- `Quarantined` has no meaningful research analog; it is a security state.
- `refuted` has no Status equivalent at all.

**EntryRecord has no `metadata` field**: Confirmed by exhaustive inspection of `schema.rs:48-102`. The `metadata` field exists only on `AuditEvent` (`schema.rs:390`) and on `GRAPH_EDGES` rows — not on entry records. A Thesis research-domain status cannot be stored in entry-level metadata as a first-class field without a schema migration.

**Available workarounds without schema change**:
1. **`tags` field** (`schema.rs:55`): Add a status tag (e.g., `"thesis-status:refuted"`). Tags are searchable via `context_lookup` with tags filter. Convention-safe, does not require schema changes. The downside: tag-based status is not indexed separately and is subject to typos.
2. **`feature_cycle` field** (`schema.rs:86`): Repurpose as a convention-defined status token (e.g., `"thesis:refuted"`). This is a hack; `feature_cycle` has explicit semantics in Unimatrix for feature tracking.
3. **Entry status mapping**: Map thesis lifecycle to the closest Unimatrix status — `Proposed` (proposed), `Active` (supported), `Deprecated` (refuted or abandoned). Conflates `refuted` and `abandoned` and is semantically imprecise.

**Indexing and query implications**: If thesis status lives in `tags`, `context_lookup` with `tags: ["thesis-status:refuted"]` works today. Tags are stored as a searchable field in the entries table (confirmed by `QueryFilter.tags` in `schema.rs:137`). No additional indexing infrastructure needed.

**Gap verdict**: Thesis status lifecycle is the only meaningful schema gap across all 9 categories. A `metadata: Option<serde_json::Value>` column on `entries` would cleanly resolve it. Without a schema change, the tags convention is the least-bad option.

---

### Schema gaps summary

| Property needed | Present in EntryRecord? | Location if present | Gap severity |
|---|---|---|---|
| title | Yes | `title: String` | None |
| content (primary payload) | Yes | `content: String` | None |
| category | Yes | `category: String` | None |
| tags | Yes | `tags: Vec<String>` | None |
| status (Active/Deprecated/Proposed/Quarantined) | Yes | `status: Status` | Partial — maps 3 of 4 thesis states; loses `refuted` vs `abandoned` distinction |
| Research-domain-specific status (proposed/supported/refuted/abandoned) | No | — | **Gap**: no `metadata` field on entries; tags convention is the workaround |
| URL/pointer field | No | — | Not a gap — `content` serves this role by convention |
| Version chain | Yes | `supersedes: Option<u64>`, `superseded_by: Option<u64>` | None |
| created_by / attribution | Yes | `created_by: String` | None |
| confidence | Yes | `confidence: f64` | None |
| feature_cycle (maps to "research cycle") | Yes | `feature_cycle: String` | Repurposable by convention |

---

## 2. Edge Type Gap Analysis

### GRAPH_EDGES table schema (DDL, confirmed from `migration.rs:340-352`)

```sql
CREATE TABLE IF NOT EXISTS graph_edges (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id      INTEGER NOT NULL,
    target_id      INTEGER NOT NULL,
    relation_type  TEXT    NOT NULL,
    weight         REAL    NOT NULL DEFAULT 1.0,
    created_at     INTEGER NOT NULL,
    created_by     TEXT    NOT NULL DEFAULT '',
    source         TEXT    NOT NULL DEFAULT '',
    bootstrap_only INTEGER NOT NULL DEFAULT 0,
    metadata       TEXT    DEFAULT NULL,
    UNIQUE(source_id, target_id, relation_type)
)
```

**Indices present** (confirmed from `migration.rs:360-385`):
- `idx_graph_edges_source_id ON graph_edges(source_id)`
- `idx_graph_edges_target_id ON graph_edges(target_id)`
- `idx_graph_edges_relation_type ON graph_edges(relation_type)`

No composite index on `(source_id, relation_type)` or `(target_id, relation_type)`. The requirements document specifies indices on `(from_id, edge_type)` and `(to_id, edge_type)` — these are absent today and would improve single-type neighbor query performance.

### Current RelationType enum (confirmed from `graph.rs:81-90`)

Six variants: `Supersedes`, `Contradicts`, `Supports`, `CoAccess`, `Prerequisite`, `Informs`.

Stored as strings in GRAPH_EDGES (`graph.rs:75`): extension does NOT require schema migration. New variants require updates at 4 sites in 2 files (Unimatrix pattern entry #3950): (1) enum body in `graph.rs`, (2) `as_str()` match arm, (3) `from_str()` match arm, (4) `edges_of_type` calls in both `personalized_pagerank` and `positive_out_degree_weight` in `graph_ppr.rs`. Missing site (3) causes R-10 guard in `build_typed_relation_graph` to silently drop rows.

### Graph traversal behavior reference

**PPR** (`graph_ppr.rs`): Reverse random walk (transpose PPR). For edge A→B, seeding B causes mass to flow back to A. Positive types only: `Supports`, `CoAccess`, `Prerequisite`, `Informs`. `Supersedes` and `Contradicts` are excluded (`graph_ppr.rs:168-187`).

**graph_expand** (`graph_expand.rs`): BFS over Direction::Outgoing from seed set. Positive types: `CoAccess`, `Supports`, `Informs`, `Prerequisite`. For edge A→B: seeding A reaches B; seeding B does NOT reach A. Supersedes and Contradicts explicitly excluded (`graph_expand.rs:62`).

**build_typed_relation_graph** (`graph.rs:289-346`): R-10 guard silently drops any unrecognized `relation_type` string with `tracing::warn!`.

### Edge type gap table

| Edge | Direction | Current RelationType | Gap | PPR behavior (reverse walk, seeding target) | graph_expand behavior (outgoing from source) |
|------|-----------|---------------------|-----|-------------|----------------------|
| `advances` | Finding/Thesis/POC/Insight/Deliverable → Goal | None | New variant needed | Seed=Goal: PPR reverse walks to all advances-sources (Findings/Theses/POCs pointing to Goal). Desirable: Goal seeding surfaces contributing evidence. | Outgoing from Finding reaches Goal. Seeding Goal does NOT reach sources. Goal-centric expand requires inbound pass or bidirectional storage. |
| `cites` | Finding → Source | None | New variant needed | Seed=Finding: mass flows to Source. Seeding Source: no useful PPR result (Sources have no outgoing cites edges). Recommend: exclude from PPR (stubs add noise). | Outgoing from Finding reaches Source. Seeding Source does not reach citing Findings. |
| `asserts` | Finding → Claim | None | New variant needed | Seed=Claim: PPR reverse walks to Finding. Desirable. | Outgoing from Finding reaches its Claims. Seeding Claim does not reach Finding. |
| `mentions` | Finding/Thesis/Claim/Insight/Goal → Entity | None | New variant needed | Seed=Entity: PPR reverse walks to all entries that mention the entity. Highly desirable for entity-centric retrieval (Q4). | Outgoing from any mention-source reaches Entity. Entity-seeded expand does not reach mentioning entries. |
| `supports` | Claim → Thesis | Supports (reuse) | Direction and semantics compatible. `strength` maps to existing `weight REAL` column by convention (no schema change). | Currently in PPR positive types. Seed=Thesis: PPR reverse walks to supporting Claims. Desirable. | Outgoing from Claim reaches Thesis. Thesis-seeded expand does not reach Claims; PPR compensates. |
| `refutes` | Claim → Thesis | None | New variant needed. Semantically distinct from Contradicts and Supports. | Seed=Thesis: PPR should surface refuting Claims for full evidence picture. Decision required: include in PPR positive types? Recommend: yes, for research domain. | Outgoing from Claim reaches Thesis. Same gap as supports: thesis-seeded expand misses refuting Claims without inbound pass. |
| `contradicts` | Claim ↔ Claim (symmetric) | Contradicts (reuse) | Symmetry storage gap — see Section 3. `confidence` and `human_confirmed` live in `metadata` JSON — see Section 4. Currently excluded from PPR and graph_expand. | Currently excluded from PPR. Recommend: domain-configurable inclusion. | Currently excluded from graph_expand. |
| `depends_on` | Thesis → Thesis | Prerequisite (reuse) | ASS-055 direction contract applies: store A→B meaning "A is prerequisite of B" (B depends on A). PPR seeding B surfaces A. No new variant. | Seeding B: PPR surfaces A. Desirable. | Outgoing from B does not reach A. Gap accepted per ASS-055. |
| `tests` | POC → Thesis | None | New variant needed. | Seed=Thesis: PPR reverse walks to testing POCs. Desirable. Recommend: include in PPR positive types. | Outgoing from POC reaches Thesis. Thesis-seeded expand misses testing POCs. |
| `derived_from` | Insight → Finding/Claim/Thesis | None | New variant needed. | Seed=Insight: PPR surfaces sources it was derived from. Desirable. | Outgoing from Insight reaches source entries. Source-seeded expand does not reach derived Insights. |
| `motivates` | Insight → Thesis | None | New variant needed. | Seed=Thesis: PPR surfaces motivating Insights. Desirable. | Outgoing from Insight reaches Thesis. Thesis-seeded expand misses motivating Insights. |
| `about` | Thesis → Entity | None | New variant needed. Entity tagging for Theses. | Seed=Entity: PPR reverse walks to all Theses about it. Desirable for entity-centric retrieval. | Outgoing from Thesis reaches Entity. Entity-seeded expand does not reach Theses. |
| `supersedes` | * → * (same category) | Supersedes (reuse) | GRAPH_EDGES Supersedes rows are skipped in `build_typed_relation_graph` Pass 2b (`graph.rs:295`); in-memory graph derives Supersedes topology from `entries.supersedes` only. `revision_reason` in `metadata` on GRAPH_EDGES Supersedes rows is accessible via direct SQL only. See OQ-5. | Supersedes excluded from PPR. Handled by dedicated functions. | Supersedes excluded from graph_expand. |
| `related_to` | * → * | None | New variant needed. Semantically weaker than Informs. | Seed=any: PPR diffuses mass through related pairs. Include in PPR positive types as weak signal. | Outgoing from A reaches B. For full symmetric use, bidirectional storage needed (same approach as CoAccess). |

**Summary**: 4 of 14 reuse existing variants (`supports`/Supports, `depends_on`/Prerequisite, `supersedes`/Supersedes, `contradicts`/Contradicts). 10 new variants required. No schema migration needed — RelationType is stored as string in GRAPH_EDGES. Extension cost: ~4 lines per variant across 2 files per established pattern, plus design decision on PPR inclusion per variant.

**Missing composite indices**: `(source_id, relation_type)` and `(target_id, relation_type)` absent. Acceptable at stated scale (few thousand nodes). Add as schema migration if context_neighbors-style queries become common.

---

## 3. `contradicts` Symmetry Resolution Analysis

### The storage problem

`contradicts` is logically symmetric. GRAPH_EDGES has `UNIQUE(source_id, target_id, relation_type)`. Storing `(A, B, Contradicts)` covers only one direction. The existing query `query_contradicts_edges_for_entry` (`read.rs:1529-1532`) uses `WHERE target_id = ?1 AND relation_type = 'Contradicts'` — it finds edges pointing TO an entry but not edges FROM that entry. This confirms the asymmetric query gap is present in current code.

### Option (a): Store both directions

Store `(A, B, Contradicts)` and `(B, A, Contradicts)` atomically when a contradiction is asserted.

- **UNIQUE constraint**: `(A,B)` and `(B,A)` are distinct tuples — constraint allows both. `INSERT OR IGNORE` on re-assertion is idempotent for each direction separately.
- **Query complexity**: Simple. `WHERE source_id=? AND relation_type='Contradicts'` finds all contradictions FROM an entry. No OR clauses, no deduplication. Both directions present in in-memory graph.
- **In-memory graph**: petgraph DiGraph stores both directed edges. Cycle detection Pass 3 (`graph.rs:349-373`) operates on Supersedes-only subgraph — bidirectional Contradicts edges do not false-positive.
- **Storage cost**: 2x rows per contradiction pair. At stated scale, maximum ~1000 rows. Negligible.
- **Precedent**: CoAccess edges are already stored bidirectionally (confirmed: `migration.rs:632-665` adds reverse CoAccess edges; `migration.rs:710-759` adds reverse Informs edges). This is the established pattern in Unimatrix.

### Option (b): Query layer deduplication (OR clause)

Store one direction only. All queries use `WHERE (source_id=? OR target_id=?) AND relation_type='Contradicts'`.

- OR clauses cannot use the single-column indices efficiently. SQLite may decompose into two scans.
- All existing `query_contradicts_edges_for_entry` call sites need updating.
- In-memory graph traversal requires two calls per node wherever contradictions are used — adds permanent code complexity.

### Option (c): Canonical direction min(A,B) → max(A,B)

- Same OR-query problem as option (b) for bidirectional lookup.
- Non-obvious write-time ordering invariant that all callers must know.
- No query advantage over option (b). More complex overall.

### Recommendation

**Use option (a): store both directions.** Follows established Unimatrix pattern (CoAccess, Informs). Simple query logic. Negligible storage cost at stated scale. No in-memory graph complications.

---

## 4. Edge Properties Gap Assessment

### What exists today

`metadata TEXT DEFAULT NULL` on GRAPH_EDGES stores arbitrary JSON. Current use: NLI-origin edges store `{"nli_entailment": f32, "nli_contradiction": f32}` (`nli_detection.rs:123-131`). Column is nullable and unindexed.

**Critical finding**: No SQL WHERE filter on `metadata` content exists anywhere in the codebase. Confirmed by exhaustive grep of `read.rs` for `json_extract` and `WHERE.*metadata` patterns — zero results. All metadata access is post-retrieval in application code. Rows are fetched by structural fields and `metadata` is read as a blob.

### Per-property verdict

| Property | Edge type(s) | Nature | SQL WHERE needed? | Verdict |
|---|---|---|---|---|
| `contribution_kind` | advances | String enum | No — display only; no filter requirement in requirements doc | **metadata JSON sufficient** |
| `strength` | supports, refutes | Float [0,1] | Conditional: `context_filter` edge_filters example specifies "strength > 0.7". At stated scale (few hundred edges per thesis), application-side filter on retrieved rows is acceptable. For SQL-side filter: `json_extract(metadata, '$.strength') > 0.7` works without schema change (full scan on edge subset, acceptable at scale). | **metadata JSON sufficient at stated scale; structured column if filter becomes hot** |
| `salience`, `count` | mentions | Float + Integer | No — display only; no filter requirement | **metadata JSON sufficient** |
| `confidence` | contradicts | Float (nullable = human-asserted) | No SQL filter requirement. Convention: `NULL = human-asserted, float = model-inferred`. Existing `weight REAL NOT NULL DEFAULT 1.0` column could alternatively encode model confidence. | **metadata JSON sufficient; alternatively, weight column serves float-confidence** |
| `human_confirmed` | contradicts | Boolean | Plausible SQL filter use case: "only human-confirmed contradictions." `json_extract(metadata, '$.human_confirmed') = 1` in WHERE works without schema change; produces full scan over Contradicts edge subset. At a few hundred contradiction edges: sub-millisecond. | **metadata JSON sufficient at stated scale; add structured column `human_confirmed INTEGER DEFAULT NULL` if filter becomes hot** |
| `revision_reason` | supersedes | String (prose) | No — display only. Note: Supersedes GRAPH_EDGES rows are skipped during in-memory graph build (Pass 2b). `revision_reason` in metadata is accessible via direct SQL on GRAPH_EDGES only, not through graph traversal layer. | **metadata JSON sufficient; accessible via direct SQL only** |
| `note` | related_to | String (prose) | No — display only | **metadata JSON sufficient** |

### Summary

No edge property requires a structured column for correctness at stated scale. The `metadata TEXT` JSON column satisfies all 7 required properties. The single watch item is `human_confirmed` on contradicts: monitor if agents need SQL-level filtering on this field; one-column schema migration resolves it if needed.

The `weight REAL` column already on GRAPH_EDGES is a natural fit for `strength` on supports/refutes edges and `confidence` on contradicts edges — mapping these to `weight` rather than JSON parsing is a convention choice that reduces overhead.

---

## 5. Open Questions and Blockers

**OQ-1 — Thesis status: schema migration vs. tags convention**
The cleanest resolution to the Thesis lifecycle gap is a `metadata JSON` column on `entries` (schema migration). The research domain is the first use case requiring per-entry structured metadata beyond tags. Decision: tags convention (no migration, semantic imprecision) vs. schema migration (clean, reusable for future domains). This is an architectural decision, not a blocker.

**OQ-2 — `refutes` in PPR positive types**
For the research domain, `refutes` (Claim→Thesis) should participate in PPR traversal — a refuting Claim is evidence worth surfacing when a Thesis is seeded. The SDLC domain has no analog. Design decision: include `refutes` in PPR positive types alongside `supports`? Recommend yes, for research domain. If PPR positive type inclusion becomes domain-configurable, this resolves automatically.

**OQ-3 — `contradicts` in PPR for research domain**
Currently excluded from PPR (correctly, for SDLC). For research corpus, including `contradicts` in PPR would surface contradicting Claims when a Claim is seeded. Domain-configurable PPR edge type inclusion list would resolve cleanly — not available today. Decision required: hard-code research domain inclusion, or make PPR edge types config-driven?

**OQ-4 — Missing composite indices**
`(source_id, relation_type)` and `(target_id, relation_type)` composite indices absent. Acceptable at stated scale. Add when context_neighbors-style queries become common.

**OQ-5 — `revision_reason` on Supersedes and the dual-source model**
Supersedes topology in the in-memory graph is derived from `entries.supersedes` field (Pass 2a), not GRAPH_EDGES Supersedes rows (skipped in Pass 2b). `revision_reason` stored in `metadata` on a GRAPH_EDGES Supersedes row is invisible to all graph traversal logic — accessible only via direct SQL. If `revision_reason` must be surfaced during supersession chain traversal, it needs to be stored either in `entries` fields (tags convention) or the supersession model must be reworked to load Supersedes edge metadata.
