## ADR-004 vnc-047: `RetrospectiveReport.tags` rides `summary_json`; `SUMMARY_SCHEMA_VERSION` 5→6 (version cascade #2); no back-fill

### Context

Read-back is via `context_cycle_review` (post-review) only (OQ-4 RESOLVED); no pre-review read surface.
`goal` already models the exact surfacing path (col-025 / col-026):

read `get_cycle_start_goal` → `report.goal` (tools.rs:3409-3428) → whole report serialized into
`summary_json` by `build_cycle_review_record` (tools.rs:4554) → JSON output automatic; markdown via
`render_goal_section` (retrospective.rs:203-217, called :49).

`summary_json` is the display copy that survives even the stale-purged memo path (where
`raw_signals_available = 0`), so tags must ride it, not be recomputed at render time. Adding a serde
field to `RetrospectiveReport` changes round-trip fidelity and requires bumping
`SUMMARY_SCHEMA_VERSION` (cycle_review_index.rs:54, pinned test :709-716). This is the recurring
gate-failure class in this codebase (#4153, #4373; SR-01) and is a **separate cascade** from ADR-001's
DB migration.

### Decision

**1. New store getter** `get_cycle_tags(feature_cycle: &str) -> Result<Vec<String>>` (db.rs), parity
with `get_cycle_start_goal` (db.rs:371): `SELECT tag FROM cycle_tags WHERE feature_cycle = ?1 ORDER BY
tag`. `ORDER BY tag` gives deterministic output — required for stable markdown and the pinned
round-trip test. Uses `idx_cycle_tags_tag`? No — the filter is on `feature_cycle` (the PK prefix), so
the PK index serves it; `ORDER BY tag` is satisfied by the PK's second column.

**2. New report field** on `RetrospectiveReport` (types.rs:382, after `goal` :436):

```rust
#[serde(default)]
pub tags: Vec<String>,
```

`Vec<String>` (not `Option`) with `#[serde(default)]`: a **required struct field** forces every
construction site of `RetrospectiveReport` to address `tags` — the compiler enforces the "three paths"
so a missed construction site is a build error, not a silent v6 fidelity gap (the exact SR-01 miss).
`#[serde(default)]` lets pre-v6 `summary_json` blobs (which lack the key) deserialize to an empty vec
(SR-10 no-back-fill support). The field is always serialized (no `skip_serializing_if`) so v6 blobs
round-trip deterministically for the pinned test.

**3. Review handler populate** — beside the goal set (tools.rs:3409-3428):
`report.tags = store.get_cycle_tags(&feature_cycle).await.unwrap_or_else(|e| { warn; Vec::new() })`.
Degrade-to-empty on DB error, `tracing::warn` (parity with the goal degrade arm :3425). Review never
fails on tag read.

**4. Markdown** — new `render_tags_section(&RetrospectiveReport) -> String` in retrospective.rs,
emitting a `## Tags` section (parity with `render_goal_section`), called immediately after
`render_goal_section` at retrospective.rs:49. A dedicated section (not a header-meta line) because tags
is a list, not a scalar; empty tags renders an explicit "No tags." line (SR-10 visibility). JSON output
is automatic once the serde field exists.

**5. Version cascade #2 — `SUMMARY_SCHEMA_VERSION` 5 → 6** (cycle_review_index.rs:54). Discrete
line-items (SR-01), independent of ADR-001's v31:
   - bump the const `5 → 6`;
   - update the pinned assertion (cycle_review_index.rs:709-716) and its message to reference vnc-047;
   - all `RetrospectiveReport` construction sites updated (compiler-enforced by the required field).
   This is a fidelity **stamp**, not a DB migration — no `cycle_review_index` `ALTER`, no
   `CURRENT_SCHEMA_VERSION` touch. Re-verify 6 is free at implementation start (SR-02).

**6. No back-fill (SR-10).** SUMMARY staleness is advisory-only; v5 reviews are not recomputed to v6
(#5022). Cycles reviewed before deployment show no tags; only runs whose review is computed post-deploy
carry them. Documented as intended, not a bug.

### Consequences

- Easier: tags survive the stale-purged memo path by riding `summary_json`; JSON is free.
- Easier: the required (non-Option) field turns the SR-01 "missed a construction path" risk into a
  compile error.
- Harder: this is the second independent version cascade; it must be tracked and tested separately from
  ADR-001 (three sub-steps + pinned test).
- Accepted (SR-10): historical reviews render `## Tags` empty forever; set human expectation.
- Cross-ref ADR-001 (schema cascade #1 — the other bump), ADR-002 (source of the data).
