## ADR-005: `observations.topic_source` — Value Taxonomy With One Write Site Per Value; v27→v28 Pragma-Guarded Migration

### Context

The F6 (#682) hook.rs-retirement gate decides on `topic_source` distribution data — SR-04 warns that a wrong or ambiguous taxonomy now poisons that future decision; each value needs an exact write site and precedence-tier mapping, "no best-guess values". AC-05 fixes the vocabulary: `declared`/`extracted`/`registry-fill`/`vote`/NULL. The migration must follow the v9→v10 `topic_signal` precedent (migration.rs:219-237) under the #4092 pattern (pragma_table_info pre-check before ALTER; all checks before any ALTER when multi-column — here single-column). Current `CURRENT_SCHEMA_VERSION = 27`.

### Decision

1. **Column**: `observations.topic_source TEXT NULL` — additive, schema-only (never on the wire). Written at insert time only; **never updated afterward** (rows are immutable; session-level close/sweep resolution lands in `sessions.feature_cycle` as today and does not retro-stamp rows).
2. **Taxonomy — one value per code path** (decision tree implemented once in the shared record-path helper, ADR-004 §4):
   | Value | Exact write condition | Precedence tier |
   |---|---|---|
   | `'declared'` | `event.cycle_stamp` present; OR row is a CYCLE_* event; OR unstamped row whose registry feature has `FeatureSource::Declared` (declared override of extraction, and declared NULL-fill) | STAMP / declaration contract |
   | `'extracted'` | no stamp, extracted `topic_signal` used as-is (registry absent, featureless, or `Inferred` — extraction wins only against non-declared registry) | heuristic, write-time |
   | `'registry-fill'` | extracted None, filled from registry feature with `Inferred(Registered)` (SessionStart registration param) | heuristic, write-time floor |
   | `'vote'` | extracted None, filled from registry feature with `Inferred(Voted)` (eager-vote-set, #198) | heuristic, vote-derived |
   | NULL | no stamp, no extraction, no registry feature | UNATTRIBUTED |
   The F6 evidence split is `'declared'` vs everything else; `'vote'` vs `'registry-fill'` exists so vote retirement arguments cannot hide vote-derived fills inside the registry-fill bucket.
3. **Migration v27→v28** (`CURRENT_SCHEMA_VERSION = 28`):
   ```rust
   // v27 → v28: topic_source column on observations (vnc-030)
   if current_version < 28 {
       let has_topic_source: bool = sqlx::query_scalar::<_, i64>(
           "SELECT COUNT(*) FROM pragma_table_info('observations') WHERE name = 'topic_source'",
       ).fetch_one(&mut **txn).await.map(|c| c > 0).unwrap_or(false);
       if !has_topic_source {
           sqlx::query("ALTER TABLE observations ADD COLUMN topic_source TEXT")
               .execute(&mut **txn).await.map_err(...)?;
       }
   }
   ```
   Inside `run_main_migrations` (single transaction, version stamp at the end as today). No backfill: pre-vnc-030 rows stay NULL-source by design — the F6 gate measures the post-soak window, and inventing historical sources would be exactly the "best guess" SR-04 forbids. No index: F6 reads are offline aggregate scans.
4. **Insert surfaces**: `ObservationRow` gains `topic_source: Option<String>`; both listener-local INSERTs (listener.rs:3015, :3055) gain the column as `?10`. The store-crate `insert_observation` (observations.rs:82, used by hook IPC/tests) and other INSERT sites (analytics, export, background) are NOT extended — they don't sit on the attribution record path; their rows are NULL-source (taxonomy: not attributed by this pipeline). Delivery must grep-audit `INSERT INTO observations` to confirm no record-path site is missed (the #4372 multi-surface lesson).

### Consequences

Easier: the F6 retirement gate decides on per-row provenance data instead of vibes; ambiguity is structurally impossible — the value is computed by the same decision tree that sets `topic_signal`, so source and signal can never disagree; the migration is the lowest-risk change in the feature (twice-proven pattern). Harder: `'declared'` aggregates three sub-paths (stamp, cycle event, declared-registry) — if F6 ever needs that split, it joins against `cycle_stamp`-era session metadata or extends the vocabulary additively then; the no-backfill choice means distribution comparisons (SR-06's before/after check) must window on post-migration rows.

Cross-references: SCOPE AC-05, SR-04/SR-06, #4092, #4358, #1263/#1264, #4372, v9→v10 precedent (migration.rs:219-237), ADR-004 (decision tree), F6 #682.
