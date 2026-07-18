//! Knowledge base export to JSONL format (nan-001).
//!
//! Exports the Unimatrix knowledge base to a portable JSONL file that preserves
//! every field needed for lossless knowledge restore. Covers 8 tables, excludes
//! derived data (embeddings, HNSW index) and ephemeral operational data.
//!
//! The export creates a tokio runtime and uses sqlx for all queries.
//! A single `BEGIN DEFERRED` transaction wraps all reads for snapshot isolation (ADR-001).

use std::collections::HashSet;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Map, Number, Value};
use sqlx::{Row, SqlitePool};
use unimatrix_store::{PoolConfig, SqlxStore};

use crate::project;
use crate::projects::slug_store::resolve_slug_store;

/// Run the export subcommand.
///
/// Opens the database, wraps the read in a single transaction for snapshot
/// consistency, and writes JSONL to `output` (or stdout if None).
///
/// When `slug` is `Some`, the store to export is the runtime's literal-slug store
/// (`{base}/<slug>/unimatrix.db`), resolved through the shared [`resolve_slug_store`]
/// funnel (validation + base derivation + pre-open existence gate, ADR-001/002).
/// When `slug` is `None`, the path-hash store flows byte-for-byte as today (AC-05).
///
/// When `skip_quarantined` is true (with `confirm`), quarantined entries
/// (status=3) and all rows referencing them are excluded from the export
/// (ADR-008). When `skip_quarantined` is true without `confirm`, the export
/// aborts immediately with a clear error message (ADR-009).
pub fn run_export(
    project_dir: Option<&Path>,
    output: Option<&Path>,
    slug: Option<&str>,
    skip_quarantined: bool,
    confirm: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    run_export_inner(project_dir, output, None, slug, skip_quarantined, confirm)
}

/// Run the export subcommand with an explicit `base_dir` for test isolation.
///
/// Identical to [`run_export`] but routes data storage to the given `base_dir`
/// instead of `~/.unimatrix/`. Use this in tests to avoid leaking directories
/// into the user's home directory.
pub fn run_export_with_base(
    project_dir: Option<&Path>,
    output: Option<&Path>,
    base_dir: &Path,
    slug: Option<&str>,
    skip_quarantined: bool,
    confirm: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    run_export_inner(
        project_dir,
        output,
        Some(base_dir),
        slug,
        skip_quarantined,
        confirm,
    )
}

fn run_export_inner(
    project_dir: Option<&Path>,
    output: Option<&Path>,
    base_dir: Option<&Path>,
    slug: Option<&str>,
    skip_quarantined: bool,
    confirm: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // ADR-009: --confirm validation BEFORE any DB access
    if skip_quarantined && !confirm {
        return Err(
            "--skip-quarantined produces a filtered export (quarantined entries and their \
             dependents are excluded). The export file will NOT be an exact copy of the \
             database. Add --confirm to acknowledge this and proceed."
                .into(),
        );
    }

    // 1. Resolve path-hash paths (unchanged; still creates + chmods the hash dir/vector, C-6).
    let paths = project::ensure_data_directory(project_dir, base_dir)?;

    // 2. Select the DB to open — the ONLY new branch. In slug mode the funnel
    //    validates the slug (AC-04), derives the base, joins the single site, and
    //    applies the pre-open existence gate (ADR-002): on a miss it fails loud
    //    naming the fully-resolved absolute db_path and creates nothing (AC-03,
    //    R-02). `?` here propagates that error BEFORE any SqlxStore::open, so the
    //    open below never auto-creates a slug store. No-slug is byte-for-byte the
    //    path-hash flow (AC-05/AC-11).
    let open_db_path = match slug {
        Some(raw) => resolve_slug_store(&paths, raw)?.db_path,
        None => paths.db_path.clone(),
    };

    // 3. Bridge async to sync: use block_in_place when inside a tokio runtime
    //    (e.g. called from tests), otherwise create a temporary runtime.
    //    Never use Builder::new_current_thread().block_on() inside an existing
    //    runtime — that panics with "Cannot start a runtime from within a runtime".
    block_export_sync(async {
        // 4. Open target DB (triggers migration if needed). In slug mode this is
        //    reached ONLY after the existence gate returned Ok (C-3, R-02).
        let store = Arc::new(SqlxStore::open(&open_db_path, PoolConfig::default()).await?);
        let pool = store.write_pool_server();

        // 5. Begin snapshot transaction (ADR-001)
        sqlx::query("BEGIN DEFERRED").execute(pool).await?;

        // 6. Build skip set INSIDE the transaction (ADR-008, SR-02)
        let skip_ids: HashSet<i64> = if skip_quarantined {
            sqlx::query_scalar::<_, i64>("SELECT id FROM entries WHERE status = 3")
                .fetch_all(pool)
                .await?
                .into_iter()
                .collect()
        } else {
            HashSet::new() // empty set -- O(1) contains() no-ops
        };

        // 7. Set up writer and run export, capturing the written row counts.
        let result = if let Some(path) = output {
            let file = File::create(path)?;
            let mut writer = BufWriter::new(file);
            do_export(pool, &mut writer, &skip_ids, skip_quarantined).await
        } else {
            let stdout = io::stdout();
            let lock = stdout.lock();
            let mut writer = BufWriter::new(lock);
            do_export(pool, &mut writer, &skip_ids, skip_quarantined).await
        };

        // 8. Commit transaction regardless of export result
        //    Read-only DEFERRED: COMMIT and ROLLBACK are equivalent.
        let _ = sqlx::query("COMMIT").execute(pool).await;

        // 9. AC-06 stderr count summary (both modes). `?` first so a failed export
        //    never prints a misleading count — the summary is success-only (ADR-006).
        let counts = result?;
        emit_export_summary(&counts, output);
        Ok(())
    })
}

/// Bridge an async future to a synchronous context.
///
/// When called from within a multi-thread tokio runtime (e.g. `#[tokio::test]`),
/// uses `block_in_place` so the current thread can block without stalling the runtime.
/// When called from a plain synchronous context (e.g. the CLI), creates a temporary
/// `current_thread` runtime.
///
/// Never use `Builder::new_current_thread().block_on()` inside an existing runtime —
/// that panics with "Cannot start a runtime from within a runtime".
///
/// `pub(crate)` so that sibling modules (`snapshot`, `eval/*`) can reuse the same
/// async-to-sync bridge without duplicating the runtime-detection logic.
pub(crate) fn block_export_sync<F>(fut: F) -> Result<(), Box<dyn std::error::Error>>
where
    F: std::future::Future<Output = Result<(), Box<dyn std::error::Error>>>,
{
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => tokio::task::block_in_place(|| handle.block_on(fut)),
        Err(_) => {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            rt.block_on(fut)
        }
    }
}

/// Row counts the export actually wrote, surfaced for the AC-06 stderr summary.
#[derive(Debug)]
struct ExportCounts {
    /// Knowledge entries written = total minus `--skip-quarantined` filtered.
    entries: u64,
    /// `audit_log` rows written.
    audit_rows: u64,
}

/// Format the one-line export summary (ADR-006 / FR-8):
/// `exported N entries, M audit rows → <dest>`, where `<dest>` is the resolved
/// output path or `stdout`. Pure (no I/O) so the wording is unit-testable without
/// capturing process stderr. `exported 0 entries` self-diagnoses a sparse export.
fn format_export_summary(counts: &ExportCounts, output: Option<&Path>) -> String {
    let dest = match output {
        Some(p) => p.display().to_string(),
        None => "stdout".to_string(),
    };
    format!(
        "exported {} entries, {} audit rows \u{2192} {}",
        counts.entries, counts.audit_rows, dest
    )
}

/// Emit the AC-06 count summary to STDERR — never stdout, so the JSONL piped on
/// stdout is unaffected (ADR-006). Called only after a successful export.
fn emit_export_summary(counts: &ExportCounts, output: Option<&Path>) {
    eprintln!("{}", format_export_summary(counts, output));
}

/// Execute all export steps against the pool and writer.
///
/// Separated from `run_export` to allow the writer type to vary (file vs stdout)
/// while keeping transaction logic in one place. Returns the [`ExportCounts`] the
/// AC-06 stderr summary reports.
///
/// `skip_ids` contains quarantined entry IDs to exclude (empty when
/// `--skip-quarantined` is not active). `skip_quarantined` controls
/// header metadata and skip-count reporting (ADR-008).
async fn do_export(
    pool: &SqlitePool,
    writer: &mut impl Write,
    skip_ids: &HashSet<i64>,
    skip_quarantined: bool,
) -> Result<ExportCounts, Box<dyn std::error::Error>> {
    write_header(pool, writer, skip_quarantined).await?;
    export_counters(pool, writer).await?;
    let skip_entries = export_entries(pool, writer, skip_ids).await?;
    let skip_tags = export_entry_tags(pool, writer, skip_ids).await?;
    let skip_co = export_co_access(pool, writer, skip_ids).await?;
    let skip_fe = export_feature_entries(pool, writer, skip_ids).await?;
    export_outcome_index(pool, writer).await?;
    export_agent_registry(pool, writer).await?;
    let audit_rows = export_audit_log(pool, writer).await?;
    // nxs-012: 3 additional tables (FR-14: after existing 8)
    let skip_edges = export_graph_edges(pool, writer, skip_ids).await?;
    export_observations(pool, writer).await?;
    export_cycle_events(pool, writer).await?;
    writer.flush()?;

    // Report skip counts to stderr (FR-27, AC-28)
    if skip_quarantined && !skip_ids.is_empty() {
        eprintln!("Skipped {} quarantined entries.", skip_ids.len());
        eprintln!("Skipped dependent rows:");
        eprintln!("  Entry tags:      {}", skip_tags);
        eprintln!("  Co-access pairs: {}", skip_co);
        eprintln!("  Feature entries: {}", skip_fe);
        eprintln!("  Graph edges:     {}", skip_edges);
    }

    // AC-06 / FR-8: entries actually written = total - skipped. `export_entries`
    // returns the SKIPPED count (its contract is unchanged, NFR-9), so derive the
    // written count from the in-txn total. A `--skip-quarantined` sparse export
    // therefore self-diagnoses as "exported 0 entries".
    let total_entries: i64 = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM entries")
        .fetch_one(pool)
        .await?;
    let entries = (total_entries.max(0) as u64).saturating_sub(skip_entries);

    Ok(ExportCounts {
        entries,
        audit_rows,
    })
}

