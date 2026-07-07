## ADR-002: No in-memory invalidation — `entry_tags` is read strictly live

### Context
SR-01 (High): a direct `entry_tags` write bypasses `update()`, so any state DERIVED from tags — in-memory tag filters, ranking caches, tag-derived edges/indices — is not refreshed unless the op touches it. A missed derived surface means stale reads or silent integrity drift. The fast path's integrity rests on completeness here (#4372: schema-extension touches N surfaces at once; #3216: serving path silently used `::default()`). The full blast radius must be enumerated before the write is designed, not assumed.

### Decision
The op performs **no in-memory refresh or invalidation**, because every surface that reads tags reads them **live from SQL** on each query. This was verified exhaustively (ARCHITECTURE §3):
- Canonical hydration `load_tags_for_entries` + `apply_tags` (read.rs:111-159) is pure live SQL, no cache.
- All read/search/graph consumers call it per query (graph_read_filter.rs:186-209; read.rs get/list/by-tag; graph_queries.rs; search.rs via `entry_store.get()`).
- The `Arc<RwLock<_>>` analytics/ranking caches (server.rs:454; co-access search.rs:1163; PPR; contradiction cache coherence.rs:73) hold **derived ranking signals only — no tags**.
- **No** tag-derived edges exist (GRAPH_EDGES are not computed from `entry_tags`).
- The only tag-derived DB objects are the `idx_entry_tags_*` SQL indices, which SQLite maintains automatically on `INSERT`/`DELETE`.

Therefore a `context_tag` write is visible to the next tag-filtered read immediately, with no stale-index window and no invalidation call (AC-02).

### Consequences
- Easier: the op is a pure store write + audit; no cache-coherence code, no tick-rebuild lag.
- Bounded risk (A1): this conclusion holds ONLY while tags stay outside the content hash (hash.rs:7-16) and outside embedding input (embed/src/text.rs:10). If any future integrity or embedding path begins consuming tags, the fast path becomes integrity-unsafe and this ADR must be revisited — flagged as a carry-forward guard.
- Verification: a test asserting immediate read-back after a direct tag write pins the live-read property against future caching changes.
- Cross-references ADR-001 (the direct write), SR-01.
