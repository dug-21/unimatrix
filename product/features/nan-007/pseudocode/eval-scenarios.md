# Pseudocode: eval/scenarios.rs (D2)

**Location**: `crates/unimatrix-server/src/eval/scenarios.rs`

## Purpose

Mine the `query_log` table from a snapshot database and produce a JSONL file of
`ScenarioRecord` objects. Each output line is a self-contained eval scenario that
`eval run` can replay. Supports filtering by retrieval source (`mcp`, `uds`, or `all`).

This module is read-only with respect to the snapshot. It never calls `SqlxStore::open()`.
It runs inside `block_export_sync` to bridge sync CLI dispatch to async sqlx.

## Dependencies

| Dependency | Location | Role |
|------------|----------|------|
| `block_export_sync` | `crates/unimatrix-server/src/export.rs` | Async-to-sync bridge |
| `sqlx::SqlitePool`, `SqliteConnectOptions` | sqlx | Raw read-only pool |
| `project::ensure_data_directory` | `crates/unimatrix-server/src/project.rs` | Live-DB path guard |
| `std::fs::canonicalize` | stdlib | Live-DB path guard |
| `serde_json` | serde_json | JSONL serialization |

## Types

### `ScenarioSource` (enum)

```
pub enum ScenarioSource {
    Mcp,   -- filter to query_log.source = "mcp"
    Uds,   -- filter to query_log.source = "uds"
    All,   -- no source filter
}

impl ScenarioSource:
  fn to_sql_filter(&self) -> Option<&'static str>:
    Mcp → Some("mcp")
    Uds → Some("uds")
    All → None

impl clap::ValueEnum for ScenarioSource:
  variants: [mcp, uds, all]  -- lowercase for CLI
```

### `ScenarioRecord` (serialized to JSONL)

```
pub struct ScenarioRecord {
    pub id: String,             -- "qlog-{row_id}" for query-log-sourced
    pub query: String,          -- query text from query_log.query_text
    pub context: ScenarioContext,
    pub baseline: Option<ScenarioBaseline>,
    pub source: String,         -- "mcp" | "uds"
    pub expected: Option<Vec<u64>>,  -- null for query-log-sourced scenarios
}

pub struct ScenarioContext {
    pub agent_id: String,
    pub feature_cycle: String,
    pub session_id: String,
    pub retrieval_mode: String,  -- "flexible" | "strict"
}

pub struct ScenarioBaseline {
    pub entry_ids: Vec<u64>,
    pub scores: Vec<f32>,  -- parallel to entry_ids; len must match
}
```

All fields derive `serde::Serialize`, `serde::Deserialize`.

## Function: `pub fn run_scenarios`

```
pub fn run_scenarios(
    db: &Path,
    source: ScenarioSource,
    limit: Option<usize>,
    out: &Path,
) -> Result<(), Box<dyn std::error::Error>>

BODY:
  1. Live-DB path guard (C-13, mirrors snapshot guard):
       paths = project::ensure_data_directory(None, None)?
         -- project_dir is None; eval scenarios does not require a project-dir arg
         --   because it does not modify the live DB.
         -- If ensure_data_directory fails (e.g. not in a unimatrix project):
         --   skip the guard and proceed (guard is best-effort when project unavailable)
         -- Preferred: accept optional project_dir arg; skip guard if None
       if paths resolved:
         active_db = canonicalize(paths.db_path)?
         db_resolved = canonicalize(db)?
         if db_resolved == active_db:
           return Err("eval scenarios --db resolves to the active database\n  use a snapshot")

  2. Validate --k is meaningful (not applicable here; k is for eval run)

  3. Validate --out parent directory exists:
       if out.parent() not exists:
         return Err format!("output directory does not exist: {}", out.parent().display())

  4. Bridge to async:
       block_export_sync(async {
         do_scenarios(db, source, limit, out).await
       })
```

## Function: `async fn do_scenarios` (private)

