//! JSONL output and async execution for eval scenarios (nan-007).

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use sqlx::sqlite::SqliteConnectOptions;

use crate::export::block_export_sync;
use crate::project;

use super::extract::build_scenario_record;
use super::types::ScenarioSource;

/// Extract scenarios from a snapshot database and write JSONL to `out`.
///
/// This is the entry point called from `run_eval_command` via `main.rs`.
/// It is synchronous (pre-tokio dispatch per ADR-005) and bridges to async
/// sqlx via `block_export_sync`.
///
/// # Live-DB path guard (C-13)
///
/// If the supplied `db` path resolves to the active daemon database, an error
/// is returned before any I/O is performed. The guard is best-effort: if the
/// project directory is not found (e.g. CI without a home directory), the guard
/// is skipped and execution continues.
pub fn run_scenarios(
    db: &Path,
    source: ScenarioSource,
    limit: Option<usize>,
    out: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // ----------------------------------------------------------------
    // Step 1: Live-DB path guard (C-13, mirrors snapshot guard)
    // ----------------------------------------------------------------
    if let Ok(paths) = project::ensure_data_directory(None, None) {
        let active_db =
            std::fs::canonicalize(&paths.db_path).unwrap_or_else(|_| paths.db_path.clone());

        if let Ok(db_resolved) = std::fs::canonicalize(db) {
            if db_resolved == active_db {
                return Err(format!(
                    "eval scenarios --db resolves to the active database\n  \
                     supplied: {}\n  \
                     active:   {}\n  \
                     use a snapshot, not the live database",
                    db.display(),
                    active_db.display()
                )
                .into());
            }
        }
    }

    // ----------------------------------------------------------------
    // Step 2: Validate output parent directory exists
    // ----------------------------------------------------------------
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            return Err(format!("output directory does not exist: {}", parent.display()).into());
        }
    }

    // ----------------------------------------------------------------
    // Step 3: Bridge to async
    // ----------------------------------------------------------------
    block_export_sync(async { do_scenarios(db, source, limit, out).await })
}

/// Async body: open read-only pool, query, write JSONL.
async fn do_scenarios(
    db: &Path,
    source: ScenarioSource,
    limit: Option<usize>,
    out: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // ----------------------------------------------------------------
    // Open read-only pool (C-02, FR-11)
    // ----------------------------------------------------------------
    let opts = SqliteConnectOptions::new().filename(db).read_only(true);
    let pool = sqlx::SqlitePool::connect_with(opts).await?;

    // ----------------------------------------------------------------
    // Open output file
    // ----------------------------------------------------------------
    let file = File::create(out)?;
    let mut writer = BufWriter::new(file);

    // ----------------------------------------------------------------
    // Build SQL with optional source filter and limit
    //
    // Note: source filter uses a literal string comparison. Using string
    // interpolation here is safe because ScenarioSource::to_sql_filter()
    // only returns static string literals ("mcp" or "uds") — no user
    // input reaches the SQL directly.
    // ----------------------------------------------------------------
    let source_clause = match source.to_sql_filter() {
        Some(s) => format!(" AND source = '{s}'"),
        None => String::new(),
    };

    let limit_clause = match limit {
        Some(n) => format!(" LIMIT {n}"),
        None => String::new(),
    };

    let sql = format!(
        "SELECT query_id, session_id, query_text, retrieval_mode, source, \
                result_entry_ids, similarity_scores \
         FROM query_log \
         WHERE 1=1{source_clause} \
         ORDER BY query_id ASC{limit_clause}"
    );

    // ----------------------------------------------------------------
    // Execute query and stream rows
    // ----------------------------------------------------------------
    let rows = sqlx::query(&sql).fetch_all(&pool).await?;

    let mut scenario_count: usize = 0;

    for row in rows {
        let record = build_scenario_record(&row)?;
        let json_line = serde_json::to_string(&record)?;
        writeln!(writer, "{json_line}")?;
        scenario_count += 1;
    }

    // ----------------------------------------------------------------
    // Flush output
    // ----------------------------------------------------------------
    writer.flush()?;

    // ----------------------------------------------------------------
    // Report stats to stderr
    // ----------------------------------------------------------------
    eprintln!(
        "eval scenarios: wrote {scenario_count} scenarios to {}",
        out.display()
    );

    // ----------------------------------------------------------------
    // Close pool
    // ----------------------------------------------------------------
    pool.close().await;

    Ok(())
}
