## ADR-003: JSON Text Column for Keywords Storage

### Context

AC-13/AC-14 require storing up to 5 semantic keywords with the session. Open Question 2 in SCOPE asks: new column on sessions table or separate keywords table?

SR-05 warns that the storage schema must serve the future injection use case (keyword-driven semantic search) and not just persistence. SR-09 notes the sessions table is shared by the observation pipeline, retrospective, and knowledge effectiveness analysis.

The keywords are:
- Written once per session (on cycle_start)
- Read by the future injection pipeline to seed a semantic search query
- Never WHERE-filtered by individual keyword element
- Small cardinality (max 5 items, each max 64 chars)

This matches the criteria established by ADR-007 (nxs-008, Unimatrix #361): "Vec fields not queried by element stored as TEXT columns with JSON arrays."

### Decision

Add a single nullable TEXT column `keywords` to the `sessions` table containing a JSON array of strings.

Schema migration v11 -> v12:
```sql
ALTER TABLE sessions ADD COLUMN keywords TEXT;
```

`SessionRecord` gains `pub keywords: Option<String>` with `#[serde(default)]`. The value is stored as a JSON string (e.g., `'["observation pipeline","feature attribution"]'`). Application code serializes/deserializes via `serde_json::to_string`/`from_str`.

For the future injection use case: the injection pipeline reads `keywords`, deserializes the JSON array, and passes the strings to `context_search` as query terms. This is a read-then-deserialize pattern, not a SQL element-level query. A JSON column is sufficient.

### Consequences

**Easier:**
- Single ALTER TABLE migration, no new tables or joins.
- `update_session` closure pattern works unchanged -- just set `record.keywords = Some(json_string)`.
- Minimal blast radius to the sessions table schema (SR-09 mitigated).
- Consistent with established ADR-007 pattern for non-queried Vec fields.

**Harder:**
- If a future feature needs to query "all sessions that have keyword X", a `json_each()` virtual table join is needed (slower than indexed column). Acceptable: this query pattern is not anticipated.
- JSON parsing overhead on read. Negligible: max 5 short strings.
