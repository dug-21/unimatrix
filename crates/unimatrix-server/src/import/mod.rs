//! Knowledge base import from JSONL format (nan-002).
//!
//! Restores a Unimatrix knowledge base from a nan-001 JSONL export dump,
//! preserving all learned signals (confidence, helpful/unhelpful counts,
//! co-access pairs, correction chains). Creates a local multi-thread tokio
//! runtime for async sqlx access (nxs-011).
//!
//! The import runs in two phases (ADR-004):
//! 1. Database restore: header validation, pre-flight, JSONL ingestion, hash check
//! 2. Embedding reconstruction: re-embed all entries, build HNSW index (separate component)

mod inserters;

use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::Path;
use std::sync::Arc;

use sqlx::SqlitePool;
use sqlx::sqlite::SqliteConnection;
use unimatrix_store::{AuditEvent, Outcome, SqlxStore};

use crate::format::{ExportHeader, ExportRow};
use crate::infra::pidfile::{is_process_alive, is_unimatrix_process, read_pid_file};
use crate::project;
use crate::projects::slug_store::{SlugStorePaths, resolve_slug_store};

use inserters::{
    insert_agent_registry, insert_audit_log, insert_co_access, insert_counter, insert_cycle_event,
    insert_entry, insert_entry_tag, insert_feature_entry, insert_graph_edge, insert_observation,
    insert_outcome_index,
};

/// Tracking struct for per-table insert counts.
#[derive(Debug, Default)]
pub struct ImportCounts {
    pub counters: u64,
    pub entries: u64,
    pub entry_tags: u64,
    pub co_access: u64,
    pub feature_entries: u64,
    pub outcome_index: u64,
    pub agent_registry: u64,
    pub audit_log: u64,
    pub graph_edges: u64,
    pub observations: u64,
    pub cycle_events: u64,
}

/// Run the import pipeline.
///
/// Supports being called from both sync and async contexts. When an existing
/// tokio runtime is detected, uses `block_in_place` to avoid nesting runtimes.
/// When called from a sync context, creates a new multi-thread runtime
/// (`block_in_place` requires multi_thread flavor; current_thread panics).
pub fn run_import(
    project_dir: Option<&Path>,
    input: &Path,
    slug: Option<&str>,
    skip_hash_validation: bool,
    force: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    run_import_inner(project_dir, input, slug, skip_hash_validation, force, None)
}

/// Run the import pipeline with an explicit `base_dir` for test isolation.
///
/// Identical to [`run_import`] but routes data storage to the given `base_dir`
/// instead of `~/.unimatrix/`. Use this in tests to avoid leaking directories
/// into the user's home directory.
pub fn run_import_with_base(
    project_dir: Option<&Path>,
    input: &Path,
    slug: Option<&str>,
    skip_hash_validation: bool,
    force: bool,
    base_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    run_import_inner(
        project_dir,
        input,
        slug,
        skip_hash_validation,
        force,
        Some(base_dir),
    )
}

fn run_import_inner(
    project_dir: Option<&Path>,
    input: &Path,
    slug: Option<&str>,
    skip_hash_validation: bool,
    force: bool,
    base_dir: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            // Already inside an async runtime — use block_in_place to avoid nesting.
            tokio::task::block_in_place(|| {
                handle.block_on(run_import_async(
                    project_dir,
                    input,
                    slug,
                    skip_hash_validation,
                    force,
                    base_dir,
                ))
            })
        }
        Err(_) => {
            // No existing runtime — create one. Must be multi_thread so that
            // block_in_place (used by embed_reconstruct) does not panic.
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            rt.block_on(run_import_async(
                project_dir,
                input,
                slug,
                skip_hash_validation,
                force,
                base_dir,
            ))
        }
    }
}

/// Async implementation of the import pipeline.
async fn run_import_async(
    project_dir: Option<&Path>,
    input: &Path,
    slug: Option<&str>,
    skip_hash_validation: bool,
    force: bool,
    base_dir: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Phase 1: Setup — path-hash paths (C-6 still creates the hash data dir).
    let paths = project::ensure_data_directory(project_dir, base_dir)?;

    // Resolve the DB + vector write targets ONCE, up front. In slug mode they come
    // from the shared funnel's `SlugStorePaths`; the PID path is NEVER redirected —
    // it stays base-scoped on `paths.pid_path` (ADR-003/004). Two path sources in
    // slug mode: do NOT tidy them into one.
    let (db_target, vector_target) = match slug {
        Some(raw) => {
            // Funnel: validate slug → derive base → join → pre-open existence gate
            // (ADR-001/002, C-3). Creates nothing; opens no DB.
            let slug_store: SlugStorePaths = resolve_slug_store(&paths, raw)?;
            (slug_store.db_path, slug_store.vector_dir)
        }
        None => (paths.db_path.clone(), paths.vector_dir.clone()),
    };

    // Live-PID hard-error gate (ADR-003, AC-13) — slug mode ONLY, pre-open and
    // structural. Reads the base-scoped daemon PID from `paths.pid_path` (NOT
    // `SlugStorePaths`, which deliberately omits it). Placed before open so a live
    // daemon refuses before any DB is touched or written.
    if slug.is_some() {
        preflight_live_pid_refusal(&paths.pid_path)?;
    }

    // Phase 1b: open the TARGET store (slug or path-hash). Reached only after the
    // funnel's existence gate returned Ok in slug mode (C-3).
    let store = Arc::new(
        SqlxStore::open(
            &db_target,
            unimatrix_store::pool_config::PoolConfig::default(),
        )
        .await?,
    );
    let pool = store.write_pool_server();

    // Phase 2: Open and parse header
    let file = File::open(input)?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();

    let header_line = lines.next().ok_or("empty file: no header line")??;
    let header = parse_header(&header_line)?;

    // Phase 3: Pre-flight checks
    let db_schema_version: i64 =
        sqlx::query_scalar::<_, i64>("SELECT value FROM counters WHERE name = 'schema_version'")
            .fetch_one(pool)
            .await?;

    check_preflight(pool, force, &paths, &db_target, slug.is_some()).await?;

    // Phase 4: Validate header against DB
    match header.format_version {
        1 | 2 => { /* ok */ }
        v => {
            return Err(format!(
                "unsupported format_version: {v}. This binary supports format_version 1 and 2."
            )
            .into());
        }
    }
    if header.schema_version > db_schema_version {
        return Err(format!(
            "export schema_version ({}) is newer than this binary's schema_version ({}). Upgrade unimatrix.",
            header.schema_version, db_schema_version
        )
        .into());
    }

    // Phase 5: Force-drop if needed
    if force {
        let entry_count: i64 = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM entries")
            .fetch_one(pool)
            .await?;
        if entry_count > 0 {
            eprintln!(
                "WARNING: --force specified. Dropping {} existing entries and all associated data in {}.",
                entry_count,
                paths.data_dir.display()
            );
        }
        drop_all_data(pool).await?;
    }

    // Phase 6: Acquire a dedicated connection and BEGIN IMMEDIATE.
    //
    // Must use a single connection (not the pool) for the entire import transaction.
    // BEGIN IMMEDIATE acquires a write lock on this connection; all subsequent INSERTs
    // must execute on the same connection — using the pool would dispatch them to a
    // different connection that cannot see the open transaction and would deadlock
    // (SQLITE_BUSY code 5) trying to acquire its own write lock.
    let mut conn = pool.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;

    // Phase 7: Ingest JSONL
    let counts = match ingest_rows(&mut conn, lines).await {
        Ok(counts) => counts,
        Err(e) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            return Err(e);
        }
    };

    // Phase 8: Hash validation (inside transaction, before commit)
    if !skip_hash_validation {
        if let Err(e) = validate_hashes(&mut conn).await {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            return Err(e);
        }
    } else {
        eprintln!("WARNING: hash validation skipped (--skip-hash-validation)");
    }

    // Phase 9: COMMIT
    sqlx::query("COMMIT").execute(&mut *conn).await?;
    // Release write connection back to pool before Phase 10+. embed_reconstruct
    // and record_provenance both acquire from the same single-connection write
    // pool; leaving conn alive would cause a pool timeout (GH#303).
    drop(conn);

    // Phase 10: Re-embed and build vector index (ADR-004: after DB commit).
    // In slug mode this targets `{slug}/vector` (vnc-048 ADR-004) so the daemon
    // loads the rebuilt index at the next `start`; in no-slug mode it is the
    // path-hash `paths.vector_dir` (unchanged).
    crate::embed_reconstruct::reconstruct_embeddings(&store, &vector_target)?;

    // Phase 11: Record provenance
    record_provenance(&store, input, &counts).await?;

    // Phase 12: Summary
    print_summary(&counts, skip_hash_validation);

    Ok(())
}

/// Parse and validate the JSONL header line.
fn parse_header(line: &str) -> Result<ExportHeader, Box<dyn std::error::Error>> {
    let header: ExportHeader =
        serde_json::from_str(line).map_err(|e| format!("invalid header line: {e}"))?;

    if !header._header {
        return Err("header line: _header must be true".into());
    }

    Ok(header)
}