```
async fn do_scenarios(
    db: &Path,
    source: ScenarioSource,
    limit: Option<usize>,
    out: &Path,
) -> Result<(), Box<dyn std::error::Error>>

BODY:
  1. Open read-only pool (C-02, FR-11):
       opts = SqliteConnectOptions::new()
                .filename(db)
                .read_only(true)
       pool = SqlitePool::connect_with(opts).await?

  2. Open output file:
       file = File::create(out)?
       writer = BufWriter::new(file)

  3. Build SQL query with optional source filter:
       -- query_log schema assumed: id, query_text, agent_id, feature_cycle,
       --   session_id, retrieval_mode, source, result_entry_ids (JSON array text),
       --   similarity_scores (JSON array text), created_at
       --
       -- IMPLEMENTATION NOTE: Confirm exact column names against the actual
       --   query_log table schema in crates/unimatrix-store/src/migrations/

       base_sql = "
         SELECT
           ql.id,
           ql.query_text,
           ql.agent_id,
           ql.feature_cycle,
           ql.session_id,
           ql.retrieval_mode,
           ql.source,
           ql.result_entry_ids,
           ql.similarity_scores
         FROM query_log ql
         WHERE 1=1
       "

       source_clause = match source.to_sql_filter():
         Some(s) → format!(" AND ql.source = '{s}'")
         None    → ""

       limit_clause = match limit:
         Some(n) → format!(" LIMIT {n}")
         None    → ""

       full_sql = base_sql + source_clause + " ORDER BY ql.id ASC" + limit_clause

  4. Execute query and stream rows:
       rows = sqlx::query(&full_sql).fetch_all(&pool).await?
         -- fetch_all is used here for simplicity.
         -- For very large snapshots, fetch() (streaming) is preferred.
         -- Implementer may switch to fetch() if memory is a concern.

  5. For each row, build ScenarioRecord and write JSONL:
       scenario_count = 0
       for row in rows:
         record = build_scenario_record(row)?
         json_line = serde_json::to_string(&record)?
         writeln!(writer, "{json_line}")?
         scenario_count += 1

  6. Flush output:
       writer.flush()?

  7. Print stats to stderr:
       eprintln!("eval scenarios: wrote {scenario_count} scenarios to {}", out.display())

  8. Close pool:
       pool.close().await

  9. return Ok(())
```

## Function: `fn build_scenario_record` (private)

```
fn build_scenario_record(
    row: sqlx::sqlite::SqliteRow,
) -> Result<ScenarioRecord, Box<dyn std::error::Error>>

BODY:
  id_raw: i64   = row.try_get("id")?
  query_text    = row.try_get::<String, _>("query_text")?
  agent_id      = row.try_get::<String, _>("agent_id").unwrap_or_else(|_| "unknown".to_string())
  feature_cycle = row.try_get::<String, _>("feature_cycle").unwrap_or_else(|_| "".to_string())
  session_id    = row.try_get::<String, _>("session_id").unwrap_or_else(|_| "".to_string())
  retrieval_mode = row.try_get::<String, _>("retrieval_mode").unwrap_or_else(|_| "flexible".to_string())
  source        = row.try_get::<String, _>("source").unwrap_or_else(|_| "mcp".to_string())
  entry_ids_json = row.try_get::<Option<String>, _>("result_entry_ids")?.unwrap_or_default()
  scores_json    = row.try_get::<Option<String>, _>("similarity_scores")?.unwrap_or_default()

  -- Parse entry_ids and scores JSON arrays:
  entry_ids: Vec<u64> = if entry_ids_json.is_empty():
    Vec::new()
  else:
    serde_json::from_str(&entry_ids_json)
      .map_err(|e| format!("failed to parse result_entry_ids for row {id_raw}: {e}"))?

  scores: Vec<f32> = if scores_json.is_empty():
    Vec::new()
  else:
    serde_json::from_str(&scores_json)
      .map_err(|e| format!("failed to parse similarity_scores for row {id_raw}: {e}"))?

  -- Length parity check (R-16, RISK-TEST-STRATEGY):
  if !entry_ids.is_empty() && entry_ids.len() != scores.len():
    eprintln!(
      "WARN: query_log row {id_raw}: entry_ids.len()={} != scores.len()={}, truncating to min",
      entry_ids.len(), scores.len()
    )
    min_len = std::cmp::min(entry_ids.len(), scores.len())
    entry_ids = entry_ids[..min_len].to_vec()
    scores = scores[..min_len].to_vec()

  -- Build baseline (only when we have result data):
  baseline = if entry_ids.is_empty():
    None
  else:
    Some(ScenarioBaseline { entry_ids, scores })

  return Ok(ScenarioRecord {
    id: format!("qlog-{id_raw}"),
    query: query_text,
    context: ScenarioContext {
      agent_id,
      feature_cycle,
      session_id,
      retrieval_mode,
    },
    baseline,
    source,
    expected: None,  -- query-log-sourced scenarios never have hard labels
  })
```

