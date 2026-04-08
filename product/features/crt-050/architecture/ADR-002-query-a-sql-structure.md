## ADR-002: Query A SQL Structure — observations JOIN entries with json_extract + CAST

### Context

The base aggregate query must extract entry IDs from `observations.input`, join
them to `entries` for category lookup, and group by `(phase, category, entry_id)`.
Several structural decisions are required:

**Column name:** The observations table schema (db.rs line 818) defines the
event-type column as `hook TEXT NOT NULL`. The in-memory `ObservationRecord`
uses `event_type` as a Rust field name, but that mapping is done at read time in
`observation.rs` and `listener.rs`. SQL queries against `observations` must use
`hook`, not `hook_event` or `event_type`. The SCOPE.md draft SQL incorrectly
uses `o.hook_event` — this is wrong and must be corrected to `o.hook`.

**Tool name filter:** Hook-path observations carry the prefixed form
`mcp__unimatrix__context_get` (confirmed in crt-049 / AC-06). Direct MCP calls
carry the bare form `context_get`. Both forms must be included in the `IN`
clause. No other prefix variants exist (confirmed crt-049 research). A 4-entry
`IN` clause is more explicit and more indexable than a REPLACE/SUBSTR approach.

**CAST mandatory:** `CAST(json_extract(input, '$.id') AS INTEGER)` is mandatory
in both the SELECT list and the JOIN predicate. Omitting the CAST causes a
text-to-integer JOIN mismatch returning zero rows silently (col-031 R-05,
documented in `query_log.rs`). The CAST correctly handles both integer-form
IDs (`{"id": 42}` → `CAST(42 AS INTEGER) = 42`) and string-form IDs
(`{"id": "42"}` → `CAST('42' AS INTEGER) = 42`), providing equivalent
coverage to crt-049 AC-16 at the SQL layer.

**ts_millis scaling:** `observations.ts_millis` is millisecond epoch.
`query_log.ts` is second epoch. The lookback boundary formula must multiply
by 1000: `ts_millis > (strftime('%s', 'now') - ?1 * 86400) * 1000`. Omitting
the `* 1000` produces a ~1000× too-narrow window (50 seconds of history instead
of 50,000 seconds) — a silent correctness error (SR-02).

**PreToolUse filter:** Without `o.hook = 'PreToolUse'`, any PostToolUse rows
for the same tool would double-count each read. The `hook` column stores the
event type string directly.

**json_extract NULL predicate:** `json_extract(o.input, '$.id') IS NOT NULL`
filters out filter-based `context_lookup` calls (which pass no `id` field) and
any rows with unparseable input. This is the single-ID predicate (AC-02).

### Decision

Query A SQL (canonical form):

```sql
SELECT o.phase,
       e.category,
       CAST(json_extract(o.input, '$.id') AS INTEGER) AS entry_id,
       COUNT(*) AS freq
FROM observations o
  JOIN entries e ON CAST(json_extract(o.input, '$.id') AS INTEGER) = e.id
WHERE o.phase IS NOT NULL
  AND o.hook = 'PreToolUse'
  AND o.tool IN ('context_get', 'mcp__unimatrix__context_get',
                 'context_lookup', 'mcp__unimatrix__context_lookup')
  AND json_extract(o.input, '$.id') IS NOT NULL
  AND o.ts_millis > (strftime('%s', 'now') - ?1 * 86400) * 1000
GROUP BY o.phase, e.category, entry_id
ORDER BY o.phase, e.category, freq DESC
```

- `o.hook = 'PreToolUse'` — correct column name (not `hook_event`)
- `?1` is bound as `i64` (sqlx 0.8 INTEGER mapping; `u32` binding would fail)
- `* 1000` on the RHS of the `ts_millis` predicate is mandatory
- Both `CAST` forms are mandatory; do not simplify

The function signature: `query_phase_freq_observations(lookback_days: u32) -> Result<Vec<PhaseFreqRow>>`.

The deserializer `row_to_phase_freq_row` is reused unchanged (column order
0:phase, 1:category, 2:entry_id, 3:freq matches the SELECT list).

### Consequences

- `hook = 'PreToolUse'` is a literal string equality check — not fragile to
  schema changes, but must be updated if the hook column value ever changes
  (unlikely given the existing corpus of observations).
- The IN clause with 4 literals is explicit and testable; new tool name variants
  would require an IN clause update (acceptable — no new variants are expected).
- The `* 1000` multiplier is visible in the SQL; the store function doc comment
  MUST note the ms-epoch contract to prevent future regressions (see ADR-006).
- Reusing `row_to_phase_freq_row` means no new deserializer is needed, reducing
  the diff surface.