/// Pre-flight checks (all before any write): DB-empty/`--force` check, the
/// slug-mode non-empty-`audit_log` refusal (ADR-005), and the no-slug PID warning.
///
/// `db_target` is the resolved store path (the slug db in slug mode) and is named
/// by the audit-refusal message so the operator sees the correct absolute path
/// (vnc-048 OQ-4). In slug mode the live-PID HARD gate already ran pre-open
/// (`preflight_live_pid_refusal`); the warning-only PID branch is therefore
/// restricted to no-slug mode so AC-05 parity holds.
async fn check_preflight(
    pool: &SqlitePool,
    force: bool,
    paths: &project::ProjectPaths,
    db_target: &Path,
    slug_mode: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let entry_count: i64 = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM entries")
        .fetch_one(pool)
        .await?;

    if entry_count > 0 && !force {
        return Err(format!(
            "database is not empty ({} entries). Use --force to drop existing data, or use a fresh --project-dir.",
            entry_count
        )
        .into());
    }

    // Non-empty-`audit_log` pre-flight refusal (ADR-005, AC-10/FR-13, C-5) —
    // slug mode ONLY, after the entry-count check and BEFORE any write. The
    // append-only `audit_log` cannot be cleared by `drop_all_data` (schema v25
    // triggers), so restoring over a non-empty target would collide on the
    // explicit-`event_id` INSERT with a raw SQLite UNIQUE error. Refuse loud first
    // with an actionable message naming the resolved slug db path; NEVER surface
    // the raw UNIQUE error. `--force` does NOT bypass this (no such override).
    if slug_mode {
        let audit_rows: i64 = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM audit_log")
            .fetch_one(pool)
            .await?;
        if audit_rows > 0 {
            return Err(format!(
                "restore target already has {audit_rows} audit rows at {abs}; restore \
                 targets a fresh slug. Run `project register <new-slug>` and import there.",
                audit_rows = audit_rows,
                abs = db_target.display()
            )
            .into());
        }
    }

    // PID file check -- warning only, do not block (SR-07). No-slug mode only: in
    // slug mode the live-PID HARD gate already ran pre-open (ADR-003), so re-warning
    // here would be redundant.
    if !slug_mode && paths.pid_path.exists() {
        eprintln!(
            "WARNING: PID file exists at {}. A server may be running. Consider stopping it before import.",
            paths.pid_path.display()
        );
    }

    Ok(())
}

/// Live-PID hard-error gate (ADR-003, AC-13) — slug-mode import refusal.
///
/// Refuses when a LIVE `unimatrix` daemon PID is present at the base-scoped
/// `pid_path`. The predicate is **liveness**, not file presence: `read_pid_file`
/// then `is_process_alive` (kill -0) AND `is_unimatrix_process` (`/proc` cmdline
/// identity). A stale/dead PID file, or a reused OS PID owned by a non-unimatrix
/// process, must NOT block (R-11). Importing into a live slug would be clobbered
/// when the daemon dumps its stale in-memory index at shutdown; refusing pre-open
/// makes that clobber structurally unreachable. No `--force` override exists.
fn preflight_live_pid_refusal(pid_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(pid) = read_pid_file(pid_path)
        && is_process_alive(pid)
        && is_unimatrix_process(pid)
    {
        return Err(format!(
            "a live unimatrix daemon (pid {pid}) is running; its PID file is {abs}. \
             Importing into a live slug would be clobbered at the daemon's next \
             shutdown. Stop the daemon first: stop → import --slug … → start.",
            pid = pid,
            abs = pid_path.display()
        )
        .into());
    }
    Ok(())
}