## Edge Cases

| Condition | Behavior |
|-----------|----------|
| `query_log` table is empty | Exit 0; write empty JSONL file (zero lines); print "0 scenarios" |
| `result_entry_ids` is NULL in a row | baseline = None for that scenario |
| `entry_ids.len() != scores.len()` | Truncate to min length with WARN to stderr (R-16) |
| `source` filter produces zero results | Exit 0; empty JSONL |
| `--limit 0` | Produces empty JSONL (SQL LIMIT 0); not an error |
| Unicode in query_text | serde_json handles UTF-8 by default |
| Very large snapshot (50k rows) | fetch_all may use significant memory; implementer may switch to fetch() streaming |

## SQL Schema Assumption

The pseudocode assumes these `query_log` column names. Implementer must verify against
the actual migration in `crates/unimatrix-store/src/migrations/`:

```sql
-- Expected query_log schema:
CREATE TABLE query_log (
  id               INTEGER PRIMARY KEY,
  query_text       TEXT NOT NULL,
  agent_id         TEXT,
  feature_cycle    TEXT,
  session_id       TEXT,
  retrieval_mode   TEXT DEFAULT 'flexible',
  source           TEXT DEFAULT 'mcp',   -- 'mcp' | 'uds'
  result_entry_ids TEXT,  -- JSON array of integers, e.g. "[1, 2, 3]"
  similarity_scores TEXT, -- JSON array of floats, e.g. "[0.9, 0.8, 0.7]"
  created_at       INTEGER
);
```

If column names differ, update the SQL in `do_scenarios` accordingly.

## Error Handling

| Failure | Behavior |
|---------|----------|
| `db` path does not exist | `SqlitePool::connect_with` returns error; propagated |
| Output file creation fails | `File::create` error propagated |
| Row deserialization failure | Propagated for critical fields; default strings for optional fields |
| JSON parse failure for arrays | Err with row ID in message |
| Writer flush fails | Propagated |

## Key Test Scenarios

1. **Empty query_log**: `run_scenarios` exits 0; output file has zero lines (AC-03).

2. **Source filter = mcp**: only rows with source="mcp" in output; verify by parsing
   each line and asserting source field (AC-04, R-08).

3. **Source filter = uds**: only rows with source="uds" in output (AC-04).

4. **Source filter = all**: both source types present in output (AC-04).

5. **--limit 3**: output file has at most 3 lines (FR-08).

6. **Entry IDs / scores length parity**: inject a row with 3 entry_ids but 2 scores;
   assert output scenario has 2 entries in both arrays, with WARN in stderr (R-16).

7. **All required fields present**: parse every output line as JSON; assert `id`,
   `query`, `context`, `baseline` (or null), `source`, `expected` (null) all present
   with correct types (AC-03).

8. **Read-only enforcement**: snapshot SHA-256 unchanged after running `eval scenarios` (NFR-04).

9. **Live-DB path guard**: pass active DB as --db; assert non-zero exit (AC-16 pattern).

## Knowledge Stewardship

Queried: /uni-query-patterns for "evaluation harness patterns conventions" (category: pattern) — 5 results; #724 (Behavior-Based Ranking Tests: Assert Ordering Not Scores) is relevant to how test scenarios for this module should be structured. Pseudocode test scenarios follow this guidance: assertions check field presence and source filter correctness, not score values.
Queried: /uni-query-patterns for "snapshot vacuum database patterns" — #1097 (snapshot isolation ADR) confirms that read-only pool access to a snapshot is the correct pattern for scan-and-export workloads. Followed in do_scenarios() step 1.
Queried: /uni-query-patterns for "block_export_sync async bridge pattern" — established pattern confirmed; do_scenarios bridges sync CLI dispatch to async sqlx via block_export_sync, consistent with export.rs conventions.
Stored: nothing novel to store — pseudocode agents are read-only; patterns are consumed not created
