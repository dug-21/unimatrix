# crt-044 Pseudocode Overview
# Bidirectional S1/S2/S8 Edge Back-fill and graph_expand Security Comment

## Problem Statement

crt-041 wrote S1 (tag co-occurrence), S2 (structural vocabulary), and S8 (co-access tick) edges
as single-direction rows using a lower-ID-first convention. With crt-042's Outgoing-only
`graph_expand` BFS traversal, seeds sitting in the higher-ID position cannot reach lower-ID
partners. This feature fixes both the historical debt (migration) and the forward write path (ticks),
plus adds a security comment to `graph_expand` making the quarantine obligation visible at every
IDE call site.

---

## Components Covered

| Component | File | Nature |
|-----------|------|--------|
| `migration_v19_v20` | `crates/unimatrix-store/src/migration.rs` | Two SQL back-fill statements + version bump |
| `graph_enrichment_tick_s1_s2_s8` | `crates/unimatrix-server/src/services/graph_enrichment_tick.rs` | Second `write_graph_edge` call per pair in three tick functions |
| `graph_expand_security_comment` | `crates/unimatrix-engine/src/graph_expand.rs` | Two-line `// SECURITY:` comment — documentation only |

---

## Data Flow Between Components

```
[migration_v19_v20]  (runs once on DB open when schema_version < 20)
    |
    |-- Statement A: INSERT OR IGNORE reverse S1+S2 Informs edges into GRAPH_EDGES
    |-- Statement B: INSERT OR IGNORE reverse S8 CoAccess edges into GRAPH_EDGES
    |-- UPDATE counters: schema_version = 20
    |
    v
[GRAPH_EDGES table]  (now bidirectional for S1, S2, S8)
    ^
    |
[graph_enrichment_tick_s1_s2_s8]  (runs every tick going forward)
    |-- run_s1_tick: write_graph_edge(lower→higher) + write_graph_edge(higher→lower)
    |-- run_s2_tick: write_graph_edge(lower→higher) + write_graph_edge(higher→lower)
    |-- run_s8_tick: write_graph_edge(a→b)          + write_graph_edge(b→a)
    |
    v
[graph_expand]  (read path — unmodified logic)
    Outgoing BFS now reaches both directions after back-fill
    // SECURITY: comment added — documentation only, no logic change
```

---

## Shared Types / Constants (existing, unchanged)

```
EDGE_SOURCE_S1: &str = "S1"
EDGE_SOURCE_S2: &str = "S2"
EDGE_SOURCE_S8: &str = "S8"
CURRENT_SCHEMA_VERSION: u64 = 20   // bumped from 19 by migration_v19_v20

write_graph_edge(
    store: &Store,
    source_id: u64,
    target_id: u64,
    relation_type: &str,
    weight: f32,
    created_at: u64,
    source: &str,
    metadata: &str,
) -> bool
  // true  = row inserted (rows_affected = 1)
  // false = UNIQUE conflict (INSERT OR IGNORE, Ok path) or SQL error (warn! inside fn)
  // Callers MUST NOT warn or error-count on false. Budget counters increment on true only.

GRAPH_EDGES UNIQUE(source_id, target_id, relation_type)  // primary idempotency mechanism
```

---

## GRAPH_EDGES Table Structure (unchanged by this feature)

```
source_id       u64    FK → ENTRIES.id
target_id       u64    FK → ENTRIES.id
relation_type   TEXT   'Informs' | 'CoAccess' | 'Supports' | 'Supersedes' | 'Contradicts'
weight          REAL
created_at      INTEGER (unix timestamp)
created_by      TEXT
source          TEXT   'S1' | 'S2' | 'S8' | 'co_access' | 'nli' | 'cosine_supports' | ...
bootstrap_only  INTEGER (0 = live, 1 = bootstrap-only)
```

---

## Sequencing Constraints

1. `migration_v19_v20` and `graph_enrichment_tick_s1_s2_s8` MUST ship in the same binary. The
   migration fixes historical debt; the tick changes fix forward writes. Shipping only one of the
   two leaves partial bidirectionality.

2. `graph_expand_security_comment` is purely additive documentation. It can ship in the same
   commit for convenience; it has no sequencing dependency on the other two components.

3. **Pre-merge gate (R-02):** Confirm `CURRENT_SCHEMA_VERSION = 19` in the target branch before
   merging. If crt-043 has already shipped and consumed v20, renumber to v21 throughout.

---

## Explicit Exclusions (what this feature does NOT touch)

- `co_access_promotion_tick.rs` / `source='co_access'` — already bidirectional since crt-035
- NLI Informs edges (`source='nli'`) — intentionally unidirectional per col-030 ADR
- `source='cosine_supports'` — directionality out of scope
- `graph_expand` BFS logic, depth, or candidate cap — logic unchanged
- New columns or UNIQUE constraint changes on GRAPH_EDGES — structure unchanged

---

*Authored by crt-044-agent-1-pseudocode (claude-sonnet-4-6). Written 2026-04-03.*
