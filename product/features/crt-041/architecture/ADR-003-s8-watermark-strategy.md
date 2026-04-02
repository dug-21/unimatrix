## ADR-003: S8 Watermark Strategy — Counters Table, Write-After-Commit Order

### Context

S8 reads `audit_log` rows for `context_search` operations and extracts co-retrieved
pairs to write as CoAccess edges. The `audit_log` table has no per-row "processed"
flag and no S8-specific index. Without a watermark, every S8 batch would re-scan all
historical audit_log rows, producing quadratic growth in work and generating duplicate
pair candidates (harmless due to INSERT OR IGNORE, but wasteful).

Two watermark strategies were considered:

**Option A** — Timestamp-based window: query only rows where `timestamp > (now - window)`.
Simple, but a process restart at an inconvenient time could miss rows in the window
if the clock advances. Also requires the window to be long enough to catch all rows
from a batch that started before restart — harder to reason about correctness.

**Option B** — event_id watermark in the `counters` table: query only rows where
`event_id > last_watermark`. Update the watermark to `MAX(event_id processed)` after
each successful batch. The `counters` table is already used for `next_audit_event_id`
and other state — using it for S8 follows the established pattern. The event_id is
monotonically increasing (INTEGER PRIMARY KEY AUTOINCREMENT) — no clock dependency.

**Option B** is the correct approach. Entry #4026 (Unimatrix pattern) explicitly
documents this: "use a persistent watermark counter in the counters table, key:
's8_audit_log_watermark', tracking the last-processed event_id."

**Ordering invariant (write-after-commit):**

The watermark MUST be updated AFTER edge writes, not before:
1. Read watermark (or default 0 if absent)
2. SELECT audit_log rows with event_id > watermark ... ORDER BY event_id ASC LIMIT ?cap
3. For each row, parse target_ids JSON, build co-retrieved pairs
4. For each valid pair (both endpoints non-quarantined): `write_graph_edge(..., source='S8', weight=0.25)`
5. UPDATE watermark to MAX(event_id) from processed rows

If the process crashes between steps 4 and 5, the same rows are re-processed on the
next run. INSERT OR IGNORE ensures no duplicate edges are written. This is at-least-once
re-processing, documented in SR-03 as accepted behavior.

Writing the watermark BEFORE edge writes (wrong order) creates a gap: if edge writes
fail, those rows are permanently skipped without error. The correct order is mandatory.

**Malformed JSON handling (SR-08):**

`audit_log.target_ids` is a TEXT column storing a JSON array of u64. A prior bug
could write malformed JSON. On per-row JSON parse failure:
- Log the `event_id` at `tracing::warn!`
- Still advance the watermark past that row (include it in the MAX(event_id) calculation)
- Do NOT leave the watermark stuck behind a single malformed row indefinitely

Implementation: collect the event_ids of successfully-processed rows AND of parse-failed
rows. The watermark advances to the maximum event_id seen in the batch, regardless of
parse success. Only rows with valid JSON and valid pair extraction write edges.

**Filters applied to audit_log rows (non-negotiable):**
- `operation = 'context_search'` (AC-22: exclude context_briefing and all other ops)
- `outcome = 0` (AC-23: exclude failed/denied searches — outcome=0 is Success)
- `event_id > watermark` (watermark gate)
- `LIMIT ?max_s8_pairs_per_batch` applied at the SQL level on candidate rows
  (note: each row can produce up to N*(N-1)/2 pairs; the batch cap is on total pairs
  written, not rows fetched — see below)

**Batch cap semantics:**

`max_s8_pairs_per_batch` is a cap on total co-retrieved pairs processed, not on audit_log
rows fetched. A single search event returning 20 results yields 190 pairs. Implementation:
- Fetch up to `max_s8_pairs_per_batch * 2` audit_log rows (generous upper bound)
- Expand pairs and stop when the running pair total reaches `max_s8_pairs_per_batch`
- The watermark advances only to the last fully-processed row's event_id (to avoid
  marking a partially-processed row as done)

**Dual-endpoint quarantine filter:**

Before writing each pair, both entry IDs must be validated as non-quarantined. Options:
(a) One SQL query per pair to check status — O(pairs) round-trips
(b) Bulk fetch: after collecting all candidate pair IDs from JSON parsing, run a single
    `SELECT id FROM entries WHERE id IN (...) AND status != 3` query, build a HashSet,
    then filter pairs against the set before writing

Option (b) is preferred for latency. The bulk fetch uses sqlx `query_builder` to bind
the collected IDs. This reduces the watermark batch from O(pairs) round-trips to
O(1) fetch + O(pairs) inserts.

### Decision

Use the `counters` table with key `'s8_audit_log_watermark'` as the S8 state store.
The module constant `const S8_WATERMARK_KEY: &str = "s8_audit_log_watermark"` is
defined in `graph_enrichment_tick.rs` (not exported).

Watermark read: `counters::get(store.write_pool_server(), S8_WATERMARK_KEY).await`
→ `Ok(None)` = first run, start from event_id 0.

Watermark write: `counters::set(store.write_pool_server(), S8_WATERMARK_KEY, max_event_id).await`
→ error logged at `warn!`, tick continues (the same batch will be re-processed next run).

Ordering: edges written BEFORE watermark update.

Malformed JSON rows: log `event_id` at `warn!`, include in watermark advancement,
skip edge writes for that row.

Quarantine filter: bulk pre-fetch active entry IDs from the pair set, build a HashSet,
filter pairs before calling `write_graph_edge`.

S8 is only called from `run_single_tick` when `current_tick % config.s8_batch_interval_ticks == 0`.

### Consequences

Easier: At-least-once re-processing on crash is safe because INSERT OR IGNORE makes
edge writes idempotent. The watermark provides O(1) scan startup — no full-table scan
on every run. Malformed JSON rows are skipped and do not block future batches.

Harder: The watermark introduces shared mutable state in the `counters` table. Two
concurrent tick invocations writing the same watermark key would race — but `run_single_tick`
is single-threaded per tick loop, so this is not a risk in practice. The batch-cap
semantics (cap on pairs, not rows) require accumulating pairs in memory before capping,
which is bounded by `max_s8_pairs_per_batch * sizeof(pair) = 500 * 16 bytes = ~8KB` at
the default cap.