/// Write the JSONL header line with export metadata.
///
/// Queries schema_version from counters and COUNT(*) from entries.
/// Key order: _header, schema_version, exported_at, entry_count, format_version,
/// and optionally skip_quarantined (R-24).
async fn write_header(
    pool: &SqlitePool,
    writer: &mut impl Write,
    skip_quarantined: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let schema_version: i64 =
        sqlx::query_scalar::<_, i64>("SELECT value FROM counters WHERE name = 'schema_version'")
            .fetch_one(pool)
            .await?;

    let entry_count: i64 = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM entries")
        .fetch_one(pool)
        .await?;

    let exported_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let mut map = Map::new();
    map.insert("_header".to_string(), Value::Bool(true));
    map.insert(
        "schema_version".to_string(),
        Value::Number(schema_version.into()),
    );
    map.insert("exported_at".to_string(), Value::Number(exported_at.into()));
    map.insert("entry_count".to_string(), Value::Number(entry_count.into()));
    map.insert("format_version".to_string(), Value::Number(2.into()));

    // Optional: indicate this is a filtered export (R-24)
    if skip_quarantined {
        map.insert("skip_quarantined".to_string(), Value::Bool(true));
    }

    let line = serde_json::to_string(&Value::Object(map))?;
    writeln!(writer, "{line}")?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers for row serialization
// ---------------------------------------------------------------------------

/// Serialize a `Map` as a single JSONL line.
fn write_row(
    map: Map<String, Value>,
    writer: &mut impl Write,
) -> Result<(), Box<dyn std::error::Error>> {
    let line = serde_json::to_string(&Value::Object(map))?;
    writeln!(writer, "{line}")?;
    Ok(())
}

/// Extract a nullable INTEGER column as `Value::Number` or `Value::Null`.
fn nullable_int(row: &sqlx::sqlite::SqliteRow, idx: usize) -> Value {
    match row.get::<Option<i64>, _>(idx) {
        Some(v) => Value::Number(v.into()),
        None => Value::Null,
    }
}

/// Extract a nullable TEXT column as `Value::String` or `Value::Null`.
fn nullable_text(row: &sqlx::sqlite::SqliteRow, idx: usize) -> Value {
    match row.get::<Option<String>, _>(idx) {
        Some(s) => Value::String(s),
        None => Value::Null,
    }
}

// ---------------------------------------------------------------------------
// Per-table export functions (ADR-002: explicit column-to-JSON mapping)
// ---------------------------------------------------------------------------

/// Export all rows from the `counters` table.
///
/// Columns: name (TEXT PK), value (INTEGER NOT NULL).
/// Order: name ASC.
async fn export_counters(
    pool: &SqlitePool,
    writer: &mut impl Write,
) -> Result<(), Box<dyn std::error::Error>> {
    let rows = sqlx::query("SELECT name, value FROM counters ORDER BY name")
        .fetch_all(pool)
        .await?;
    for row in &rows {
        let mut map = Map::new();
        map.insert("_table".into(), Value::String("counters".into()));
        map.insert("name".into(), Value::String(row.get::<String, _>(0)));
        map.insert("value".into(), Value::Number(row.get::<i64, _>(1).into()));
        write_row(map, writer)?;
    }
    Ok(())
}

/// Export all rows from the `entries` table (26 columns).
///
/// Order: id ASC. Nullable columns emit JSON null for SQL NULL.
/// Confidence (REAL) uses `Number::from_f64` with NaN fallback to 0 (ADR-002).
/// Returns the count of skipped rows (ADR-008).
async fn export_entries(
    pool: &SqlitePool,
    writer: &mut impl Write,
    skip_ids: &HashSet<i64>,
) -> Result<u64, Box<dyn std::error::Error>> {
    let rows = sqlx::query(
        "SELECT id, title, content, topic, category, source, status, confidence,
                created_at, updated_at, last_accessed_at, access_count,
                supersedes, superseded_by, correction_count, embedding_dim,
                created_by, modified_by, content_hash, previous_hash,
                version, feature_cycle, trust_source,
                helpful_count, unhelpful_count, pre_quarantine_status
         FROM entries ORDER BY id",
    )
    .fetch_all(pool)
    .await?;

    let mut skipped: u64 = 0;

    for row in &rows {
        let id: i64 = row.get::<i64, _>(0);

        // Skip quarantined entries (ADR-008)
        if skip_ids.contains(&id) {
            skipped += 1;
            continue;
        }

        let mut map = Map::new();
        map.insert("_table".into(), Value::String("entries".into()));
        // INTEGER NOT NULL (PK)
        map.insert("id".into(), Value::Number(row.get::<i64, _>(0).into()));
        // TEXT NOT NULL columns
        map.insert("title".into(), Value::String(row.get::<String, _>(1)));
        map.insert("content".into(), Value::String(row.get::<String, _>(2)));
        map.insert("topic".into(), Value::String(row.get::<String, _>(3)));
        map.insert("category".into(), Value::String(row.get::<String, _>(4)));
        map.insert("source".into(), Value::String(row.get::<String, _>(5)));
        // INTEGER NOT NULL
        map.insert("status".into(), Value::Number(row.get::<i64, _>(6).into()));
        // REAL NOT NULL (f64) -- NaN safety per ADR-002
        let confidence: f64 = row.get::<f64, _>(7);
        map.insert(
            "confidence".into(),
            Value::Number(Number::from_f64(confidence).unwrap_or(Number::from(0))),
        );
        // INTEGER NOT NULL timestamps
        map.insert(
            "created_at".into(),
            Value::Number(row.get::<i64, _>(8).into()),
        );
        map.insert(
            "updated_at".into(),
            Value::Number(row.get::<i64, _>(9).into()),
        );
        map.insert(
            "last_accessed_at".into(),
            Value::Number(row.get::<i64, _>(10).into()),
        );
        map.insert(
            "access_count".into(),
            Value::Number(row.get::<i64, _>(11).into()),
        );
        // INTEGER nullable
        map.insert("supersedes".into(), nullable_int(row, 12));
        map.insert("superseded_by".into(), nullable_int(row, 13));
        // INTEGER NOT NULL
        map.insert(
            "correction_count".into(),
            Value::Number(row.get::<i64, _>(14).into()),
        );
        map.insert(
            "embedding_dim".into(),
            Value::Number(row.get::<i64, _>(15).into()),
        );
        // TEXT NOT NULL
        map.insert("created_by".into(), Value::String(row.get::<String, _>(16)));
        map.insert(
            "modified_by".into(),
            Value::String(row.get::<String, _>(17)),
        );
        map.insert(
            "content_hash".into(),
            Value::String(row.get::<String, _>(18)),
        );
        map.insert(
            "previous_hash".into(),
            Value::String(row.get::<String, _>(19)),
        );
        // INTEGER NOT NULL
        map.insert(
            "version".into(),
            Value::Number(row.get::<i64, _>(20).into()),
        );
        // TEXT NOT NULL
        map.insert(
            "feature_cycle".into(),
            Value::String(row.get::<String, _>(21)),
        );
        map.insert(
            "trust_source".into(),
            Value::String(row.get::<String, _>(22)),
        );
        // INTEGER NOT NULL
        map.insert(
            "helpful_count".into(),
            Value::Number(row.get::<i64, _>(23).into()),
        );
        map.insert(
            "unhelpful_count".into(),
            Value::Number(row.get::<i64, _>(24).into()),
        );
        // INTEGER nullable
        map.insert("pre_quarantine_status".into(), nullable_int(row, 25));

        write_row(map, writer)?;
    }
    Ok(skipped)
}

/// Export all rows from the `entry_tags` table.
///
/// Columns: entry_id (INTEGER), tag (TEXT). Order: entry_id ASC, tag ASC.
/// Returns the count of skipped rows (ADR-008).
async fn export_entry_tags(
    pool: &SqlitePool,
    writer: &mut impl Write,
    skip_ids: &HashSet<i64>,
) -> Result<u64, Box<dyn std::error::Error>> {
    let rows = sqlx::query("SELECT entry_id, tag FROM entry_tags ORDER BY entry_id, tag")
        .fetch_all(pool)
        .await?;

    let mut skipped: u64 = 0;

    for row in &rows {
        let entry_id: i64 = row.get::<i64, _>(0);

        if skip_ids.contains(&entry_id) {
            skipped += 1;
            continue;
        }

        let mut map = Map::new();
        map.insert("_table".into(), Value::String("entry_tags".into()));
        map.insert("entry_id".into(), Value::Number(entry_id.into()));
        map.insert("tag".into(), Value::String(row.get::<String, _>(1)));
        write_row(map, writer)?;
    }
    Ok(skipped)
}

/// Export all rows from the `co_access` table.
///
/// Columns: entry_id_a, entry_id_b, count, last_updated (all INTEGER NOT NULL).
/// Order: entry_id_a ASC, entry_id_b ASC.
/// Both entry_id_a and entry_id_b are checked against skip_ids (R-19, FR-23).
/// Returns the count of skipped rows (ADR-008).
async fn export_co_access(
    pool: &SqlitePool,
    writer: &mut impl Write,
    skip_ids: &HashSet<i64>,
) -> Result<u64, Box<dyn std::error::Error>> {
    let rows = sqlx::query(
        "SELECT entry_id_a, entry_id_b, count, last_updated
         FROM co_access ORDER BY entry_id_a, entry_id_b",
    )
    .fetch_all(pool)
    .await?;

    let mut skipped: u64 = 0;

    for row in &rows {
        let entry_id_a: i64 = row.get::<i64, _>(0);
        let entry_id_b: i64 = row.get::<i64, _>(1);

        // BOTH sides checked (R-19, FR-23)
        if skip_ids.contains(&entry_id_a) || skip_ids.contains(&entry_id_b) {
            skipped += 1;
            continue;
        }

        let mut map = Map::new();
        map.insert("_table".into(), Value::String("co_access".into()));
        map.insert("entry_id_a".into(), Value::Number(entry_id_a.into()));
        map.insert("entry_id_b".into(), Value::Number(entry_id_b.into()));
        map.insert("count".into(), Value::Number(row.get::<i64, _>(2).into()));
        map.insert(
            "last_updated".into(),
            Value::Number(row.get::<i64, _>(3).into()),
        );
        write_row(map, writer)?;
    }
    Ok(skipped)
}

/// Export all rows from the `feature_entries` table.
///
/// Columns: feature_id (TEXT), entry_id (INTEGER). Order: feature_id ASC, entry_id ASC.
/// Returns the count of skipped rows (ADR-008).
async fn export_feature_entries(
    pool: &SqlitePool,
    writer: &mut impl Write,
    skip_ids: &HashSet<i64>,
) -> Result<u64, Box<dyn std::error::Error>> {
    let rows = sqlx::query(
        "SELECT feature_id, entry_id FROM feature_entries ORDER BY feature_id, entry_id",
    )
    .fetch_all(pool)
    .await?;

    let mut skipped: u64 = 0;

    for row in &rows {
        let entry_id: i64 = row.get::<i64, _>(1); // entry_id is column index 1

        if skip_ids.contains(&entry_id) {
            skipped += 1;
            continue;
        }

        let mut map = Map::new();
        map.insert("_table".into(), Value::String("feature_entries".into()));
        map.insert("feature_id".into(), Value::String(row.get::<String, _>(0)));
        map.insert("entry_id".into(), Value::Number(entry_id.into()));
        write_row(map, writer)?;
    }
    Ok(skipped)
}

/// Export all rows from the `outcome_index` table.
///
/// Columns: feature_cycle (TEXT), entry_id (INTEGER). Order: feature_cycle ASC, entry_id ASC.
async fn export_outcome_index(
    pool: &SqlitePool,
    writer: &mut impl Write,
) -> Result<(), Box<dyn std::error::Error>> {
    let rows = sqlx::query(
        "SELECT feature_cycle, entry_id FROM outcome_index ORDER BY feature_cycle, entry_id",
    )
    .fetch_all(pool)
    .await?;
    for row in &rows {
        let mut map = Map::new();
        map.insert("_table".into(), Value::String("outcome_index".into()));
        map.insert(
            "feature_cycle".into(),
            Value::String(row.get::<String, _>(0)),
        );
        map.insert(
            "entry_id".into(),
            Value::Number(row.get::<i64, _>(1).into()),
        );
        write_row(map, writer)?;
    }
    Ok(())
}

/// Export all rows from the `agent_registry` table.
///
/// Columns: agent_id (TEXT PK), trust_level (INTEGER), capabilities (TEXT, JSON-in-TEXT),
/// allowed_topics (TEXT nullable, JSON-in-TEXT), allowed_categories (TEXT nullable, JSON-in-TEXT),
/// enrolled_at (INTEGER), last_seen_at (INTEGER), active (INTEGER).
/// Order: agent_id ASC.
///
/// JSON-in-TEXT columns are emitted as raw strings, not parsed/re-encoded (ADR-002).
async fn export_agent_registry(
    pool: &SqlitePool,
    writer: &mut impl Write,
) -> Result<(), Box<dyn std::error::Error>> {
    let rows = sqlx::query(
        "SELECT agent_id, trust_level, capabilities, allowed_topics,
                allowed_categories, enrolled_at, last_seen_at, active
         FROM agent_registry ORDER BY agent_id",
    )
    .fetch_all(pool)
    .await?;
    for row in &rows {
        let mut map = Map::new();
        map.insert("_table".into(), Value::String("agent_registry".into()));
        map.insert("agent_id".into(), Value::String(row.get::<String, _>(0)));
        map.insert(
            "trust_level".into(),
            Value::Number(row.get::<i64, _>(1).into()),
        );
        // JSON-in-TEXT: emitted as string, not parsed
        map.insert(
            "capabilities".into(),
            Value::String(row.get::<String, _>(2)),
        );
        // Nullable JSON-in-TEXT
        map.insert("allowed_topics".into(), nullable_text(row, 3));
        map.insert("allowed_categories".into(), nullable_text(row, 4));
        map.insert(
            "enrolled_at".into(),
            Value::Number(row.get::<i64, _>(5).into()),
        );
        map.insert(
            "last_seen_at".into(),
            Value::Number(row.get::<i64, _>(6).into()),
        );
        map.insert("active".into(), Value::Number(row.get::<i64, _>(7).into()));
        write_row(map, writer)?;
    }
    Ok(())
}

/// Export all rows from the `audit_log` table.
///
/// Columns: event_id (INTEGER PK), timestamp (INTEGER), session_id (TEXT),
/// agent_id (TEXT), operation (TEXT), target_ids (TEXT, JSON-in-TEXT),
/// outcome (INTEGER), detail (TEXT).
/// Order: event_id ASC.
///
/// The `target_ids` column is JSON-in-TEXT: emitted as a raw string (ADR-002).
/// Returns the count of rows written (the AC-06 summary's audit-row number).
async fn export_audit_log(
    pool: &SqlitePool,
    writer: &mut impl Write,
) -> Result<u64, Box<dyn std::error::Error>> {
    let rows = sqlx::query(
        "SELECT event_id, timestamp, session_id, agent_id, operation,
                target_ids, outcome, detail
         FROM audit_log ORDER BY event_id",
    )
    .fetch_all(pool)
    .await?;
    for row in &rows {
        let mut map = Map::new();
        map.insert("_table".into(), Value::String("audit_log".into()));
        map.insert(
            "event_id".into(),
            Value::Number(row.get::<i64, _>(0).into()),
        );
        map.insert(
            "timestamp".into(),
            Value::Number(row.get::<i64, _>(1).into()),
        );
        map.insert("session_id".into(), Value::String(row.get::<String, _>(2)));
        map.insert("agent_id".into(), Value::String(row.get::<String, _>(3)));
        map.insert("operation".into(), Value::String(row.get::<String, _>(4)));
        // JSON-in-TEXT: emitted as string, not parsed
        map.insert("target_ids".into(), Value::String(row.get::<String, _>(5)));
        map.insert("outcome".into(), Value::Number(row.get::<i64, _>(6).into()));
        map.insert("detail".into(), Value::String(row.get::<String, _>(7)));
        write_row(map, writer)?;
    }
    Ok(rows.len() as u64)
}

/// Export all rows from the `graph_edges` table (9 columns, no id — ADR-005).
///
/// Columns: source_id, target_id, relation_type, weight, created_at, created_by,
/// source, bootstrap_only, metadata.
/// Order: source_id ASC, target_id ASC, relation_type ASC.
/// Weight uses `Number::from_f64` with NaN/Inf fallback to 1.0 (ADR-003).
/// Both source_id and target_id are checked against skip_ids (R-20, FR-24).
/// Returns the count of skipped rows (ADR-008).
async fn export_graph_edges(
    pool: &SqlitePool,
    writer: &mut impl Write,
    skip_ids: &HashSet<i64>,
) -> Result<u64, Box<dyn std::error::Error>> {
    let rows = sqlx::query(
        "SELECT source_id, target_id, relation_type, weight,
                created_at, created_by, source, bootstrap_only, metadata
         FROM graph_edges
         ORDER BY source_id, target_id, relation_type",
    )
    .fetch_all(pool)
    .await?;

    let mut skipped: u64 = 0;

    for row in &rows {
        let source_id: i64 = row.get::<i64, _>(0);
        let target_id: i64 = row.get::<i64, _>(1);

        // BOTH sides checked (R-20, FR-24)
        if skip_ids.contains(&source_id) || skip_ids.contains(&target_id) {
            skipped += 1;
            continue;
        }

        let mut map = Map::new();
        map.insert("_table".into(), Value::String("graph_edges".into()));
        map.insert("source_id".into(), Value::Number(source_id.into()));
        map.insert("target_id".into(), Value::Number(target_id.into()));
        map.insert(
            "relation_type".into(),
            Value::String(row.get::<String, _>(2)),
        );
        // f64 weight with NaN safety (ADR-003): fallback to 1.0, not 0
        let weight: f64 = row.get::<f64, _>(3);
        map.insert(
            "weight".into(),
            Value::Number(
                Number::from_f64(weight)
                    .unwrap_or_else(|| Number::from_f64(1.0).expect("1.0 is valid f64")),
            ),
        );
        map.insert(
            "created_at".into(),
            Value::Number(row.get::<i64, _>(4).into()),
        );
        map.insert("created_by".into(), Value::String(row.get::<String, _>(5)));
        map.insert("source".into(), Value::String(row.get::<String, _>(6)));
        map.insert(
            "bootstrap_only".into(),
            Value::Number(row.get::<i64, _>(7).into()),
        );
        map.insert("metadata".into(), nullable_text(row, 8));
        write_row(map, writer)?;
    }
    Ok(skipped)
}

/// Export all rows from the `observations` table (10 columns, id preserved — ADR-006).
///
/// Columns: id, session_id, ts_millis, hook, tool, input, response_size,
/// response_snippet, topic_signal, phase.
/// Order: id ASC.
async fn export_observations(
    pool: &SqlitePool,
    writer: &mut impl Write,
) -> Result<(), Box<dyn std::error::Error>> {
    let rows = sqlx::query(
        "SELECT id, session_id, ts_millis, hook, tool, input,
                response_size, response_snippet, topic_signal, phase
         FROM observations
         ORDER BY id",
    )
    .fetch_all(pool)
    .await?;

    for row in &rows {
        let mut map = Map::new();
        map.insert("_table".into(), Value::String("observations".into()));
        map.insert("id".into(), Value::Number(row.get::<i64, _>(0).into()));
        map.insert("session_id".into(), Value::String(row.get::<String, _>(1)));
        map.insert(
            "ts_millis".into(),
            Value::Number(row.get::<i64, _>(2).into()),
        );
        map.insert("hook".into(), Value::String(row.get::<String, _>(3)));
        map.insert("tool".into(), nullable_text(row, 4));
        map.insert("input".into(), nullable_text(row, 5));
        map.insert("response_size".into(), nullable_int(row, 6));
        map.insert("response_snippet".into(), nullable_text(row, 7));
        map.insert("topic_signal".into(), nullable_text(row, 8));
        map.insert("phase".into(), nullable_text(row, 9));
        write_row(map, writer)?;
    }
    Ok(())
}

/// Export all rows from the `cycle_events` table (9 columns, id preserved — ADR-006).
///
/// Columns: id, cycle_id, seq, event_type, phase, outcome, next_phase,
/// timestamp, goal. goal_embedding excluded from SELECT (ADR-004).
/// Order: id ASC.
async fn export_cycle_events(
    pool: &SqlitePool,
    writer: &mut impl Write,
) -> Result<(), Box<dyn std::error::Error>> {
    let rows = sqlx::query(
        "SELECT id, cycle_id, seq, event_type, phase, outcome,
                next_phase, timestamp, goal
         FROM cycle_events
         ORDER BY id",
    )
    .fetch_all(pool)
    .await?;

    for row in &rows {
        let mut map = Map::new();
        map.insert("_table".into(), Value::String("cycle_events".into()));
        map.insert("id".into(), Value::Number(row.get::<i64, _>(0).into()));
        map.insert("cycle_id".into(), Value::String(row.get::<String, _>(1)));
        map.insert("seq".into(), Value::Number(row.get::<i64, _>(2).into()));
        map.insert("event_type".into(), Value::String(row.get::<String, _>(3)));
        map.insert("phase".into(), nullable_text(row, 4));
        map.insert("outcome".into(), nullable_text(row, 5));
        map.insert("next_phase".into(), nullable_text(row, 6));
        map.insert(
            "timestamp".into(),
            Value::Number(row.get::<i64, _>(7).into()),
        );
        map.insert("goal".into(), nullable_text(row, 8));
        write_row(map, writer)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use unimatrix_store::test_helpers::open_test_store;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Create a fresh database in a temp directory and return (store, pool, temp_dir).
    async fn setup_test_db() -> (Arc<SqlxStore>, tempfile::TempDir) {
        let tmp = tempfile::TempDir::new().expect("create temp dir");
        let store = Arc::new(open_test_store(&tmp).await);
        (store, tmp)
    }

    /// Parse the first non-empty line from a buffer as JSON.
    fn parse_line(buf: &[u8]) -> Value {
        let s = std::str::from_utf8(buf).unwrap();
        let line = s.lines().next().unwrap();
        serde_json::from_str(line).unwrap()
    }

    /// Parse all non-empty lines from a buffer as JSON values.
    fn parse_lines(buf: &[u8]) -> Vec<Value> {
        let s = std::str::from_utf8(buf).unwrap();
        s.lines()
            .filter(|l| !l.is_empty())
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    // -----------------------------------------------------------------------
    // Export-module agent tests (header, orchestration)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_write_header_fields_correct() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        let mut buf = Vec::new();

        write_header(pool, &mut buf, false)
            .await
            .expect("write_header");

        let line = String::from_utf8(buf).expect("utf8");
        let val: Value = serde_json::from_str(line.trim()).expect("parse json");
        let obj = val.as_object().expect("object");

        assert_eq!(obj.get("_header"), Some(&Value::Bool(true)));
        assert!(
            obj.get("schema_version")
                .expect("schema_version")
                .is_number()
        );
        assert!(obj.get("exported_at").expect("exported_at").is_number());
        assert_eq!(obj.get("entry_count"), Some(&Value::Number(0.into())));
        assert_eq!(obj.get("format_version"), Some(&Value::Number(2.into())));
        assert_eq!(obj.len(), 5);
    }

    #[tokio::test]
    async fn test_write_header_exported_at_recent() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        let mut buf = Vec::new();

        let before = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        write_header(pool, &mut buf, false)
            .await
            .expect("write_header");

        let after = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let val: Value = serde_json::from_str(String::from_utf8(buf).unwrap().trim()).unwrap();
        let exported_at = val["exported_at"].as_i64().unwrap();

        assert!(exported_at >= before);
        assert!(exported_at <= after);
    }

    #[tokio::test]
    async fn test_do_export_empty_db() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        let mut buf = Vec::new();

        do_export(pool, &mut buf, &HashSet::new(), false)
            .await
            .expect("do_export");

        let output = String::from_utf8(buf).expect("utf8");
        let lines: Vec<&str> = output.lines().collect();

        assert!(!lines.is_empty(), "should have at least a header line");

        let header: Value = serde_json::from_str(lines[0]).expect("parse header");
        assert_eq!(header["_header"], Value::Bool(true));
        assert_eq!(header["entry_count"], Value::Number(0.into()));
    }

    #[tokio::test]
    async fn test_do_export_all_lines_valid_json() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        let mut buf = Vec::new();

        do_export(pool, &mut buf, &HashSet::new(), false)
            .await
            .expect("do_export");

        let output = String::from_utf8(buf).expect("utf8");
        for line in output.lines() {
            let _: Value = serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("invalid JSON line: {e}: {line}"));
        }
    }

    #[test]
    fn test_run_export_to_file() {
        let _tmp = tempfile::TempDir::new().expect("create temp dir");
        // run_export needs ensure_data_directory which uses project root detection.
        // For unit tests, we test do_export directly. File output is tested via
        // integration tests that set up proper project directories.
    }

    #[tokio::test]
    async fn test_header_key_order_preserved() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        let mut buf = Vec::new();

        write_header(pool, &mut buf, false)
            .await
            .expect("write_header");

        let output = String::from_utf8(buf).expect("utf8");
        let val: Value = serde_json::from_str(output.trim()).expect("parse json");
        let obj = val.as_object().expect("object");

        let keys: Vec<&String> = obj.keys().collect();
        assert_eq!(
            keys,
            vec![
                "_header",
                "schema_version",
                "exported_at",
                "entry_count",
                "format_version"
            ]
        );
    }

    // -----------------------------------------------------------------------
    // T-RS-01: All 26 entry columns present with correct values
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_export_entries_all_26_columns_present() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        sqlx::query(
            "INSERT INTO entries (
                id, title, content, topic, category, source, status, confidence,
                created_at, updated_at, last_accessed_at, access_count,
                supersedes, superseded_by, correction_count, embedding_dim,
                created_by, modified_by, content_hash, previous_hash,
                version, feature_cycle, trust_source,
                helpful_count, unhelpful_count, pre_quarantine_status
            ) VALUES (
                42, 'Test Entry', 'Content here', 'testing', 'pattern', 'unit-test', 1, 0.87654321,
                1700000000, 1700000001, 1700000002, 15,
                10, 50, 3, 384,
                'agent-x', 'agent-y', 'abc123', 'def456',
                7, 'crt-002', 'human',
                12, 2, 0
            )",
        )
        .execute(pool)
        .await
        .unwrap();

        let mut buf = Vec::new();
        export_entries(pool, &mut buf, &HashSet::new())
            .await
            .unwrap();
        let row = parse_line(&buf);

        let obj = row.as_object().unwrap();
        assert_eq!(obj.len(), 27); // 26 columns + _table
        assert_eq!(obj["_table"], "entries");
        assert_eq!(obj["id"], 42);
        assert_eq!(obj["title"], "Test Entry");
        assert_eq!(obj["content"], "Content here");
        assert_eq!(obj["topic"], "testing");
        assert_eq!(obj["category"], "pattern");
        assert_eq!(obj["source"], "unit-test");
        assert_eq!(obj["status"], 1);
        assert_eq!(obj["created_at"], 1_700_000_000i64);
        assert_eq!(obj["updated_at"], 1_700_000_001i64);
        assert_eq!(obj["last_accessed_at"], 1_700_000_002i64);
        assert_eq!(obj["access_count"], 15);
        assert_eq!(obj["supersedes"], 10);
        assert_eq!(obj["superseded_by"], 50);
        assert_eq!(obj["correction_count"], 3);
        assert_eq!(obj["embedding_dim"], 384);
        assert_eq!(obj["created_by"], "agent-x");
        assert_eq!(obj["modified_by"], "agent-y");
        assert_eq!(obj["content_hash"], "abc123");
        assert_eq!(obj["previous_hash"], "def456");
        assert_eq!(obj["version"], 7);
        assert_eq!(obj["feature_cycle"], "crt-002");
        assert_eq!(obj["trust_source"], "human");
        assert_eq!(obj["helpful_count"], 12);
        assert_eq!(obj["unhelpful_count"], 2);
        assert_eq!(obj["pre_quarantine_status"], 0);
    }

    // -----------------------------------------------------------------------
    // T-RS-03: Per-table key counts
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_export_counters_key_count_and_values() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();

        let mut buf = Vec::new();
        export_counters(pool, &mut buf).await.unwrap();
        let rows = parse_lines(&buf);
        // counters table has schema_version from Store::open migration
        assert!(!rows.is_empty());
        for row in &rows {
            assert_eq!(row.as_object().unwrap().len(), 3);
            assert_eq!(row["_table"], "counters");
        }
    }

    #[tokio::test]
    async fn test_export_entry_tags_key_count() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        sqlx::query(
            "INSERT INTO entries (id, title, content, topic, category, source, created_at, updated_at)
             VALUES (1, 't', 'c', 't', 'p', 's', 1, 1)",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO entry_tags (entry_id, tag) VALUES (1, 'rust')")
            .execute(pool)
            .await
            .unwrap();

        let mut buf = Vec::new();
        export_entry_tags(pool, &mut buf, &HashSet::new())
            .await
            .unwrap();
        let row = parse_line(&buf);
        assert_eq!(row.as_object().unwrap().len(), 3);
        assert_eq!(row["_table"], "entry_tags");
    }

    #[tokio::test]
    async fn test_export_co_access_key_count() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        sqlx::query(
            "INSERT INTO co_access (entry_id_a, entry_id_b, count, last_updated)
             VALUES (1, 2, 5, 1700000000)",
        )
        .execute(pool)
        .await
        .unwrap();

        let mut buf = Vec::new();
        export_co_access(pool, &mut buf, &HashSet::new())
            .await
            .unwrap();
        let row = parse_line(&buf);
        assert_eq!(row.as_object().unwrap().len(), 5);
        assert_eq!(row["_table"], "co_access");
        assert_eq!(row["entry_id_a"], 1);
        assert_eq!(row["entry_id_b"], 2);
        assert_eq!(row["count"], 5);
        assert_eq!(row["last_updated"], 1_700_000_000i64);
    }

    #[tokio::test]
    async fn test_export_feature_entries_key_count() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        sqlx::query("INSERT INTO feature_entries (feature_id, entry_id) VALUES ('nxs-001', 42)")
            .execute(pool)
            .await
            .unwrap();

        let mut buf = Vec::new();
        export_feature_entries(pool, &mut buf, &HashSet::new())
            .await
            .unwrap();
        let row = parse_line(&buf);
        assert_eq!(row.as_object().unwrap().len(), 3);
        assert_eq!(row["feature_id"], "nxs-001");
        assert_eq!(row["entry_id"], 42);
    }

    #[tokio::test]
    async fn test_export_outcome_index_key_count() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        sqlx::query("INSERT INTO outcome_index (feature_cycle, entry_id) VALUES ('crt-001', 7)")
            .execute(pool)
            .await
            .unwrap();

        let mut buf = Vec::new();
        export_outcome_index(pool, &mut buf).await.unwrap();
        let row = parse_line(&buf);
        assert_eq!(row.as_object().unwrap().len(), 3);
        assert_eq!(row["feature_cycle"], "crt-001");
        assert_eq!(row["entry_id"], 7);
    }

    #[tokio::test]
    async fn test_export_agent_registry_key_count() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        sqlx::query(
            "INSERT INTO agent_registry (agent_id, trust_level, capabilities,
             allowed_topics, allowed_categories, enrolled_at, last_seen_at, active)
             VALUES ('bot-1', 2, '[\"Admin\"]', '[\"security\"]', '[\"decision\"]', 1700000000, 1700000001, 1)",
        )
        .execute(pool)
        .await
        .unwrap();

        let mut buf = Vec::new();
        export_agent_registry(pool, &mut buf).await.unwrap();
        let row = parse_line(&buf);
        assert_eq!(row.as_object().unwrap().len(), 9);
        assert_eq!(row["_table"], "agent_registry");
    }

    #[tokio::test]
    async fn test_export_audit_log_key_count() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        sqlx::query(
            "INSERT INTO audit_log (event_id, timestamp, session_id, agent_id,
             operation, target_ids, outcome, detail)
             VALUES (1, 1700000000, 'sess-1', 'bot-1', 'store', '[1,2]', 0, 'ok')",
        )
        .execute(pool)
        .await
        .unwrap();

        let mut buf = Vec::new();
        export_audit_log(pool, &mut buf).await.unwrap();
        let row = parse_line(&buf);
        assert_eq!(row.as_object().unwrap().len(), 9);
        assert_eq!(row["_table"], "audit_log");
    }

    // -----------------------------------------------------------------------
    // T-RS-04: f64 confidence round-trip fidelity
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_export_entries_f64_precision() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        let values = [0.0, 1.0, 0.123456789012345, f64::MIN_POSITIVE, 0.1 + 0.2];

        for (i, &v) in values.iter().enumerate() {
            let id = (i as i64) + 1;
            sqlx::query(
                "INSERT INTO entries (
                    id, title, content, topic, category, source, status, confidence,
                    created_at, updated_at
                ) VALUES (?1, 'test', 'c', 't', 'p', 's', 0, ?2, 1, 1)",
            )
            .bind(id)
            .bind(v)
            .execute(pool)
            .await
            .unwrap();
        }

        let mut buf = Vec::new();
        export_entries(pool, &mut buf, &HashSet::new())
            .await
            .unwrap();
        let rows = parse_lines(&buf);
        assert_eq!(rows.len(), values.len());

        for (row, &expected) in rows.iter().zip(values.iter()) {
            let parsed = row["confidence"].as_f64().unwrap();
            assert_eq!(
                parsed.to_bits(),
                expected.to_bits(),
                "f64 mismatch for {expected}: got {parsed}"
            );
        }
    }

    // -----------------------------------------------------------------------
    // T-RS-05: JSON-in-TEXT columns emitted as raw strings
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_export_agent_registry_json_in_text_as_string() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        sqlx::query(
            "INSERT INTO agent_registry (agent_id, trust_level, capabilities,
             allowed_topics, allowed_categories, enrolled_at, last_seen_at, active)
             VALUES ('bot-1', 2, '[\"Admin\",\"Read\"]', '[\"security\"]', '[\"decision\"]', 1, 1, 1)",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO agent_registry (agent_id, trust_level, capabilities,
             allowed_topics, allowed_categories, enrolled_at, last_seen_at, active)
             VALUES ('bot-2', 1, '[]', NULL, NULL, 2, 2, 1)",
        )
        .execute(pool)
        .await
        .unwrap();

        let mut buf = Vec::new();
        export_agent_registry(pool, &mut buf).await.unwrap();
        let rows = parse_lines(&buf);
        assert_eq!(rows.len(), 2);

        // bot-1: JSON-in-TEXT are strings, NOT parsed arrays
        let r1 = &rows[0];
        assert!(r1["capabilities"].is_string());
        assert_eq!(r1["capabilities"].as_str().unwrap(), "[\"Admin\",\"Read\"]");
        assert!(r1["allowed_topics"].is_string());
        assert_eq!(r1["allowed_topics"].as_str().unwrap(), "[\"security\"]");
        assert!(r1["allowed_categories"].is_string());
        assert_eq!(r1["allowed_categories"].as_str().unwrap(), "[\"decision\"]");

        // bot-2: empty array as string, nullable fields as null
        let r2 = &rows[1];
        assert!(r2["capabilities"].is_string());
        assert_eq!(r2["capabilities"].as_str().unwrap(), "[]");
        assert!(r2["allowed_topics"].is_null());
        assert!(r2["allowed_categories"].is_null());
    }

    #[tokio::test]
    async fn test_export_audit_log_json_in_text_target_ids() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        sqlx::query(
            "INSERT INTO audit_log (event_id, timestamp, session_id, agent_id,
             operation, target_ids, outcome, detail)
             VALUES (1, 100, 's1', 'a1', 'op', '[1,2,3]', 0, 'detail')",
        )
        .execute(pool)
        .await
        .unwrap();

        let mut buf = Vec::new();
        export_audit_log(pool, &mut buf).await.unwrap();
        let row = parse_line(&buf);

        assert!(row["target_ids"].is_string());
        assert_eq!(row["target_ids"].as_str().unwrap(), "[1,2,3]");
    }

    // -----------------------------------------------------------------------
    // T-RS-06: NULL columns serialized as JSON null
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_export_entries_null_handling() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        sqlx::query(
            "INSERT INTO entries (
                id, title, content, topic, category, source, status, confidence,
                created_at, updated_at, supersedes, superseded_by, pre_quarantine_status
            ) VALUES (1, 'test', 'c', 't', 'p', 's', 0, 0.5, 1, 1, NULL, NULL, NULL)",
        )
        .execute(pool)
        .await
        .unwrap();

        let mut buf = Vec::new();
        export_entries(pool, &mut buf, &HashSet::new())
            .await
            .unwrap();
        let row = parse_line(&buf);

        assert!(row.as_object().unwrap().contains_key("supersedes"));
        assert!(row["supersedes"].is_null());
        assert!(row.as_object().unwrap().contains_key("superseded_by"));
        assert!(row["superseded_by"].is_null());
        assert!(
            row.as_object()
                .unwrap()
                .contains_key("pre_quarantine_status")
        );
        assert!(row["pre_quarantine_status"].is_null());
        assert_eq!(row.as_object().unwrap().len(), 27);
    }

    #[tokio::test]
    async fn test_export_agent_registry_null_handling() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        sqlx::query(
            "INSERT INTO agent_registry (agent_id, trust_level, capabilities,
             allowed_topics, allowed_categories, enrolled_at, last_seen_at, active)
             VALUES ('bot-null', 0, '[]', NULL, NULL, 1, 1, 1)",
        )
        .execute(pool)
        .await
        .unwrap();

        let mut buf = Vec::new();
        export_agent_registry(pool, &mut buf).await.unwrap();
        let row = parse_line(&buf);

        assert!(row.as_object().unwrap().contains_key("allowed_topics"));
        assert!(row["allowed_topics"].is_null());
        assert!(row.as_object().unwrap().contains_key("allowed_categories"));
        assert!(row["allowed_categories"].is_null());
    }

    // -----------------------------------------------------------------------
    // T-RS-06b: Empty strings are NOT null
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_export_entries_empty_string_not_null() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        sqlx::query(
            "INSERT INTO entries (
                id, title, content, topic, category, source, status, confidence,
                created_at, updated_at, created_by, content_hash, feature_cycle
            ) VALUES (1, 'test', 'c', 't', 'p', 's', 0, 0.0, 1, 1, '', '', '')",
        )
        .execute(pool)
        .await
        .unwrap();

        let mut buf = Vec::new();
        export_entries(pool, &mut buf, &HashSet::new())
            .await
            .unwrap();
        let row = parse_line(&buf);

        assert!(row["created_by"].is_string());
        assert_eq!(row["created_by"].as_str().unwrap(), "");
        assert!(row["content_hash"].is_string());
        assert_eq!(row["content_hash"].as_str().unwrap(), "");
        assert!(row["feature_cycle"].is_string());
        assert_eq!(row["feature_cycle"].as_str().unwrap(), "");
    }

    // -----------------------------------------------------------------------
    // T-RS-07: _table is first key, columns follow DDL declaration order
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_export_entries_key_ordering() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        sqlx::query(
            "INSERT INTO entries (
                id, title, content, topic, category, source, status, confidence,
                created_at, updated_at
            ) VALUES (1, 'test', 'content', 'topic', 'cat', 'src', 0, 0.5, 1, 1)",
        )
        .execute(pool)
        .await
        .unwrap();

        let mut buf = Vec::new();
        export_entries(pool, &mut buf, &HashSet::new())
            .await
            .unwrap();

        let raw = std::str::from_utf8(&buf).unwrap();
        let line = raw.lines().next().unwrap();

        // _table must be first key in raw JSON
        assert!(
            line.starts_with("{\"_table\":"),
            "Expected _table as first key, got: {}",
            &line[..50.min(line.len())]
        );

        // Verify full key order via preserve_order map
        let v: Value = serde_json::from_str(line).unwrap();
        let keys: Vec<&String> = v.as_object().unwrap().keys().collect();
        let expected_keys = [
            "_table",
            "id",
            "title",
            "content",
            "topic",
            "category",
            "source",
            "status",
            "confidence",
            "created_at",
            "updated_at",
            "last_accessed_at",
            "access_count",
            "supersedes",
            "superseded_by",
            "correction_count",
            "embedding_dim",
            "created_by",
            "modified_by",
            "content_hash",
            "previous_hash",
            "version",
            "feature_cycle",
            "trust_source",
            "helpful_count",
            "unhelpful_count",
            "pre_quarantine_status",
        ];
        assert_eq!(keys.len(), expected_keys.len());
        for (got, expected) in keys.iter().zip(expected_keys.iter()) {
            assert_eq!(got.as_str(), *expected);
        }
    }

    #[tokio::test]
    async fn test_export_counters_table_key_first() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();

        let mut buf = Vec::new();
        export_counters(pool, &mut buf).await.unwrap();
        // counters has at least schema_version from migration
        let raw = std::str::from_utf8(&buf).unwrap();
        if let Some(line) = raw.lines().next() {
            assert!(line.starts_with("{\"_table\":"));
        }
    }

    // -----------------------------------------------------------------------
    // T-RS-09: Unicode content preserved
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_export_entries_unicode_cjk_and_emoji() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        sqlx::query(
            "INSERT INTO entries (
                id, title, content, topic, category, source, status, confidence,
                created_at, updated_at
            ) VALUES (1, '\u{77E5}\u{8B58}', 'Status: \u{2705} approved', 't', 'p', 's', 0, 0.0, 1, 1)",
        )
        .execute(pool)
        .await
        .unwrap();

        let mut buf = Vec::new();
        export_entries(pool, &mut buf, &HashSet::new())
            .await
            .unwrap();
        let row = parse_line(&buf);

        assert_eq!(row["title"].as_str().unwrap(), "\u{77E5}\u{8B58}");
        assert_eq!(
            row["content"].as_str().unwrap(),
            "Status: \u{2705} approved"
        );
    }

    #[tokio::test]
    async fn test_export_entry_tags_unicode_accented() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        sqlx::query(
            "INSERT INTO entries (id, title, content, topic, category, source, created_at, updated_at)
             VALUES (1, 't', 'c', 't', 'p', 's', 1, 1)",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO entry_tags (entry_id, tag) VALUES (1, 'resume\u{0301}')")
            .execute(pool)
            .await
            .unwrap();

        let mut buf = Vec::new();
        export_entry_tags(pool, &mut buf, &HashSet::new())
            .await
            .unwrap();
        let row = parse_line(&buf);
        assert_eq!(row["tag"].as_str().unwrap(), "resume\u{0301}");
    }

    // -----------------------------------------------------------------------
    // T-RS-10: Large integer values preserved
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_export_entries_large_integers() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        sqlx::query(
            "INSERT INTO entries (
                id, title, content, topic, category, source, status, confidence,
                created_at, updated_at, version, access_count
            ) VALUES (1, 't', 'c', 't', 'p', 's', 0, 0.0, 9999999999, 1, 2147483647, 1000000)",
        )
        .execute(pool)
        .await
        .unwrap();

        let mut buf = Vec::new();
        export_entries(pool, &mut buf, &HashSet::new())
            .await
            .unwrap();
        let row = parse_line(&buf);

        assert_eq!(row["created_at"].as_i64().unwrap(), 9_999_999_999i64);
        assert_eq!(row["version"].as_i64().unwrap(), 2_147_483_647i64);
        assert_eq!(row["access_count"].as_i64().unwrap(), 1_000_000i64);
    }

    #[tokio::test]
    async fn test_export_counters_i64_max() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        sqlx::query(
            "INSERT OR REPLACE INTO counters (name, value) VALUES ('big', 9223372036854775807)",
        )
        .execute(pool)
        .await
        .unwrap();

        let mut buf = Vec::new();
        export_counters(pool, &mut buf).await.unwrap();
        let rows = parse_lines(&buf);
        let big_row = rows.iter().find(|r| r["name"] == "big").unwrap();
        assert_eq!(big_row["value"].as_i64().unwrap(), i64::MAX);
    }

    // -----------------------------------------------------------------------
    // T-RS-11: Entry with all nullable fields NULL simultaneously
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_export_entries_all_nullable_null() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        sqlx::query(
            "INSERT INTO entries (
                id, title, content, topic, category, source, created_at, updated_at,
                supersedes, superseded_by, pre_quarantine_status
            ) VALUES (1, 't', 'c', 't', 'p', 's', 1, 1, NULL, NULL, NULL)",
        )
        .execute(pool)
        .await
        .unwrap();

        let mut buf = Vec::new();
        export_entries(pool, &mut buf, &HashSet::new())
            .await
            .unwrap();
        let row = parse_line(&buf);

        assert!(row["supersedes"].is_null());
        assert!(row["superseded_by"].is_null());
        assert!(row["pre_quarantine_status"].is_null());
        assert_eq!(row.as_object().unwrap().len(), 27);
    }

    // -----------------------------------------------------------------------
    // T-RS-12: Timestamp of 0 is not treated as NULL
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_export_entries_zero_timestamp_not_null() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        sqlx::query(
            "INSERT INTO entries (
                id, title, content, topic, category, source, created_at, updated_at,
                last_accessed_at
            ) VALUES (1, 't', 'c', 't', 'p', 's', 0, 0, 0)",
        )
        .execute(pool)
        .await
        .unwrap();

        let mut buf = Vec::new();
        export_entries(pool, &mut buf, &HashSet::new())
            .await
            .unwrap();
        let row = parse_line(&buf);

        assert_eq!(row["created_at"].as_i64().unwrap(), 0);
        assert_eq!(row["last_accessed_at"].as_i64().unwrap(), 0);
        assert!(!row["created_at"].is_null());
    }

    // -----------------------------------------------------------------------
    // T-RS-13: JSONL line integrity -- no raw newlines in output lines
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_export_entries_newline_in_content_escaped() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        sqlx::query(
            "INSERT INTO entries (
                id, title, content, topic, category, source, created_at, updated_at
            ) VALUES (1, 't', 'line1\nline2\nline3', 't', 'p', 's', 1, 1)",
        )
        .execute(pool)
        .await
        .unwrap();

        let mut buf = Vec::new();
        export_entries(pool, &mut buf, &HashSet::new())
            .await
            .unwrap();
        let raw = std::str::from_utf8(&buf).unwrap();

        let lines: Vec<&str> = raw.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), 1, "Multi-line content must not break JSONL");

        let row: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(row["content"].as_str().unwrap(), "line1\nline2\nline3");
    }

    // -----------------------------------------------------------------------
    // Row ordering tests
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_export_entries_ordered_by_id() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        for id in [5i64, 2, 8] {
            sqlx::query(
                "INSERT INTO entries (id, title, content, topic, category, source, created_at, updated_at)
                 VALUES (?1, 't', 'c', 't', 'p', 's', 1, 1)",
            )
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
        }

        let mut buf = Vec::new();
        export_entries(pool, &mut buf, &HashSet::new())
            .await
            .unwrap();
        let rows = parse_lines(&buf);
        let ids: Vec<i64> = rows.iter().map(|r| r["id"].as_i64().unwrap()).collect();
        assert_eq!(ids, vec![2, 5, 8]);
    }

    #[tokio::test]
    async fn test_export_entry_tags_ordered() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        sqlx::query(
            "INSERT INTO entries (id, title, content, topic, category, source, created_at, updated_at)
             VALUES (1, 't', 'c', 't', 'p', 's', 1, 1)",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO entry_tags (entry_id, tag) VALUES (1, 'z')")
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO entry_tags (entry_id, tag) VALUES (1, 'a')")
            .execute(pool)
            .await
            .unwrap();

        let mut buf = Vec::new();
        export_entry_tags(pool, &mut buf, &HashSet::new())
            .await
            .unwrap();
        let rows = parse_lines(&buf);
        let tags: Vec<&str> = rows.iter().map(|r| r["tag"].as_str().unwrap()).collect();
        assert_eq!(tags, vec!["a", "z"]);
    }

    // -----------------------------------------------------------------------
    // Empty tables produce no output
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_export_empty_tables_no_output() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();

        let mut buf = Vec::new();
        export_entries(pool, &mut buf, &HashSet::new())
            .await
            .unwrap();
        assert!(buf.is_empty());

        buf.clear();
        export_entry_tags(pool, &mut buf, &HashSet::new())
            .await
            .unwrap();
        assert!(buf.is_empty());

        buf.clear();
        export_co_access(pool, &mut buf, &HashSet::new())
            .await
            .unwrap();
        assert!(buf.is_empty());

        buf.clear();
        export_feature_entries(pool, &mut buf, &HashSet::new())
            .await
            .unwrap();
        assert!(buf.is_empty());

        buf.clear();
        export_outcome_index(pool, &mut buf).await.unwrap();
        assert!(buf.is_empty());

        buf.clear();
        export_agent_registry(pool, &mut buf).await.unwrap();
        assert!(buf.is_empty());

        buf.clear();
        export_audit_log(pool, &mut buf).await.unwrap();
        assert!(buf.is_empty());
    }

    // -----------------------------------------------------------------------
    // JSON-special characters in content
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn test_export_entries_json_special_chars_in_content() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        sqlx::query(
            r#"INSERT INTO entries (
                id, title, content, topic, category, source, created_at, updated_at
            ) VALUES (1, 't', 'He said "hello" and used a \backslash', 't', 'p', 's', 1, 1)"#,
        )
        .execute(pool)
        .await
        .unwrap();

        let mut buf = Vec::new();
        export_entries(pool, &mut buf, &HashSet::new())
            .await
            .unwrap();
        let row = parse_line(&buf);
        assert_eq!(
            row["content"].as_str().unwrap(),
            r#"He said "hello" and used a \backslash"#
        );
    }

    // -----------------------------------------------------------------------
    // nxs-012: graph_edges export tests
    // -----------------------------------------------------------------------

    /// Helper: insert a graph_edge row with specified values.
    async fn insert_test_graph_edge(
        pool: &SqlitePool,
        source_id: i64,
        target_id: i64,
        relation_type: &str,
        weight: f64,
        metadata: Option<&str>,
    ) {
        sqlx::query(
            "INSERT INTO graph_edges (source_id, target_id, relation_type, weight,
                created_at, created_by, source, bootstrap_only, metadata)
             VALUES (?1, ?2, ?3, ?4, 1700000000, 'test-agent', 'unit-test', 0, ?5)",
        )
        .bind(source_id)
        .bind(target_id)
        .bind(relation_type)
        .bind(weight)
        .bind(metadata)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn test_export_graph_edges_9_columns() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        // Insert 3 edges in non-sorted order
        insert_test_graph_edge(pool, 5, 10, "similar", 0.8, Some(r#"{"nli":0.9}"#)).await;
        insert_test_graph_edge(pool, 1, 2, "related", 0.5, None).await;
        insert_test_graph_edge(pool, 5, 10, "derived", 1.0, Some("")).await;

        let mut buf = Vec::new();
        export_graph_edges(pool, &mut buf, &HashSet::new())
            .await
            .unwrap();
        let rows = parse_lines(&buf);

        assert_eq!(rows.len(), 3);
        for row in &rows {
            // Each line has 10 keys (9 data + _table)
            assert_eq!(row.as_object().unwrap().len(), 10);
            assert_eq!(row["_table"], "graph_edges");
            // No id field present (ADR-005)
            assert!(!row.as_object().unwrap().contains_key("id"));
        }
        // Rows sorted by (source_id, target_id, relation_type)
        assert_eq!(rows[0]["source_id"], 1);
        assert_eq!(rows[0]["target_id"], 2);
        assert_eq!(rows[0]["relation_type"], "related");
        assert_eq!(rows[1]["source_id"], 5);
        assert_eq!(rows[1]["target_id"], 10);
        assert_eq!(rows[1]["relation_type"], "derived");
        assert_eq!(rows[2]["source_id"], 5);
        assert_eq!(rows[2]["target_id"], 10);
        assert_eq!(rows[2]["relation_type"], "similar");
    }

    /// Verify the NaN safety logic directly (ADR-003).
    /// SQLite NOT NULL columns cannot store NaN, so we test the
    /// `Number::from_f64` fallback pattern in isolation.
    #[test]
    fn test_export_graph_edges_weight_nan_fallback() {
        let weight = f64::NAN;
        let result = Number::from_f64(weight)
            .unwrap_or_else(|| Number::from_f64(1.0).expect("1.0 is valid f64"));
        assert_eq!(result, Number::from_f64(1.0).unwrap());
    }

    #[test]
    fn test_export_graph_edges_weight_infinity_fallback() {
        let weight = f64::INFINITY;
        let result = Number::from_f64(weight)
            .unwrap_or_else(|| Number::from_f64(1.0).expect("1.0 is valid f64"));
        assert_eq!(result, Number::from_f64(1.0).unwrap());
    }

    #[test]
    fn test_export_graph_edges_weight_neg_infinity_fallback() {
        let weight = f64::NEG_INFINITY;
        let result = Number::from_f64(weight)
            .unwrap_or_else(|| Number::from_f64(1.0).expect("1.0 is valid f64"));
        assert_eq!(result, Number::from_f64(1.0).unwrap());
    }

    #[tokio::test]
    async fn test_export_graph_edges_weight_normal_precision() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        insert_test_graph_edge(pool, 1, 2, "test", 0.7777777, None).await;

        let mut buf = Vec::new();
        export_graph_edges(pool, &mut buf, &HashSet::new())
            .await
            .unwrap();
        let row = parse_line(&buf);
        let exported = row["weight"].as_f64().unwrap();
        assert_eq!(
            exported.to_bits(),
            0.7777777_f64.to_bits(),
            "full f64 precision must be preserved"
        );
    }

    #[tokio::test]
    async fn test_export_graph_edges_weight_zero() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        insert_test_graph_edge(pool, 1, 2, "test", 0.0, None).await;

        let mut buf = Vec::new();
        export_graph_edges(pool, &mut buf, &HashSet::new())
            .await
            .unwrap();
        let row = parse_line(&buf);
        assert_eq!(row["weight"].as_f64().unwrap(), 0.0);
    }

    #[tokio::test]
    async fn test_export_graph_edges_nullable_metadata() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        insert_test_graph_edge(pool, 1, 2, "test", 1.0, None).await;

        let mut buf = Vec::new();
        export_graph_edges(pool, &mut buf, &HashSet::new())
            .await
            .unwrap();
        let row = parse_line(&buf);
        assert!(row.as_object().unwrap().contains_key("metadata"));
        assert!(row["metadata"].is_null());
    }

    #[tokio::test]
    async fn test_export_graph_edges_metadata_empty_string() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        insert_test_graph_edge(pool, 1, 2, "test", 1.0, Some("")).await;

        let mut buf = Vec::new();
        export_graph_edges(pool, &mut buf, &HashSet::new())
            .await
            .unwrap();
        let row = parse_line(&buf);
        assert!(row["metadata"].is_string());
        assert_eq!(row["metadata"].as_str().unwrap(), "");
    }

    #[tokio::test]
    async fn test_export_graph_edges_metadata_populated() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        insert_test_graph_edge(pool, 1, 2, "test", 1.0, Some(r#"{"nli_score": 0.8}"#)).await;

        let mut buf = Vec::new();
        export_graph_edges(pool, &mut buf, &HashSet::new())
            .await
            .unwrap();
        let row = parse_line(&buf);
        assert!(row["metadata"].is_string());
        assert_eq!(row["metadata"].as_str().unwrap(), r#"{"nli_score": 0.8}"#);
    }

    // -----------------------------------------------------------------------
    // nxs-012: observations export tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_export_observations_10_columns() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        for (id, sess) in [(5, "s1"), (2, "s2"), (8, "s3")] {
            sqlx::query(
                "INSERT INTO observations (id, session_id, ts_millis, hook, tool, input,
                    response_size, response_snippet, topic_signal, phase)
                 VALUES (?1, ?2, 1700000000, 'on_response', 'grep', 'query', 512, 'snippet', 'rust', 'explore')",
            )
            .bind(id)
            .bind(sess)
            .execute(pool)
            .await
            .unwrap();
        }

        let mut buf = Vec::new();
        export_observations(pool, &mut buf).await.unwrap();
        let rows = parse_lines(&buf);

        assert_eq!(rows.len(), 3);
        for row in &rows {
            assert_eq!(row.as_object().unwrap().len(), 11);
            assert_eq!(row["_table"], "observations");
            assert!(row.as_object().unwrap().contains_key("id"));
        }
        let ids: Vec<i64> = rows.iter().map(|r| r["id"].as_i64().unwrap()).collect();
        assert_eq!(ids, vec![2, 5, 8]);
    }

    #[tokio::test]
    async fn test_export_observations_nullable_fields() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        sqlx::query(
            "INSERT INTO observations (id, session_id, ts_millis, hook, tool, input,
                response_size, response_snippet, topic_signal, phase)
             VALUES (1, 's1', 1700000000, 'on_response', NULL, NULL, NULL, NULL, NULL, NULL)",
        )
        .execute(pool)
        .await
        .unwrap();

        let mut buf = Vec::new();
        export_observations(pool, &mut buf).await.unwrap();
        let row = parse_line(&buf);

        assert!(row["tool"].is_null());
        assert!(row["input"].is_null());
        assert!(row["response_size"].is_null());
        assert!(row["response_snippet"].is_null());
        assert!(row["topic_signal"].is_null());
        assert!(row["phase"].is_null());
    }

    #[tokio::test]
    async fn test_export_observations_embedded_newlines() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        sqlx::query(
            "INSERT INTO observations (id, session_id, ts_millis, hook, tool, input,
                response_size, response_snippet, topic_signal, phase)
             VALUES (1, 's1', 1700000000, 'on_response', NULL, 'line1
line2
line3', NULL, NULL, NULL, NULL)",
        )
        .execute(pool)
        .await
        .unwrap();

        let mut buf = Vec::new();
        export_observations(pool, &mut buf).await.unwrap();
        let raw = std::str::from_utf8(&buf).unwrap();

        let lines: Vec<&str> = raw.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), 1, "Multi-line content must not break JSONL");

        let row: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(row["input"].as_str().unwrap(), "line1\nline2\nline3");
    }

    // -----------------------------------------------------------------------
    // nxs-012: cycle_events export tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_export_cycle_events_9_columns() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        for (id, cycle) in [(7, "c1"), (3, "c2"), (12, "c3")] {
            sqlx::query(
                "INSERT INTO cycle_events (id, cycle_id, seq, event_type, phase,
                    outcome, next_phase, timestamp, goal)
                 VALUES (?1, ?2, 1, 'phase_transition', 'explore', 'ok', 'refine', 1700000000, 'test goal')",
            )
            .bind(id)
            .bind(cycle)
            .execute(pool)
            .await
            .unwrap();
        }

        let mut buf = Vec::new();
        export_cycle_events(pool, &mut buf).await.unwrap();
        let rows = parse_lines(&buf);

        assert_eq!(rows.len(), 3);
        for row in &rows {
            assert_eq!(row.as_object().unwrap().len(), 10);
            assert_eq!(row["_table"], "cycle_events");
            assert!(!row.as_object().unwrap().contains_key("goal_embedding"));
        }
        let ids: Vec<i64> = rows.iter().map(|r| r["id"].as_i64().unwrap()).collect();
        assert_eq!(ids, vec![3, 7, 12]);
    }

    #[tokio::test]
    async fn test_export_cycle_events_nullable_fields() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        sqlx::query(
            "INSERT INTO cycle_events (id, cycle_id, seq, event_type, phase,
                outcome, next_phase, timestamp, goal)
             VALUES (1, 'c1', 1, 'phase_transition', NULL, NULL, NULL, 1700000000, NULL)",
        )
        .execute(pool)
        .await
        .unwrap();

        let mut buf = Vec::new();
        export_cycle_events(pool, &mut buf).await.unwrap();
        let row = parse_line(&buf);

        assert!(row["phase"].is_null());
        assert!(row["outcome"].is_null());
        assert!(row["next_phase"].is_null());
        assert!(row["goal"].is_null());
    }

    // -----------------------------------------------------------------------
    // nxs-012: header and table emission order tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_export_header_format_version_2() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        let mut buf = Vec::new();

        write_header(pool, &mut buf, false).await.unwrap();
        let header = parse_line(&buf);
        assert_eq!(header["format_version"], Value::Number(2.into()));
    }

    #[tokio::test]
    async fn test_export_table_emission_order_11_tables() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();

        // Populate all 11 tables
        sqlx::query(
            "INSERT INTO entries (id, title, content, topic, category, source, created_at, updated_at)
             VALUES (1, 't', 'c', 't', 'p', 's', 1, 1)",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO entry_tags (entry_id, tag) VALUES (1, 'test')")
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO co_access (entry_id_a, entry_id_b, count, last_updated) VALUES (1, 2, 1, 1)",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO feature_entries (feature_id, entry_id) VALUES ('f1', 1)")
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO outcome_index (feature_cycle, entry_id) VALUES ('c1', 1)")
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO agent_registry (agent_id, trust_level, capabilities, enrolled_at, last_seen_at, active)
             VALUES ('a1', 1, '[]', 1, 1, 1)",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO audit_log (event_id, timestamp, session_id, agent_id, operation, target_ids, outcome, detail)
             VALUES (1, 1, 's1', 'a1', 'store', '[]', 0, 'ok')",
        )
        .execute(pool)
        .await
        .unwrap();
        insert_test_graph_edge(pool, 1, 2, "test", 1.0, None).await;
        sqlx::query(
            "INSERT INTO observations (id, session_id, ts_millis, hook) VALUES (1, 's1', 1, 'on_response')",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO cycle_events (id, cycle_id, seq, event_type, timestamp) VALUES (1, 'c1', 1, 'start', 1)",
        )
        .execute(pool)
        .await
        .unwrap();

        let mut buf = Vec::new();
        do_export(pool, &mut buf, &HashSet::new(), false)
            .await
            .unwrap();
        let all = parse_lines(&buf);

        // Extract _table values in order of first appearance (skip header)
        let mut table_order = Vec::new();
        for val in &all {
            if let Some(tbl) = val.get("_table").and_then(|v| v.as_str())
                && !table_order.contains(&tbl.to_string())
            {
                table_order.push(tbl.to_string());
            }
        }

        assert_eq!(
            table_order,
            vec![
                "counters",
                "entries",
                "entry_tags",
                "co_access",
                "feature_entries",
                "outcome_index",
                "agent_registry",
                "audit_log",
                "graph_edges",
                "observations",
                "cycle_events",
            ]
        );
    }

    #[tokio::test]
    async fn test_export_empty_new_tables() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();

        let mut buf = Vec::new();
        export_graph_edges(pool, &mut buf, &HashSet::new())
            .await
            .unwrap();
        assert!(buf.is_empty());

        buf.clear();
        export_observations(pool, &mut buf).await.unwrap();
        assert!(buf.is_empty());

        buf.clear();
        export_cycle_events(pool, &mut buf).await.unwrap();
        assert!(buf.is_empty());
    }

    // -----------------------------------------------------------------------
    // nxs-012: skip-quarantined unit tests (C5)
    // -----------------------------------------------------------------------

    /// Helper: insert an entry with a specific id and status.
    /// Uses only the required columns and lets NOT NULL DEFAULT columns
    /// get their defaults automatically.
    async fn insert_entry_with_status(pool: &SqlitePool, id: i64, status: i64) {
        sqlx::query(
            "INSERT INTO entries (
                id, title, content, topic, category, source, status, confidence,
                created_at, updated_at
            ) VALUES (
                ?1, 'entry', 'content', 'topic', 'pattern', 'test', ?2, 0.9,
                1700000000, 1700000001
            )",
        )
        .bind(id)
        .bind(status)
        .execute(pool)
        .await
        .unwrap();
    }

    // R-23, AC-30: --skip-quarantined without --confirm aborts
    #[test]
    fn test_confirm_safeguard_missing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let proj = tmp.path().join("project");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(proj.join("unimatrix.toml"), "").unwrap();
        let out = tmp.path().join("out.jsonl");

        let result = run_export_with_base(
            Some(proj.as_path()),
            Some(out.as_path()),
            tmp.path(),
            None,  // slug
            true,  // skip_quarantined
            false, // confirm missing
        );

        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("--confirm"),
            "error should mention --confirm: {msg}"
        );
        assert!(!out.exists(), "no output file should be created");
    }

    // R-23, AC-30: --skip-quarantined with --confirm succeeds
    #[tokio::test(flavor = "multi_thread")]
    async fn test_confirm_safeguard_present() {
        let tmp = tempfile::TempDir::new().unwrap();
        let proj = tmp.path().join("project");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(proj.join("unimatrix.toml"), "").unwrap();
        let out = tmp.path().join("out.jsonl");

        let result = run_export_with_base(
            Some(proj.as_path()),
            Some(out.as_path()),
            tmp.path(),
            None, // slug
            true, // skip_quarantined
            true, // confirm
        );

        assert!(result.is_ok(), "export should succeed: {:?}", result.err());
        assert!(out.exists(), "output file should be created");
    }

    // R-23, AC-29: --confirm alone is silently ignored
    #[tokio::test(flavor = "multi_thread")]
    async fn test_confirm_alone_ignored() {
        let tmp = tempfile::TempDir::new().unwrap();
        let proj = tmp.path().join("project");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(proj.join("unimatrix.toml"), "").unwrap();
        let out = tmp.path().join("out.jsonl");

        let result = run_export_with_base(
            Some(proj.as_path()),
            Some(out.as_path()),
            tmp.path(),
            None,  // slug
            false, // skip_quarantined off
            true,  // confirm present but irrelevant
        );

        assert!(result.is_ok(), "export should succeed: {:?}", result.err());
    }

    // R-24, AC-31: header contains skip_quarantined when active
    #[tokio::test]
    async fn test_header_skip_quarantined_metadata_active() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        let mut buf = Vec::new();

        write_header(pool, &mut buf, true).await.unwrap();

        let header = parse_line(&buf);
        assert_eq!(header["skip_quarantined"], Value::Bool(true));
    }

    // R-24: header omits skip_quarantined when inactive
    #[tokio::test]
    async fn test_header_skip_quarantined_metadata_inactive() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        let mut buf = Vec::new();

        write_header(pool, &mut buf, false).await.unwrap();

        let header = parse_line(&buf);
        assert!(
            !header.as_object().unwrap().contains_key("skip_quarantined"),
            "skip_quarantined should be absent from header when not active"
        );
    }

    // R-16, R-18, AC-23: entries with status=3 are filtered
    #[tokio::test]
    async fn test_skip_entries_filtered() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();

        // 3 active, 2 quarantined
        for id in 1..=3 {
            insert_entry_with_status(pool, id, 1).await;
        }
        insert_entry_with_status(pool, 4, 3).await;
        insert_entry_with_status(pool, 5, 3).await;

        let skip_ids: HashSet<i64> = [4, 5].into_iter().collect();
        let mut buf = Vec::new();
        let skipped = export_entries(pool, &mut buf, &skip_ids).await.unwrap();

        let rows = parse_lines(&buf);
        assert_eq!(rows.len(), 3, "only active entries exported");
        assert_eq!(skipped, 2, "two entries skipped");

        let ids: Vec<i64> = rows.iter().map(|r| r["id"].as_i64().unwrap()).collect();
        assert!(!ids.contains(&4));
        assert!(!ids.contains(&5));
    }

    // R-16, AC-24: entry_tags for quarantined entries are filtered
    #[tokio::test]
    async fn test_skip_entry_tags_filtered() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();

        insert_entry_with_status(pool, 1, 1).await;
        insert_entry_with_status(pool, 2, 3).await; // quarantined

        sqlx::query("INSERT INTO entry_tags (entry_id, tag) VALUES (1, 'good')")
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO entry_tags (entry_id, tag) VALUES (2, 'bad')")
            .execute(pool)
            .await
            .unwrap();

        let skip_ids: HashSet<i64> = [2].into_iter().collect();
        let mut buf = Vec::new();
        let skipped = export_entry_tags(pool, &mut buf, &skip_ids).await.unwrap();

        let rows = parse_lines(&buf);
        assert_eq!(rows.len(), 1, "only active entry tags exported");
        assert_eq!(skipped, 1);
        assert_eq!(rows[0]["entry_id"].as_i64().unwrap(), 1);
    }

    // R-16, AC-25: feature_entries for quarantined entries are filtered
    #[tokio::test]
    async fn test_skip_feature_entries_filtered() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();

        insert_entry_with_status(pool, 1, 1).await;
        insert_entry_with_status(pool, 2, 3).await;

        sqlx::query("INSERT INTO feature_entries (feature_id, entry_id) VALUES ('f1', 1)")
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO feature_entries (feature_id, entry_id) VALUES ('f1', 2)")
            .execute(pool)
            .await
            .unwrap();

        let skip_ids: HashSet<i64> = [2].into_iter().collect();
        let mut buf = Vec::new();
        let skipped = export_feature_entries(pool, &mut buf, &skip_ids)
            .await
            .unwrap();

        let rows = parse_lines(&buf);
        assert_eq!(rows.len(), 1);
        assert_eq!(skipped, 1);
        assert_eq!(rows[0]["entry_id"].as_i64().unwrap(), 1);
    }

    // R-19, AC-26: co_access dual-column check
    #[tokio::test]
    async fn test_skip_co_access_dual_column() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();

        insert_entry_with_status(pool, 1, 1).await;
        insert_entry_with_status(pool, 2, 1).await;
        insert_entry_with_status(pool, 4, 3).await;
        insert_entry_with_status(pool, 5, 3).await;

        // co_access CHECK: entry_id_a < entry_id_b
        // (active, active): 1<2, (quarantined, active): 1<4, (active, quarantined): 2<5, (q,q): 4<5
        for (a, b) in [(1, 2), (1, 4), (2, 5), (4, 5)] {
            sqlx::query(
                "INSERT INTO co_access (entry_id_a, entry_id_b, count, last_updated)
                 VALUES (?1, ?2, 1, 1700000000)",
            )
            .bind(a)
            .bind(b)
            .execute(pool)
            .await
            .unwrap();
        }

        let skip_ids: HashSet<i64> = [4, 5].into_iter().collect();
        let mut buf = Vec::new();
        let skipped = export_co_access(pool, &mut buf, &skip_ids).await.unwrap();

        let rows = parse_lines(&buf);
        assert_eq!(rows.len(), 1, "only (active, active) exported");
        assert_eq!(skipped, 3, "three rows with quarantined endpoints");
        assert_eq!(rows[0]["entry_id_a"].as_i64().unwrap(), 1);
        assert_eq!(rows[0]["entry_id_b"].as_i64().unwrap(), 2);
    }

    // R-20, AC-27: graph_edges dual-column check
    #[tokio::test]
    async fn test_skip_graph_edges_dual_column() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();

        insert_entry_with_status(pool, 1, 1).await;
        insert_entry_with_status(pool, 2, 1).await;
        insert_entry_with_status(pool, 4, 3).await;
        insert_entry_with_status(pool, 5, 3).await;

        // (active, active), (quarantined, active), (active, quarantined), (q, q)
        for (s, t) in [(1, 2), (4, 1), (2, 5), (4, 5)] {
            insert_test_graph_edge(pool, s, t, "relates_to", 1.0, None).await;
        }

        let skip_ids: HashSet<i64> = [4, 5].into_iter().collect();
        let mut buf = Vec::new();
        let skipped = export_graph_edges(pool, &mut buf, &skip_ids).await.unwrap();

        let rows = parse_lines(&buf);
        assert_eq!(rows.len(), 1, "only (active, active) exported");
        assert_eq!(skipped, 3);
        assert_eq!(rows[0]["source_id"].as_i64().unwrap(), 1);
        assert_eq!(rows[0]["target_id"].as_i64().unwrap(), 2);
    }

    // R-18: empty skip set means no filtering
    #[tokio::test]
    async fn test_skip_empty_set_no_change() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();

        insert_entry_with_status(pool, 1, 1).await;
        insert_entry_with_status(pool, 2, 3).await; // quarantined but NOT in skip set

        let skip_ids: HashSet<i64> = HashSet::new();
        let mut buf = Vec::new();
        let skipped = export_entries(pool, &mut buf, &skip_ids).await.unwrap();

        let rows = parse_lines(&buf);
        assert_eq!(rows.len(), 2, "empty skip set exports all entries");
        assert_eq!(skipped, 0);
    }

    // R-18 (edge case #9): skip_quarantined active but no entries have status=3
    #[tokio::test]
    async fn test_skip_quarantined_zero_quarantined() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();

        insert_entry_with_status(pool, 1, 1).await;
        insert_entry_with_status(pool, 2, 1).await;

        // Build skip_ids the way run_export_inner does: query for status=3
        let ids: HashSet<i64> =
            sqlx::query_scalar::<_, i64>("SELECT id FROM entries WHERE status = 3")
                .fetch_all(pool)
                .await
                .unwrap()
                .into_iter()
                .collect();
        assert!(ids.is_empty(), "no quarantined entries");

        let mut buf = Vec::new();
        let skipped = export_entries(pool, &mut buf, &ids).await.unwrap();
        assert_eq!(skipped, 0);
        assert_eq!(parse_lines(&buf).len(), 2);
    }

    // R-16 (edge case #10): all entries quarantined
    #[tokio::test]
    async fn test_skip_quarantined_all_quarantined() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();

        insert_entry_with_status(pool, 1, 3).await;
        insert_entry_with_status(pool, 2, 3).await;

        let skip_ids: HashSet<i64> = [1, 2].into_iter().collect();
        let mut buf = Vec::new();
        let skipped = export_entries(pool, &mut buf, &skip_ids).await.unwrap();

        assert_eq!(skipped, 2);
        assert!(buf.is_empty(), "no entries exported");
    }

    // Full do_export with skip_quarantined active (integrated unit test)
    #[tokio::test]
    async fn test_do_export_skip_quarantined_full() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();

        // Insert active and quarantined entries
        insert_entry_with_status(pool, 1, 1).await;
        insert_entry_with_status(pool, 2, 3).await; // quarantined

        // Add dependents for both
        sqlx::query("INSERT INTO entry_tags (entry_id, tag) VALUES (1, 'active')")
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO entry_tags (entry_id, tag) VALUES (2, 'quarantined')")
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO co_access (entry_id_a, entry_id_b, count, last_updated) VALUES (1, 2, 1, 1700000000)")
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO feature_entries (feature_id, entry_id) VALUES ('f1', 2)")
            .execute(pool)
            .await
            .unwrap();
        insert_test_graph_edge(pool, 2, 1, "relates_to", 1.0, None).await;

        let skip_ids: HashSet<i64> = [2].into_iter().collect();
        let mut buf = Vec::new();
        do_export(pool, &mut buf, &skip_ids, true).await.unwrap();

        let lines = parse_lines(&buf);

        // Header should have skip_quarantined
        let header = &lines[0];
        assert_eq!(header["_header"], Value::Bool(true));
        assert_eq!(header["skip_quarantined"], Value::Bool(true));

        // No row should reference entry id 2
        for line in &lines[1..] {
            if let Some(id) = line.get("id").and_then(|v| v.as_i64()) {
                assert_ne!(id, 2, "quarantined entry should not appear");
            }
            if let Some(eid) = line.get("entry_id").and_then(|v| v.as_i64()) {
                assert_ne!(eid, 2, "quarantined entry_id should not appear");
            }
            if let Some(a) = line.get("entry_id_a").and_then(|v| v.as_i64()) {
                assert_ne!(a, 2, "quarantined entry_id_a should not appear");
            }
            if let Some(b) = line.get("entry_id_b").and_then(|v| v.as_i64()) {
                assert_ne!(b, 2, "quarantined entry_id_b should not appear");
            }
            if let Some(s) = line.get("source_id").and_then(|v| v.as_i64()) {
                assert_ne!(s, 2, "quarantined source_id should not appear");
            }
            if let Some(t) = line.get("target_id").and_then(|v| v.as_i64()) {
                assert_ne!(t, 2, "quarantined target_id should not appear");
            }
        }
    }

    // R-19 (edge case #11): quarantined entry on both sides of different co_access rows
    // NOTE: co_access has CHECK (entry_id_a < entry_id_b), so true self-referencing
    // rows are impossible. Instead we test quarantined appearing in both a-column
    // and b-column across different rows.
    #[tokio::test]
    async fn test_co_access_quarantined_both_columns() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();

        insert_entry_with_status(pool, 1, 1).await; // active
        insert_entry_with_status(pool, 3, 3).await; // quarantined
        insert_entry_with_status(pool, 5, 1).await; // active

        // quarantined as entry_id_a (3 < 5)
        sqlx::query(
            "INSERT INTO co_access (entry_id_a, entry_id_b, count, last_updated)
             VALUES (3, 5, 1, 1700000000)",
        )
        .execute(pool)
        .await
        .unwrap();
        // quarantined as entry_id_b (1 < 3)
        sqlx::query(
            "INSERT INTO co_access (entry_id_a, entry_id_b, count, last_updated)
             VALUES (1, 3, 2, 1700000000)",
        )
        .execute(pool)
        .await
        .unwrap();

        let skip_ids: HashSet<i64> = [3].into_iter().collect();
        let mut buf = Vec::new();
        let skipped = export_co_access(pool, &mut buf, &skip_ids).await.unwrap();

        assert_eq!(skipped, 2, "both rows filtered");
        assert!(buf.is_empty());
    }

    // R-20 (edge case #12): self-loop graph edge with quarantined entry
    #[tokio::test]
    async fn test_graph_edges_self_loop_quarantined() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();

        insert_entry_with_status(pool, 1, 3).await;
        insert_test_graph_edge(pool, 1, 1, "self_ref", 1.0, None).await;

        let skip_ids: HashSet<i64> = [1].into_iter().collect();
        let mut buf = Vec::new();
        let skipped = export_graph_edges(pool, &mut buf, &skip_ids).await.unwrap();

        assert_eq!(skipped, 1);
        assert!(buf.is_empty(), "self-loop quarantined edge filtered");
    }

    // =======================================================================
    // vnc-048: `--slug` branch + AC-06 stderr count summary
    // =======================================================================
    //
    // These drive `run_export_with_base(..., slug, ...)` — the operator entry
    // point — with `base` pinned to a TempDir. The seed path (the runtime
    // `http_provision` literal-slug layout) is DISTINCT code from the CLI read
    // path (`run_export_with_base` → `resolve_slug_store`).

    use crate::http::ProjectSlug;
    use crate::project::ensure_data_directory;
    use crate::projects::per_slug_data_dir;
    use std::path::PathBuf;

    /// Open a store at `db_path` and insert `entries` rows with the given ids.
    /// The parent dir is created first; the store handle is dropped on return so
    /// the CLI read path opens it fresh.
    async fn seed_store_at(db_path: &Path, ids: &[i64]) {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).expect("create store parent dir");
        }
        let store = SqlxStore::open(db_path, PoolConfig::default())
            .await
            .expect("open seed store");
        let pool = store.write_pool_server();
        for &id in ids {
            sqlx::query(
                "INSERT INTO entries (
                    id, title, content, topic, category, source, status, confidence,
                    created_at, updated_at
                ) VALUES (?1, ?2, ?3, 't', 'p', 's', 1, 0.5, 1, 1)",
            )
            .bind(id)
            .bind(format!("title-{id}"))
            .bind(format!("content-{id}"))
            .execute(pool)
            .await
            .expect("insert seed entry");
        }
    }

    /// Seed a per-slug store via the runtime literal-slug layout
    /// (`per_slug_data_dir(base, &ProjectSlug) / "unimatrix.db"`). Returns db_path.
    async fn seed_slug_store(base: &Path, slug: &str, ids: &[i64]) -> PathBuf {
        let pslug = ProjectSlug::try_from(slug).expect("valid slug");
        let db_path = per_slug_data_dir(base, &pslug).join("unimatrix.db");
        seed_store_at(&db_path, ids).await;
        db_path
    }

    /// Read a JSONL export file and return the sorted `entries`-table ids emitted.
    fn emitted_entry_ids(path: &Path) -> Vec<i64> {
        let content = std::fs::read_to_string(path).expect("read export file");
        let mut ids: Vec<i64> = content
            .lines()
            .filter(|l| !l.is_empty())
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .filter(|v| v.get("_table").and_then(|t| t.as_str()) == Some("entries"))
            .filter_map(|v| v.get("id").and_then(|i| i.as_i64()))
            .collect();
        ids.sort_unstable();
        ids
    }

    /// Create a canonicalizable project dir under `base`.
    fn make_project_dir(base: &Path) -> PathBuf {
        let proj = base.join("project");
        std::fs::create_dir_all(&proj).expect("create project dir");
        proj
    }

    // ── R-01 S1: disagreement seam (AC-09, TOP weight, gate non-negotiable) ──
    //
    // #4974 / #5507: an N=1 same-path test — B empty, or B seeded through the
    // SAME layout as A onto the same store — is CEREMONIAL and does NOT satisfy
    // AC-09. Here the seed layout (per_slug_data_dir + direct SqlxStore::open) and
    // the CLI resolver (run_export_with_base → resolve_slug_store) are DIFFERENT
    // code, and set B is non-empty AND disjoint from A by construction, so
    // `emitted == A` and `emitted ∩ B == ∅` actually exercises slug-vs-hash divergence.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_export_slug_emits_slug_store_not_hash_store() {
        let base = tempfile::TempDir::new().unwrap();
        let proj = make_project_dir(base.path());

        let set_a = [101i64, 102, 103];
        let set_b = [201i64, 202, 203];
        seed_slug_store(base.path(), "alpha", &set_a).await;
        let paths = ensure_data_directory(Some(&proj), Some(base.path())).expect("hash paths");
        seed_store_at(&paths.db_path, &set_b).await;

        let out = base.path().join("out.jsonl");
        run_export_with_base(
            Some(&proj),
            Some(&out),
            base.path(),
            Some("alpha"),
            false,
            false,
        )
        .expect("export slug store");

        let emitted = emitted_entry_ids(&out);
        assert_eq!(emitted, set_a.to_vec(), "must emit the slug store's set A");
        for b in set_b {
            assert!(
                !emitted.contains(&b),
                "emitted ∩ B must be empty, but found hash-store id {b}"
            );
        }
    }

    // ── R-01 S2: no-slug divergence guard (proves the two paths genuinely differ) ──
    #[tokio::test(flavor = "multi_thread")]
    async fn test_export_no_slug_emits_hash_store_divergence_guard() {
        let base = tempfile::TempDir::new().unwrap();
        let proj = make_project_dir(base.path());

        let set_a = [101i64, 102, 103];
        let set_b = [201i64, 202, 203];
        seed_slug_store(base.path(), "alpha", &set_a).await;
        let paths = ensure_data_directory(Some(&proj), Some(base.path())).expect("hash paths");
        seed_store_at(&paths.db_path, &set_b).await;

        let out = base.path().join("out.jsonl");
        run_export_with_base(Some(&proj), Some(&out), base.path(), None, false, false)
            .expect("export hash store");

        let emitted = emitted_entry_ids(&out);
        assert_eq!(
            emitted,
            set_b.to_vec(),
            "no-slug must emit the path-hash store's set B"
        );
        for a in set_a {
            assert!(
                !emitted.contains(&a),
                "no-slug must NOT emit slug-store id {a}"
            );
        }
    }

    // ── R-02 / R-14: missing store fails loud, existence gate creates nothing (AC-03) ──
    #[tokio::test(flavor = "multi_thread")]
    async fn test_export_slug_missing_store_fails_loud_fs_unchanged() {
        let base = tempfile::TempDir::new().unwrap();
        let proj = make_project_dir(base.path());
        let out = base.path().join("out.jsonl");
        let slug_dir = base.path().join("ghost");
        assert!(!slug_dir.exists(), "precondition: no slug store");

        let res = run_export_with_base(
            Some(&proj),
            Some(&out),
            base.path(),
            Some("ghost"),
            false,
            false,
        );

        let err = res.expect_err("missing store must fail");
        let msg = err.to_string();
        let expected_db = slug_dir.join("unimatrix.db");
        assert!(
            msg.contains(&expected_db.display().to_string()),
            "error must name the fully-resolved absolute db path: {msg}"
        );
        // The existence gate is before `open`: nothing created under the slug dir.
        assert!(!out.exists(), "no output file written on miss");
        assert!(
            !slug_dir.exists(),
            "existence gate created nothing (no slug dir)"
        );
        assert!(!expected_db.exists(), "no unimatrix.db auto-created");
        assert!(
            !slug_dir.join("unimatrix.db-wal").exists(),
            "no -wal created"
        );
        assert!(
            !slug_dir.join("unimatrix.db-shm").exists(),
            "no -shm created"
        );
    }

    // ── R-08: validation at the CLI edge (AC-04) — charset/reserved/traversal ──
    #[tokio::test(flavor = "multi_thread")]
    async fn test_export_slug_invalid_rejected_no_fs_touch() {
        let base = tempfile::TempDir::new().unwrap();
        let proj = make_project_dir(base.path());
        let out = base.path().join("out.jsonl");

        for bad in ["Foo!", "UPPER", "a_b", "v1", "tools", "../etc", "a/b", ".."] {
            let res = run_export_with_base(
                Some(&proj),
                Some(&out),
                base.path(),
                Some(bad),
                false,
                false,
            );
            assert!(res.is_err(), "invalid slug {bad:?} must be rejected");
            assert!(!out.exists(), "no output file for rejected slug {bad:?}");
        }
    }

    // ── R-13: stray/hash-looking slug dir never reinterpreted in no-slug mode (AC-11) ──
    #[tokio::test(flavor = "multi_thread")]
    async fn test_export_no_slug_with_populated_slug_dir_emits_only_hash() {
        let base = tempfile::TempDir::new().unwrap();
        let proj = make_project_dir(base.path());

        // A populated slug dir whose name looks like a 16-hex path-hash segment —
        // charset-valid, but no-slug mode must never reinterpret it (documented, AC-11).
        let set_a = [301i64, 302];
        let set_b = [401i64, 402, 403];
        seed_slug_store(base.path(), "abcdef0123456789", &set_a).await;
        let paths = ensure_data_directory(Some(&proj), Some(base.path())).expect("hash paths");
        seed_store_at(&paths.db_path, &set_b).await;

        let out = base.path().join("out.jsonl");
        run_export_with_base(Some(&proj), Some(&out), base.path(), None, false, false)
            .expect("export hash store");

        let emitted = emitted_entry_ids(&out);
        assert_eq!(emitted, set_b.to_vec(), "no-slug emits only the hash store");
        for a in set_a {
            assert!(
                !emitted.contains(&a),
                "stray slug dir id {a} must not appear"
            );
        }
    }

    // ── AC-06: stderr count summary wording (format is pure + unit-testable) ──
    #[test]
    fn test_format_export_summary_file_dest() {
        let counts = ExportCounts {
            entries: 5,
            audit_rows: 3,
        };
        let p = PathBuf::from("/tmp/out.jsonl");
        let s = format_export_summary(&counts, Some(p.as_path()));
        assert!(s.contains("exported 5 entries"), "{s}");
        assert!(s.contains("3 audit rows"), "{s}");
        assert!(s.contains("/tmp/out.jsonl"), "{s}");
        assert!(
            s.contains('\u{2192}'),
            "summary must include the → arrow: {s}"
        );
    }

    #[test]
    fn test_format_export_summary_stdout_dest_sparse_self_diagnoses() {
        // 0 entries + audit rows → the self-diagnosing sparse line (ADR-006).
        let counts = ExportCounts {
            entries: 0,
            audit_rows: 7,
        };
        let s = format_export_summary(&counts, None);
        assert!(
            s.contains("exported 0 entries"),
            "sparse self-diagnoses: {s}"
        );
        assert!(s.contains("7 audit rows"), "{s}");
        assert!(s.contains("stdout"), "None output prints → stdout: {s}");
    }

    // ── do_export returns the counts the summary reports ──
    #[tokio::test]
    async fn test_do_export_returns_written_counts() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        insert_entry_with_status(pool, 1, 1).await;
        insert_entry_with_status(pool, 2, 1).await;
        sqlx::query(
            "INSERT INTO audit_log (event_id, timestamp, session_id, agent_id,
             operation, target_ids, outcome, detail)
             VALUES (1, 1, 's1', 'a1', 'store', '[]', 0, 'ok')",
        )
        .execute(pool)
        .await
        .unwrap();

        let mut buf = Vec::new();
        let counts = do_export(pool, &mut buf, &HashSet::new(), false)
            .await
            .unwrap();
        assert_eq!(counts.entries, 2, "two entries written");
        assert_eq!(counts.audit_rows, 1, "one audit row written");
    }

    #[tokio::test]
    async fn test_do_export_written_count_excludes_skipped() {
        let (store, _tmp) = setup_test_db().await;
        let pool = store.write_pool_server();
        insert_entry_with_status(pool, 1, 1).await; // active
        insert_entry_with_status(pool, 2, 3).await; // quarantined

        let skip_ids: HashSet<i64> = [2].into_iter().collect();
        let mut buf = Vec::new();
        let counts = do_export(pool, &mut buf, &skip_ids, true).await.unwrap();
        assert_eq!(
            counts.entries, 1,
            "written excludes the skipped quarantined entry"
        );
    }
}