/// Drop all data from importable tables (excludes audit_log).
///
/// Uses DELETE (not DROP TABLE) to preserve schema.
/// FK-dependent tables deleted first, then parent tables.
///
/// audit_log is excluded: append-only triggers (vnc-014 / ASS-050 schema v25)
/// reject DELETE statements. Audit history is preserved across import resets
/// per ADR-005. See retention.rs gc_audit_log for the GC deferral note.
async fn drop_all_data(pool: &SqlitePool) -> Result<(), Box<dyn std::error::Error>> {
    sqlx::query(
        "DELETE FROM entry_tags;
         DELETE FROM co_access;
         DELETE FROM feature_entries;
         DELETE FROM outcome_index;
         DELETE FROM agent_registry;
         DELETE FROM vector_map;
         DELETE FROM observation_phase_metrics;
         DELETE FROM observation_metrics;
         DELETE FROM graph_edges;
         DELETE FROM observations;
         DELETE FROM cycle_events;
         DELETE FROM entries;
         DELETE FROM counters;",
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Ingest JSONL data lines into the database.
///
/// Reads lines one-by-one, deserializes via `ExportRow`, and routes to
/// per-table INSERT functions. Tracks counts for progress reporting.
///
/// `conn` must be a single connection that already has `BEGIN IMMEDIATE` active.
/// All INSERTs execute on this connection to remain within the same transaction.
async fn ingest_rows(
    conn: &mut SqliteConnection,
    lines: impl Iterator<Item = io::Result<String>>,
) -> Result<ImportCounts, Box<dyn std::error::Error>> {
    let mut counts = ImportCounts::default();

    // header was line 1; data lines start at line 2.
    for (line_number, line_result) in (2_u64..).zip(lines) {
        let line = line_result.map_err(|e| format!("I/O error reading line {line_number}: {e}"))?;

        if line.is_empty() {
            continue;
        }

        let row: ExportRow = serde_json::from_str(&line)
            .map_err(|e| format!("JSON parse error on line {line_number}: {e}"))?;

        match row {
            ExportRow::Counter(r) => {
                insert_counter(conn, &r).await?;
                counts.counters += 1;
            }
            ExportRow::Entry(r) => {
                insert_entry(conn, &r).await?;
                counts.entries += 1;
                if counts.entries % 100 == 0 {
                    eprintln!("  Inserted {} entries...", counts.entries);
                }
            }
            ExportRow::EntryTag(r) => {
                insert_entry_tag(conn, &r).await?;
                counts.entry_tags += 1;
            }
            ExportRow::CoAccess(r) => {
                insert_co_access(conn, &r).await?;
                counts.co_access += 1;
            }
            ExportRow::FeatureEntry(r) => {
                insert_feature_entry(conn, &r).await?;
                counts.feature_entries += 1;
            }
            ExportRow::OutcomeIndex(r) => {
                insert_outcome_index(conn, &r).await?;
                counts.outcome_index += 1;
            }
            ExportRow::AgentRegistry(r) => {
                insert_agent_registry(conn, &r).await?;
                counts.agent_registry += 1;
            }
            ExportRow::AuditLog(r) => {
                insert_audit_log(conn, &r).await?;
                counts.audit_log += 1;
            }
            ExportRow::GraphEdge(r) => {
                insert_graph_edge(conn, &r).await?;
                counts.graph_edges += 1;
            }
            ExportRow::Observation(r) => {
                insert_observation(conn, &r).await?;
                counts.observations += 1;
            }
            ExportRow::CycleEvent(r) => {
                insert_cycle_event(conn, &r).await?;
                counts.cycle_events += 1;
            }
        }
    }

    eprintln!("  Inserted {} entries", counts.entries);
    Ok(counts)
}

// ---------------------------------------------------------------------------
// Hash validation
// ---------------------------------------------------------------------------

/// Validate content hashes and chain integrity for all imported entries.
///
/// Thin adapter over the single integrity oracle
/// [`unimatrix_store::chain_verify::verify_entries`] (ADR-001 — one oracle, no
/// second/divergent implementation). Loads ALL entries from the **in-flight**
/// `BEGIN IMMEDIATE` transaction connection so the just-inserted, uncommitted
/// rows are visible; a pooled/committed read would see zero rows mid-import.
///
/// No status filter and the full `ENTRY_COLUMNS` set: `entry_from_row` needs
/// `supersedes`/`version`/`status` to reconstruct each `EntryRecord`, and
/// Deprecated predecessors (superseded originals) must load so a chained
/// successor's `supersedes` target is present in the corpus (R-02).
///
/// The core recomputes each `content_hash` AND checks every populated chain link
/// against its `supersedes` predecessor — a strictly stronger check than the old
/// "previous_hash references *some* known hash" existence test (R-04). Empty
/// `previous_hash` is skipped as unverifiable-legacy (preserved from the prior
/// behavior). On a non-clean report we return `Err`; the caller ROLLBACKs before
/// COMMIT, so a tampered corpus is never committed.
async fn validate_hashes(conn: &mut SqliteConnection) -> Result<(), Box<dyn std::error::Error>> {
    let sql = format!(
        "SELECT {} FROM entries ORDER BY id",
        unimatrix_store::read::ENTRY_COLUMNS
    );
    let rows = sqlx::query(&sql).fetch_all(&mut *conn).await?;

    let entries = rows
        .iter()
        .map(unimatrix_store::read::entry_from_row)
        .collect::<Result<Vec<_>, _>>()?;

    let report = unimatrix_store::chain_verify::verify_entries(&entries);

    if !report.is_clean() {
        return Err(report.describe().into());
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Provenance and summary
// ---------------------------------------------------------------------------

/// Record an audit log entry documenting the import operation.
///
/// Uses `SqlxStore::log_audit_event()` which allocates the event_id via the
/// atomic `next_audit_id` counter, avoiding the counter desync that occurred
/// when this function previously computed `MAX(event_id)+1` manually (GH#633).
async fn record_provenance(
    store: &SqlxStore,
    input_path: &Path,
    counts: &ImportCounts,
) -> Result<(), Box<dyn std::error::Error>> {
    let detail = format!(
        "Imported from '{}': {} entries, {} tags, {} co-access pairs, {} counters, \
         {} graph_edges, {} observations, {} cycle_events",
        input_path.display(),
        counts.entries,
        counts.entry_tags,
        counts.co_access,
        counts.counters,
        counts.graph_edges,
        counts.observations,
        counts.cycle_events
    );

    let event = AuditEvent {
        session_id: "import".to_string(),
        agent_id: "system".to_string(),
        operation: "import".to_string(),
        outcome: Outcome::Success,
        detail,
        ..AuditEvent::default()
    };

    store
        .log_audit_event(event)
        .await
        .map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })?;

    Ok(())
}

/// Print import summary to stderr.
fn print_summary(counts: &ImportCounts, skip_hash_validation: bool) {
    eprintln!("Import complete:");
    eprintln!("  Counters:        {}", counts.counters);
    eprintln!("  Entries:         {}", counts.entries);
    eprintln!("  Entry tags:      {}", counts.entry_tags);
    eprintln!("  Co-access pairs: {}", counts.co_access);
    eprintln!("  Feature entries: {}", counts.feature_entries);
    eprintln!("  Outcome index:   {}", counts.outcome_index);
    eprintln!("  Agent registry:  {}", counts.agent_registry);
    eprintln!("  Audit log:       {}", counts.audit_log);
    eprintln!("  Graph edges:     {}", counts.graph_edges);
    eprintln!("  Observations:    {}", counts.observations);
    eprintln!("  Cycle events:    {}", counts.cycle_events);

    if skip_hash_validation {
        eprintln!("  Hash validation: SKIPPED");
    } else {
        eprintln!("  Hash validation: PASSED");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;
    use unimatrix_store::compute_content_hash;
    // --- Helpers ---

    /// Current schema version constant — must match CURRENT_SCHEMA_VERSION in
    /// unimatrix-store/src/migration.rs. Update when the schema advances.
    const CURRENT_SCHEMA_VERSION: i64 = 12;

    /// Create a project dir structure with an isolated base_dir and return both.
    ///
    /// Does NOT open a SqlxStore — run_import will create and migrate the database
    /// on first use. This avoids holding a pool connection that would conflict with
    /// run_import's BEGIN IMMEDIATE when both try to write the same SQLite file.
    fn make_project_dir() -> (TempDir, TempDir) {
        let project_dir = TempDir::new().expect("create project temp dir");
        let base_dir = TempDir::new().expect("create base temp dir");
        let paths = project::ensure_data_directory(Some(project_dir.path()), Some(base_dir.path()))
            .unwrap();
        // Meta-assertion: data_dir must live inside base_dir (GH#640 guard).
        assert!(
            paths.data_dir.starts_with(base_dir.path()),
            "data_dir must be inside base_dir to prevent home directory leaks"
        );
        (project_dir, base_dir)
    }

    async fn open_test_store_at(db_path: &Path) -> SqlxStore {
        SqlxStore::open(db_path, unimatrix_store::pool_config::PoolConfig::default())
            .await
            .expect("open store")
    }

    /// Build an ExportHeader JSON line.
    fn make_header(schema_version: i64, format_version: i64, entry_count: i64) -> String {
        serde_json::json!({
            "_header": true,
            "schema_version": schema_version,
            "exported_at": 1700000000i64,
            "entry_count": entry_count,
            "format_version": format_version
        })
        .to_string()
    }

    /// Build a minimal valid entry JSON line with correct content_hash, no
    /// supersedes edge (genesis / standalone). `version` = 1.
    fn make_entry_line(id: i64, title: &str, content: &str, previous_hash: &str) -> String {
        make_entry_line_full(id, title, content, previous_hash, None, 1, 0)
    }

    /// Build an entry JSON line with full control over the chaining fields.
    ///
    /// `supersedes`/`version`/`status` let tests model real correction chains
    /// (a populated `previous_hash` is always co-produced with a `supersedes`
    /// edge on the production write path — nxs-014). `content_hash` is always
    /// computed correctly unless a caller mutates the emitted JSON afterward.
    fn make_entry_line_full(
        id: i64,
        title: &str,
        content: &str,
        previous_hash: &str,
        supersedes: Option<i64>,
        version: i64,
        status: i64,
    ) -> String {
        let hash = compute_content_hash(title, content);
        serde_json::json!({
            "_table": "entries",
            "id": id,
            "title": title,
            "content": content,
            "topic": "testing",
            "category": "pattern",
            "source": "test",
            "status": status,
            "confidence": 0.5,
            "created_at": 1700000000i64,
            "updated_at": 1700000001i64,
            "last_accessed_at": 0,
            "access_count": 0,
            "supersedes": supersedes,
            "superseded_by": null,
            "correction_count": 0,
            "embedding_dim": 384,
            "created_by": "agent",
            "modified_by": "agent",
            "content_hash": hash,
            "previous_hash": previous_hash,
            "version": version,
            "feature_cycle": "",
            "trust_source": "direct",
            "helpful_count": 0,
            "unhelpful_count": 0,
            "pre_quarantine_status": null
        })
        .to_string()
    }

    /// Build a counter JSON line.
    fn make_counter_line(name: &str, value: i64) -> String {
        serde_json::json!({
            "_table": "counters",
            "name": name,
            "value": value
        })
        .to_string()
    }

    /// Write lines to a temporary file and return the path.
    fn write_jsonl(dir: &TempDir, lines: &[String]) -> std::path::PathBuf {
        let path = dir.path().join("import.jsonl");
        let mut f = File::create(&path).unwrap();
        for line in lines {
            writeln!(f, "{line}").unwrap();
        }
        path
    }

    // --- Header Validation ---

    #[test]
    fn test_validate_header_valid() {
        let json = make_header(11, 1, 5);
        let h = parse_header(&json).unwrap();
        assert!(h._header);
        assert_eq!(h.format_version, 1);
        assert_eq!(h.schema_version, 11);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_validate_header_bad_format_version() {
        let (project_dir, base_dir) = make_project_dir();
        let sv = CURRENT_SCHEMA_VERSION;
        let output_dir = TempDir::new().unwrap();
        let lines = vec![make_header(sv, 3, 0)];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import_with_base(
            Some(project_dir.path()),
            &input_path,
            None,
            false,
            false,
            base_dir.path(),
        );
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("3"), "should mention version 3: {err}");
        assert!(
            err.contains("format_version"),
            "should mention format_version: {err}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_validate_header_future_schema_version() {
        let (project_dir, base_dir) = make_project_dir();
        let output_dir = TempDir::new().unwrap();
        let lines = vec![make_header(999, 1, 0)];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import_with_base(
            Some(project_dir.path()),
            &input_path,
            None,
            false,
            false,
            base_dir.path(),
        );
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.to_lowercase().contains("upgrade"),
            "should suggest upgrade: {err}"
        );
    }

    #[test]
    fn test_validate_header_missing_header_flag() {
        let json = r#"{"_header":false,"schema_version":11,"exported_at":1,"entry_count":0,"format_version":1}"#;
        let result = parse_header(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("_header"), "should mention _header: {err}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_validate_header_format_version_zero() {
        let (project_dir, base_dir) = make_project_dir();
        let sv = CURRENT_SCHEMA_VERSION;
        let output_dir = TempDir::new().unwrap();
        let lines = vec![make_header(sv, 0, 0)];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import_with_base(
            Some(project_dir.path()),
            &input_path,
            None,
            false,
            false,
            base_dir.path(),
        );
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("0"), "should mention version 0: {err}");
    }

    // --- Hash Validation ---

    #[tokio::test(flavor = "multi_thread")]
    async fn test_hash_validation_valid_chain() {
        let (project_dir, base_dir) = make_project_dir();
        let sv = CURRENT_SCHEMA_VERSION;

        // R-04: a populated previous_hash requires a supersedes edge (the write
        // path co-populates both). Entry 2 supersedes entry 1 with
        // previous_hash = entry 1's content_hash → clean chain.
        let hash_a = compute_content_hash("Entry A", "Content A");
        let output_dir = TempDir::new().unwrap();
        let lines = vec![
            make_header(sv, 1, 2),
            make_counter_line("schema_version", sv),
            make_counter_line("next_entry_id", 3),
            make_entry_line(1, "Entry A", "Content A", ""),
            make_entry_line_full(2, "Entry B", "Content B", &hash_a, Some(1), 2, 0),
        ];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import_with_base(
            Some(project_dir.path()),
            &input_path,
            None,
            false,
            false,
            base_dir.path(),
        );
        assert!(result.is_ok(), "valid chain should pass: {result:?}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_hash_validation_broken_chain() {
        let (project_dir, base_dir) = make_project_dir();
        let sv = CURRENT_SCHEMA_VERSION;

        // R-04: entry 2 carries a real supersedes edge to entry 1 but its
        // previous_hash does NOT match entry 1's content_hash → ChainLinkMismatch
        // on the authoritative edge (the old existence check would have flagged
        // this only as "unknown hash"; the stronger edge-keyed check names the
        // predecessor and the mismatched value).
        let output_dir = TempDir::new().unwrap();
        let lines = vec![
            make_header(sv, 1, 2),
            make_counter_line("schema_version", sv),
            make_counter_line("next_entry_id", 3),
            make_entry_line(1, "Entry A", "Content A", ""),
            make_entry_line_full(2, "Entry B", "Content B", "nonexistent_hash", Some(1), 2, 0),
        ];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import_with_base(
            Some(project_dir.path()),
            &input_path,
            None,
            false,
            false,
            base_dir.path(),
        );
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("entry 2"), "should mention entry ID: {err}");
        assert!(
            err.contains("nonexistent_hash"),
            "should mention broken hash: {err}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_hash_validation_content_mismatch() {
        let (project_dir, base_dir) = make_project_dir();
        let sv = CURRENT_SCHEMA_VERSION;

        // Build entry with wrong content_hash
        let mut entry_json: serde_json::Value =
            serde_json::from_str(&make_entry_line(1, "Title", "Content", "")).unwrap();
        entry_json["content_hash"] = serde_json::Value::String("wrong_hash".to_string());

        let output_dir = TempDir::new().unwrap();
        let lines = vec![
            make_header(sv, 1, 1),
            make_counter_line("schema_version", sv),
            make_counter_line("next_entry_id", 2),
            entry_json.to_string(),
        ];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import_with_base(
            Some(project_dir.path()),
            &input_path,
            None,
            false,
            false,
            base_dir.path(),
        );
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("1"), "should mention entry ID: {err}");
        assert!(err.contains("mismatch"), "should mention mismatch: {err}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_hash_validation_empty_previous_hash() {
        let (project_dir, base_dir) = make_project_dir();
        let sv = CURRENT_SCHEMA_VERSION;

        let output_dir = TempDir::new().unwrap();
        let lines = vec![
            make_header(sv, 1, 1),
            make_counter_line("schema_version", sv),
            make_counter_line("next_entry_id", 2),
            make_entry_line(1, "Entry A", "Content A", ""),
        ];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import_with_base(
            Some(project_dir.path()),
            &input_path,
            None,
            false,
            false,
            base_dir.path(),
        );
        assert!(
            result.is_ok(),
            "empty previous_hash should pass: {result:?}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_hash_validation_empty_title_edge_case() {
        let (project_dir, base_dir) = make_project_dir();
        let sv = CURRENT_SCHEMA_VERSION;

        let output_dir = TempDir::new().unwrap();
        let lines = vec![
            make_header(sv, 1, 1),
            make_counter_line("schema_version", sv),
            make_counter_line("next_entry_id", 2),
            make_entry_line(1, "", "some text", ""),
        ];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import_with_base(
            Some(project_dir.path()),
            &input_path,
            None,
            false,
            false,
            base_dir.path(),
        );
        assert!(result.is_ok(), "empty title should pass: {result:?}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_hash_validation_empty_both() {
        let (project_dir, base_dir) = make_project_dir();
        let sv = CURRENT_SCHEMA_VERSION;

        let output_dir = TempDir::new().unwrap();
        let lines = vec![
            make_header(sv, 1, 1),
            make_counter_line("schema_version", sv),
            make_counter_line("next_entry_id", 2),
            make_entry_line(1, "", "", ""),
        ];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import_with_base(
            Some(project_dir.path()),
            &input_path,
            None,
            false,
            false,
            base_dir.path(),
        );
        assert!(
            result.is_ok(),
            "empty title+content should pass: {result:?}"
        );
    }

    // --- Malformed Input ---

    #[tokio::test(flavor = "multi_thread")]
    async fn test_malformed_jsonl_line_with_line_number() {
        let (project_dir, base_dir) = make_project_dir();
        let sv = CURRENT_SCHEMA_VERSION;

        let output_dir = TempDir::new().unwrap();
        let lines = vec![
            make_header(sv, 1, 3),
            make_counter_line("schema_version", sv),
            make_counter_line("next_entry_id", 4),
            make_entry_line(1, "A", "A", ""),
            "THIS IS NOT VALID JSON".to_string(), // line 5
            make_entry_line(3, "C", "C", ""),
        ];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import_with_base(
            Some(project_dir.path()),
            &input_path,
            None,
            false,
            false,
            base_dir.path(),
        );
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("line 5"), "should mention line 5: {err}");
    }

    /// Regression test for GH#554: run_import called from a plain sync context
    /// (no ambient tokio runtime — exercises the Handle::try_current() Err arm).
    ///
    /// Before the fix this panicked with:
    ///   "can call blocking only when running on the multi-threaded runtime"
    /// because the Err arm built a new_current_thread runtime, which does not
    /// support block_in_place used by embed_reconstruct.
    ///
    /// This is intentionally a plain #[test] — NOT #[tokio::test] — so that
    /// Handle::try_current() returns Err and the new runtime path is taken.
    #[test]
    fn test_run_import_no_ambient_runtime_does_not_panic() {
        let (project_dir, base_dir) = make_project_dir();
        let sv = CURRENT_SCHEMA_VERSION;

        let output_dir = TempDir::new().unwrap();
        let lines = vec![
            make_header(sv, 1, 1),
            make_counter_line("schema_version", sv),
            make_counter_line("next_entry_id", 2),
            make_entry_line(1, "Regression entry", "GH#554 regression content", ""),
        ];
        let input_path = write_jsonl(&output_dir, &lines);

        // Must not panic. The import succeeds (embedding may fail if model is
        // unavailable, but that is an Err return — not a panic).
        let result = run_import_with_base(
            Some(project_dir.path()),
            &input_path,
            None,
            false,
            false,
            base_dir.path(),
        );
        // The important invariant: no panic occurred. Accept Ok or Err.
        // If the ONNX model is present this will be Ok; in CI without the model
        // it returns Err from embed_reconstruct, which is acceptable.
        match &result {
            Ok(_) => {}
            Err(e) => {
                let msg = e.to_string();
                // Any error from embed_reconstruct is expected when model is absent;
                // a panic would have been caught by the test harness before this point.
                assert!(
                    msg.contains("ONNX")
                        || msg.contains("onnx")
                        || msg.contains("model")
                        || msg.contains("embed")
                        || msg.contains("No such file"),
                    "unexpected error (not a model-missing error): {msg}"
                );
            }
        }
    }

    #[test]
    fn test_empty_file_errors() {
        let project_dir = TempDir::new().expect("create project temp dir");
        let base_dir = TempDir::new().expect("create base temp dir");
        let _ = project::ensure_data_directory(Some(project_dir.path()), Some(base_dir.path()))
            .unwrap();
        let output_dir = TempDir::new().unwrap();
        let path = output_dir.path().join("empty.jsonl");
        File::create(&path).unwrap();

        let result = run_import_with_base(
            Some(project_dir.path()),
            &path,
            None,
            false,
            false,
            base_dir.path(),
        );
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("empty") || err.contains("header"),
            "should mention empty/header: {err}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_header_only_file() {
        let (project_dir, base_dir) = make_project_dir();
        let sv = CURRENT_SCHEMA_VERSION;

        let output_dir = TempDir::new().unwrap();
        let lines = vec![make_header(sv, 1, 0)];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import_with_base(
            Some(project_dir.path()),
            &input_path,
            None,
            false,
            false,
            base_dir.path(),
        );
        assert!(
            result.is_ok(),
            "header-only should be valid empty import: {result:?}"
        );
    }

    // --- SQL Injection Prevention ---

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sql_injection_in_title() {
        let (project_dir, base_dir) = make_project_dir();
        let sv = CURRENT_SCHEMA_VERSION;
        let db_path =
            project::ensure_data_directory(Some(project_dir.path()), Some(base_dir.path()))
                .unwrap()
                .db_path;

        let malicious_title = "'; DROP TABLE entries; --";
        let output_dir = TempDir::new().unwrap();
        let lines = vec![
            make_header(sv, 1, 1),
            make_counter_line("schema_version", sv),
            make_counter_line("next_entry_id", 2),
            make_entry_line(1, malicious_title, "safe content", ""),
        ];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import_with_base(
            Some(project_dir.path()),
            &input_path,
            None,
            true, // skip hash -- hash won't match the SQL injection string
            false,
            base_dir.path(),
        );
        assert!(
            result.is_ok(),
            "SQL injection in title should be safe: {result:?}"
        );

        // Reopen a fresh pool to verify import results.
        let verify_store = open_test_store_at(&db_path).await;
        let pool = verify_store.write_pool_server();
        let title: String =
            sqlx::query_scalar::<_, String>("SELECT title FROM entries WHERE id = 1")
                .fetch_one(pool)
                .await
                .unwrap();
        assert_eq!(title, malicious_title);

        // Verify entries table still exists
        let count: i64 = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM entries")
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sql_injection_in_content() {
        let (project_dir, base_dir) = make_project_dir();
        let sv = CURRENT_SCHEMA_VERSION;
        let db_path =
            project::ensure_data_directory(Some(project_dir.path()), Some(base_dir.path()))
                .unwrap()
                .db_path;

        let malicious = "Robert'); DROP TABLE entries;--";
        let output_dir = TempDir::new().unwrap();
        let lines = vec![
            make_header(sv, 1, 1),
            make_counter_line("schema_version", sv),
            make_counter_line("next_entry_id", 2),
            make_entry_line(1, "Safe title", malicious, ""),
        ];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import_with_base(
            Some(project_dir.path()),
            &input_path,
            None,
            true,
            false,
            base_dir.path(),
        );
        assert!(
            result.is_ok(),
            "SQL injection in content should be safe: {result:?}"
        );

        // Reopen a fresh pool to verify import results.
        let verify_store = open_test_store_at(&db_path).await;
        let pool = verify_store.write_pool_server();
        let content: String =
            sqlx::query_scalar::<_, String>("SELECT content FROM entries WHERE id = 1")
                .fetch_one(pool)
                .await
                .unwrap();
        assert_eq!(content, malicious);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_duplicate_entry_ids() {
        let (project_dir, base_dir) = make_project_dir();
        let sv = CURRENT_SCHEMA_VERSION;

        let output_dir = TempDir::new().unwrap();
        let lines = vec![
            make_header(sv, 1, 2),
            make_counter_line("schema_version", sv),
            make_counter_line("next_entry_id", 3),
            make_entry_line(1, "First", "First content", ""),
            make_entry_line(1, "Duplicate", "Duplicate content", ""),
        ];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import_with_base(
            Some(project_dir.path()),
            &input_path,
            None,
            true,
            false,
            base_dir.path(),
        );
        assert!(result.is_err(), "duplicate PK should fail");
    }

    // --- GH#633 Regression: audit counter desync after import ---

    /// Build an audit_log JSON line for export/import.
    fn make_audit_log_line(event_id: i64, operation: &str, detail: &str) -> String {
        serde_json::json!({
            "_table": "audit_log",
            "event_id": event_id,
            "timestamp": 1700000000i64 + event_id,
            "session_id": "pre-export-session",
            "agent_id": "system",
            "operation": operation,
            "target_ids": "[]",
            "outcome": 0,
            "detail": detail
        })
        .to_string()
    }

    /// Regression test for GH#633: after export/import round-trip,
    /// log_audit_event() must succeed multiple times without UNIQUE constraint
    /// collisions. The bug was that record_provenance() wrote event_id via
    /// MAX(event_id)+1, desynchronizing the next_audit_id counter permanently.
    ///
    /// This test imports audit_log rows with event_ids 1..=3 and a
    /// next_audit_id counter set to 3, then calls log_audit_event() twice.
    /// Both must succeed with monotonically increasing IDs > all imported rows
    /// AND > the provenance record written during import.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_gh633_log_audit_event_succeeds_after_import() {
        let (project_dir, base_dir) = make_project_dir();
        let sv = CURRENT_SCHEMA_VERSION;
        let db_path =
            project::ensure_data_directory(Some(project_dir.path()), Some(base_dir.path()))
                .unwrap()
                .db_path;

        let output_dir = TempDir::new().unwrap();
        let lines = vec![
            make_header(sv, 1, 1),
            // Counters
            make_counter_line("schema_version", sv),
            make_counter_line("next_entry_id", 2),
            make_counter_line("next_audit_id", 3),
            // One entry
            make_entry_line(1, "Test entry", "Content for GH#633", ""),
            // Three audit_log rows (event_ids 1, 2, 3)
            make_audit_log_line(1, "context_store", "stored entry 1"),
            make_audit_log_line(2, "context_search", "searched for test"),
            make_audit_log_line(3, "context_store", "stored entry 2"),
        ];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import_with_base(
            Some(project_dir.path()),
            &input_path,
            None,
            false,
            false,
            base_dir.path(),
        );
        assert!(result.is_ok(), "import must succeed: {result:?}");

        // Re-open the store and call log_audit_event() twice.
        let store = open_test_store_at(&db_path).await;

        let event1 = AuditEvent {
            session_id: "post-import-session".to_string(),
            agent_id: "test-agent".to_string(),
            operation: "context_search".to_string(),
            detail: "GH#633 regression call 1".to_string(),
            ..AuditEvent::default()
        };
        let id1 = store
            .log_audit_event(event1)
            .await
            .expect("first log_audit_event after import must succeed (GH#633)");

        let event2 = AuditEvent {
            session_id: "post-import-session".to_string(),
            agent_id: "test-agent".to_string(),
            operation: "context_search".to_string(),
            detail: "GH#633 regression call 2".to_string(),
            ..AuditEvent::default()
        };
        let id2 = store
            .log_audit_event(event2)
            .await
            .expect("second log_audit_event after import must succeed (GH#633)");

        // Monotonically increasing
        assert!(
            id2 > id1,
            "event_ids must be monotonically increasing: id1={id1}, id2={id2}"
        );

        // Both IDs must be greater than the max imported audit_log event_id (3)
        // AND greater than the provenance record written during import.
        // The provenance record uses event_id 4 (counter was 3, incremented to 4).
        // So the first post-import call should get 5, second 6.
        assert!(
            id1 > 3,
            "post-import event_id must exceed all imported rows: id1={id1}"
        );

        // Verify the provenance record exists and has the expected event_id.
        let provenance = store
            .read_audit_event(4)
            .await
            .expect("read provenance event must succeed")
            .expect("provenance event must exist at event_id 4");
        assert_eq!(
            provenance.operation, "import",
            "provenance record must have operation='import'"
        );
    }

    /// Complementary regression test for GH#633: verify that the provenance
    /// record written during import uses the counter (not MAX+1), by checking
    /// the counter value after import matches the provenance event_id.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_gh633_provenance_uses_counter_not_max_plus_one() {
        let (project_dir, base_dir) = make_project_dir();
        let sv = CURRENT_SCHEMA_VERSION;
        let db_path =
            project::ensure_data_directory(Some(project_dir.path()), Some(base_dir.path()))
                .unwrap()
                .db_path;

        // Import with next_audit_id=5 but only 2 audit_log rows (event_ids 1, 2).
        // If record_provenance used MAX+1, it would write event_id 3.
        // If it uses the counter, it increments 5→6 and writes event_id 6.
        let output_dir = TempDir::new().unwrap();
        let lines = vec![
            make_header(sv, 1, 1),
            make_counter_line("schema_version", sv),
            make_counter_line("next_entry_id", 2),
            make_counter_line("next_audit_id", 5),
            make_entry_line(1, "Test entry", "Content", ""),
            make_audit_log_line(1, "context_store", "row 1"),
            make_audit_log_line(2, "context_store", "row 2"),
        ];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import_with_base(
            Some(project_dir.path()),
            &input_path,
            None,
            false,
            false,
            base_dir.path(),
        );
        assert!(result.is_ok(), "import must succeed: {result:?}");

        // Re-open and verify counter-based allocation.
        let store = open_test_store_at(&db_path).await;

        // The provenance record should be at event_id 6 (counter 5 → 6).
        let provenance = store
            .read_audit_event(6)
            .await
            .expect("read must succeed")
            .expect("provenance at event_id 6 (counter-based, not MAX+1=3)");
        assert_eq!(provenance.operation, "import");

        // event_id 3 should NOT have the provenance record (that's the MAX+1 bug path).
        let at_three = store.read_audit_event(3).await.expect("read must succeed");
        assert!(
            at_three.is_none(),
            "event_id 3 must be empty — provenance must not use MAX+1"
        );

        // Subsequent log_audit_event must succeed at event_id 7.
        let event = AuditEvent {
            operation: "context_search".to_string(),
            detail: "post-import check".to_string(),
            ..AuditEvent::default()
        };
        let id = store
            .log_audit_event(event)
            .await
            .expect("log_audit_event must succeed after import");
        assert_eq!(id, 7, "next event_id must be 7 (counter 6 → 7)");
    }

    // --- nxs-012: format_version validation (ADR-002) ---

    #[test]
    fn test_format_version_0_rejected() {
        // R-04, AC-07: version 0 must be rejected with supported range message.
        let json = make_header(CURRENT_SCHEMA_VERSION, 0, 0);
        let header = parse_header(&json).unwrap();
        // Simulate the validation logic
        let result: Result<(), Box<dyn std::error::Error>> = match header.format_version {
            1 | 2 => Ok(()),
            v => Err(format!(
                "unsupported format_version: {v}. This binary supports format_version 1 and 2."
            )
            .into()),
        };
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("0"), "should mention version 0: {err}");
        assert!(
            err.contains("1 and 2"),
            "should mention supported range: {err}"
        );
    }

    #[test]
    fn test_format_version_1_accepted() {
        // R-04, AC-05: version 1 must be accepted.
        let json = make_header(CURRENT_SCHEMA_VERSION, 1, 0);
        let header = parse_header(&json).unwrap();
        let result: Result<(), Box<dyn std::error::Error>> = match header.format_version {
            1 | 2 => Ok(()),
            v => Err(format!(
                "unsupported format_version: {v}. This binary supports format_version 1 and 2."
            )
            .into()),
        };
        assert!(result.is_ok(), "format_version 1 should be accepted");
    }

    #[test]
    fn test_format_version_2_accepted() {
        // R-04, AC-06: version 2 must be accepted.
        let json = make_header(CURRENT_SCHEMA_VERSION, 2, 0);
        let header = parse_header(&json).unwrap();
        let result: Result<(), Box<dyn std::error::Error>> = match header.format_version {
            1 | 2 => Ok(()),
            v => Err(format!(
                "unsupported format_version: {v}. This binary supports format_version 1 and 2."
            )
            .into()),
        };
        assert!(result.is_ok(), "format_version 2 should be accepted");
    }

    #[test]
    fn test_format_version_3_rejected() {
        // R-04, AC-07: version 3 must be rejected with supported range message.
        let json = make_header(CURRENT_SCHEMA_VERSION, 3, 0);
        let header = parse_header(&json).unwrap();
        let result: Result<(), Box<dyn std::error::Error>> = match header.format_version {
            1 | 2 => Ok(()),
            v => Err(format!(
                "unsupported format_version: {v}. This binary supports format_version 1 and 2."
            )
            .into()),
        };
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("3"), "should mention version 3: {err}");
        assert!(
            err.contains("1 and 2"),
            "should mention supported range: {err}"
        );
    }

    #[test]
    fn test_format_version_999_rejected() {
        // R-04, AC-07: boundary value test.
        let json = make_header(CURRENT_SCHEMA_VERSION, 999, 0);
        let header = parse_header(&json).unwrap();
        let result: Result<(), Box<dyn std::error::Error>> = match header.format_version {
            1 | 2 => Ok(()),
            v => Err(format!(
                "unsupported format_version: {v}. This binary supports format_version 1 and 2."
            )
            .into()),
        };
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("999"), "should mention version 999: {err}");
    }

    // --- nxs-012: ImportCounts default ---

    #[test]
    fn test_import_counts_default_includes_new_fields() {
        // R-13: default ImportCounts should have 0 for all new fields.
        let counts = ImportCounts::default();
        assert_eq!(counts.graph_edges, 0);
        assert_eq!(counts.observations, 0);
        assert_eq!(counts.cycle_events, 0);
    }

    // --- nxs-012: format_version 2 import (integration) ---

    /// Build a graph_edges JSON line.
    fn make_graph_edge_line(
        source_id: i64,
        target_id: i64,
        relation_type: &str,
        weight: f64,
    ) -> String {
        serde_json::json!({
            "_table": "graph_edges",
            "source_id": source_id,
            "target_id": target_id,
            "relation_type": relation_type,
            "weight": weight,
            "created_at": 1700000000i64,
            "created_by": "test-agent",
            "source": "runtime",
            "bootstrap_only": 0,
            "metadata": null
        })
        .to_string()
    }

    /// Build an observations JSON line.
    fn make_observation_line(id: i64, session_id: &str, hook: &str) -> String {
        serde_json::json!({
            "_table": "observations",
            "id": id,
            "session_id": session_id,
            "ts_millis": 1700000000i64,
            "hook": hook,
            "tool": null,
            "input": null,
            "response_size": null,
            "response_snippet": null,
            "topic_signal": null,
            "phase": null
        })
        .to_string()
    }

    /// Build a cycle_events JSON line.
    fn make_cycle_event_line(id: i64, cycle_id: &str, seq: i64, event_type: &str) -> String {
        serde_json::json!({
            "_table": "cycle_events",
            "id": id,
            "cycle_id": cycle_id,
            "seq": seq,
            "event_type": event_type,
            "phase": null,
            "outcome": null,
            "next_phase": null,
            "timestamp": 1700000000i64,
            "goal": null
        })
        .to_string()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_format_version_2_import_succeeds() {
        // AC-06: v2 file with all 11 table types imports successfully.
        let (project_dir, base_dir) = make_project_dir();
        let sv = CURRENT_SCHEMA_VERSION;
        let db_path =
            project::ensure_data_directory(Some(project_dir.path()), Some(base_dir.path()))
                .unwrap()
                .db_path;

        // Both entries are genesis (previous_hash = ""): this test asserts a
        // derived graph_edges count, so it must not introduce a supersedes edge
        // (which the store materializes as a Supersedes graph_edge). Chain
        // verification of populated links is covered by the dedicated chain tests.
        let output_dir = TempDir::new().unwrap();
        let lines = vec![
            make_header(sv, 2, 2),
            make_counter_line("schema_version", sv),
            make_counter_line("next_entry_id", 3),
            make_counter_line("next_audit_id", 2),
            make_entry_line(1, "Entry A", "Content A", ""),
            make_entry_line(2, "Entry B", "Content B", ""),
            serde_json::json!({"_table": "entry_tags", "entry_id": 1, "tag": "test"}).to_string(),
            serde_json::json!({"_table": "co_access", "entry_id_a": 1, "entry_id_b": 2, "count": 1, "last_updated": 1700000000i64}).to_string(),
            serde_json::json!({"_table": "feature_entries", "feature_id": "nxs-012", "entry_id": 1}).to_string(),
            serde_json::json!({"_table": "outcome_index", "feature_cycle": "nxs-012", "entry_id": 1}).to_string(),
            serde_json::json!({"_table": "agent_registry", "agent_id": "system", "trust_level": 3, "capabilities": "[]", "allowed_topics": null, "allowed_categories": null, "enrolled_at": 1700000000i64, "last_seen_at": 1700000000i64, "active": 1}).to_string(),
            make_audit_log_line(1, "context_store", "stored entry 1"),
            make_graph_edge_line(1, 2, "Supports", 0.85),
            make_observation_line(1, "sess-1", "on_tool"),
            make_cycle_event_line(1, "nxs-012", 1, "cycle_start"),
        ];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import_with_base(
            Some(project_dir.path()),
            &input_path,
            None,
            false,
            false,
            base_dir.path(),
        );
        assert!(result.is_ok(), "v2 import should succeed: {result:?}");

        // Verify all tables have rows.
        let store = open_test_store_at(&db_path).await;
        let pool = store.write_pool_server();

        let ge_count: i64 = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM graph_edges")
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(ge_count, 1, "graph_edges should have 1 row");

        let obs_count: i64 = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM observations")
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(obs_count, 1, "observations should have 1 row");

        let ce_count: i64 = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM cycle_events")
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(ce_count, 1, "cycle_events should have 1 row");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_v1_import_zero_new_table_counts() {
        // AC-05: v1 file imports cleanly, new tables have 0 rows.
        let (project_dir, base_dir) = make_project_dir();
        let sv = CURRENT_SCHEMA_VERSION;
        let db_path =
            project::ensure_data_directory(Some(project_dir.path()), Some(base_dir.path()))
                .unwrap()
                .db_path;

        let output_dir = TempDir::new().unwrap();
        let lines = vec![
            make_header(sv, 1, 1),
            make_counter_line("schema_version", sv),
            make_counter_line("next_entry_id", 2),
            make_entry_line(1, "Entry A", "Content A", ""),
        ];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import_with_base(
            Some(project_dir.path()),
            &input_path,
            None,
            false,
            false,
            base_dir.path(),
        );
        assert!(result.is_ok(), "v1 import should succeed: {result:?}");

        let store = open_test_store_at(&db_path).await;
        let pool = store.write_pool_server();

        let ge: i64 = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM graph_edges")
            .fetch_one(pool)
            .await
            .unwrap();
        let obs: i64 = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM observations")
            .fetch_one(pool)
            .await
            .unwrap();
        let ce: i64 = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM cycle_events")
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(ge, 0, "graph_edges should be 0 for v1 import");
        assert_eq!(obs, 0, "observations should be 0 for v1 import");
        assert_eq!(ce, 0, "cycle_events should be 0 for v1 import");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_drop_all_data_clears_new_tables() {
        // AC-13: --force import clears all new tables.
        let (project_dir, base_dir) = make_project_dir();
        let sv = CURRENT_SCHEMA_VERSION;
        let db_path =
            project::ensure_data_directory(Some(project_dir.path()), Some(base_dir.path()))
                .unwrap()
                .db_path;

        // First import: populate new tables via v2 file.
        let output_dir = TempDir::new().unwrap();
        let lines = vec![
            make_header(sv, 2, 1),
            make_counter_line("schema_version", sv),
            make_counter_line("next_entry_id", 2),
            make_entry_line(1, "Entry A", "Content A", ""),
            make_graph_edge_line(1, 1, "SelfRef", 1.0),
            make_observation_line(1, "sess-1", "on_tool"),
            make_cycle_event_line(1, "nxs-012", 1, "cycle_start"),
        ];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import_with_base(
            Some(project_dir.path()),
            &input_path,
            None,
            false,
            false,
            base_dir.path(),
        );
        assert!(result.is_ok(), "first import should succeed: {result:?}");

        // Second import with --force and empty data (counters only).
        // next_audit_id must be set high enough to avoid colliding with
        // audit_log entries from the first import's record_provenance
        // (audit_log is append-only and not cleared by drop_all_data).
        let output_dir2 = TempDir::new().unwrap();
        let lines2 = vec![
            make_header(sv, 2, 0),
            make_counter_line("schema_version", sv),
            make_counter_line("next_entry_id", 1),
            make_counter_line("next_audit_id", 100),
        ];
        let input_path2 = write_jsonl(&output_dir2, &lines2);

        let result2 = run_import_with_base(
            Some(project_dir.path()),
            &input_path2,
            None,
            true,
            true, // force
            base_dir.path(),
        );
        assert!(result2.is_ok(), "force import should succeed: {result2:?}");

        // Verify all new tables are empty after force import.
        let store = open_test_store_at(&db_path).await;
        let pool = store.write_pool_server();

        let ge: i64 = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM graph_edges")
            .fetch_one(pool)
            .await
            .unwrap();
        let obs: i64 = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM observations")
            .fetch_one(pool)
            .await
            .unwrap();
        let ce: i64 = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM cycle_events")
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(ge, 0, "graph_edges should be 0 after force import");
        assert_eq!(obs, 0, "observations should be 0 after force import");
        assert_eq!(ce, 0, "cycle_events should be 0 after force import");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_record_provenance_includes_new_counts() {
        // FR-17: provenance detail string includes 3 new table counts.
        let (project_dir, base_dir) = make_project_dir();
        let sv = CURRENT_SCHEMA_VERSION;
        let db_path =
            project::ensure_data_directory(Some(project_dir.path()), Some(base_dir.path()))
                .unwrap()
                .db_path;

        let output_dir = TempDir::new().unwrap();
        let lines = vec![
            make_header(sv, 2, 1),
            make_counter_line("schema_version", sv),
            make_counter_line("next_entry_id", 2),
            make_counter_line("next_audit_id", 1),
            make_entry_line(1, "Entry A", "Content A", ""),
            make_graph_edge_line(1, 1, "SelfRef", 1.0),
            make_graph_edge_line(1, 1, "Related", 0.5),
            make_observation_line(1, "sess-1", "on_tool"),
            make_observation_line(2, "sess-1", "on_result"),
            make_observation_line(3, "sess-2", "on_tool"),
            make_cycle_event_line(1, "nxs-012", 1, "cycle_start"),
        ];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import_with_base(
            Some(project_dir.path()),
            &input_path,
            None,
            false,
            false,
            base_dir.path(),
        );
        assert!(result.is_ok(), "import should succeed: {result:?}");

        // Re-open store and check the audit_log provenance record.
        let store = open_test_store_at(&db_path).await;
        let pool = store.write_pool_server();

        // Find the import provenance record in audit_log.
        let detail: String = sqlx::query_scalar::<_, String>(
            "SELECT detail FROM audit_log WHERE operation = 'import' ORDER BY event_id DESC LIMIT 1",
        )
        .fetch_one(pool)
        .await
        .unwrap();

        assert!(
            detail.contains("2 graph_edges"),
            "provenance should mention 2 graph_edges: {detail}"
        );
        assert!(
            detail.contains("3 observations"),
            "provenance should mention 3 observations: {detail}"
        );
        assert!(
            detail.contains("1 cycle_events"),
            "provenance should mention 1 cycle_events: {detail}"
        );
    }

    // --- nxs-014: single-oracle chain verification via the import path ---
    //
    // `validate_hashes` is now a thin adapter over
    // `unimatrix_store::chain_verify::verify_entries` (ADR-001). The tests below
    // prove BOTH oracle halves run on the import path (content-hash AND the
    // supersedes-keyed chain link), that the load reads ALL statuses from the
    // in-flight transaction (R-02), atomic ROLLBACK on tamper (R-05), and
    // lossless round-trip of `previous_hash`/`version` (R-07).

    /// AND-half 1 (AC-04, R-04): every content_hash is correct, but one
    /// successor's `previous_hash` does not match its `supersedes` predecessor's
    /// `content_hash`. The old existence check ("references *some* known hash")
    /// would have accepted a value that happened to collide with any known hash;
    /// the stronger edge-keyed check rejects it. Import must fail.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_import_rejects_broken_link_with_good_content_hash() {
        let (project_dir, base_dir) = make_project_dir();
        let sv = CURRENT_SCHEMA_VERSION;

        let hash_a = compute_content_hash("Entry A", "Content A");
        // A THIRD entry's hash — a "known hash" in the corpus that is NOT entry
        // 2's true predecessor. The old check would pass entry 2 pointing here.
        let hash_c = compute_content_hash("Entry C", "Content C");
        let output_dir = TempDir::new().unwrap();
        let lines = vec![
            make_header(sv, 1, 3),
            make_counter_line("schema_version", sv),
            make_counter_line("next_entry_id", 4),
            make_entry_line(1, "Entry A", "Content A", ""),
            // entry 2 supersedes entry 1 but links to entry 3's hash (wrong edge).
            make_entry_line_full(2, "Entry B", "Content B", &hash_c, Some(1), 2, 0),
            make_entry_line(3, "Entry C", "Content C", ""),
        ];
        // Content hashes are all internally correct — only the link is wrong.
        let _ = hash_a;
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import_with_base(
            Some(project_dir.path()),
            &input_path,
            None,
            false,
            false,
            base_dir.path(),
        );
        assert!(
            result.is_err(),
            "broken supersedes link must fail even with a colliding known hash: {result:?}"
        );
        let err = result.unwrap_err().to_string();
        assert!(err.contains("entry 2"), "should name entry 2: {err}");
        assert!(
            err.contains("chain link mismatch"),
            "should be a chain-link violation: {err}"
        );
    }

    /// AND-half 2 (AC-04): chain links are internally consistent, but one
    /// entry's `content` was mutated so its stored `content_hash` is stale.
    /// Import must fail (content-hash recompute half runs). Together with the
    /// test above this proves the single oracle runs BOTH halves on import.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_import_rejects_mutated_content_with_good_link() {
        let (project_dir, base_dir) = make_project_dir();
        let sv = CURRENT_SCHEMA_VERSION;

        // Entry 1 has a valid content_hash for its title/content, then we mutate
        // ONLY the content field so the stored hash is stale.
        let mut entry_json: serde_json::Value =
            serde_json::from_str(&make_entry_line(1, "Title", "Content", "")).unwrap();
        entry_json["content"] = serde_json::Value::String("mutated content".to_string());

        let output_dir = TempDir::new().unwrap();
        let lines = vec![
            make_header(sv, 1, 1),
            make_counter_line("schema_version", sv),
            make_counter_line("next_entry_id", 2),
            entry_json.to_string(),
        ];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import_with_base(
            Some(project_dir.path()),
            &input_path,
            None,
            false,
            false,
            base_dir.path(),
        );
        assert!(result.is_err(), "mutated content must fail import");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("entry 1"), "should name entry 1: {err}");
        assert!(
            err.contains("content hash mismatch"),
            "should be a content-hash violation: {err}"
        );
    }

    /// R-02 (import half): predecessors are `Deprecated` (superseded); the
    /// successor is `Active` with a populated `previous_hash`. Import must
    /// SUCCEED — proving the import loader reads ALL statuses from the in-flight
    /// `BEGIN IMMEDIATE` connection. If it filtered to `Active`, entry 1 would be
    /// absent from the corpus and entry 2 would raise `MissingPredecessor`.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_import_deprecated_predecessor_verifies_clean() {
        let (project_dir, base_dir) = make_project_dir();
        let sv = CURRENT_SCHEMA_VERSION;
        let db_path =
            project::ensure_data_directory(Some(project_dir.path()), Some(base_dir.path()))
                .unwrap()
                .db_path;

        let hash_a = compute_content_hash("Entry A", "Content A");
        let output_dir = TempDir::new().unwrap();
        let lines = vec![
            make_header(sv, 1, 2),
            make_counter_line("schema_version", sv),
            make_counter_line("next_entry_id", 3),
            // Predecessor is Deprecated (status = 1), genesis link.
            make_entry_line_full(1, "Entry A", "Content A", "", None, 1, 1),
            // Active successor supersedes the Deprecated predecessor.
            make_entry_line_full(2, "Entry B", "Content B", &hash_a, Some(1), 2, 0),
        ];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import_with_base(
            Some(project_dir.path()),
            &input_path,
            None,
            false,
            false,
            base_dir.path(),
        );
        assert!(
            result.is_ok(),
            "Deprecated predecessor must load and verify clean: {result:?}"
        );

        // Sanity: both rows are present with the predecessor still Deprecated.
        let store = open_test_store_at(&db_path).await;
        let pool = store.write_pool_server();
        let status_1: i64 = sqlx::query_scalar::<_, i64>("SELECT status FROM entries WHERE id = 1")
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(status_1, 1, "predecessor must remain Deprecated");
    }

    /// R-05: a tampered corpus must ROLLBACK, proven by DB state — the
    /// post-failure row count equals the pre-import count (fresh DB → 0), so NO
    /// rows from the failed import remain. `ingest_rows` inserts the rows inside
    /// the transaction BEFORE `validate_hashes` runs, so a non-empty count would
    /// mean the ROLLBACK branch was skipped.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_import_tampered_corpus_rollback_no_rows() {
        let (project_dir, base_dir) = make_project_dir();
        let sv = CURRENT_SCHEMA_VERSION;
        let db_path =
            project::ensure_data_directory(Some(project_dir.path()), Some(base_dir.path()))
                .unwrap()
                .db_path;

        // Broken link: entry 2 supersedes entry 1 but previous_hash is wrong.
        let output_dir = TempDir::new().unwrap();
        let lines = vec![
            make_header(sv, 1, 2),
            make_counter_line("schema_version", sv),
            make_counter_line("next_entry_id", 3),
            make_entry_line(1, "Entry A", "Content A", ""),
            make_entry_line_full(2, "Entry B", "Content B", "tampered_hash", Some(1), 2, 0),
        ];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import_with_base(
            Some(project_dir.path()),
            &input_path,
            None,
            false,
            false,
            base_dir.path(),
        );
        assert!(result.is_err(), "tampered corpus must fail import");

        // ROLLBACK proven by post-failure DB state.
        let store = open_test_store_at(&db_path).await;
        let pool = store.write_pool_server();
        let count: i64 = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM entries")
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(
            count, 0,
            "no rows from the failed import may remain after ROLLBACK"
        );
    }

    /// R-05 positive control: a clean corrected corpus COMMITs, and the rows are
    /// present with intact `previous_hash`/`version` (read back from the DB).
    #[tokio::test(flavor = "multi_thread")]
    async fn test_import_clean_corrected_corpus_commits() {
        let (project_dir, base_dir) = make_project_dir();
        let sv = CURRENT_SCHEMA_VERSION;
        let db_path =
            project::ensure_data_directory(Some(project_dir.path()), Some(base_dir.path()))
                .unwrap()
                .db_path;

        let hash_a = compute_content_hash("Entry A", "Content A");
        let output_dir = TempDir::new().unwrap();
        let lines = vec![
            make_header(sv, 1, 2),
            make_counter_line("schema_version", sv),
            make_counter_line("next_entry_id", 3),
            make_entry_line_full(1, "Entry A", "Content A", "", None, 1, 1),
            make_entry_line_full(2, "Entry B", "Content B", &hash_a, Some(1), 2, 0),
        ];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import_with_base(
            Some(project_dir.path()),
            &input_path,
            None,
            false,
            false,
            base_dir.path(),
        );
        assert!(
            result.is_ok(),
            "clean corrected corpus must commit: {result:?}"
        );

        let store = open_test_store_at(&db_path).await;
        let pool = store.write_pool_server();
        let (prev, version): (String, i64) = sqlx::query_as::<_, (String, i64)>(
            "SELECT previous_hash, version FROM entries WHERE id = 2",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(prev, hash_a, "previous_hash must persist intact");
        assert_eq!(version, 2, "version must persist intact");
    }

    /// R-07 (AC-05): a corpus mixing a legacy entry (`previous_hash == ""`) and a
    /// multi-hop corrected chain round-trips losslessly. Every `previous_hash`
    /// and `version` is byte-identical after import (empty stays empty, populated
    /// stays populated, version not reset), and import-time chain-verify PASSES.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_roundtrip_multihop_including_legacy_byte_identical() {
        let (project_dir, base_dir) = make_project_dir();
        let sv = CURRENT_SCHEMA_VERSION;
        let db_path =
            project::ensure_data_directory(Some(project_dir.path()), Some(base_dir.path()))
                .unwrap()
                .db_path;

        let hash_1 = compute_content_hash("Entry 1", "Content 1");
        let hash_2 = compute_content_hash("Entry 2", "Content 2");
        let output_dir = TempDir::new().unwrap();
        let lines = vec![
            make_header(sv, 1, 4),
            make_counter_line("schema_version", sv),
            make_counter_line("next_entry_id", 5),
            // Legacy genesis (empty previous_hash).
            make_entry_line(10, "Legacy", "Legacy content", ""),
            // Multi-hop chain 1 -> 2 -> 3.
            make_entry_line_full(1, "Entry 1", "Content 1", "", None, 1, 1),
            make_entry_line_full(2, "Entry 2", "Content 2", &hash_1, Some(1), 2, 1),
            make_entry_line_full(3, "Entry 3", "Content 3", &hash_2, Some(2), 3, 0),
        ];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import_with_base(
            Some(project_dir.path()),
            &input_path,
            None,
            false,
            false,
            base_dir.path(),
        );
        assert!(
            result.is_ok(),
            "mixed legacy + multi-hop chain must verify clean: {result:?}"
        );

        let store = open_test_store_at(&db_path).await;
        let pool = store.write_pool_server();
        let read_prev_ver = |id: i64| {
            let pool = pool.clone();
            async move {
                sqlx::query_as::<_, (String, i64)>(
                    "SELECT previous_hash, version FROM entries WHERE id = ?",
                )
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap()
            }
        };

        // Legacy: empty stays empty, version stays 1 (not coerced to NULL).
        let (p10, v10) = read_prev_ver(10).await;
        assert_eq!(p10, "", "legacy previous_hash must stay empty");
        assert_eq!(v10, 1);
        // Chain: populated stays byte-identical, versions preserved.
        let (p1, v1) = read_prev_ver(1).await;
        assert_eq!(p1, "");
        assert_eq!(v1, 1);
        let (p2, v2) = read_prev_ver(2).await;
        assert_eq!(p2, hash_1, "hop-2 previous_hash must be byte-identical");
        assert_eq!(v2, 2);
        let (p3, v3) = read_prev_ver(3).await;
        assert_eq!(p3, hash_2, "hop-3 previous_hash must be byte-identical");
        assert_eq!(v3, 3);
    }

    /// R-07 boundary: a large `version` (near the `u32` range) survives the
    /// `u32` <-> `i64` bind round-trip without truncation.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_roundtrip_version_large_value_survives_u32_i64_bind() {
        let (project_dir, base_dir) = make_project_dir();
        let sv = CURRENT_SCHEMA_VERSION;
        let db_path =
            project::ensure_data_directory(Some(project_dir.path()), Some(base_dir.path()))
                .unwrap()
                .db_path;

        let big_version = u32::MAX as i64; // 4_294_967_295
        let output_dir = TempDir::new().unwrap();
        let lines = vec![
            make_header(sv, 1, 1),
            make_counter_line("schema_version", sv),
            make_counter_line("next_entry_id", 2),
            make_entry_line_full(1, "Big", "Big version content", "", None, big_version, 0),
        ];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import_with_base(
            Some(project_dir.path()),
            &input_path,
            None,
            false,
            false,
            base_dir.path(),
        );
        assert!(
            result.is_ok(),
            "large version import must succeed: {result:?}"
        );

        let store = open_test_store_at(&db_path).await;
        let pool = store.write_pool_server();
        let version: i64 = sqlx::query_scalar::<_, i64>("SELECT version FROM entries WHERE id = 1")
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(
            version, big_version,
            "large version must survive round-trip"
        );
    }

    /// R-07 paired negative (AC-05): after a clean round-trip, mutate a
    /// *superseded* (Deprecated) predecessor's content and re-run the import
    /// oracle directly on the committed DB. Verify must be non-clean AND name the
    /// offending `entry_id` — proving the legacy skip is scoped to empty links,
    /// not a blanket pass, and that the Deprecated predecessor IS checked.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_roundtrip_then_mutation_fails_loud() {
        let (project_dir, base_dir) = make_project_dir();
        let sv = CURRENT_SCHEMA_VERSION;
        let db_path =
            project::ensure_data_directory(Some(project_dir.path()), Some(base_dir.path()))
                .unwrap()
                .db_path;

        let hash_a = compute_content_hash("Entry A", "Content A");
        let output_dir = TempDir::new().unwrap();
        let lines = vec![
            make_header(sv, 1, 2),
            make_counter_line("schema_version", sv),
            make_counter_line("next_entry_id", 3),
            make_entry_line_full(1, "Entry A", "Content A", "", None, 1, 1),
            make_entry_line_full(2, "Entry B", "Content B", &hash_a, Some(1), 2, 0),
        ];
        let input_path = write_jsonl(&output_dir, &lines);
        run_import_with_base(
            Some(project_dir.path()),
            &input_path,
            None,
            false,
            false,
            base_dir.path(),
        )
        .expect("clean round-trip must import");

        // Mutate the superseded predecessor's content (stored content_hash now stale).
        let store = open_test_store_at(&db_path).await;
        let pool = store.write_pool_server();
        sqlx::query("UPDATE entries SET content = 'tampered' WHERE id = 1")
            .execute(pool)
            .await
            .unwrap();

        // Re-run the SAME oracle the import path uses, on the committed DB.
        let mut conn = pool.acquire().await.unwrap();
        let verify = validate_hashes(&mut conn).await;
        assert!(verify.is_err(), "content mutation must be caught");
        let err = verify.unwrap_err().to_string();
        assert!(
            err.contains("entry 1"),
            "must name the mutated predecessor entry 1: {err}"
        );
        assert!(
            err.contains("content hash mismatch"),
            "must report a content-hash violation: {err}"
        );
    }
}
