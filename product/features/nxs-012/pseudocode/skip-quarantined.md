# Pseudocode: skip-quarantined (export.rs + main.rs)

## Purpose

Add `--skip-quarantined` and `--confirm` CLI flags to the export subcommand. When `--skip-quarantined --confirm` is active, build a `HashSet<i64>` of quarantined entry IDs inside the DEFERRED transaction, then pass it to 5 affected table exporters which skip rows referencing those IDs. Report skip counts to stderr.

## CLI Flags (main.rs)

### Export Subcommand Changes

```
Export {
    /// Output file path. Defaults to stdout.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Exclude quarantined entries (status=3) and their dependents from export.
    /// Produces a clean snapshot. Requires --confirm.
    #[arg(long)]
    skip_quarantined: bool,

    /// Confirm intent to produce a filtered (non-exact) export.
    /// Required with --skip-quarantined. Silently ignored otherwise.
    #[arg(long)]
    confirm: bool,
},
```

### CLI Dispatch in main()

```
Some(Command::Export { output, skip_quarantined, confirm }) => {
    return unimatrix_server::export::run_export(
        cli.project_dir.as_deref(),
        output.as_deref(),
        skip_quarantined,
        confirm,
    );
}
```

## Public Entry Points (export.rs)

### run_export Signature Change

```
pub fn run_export(
    project_dir: Option<&Path>,
    output: Option<&Path>,
    skip_quarantined: bool,
    confirm: bool,
) -> Result<(), Box<dyn Error>>
{
    run_export_inner(project_dir, output, None, skip_quarantined, confirm)
}
```

### run_export_with_base Signature Change

```
pub fn run_export_with_base(
    project_dir: Option<&Path>,
    output: Option<&Path>,
    base_dir: &Path,
    skip_quarantined: bool,
    confirm: bool,
) -> Result<(), Box<dyn Error>>
{
    run_export_inner(project_dir, output, Some(base_dir), skip_quarantined, confirm)
}
```

## run_export_inner Changes

```
fn run_export_inner(
    project_dir: Option<&Path>,
    output: Option<&Path>,
    base_dir: Option<&Path>,
    skip_quarantined: bool,
    confirm: bool,
) -> Result<(), Box<dyn Error>>
{
    // ADR-009: --confirm validation BEFORE any DB access
    if skip_quarantined && !confirm {
        return Err(
            "--skip-quarantined produces a filtered export (quarantined entries and their \
             dependents are excluded). The export file will NOT be an exact copy of the \
             database. Add --confirm to acknowledge this and proceed."
            .into()
        );
    }

    let paths = project::ensure_data_directory(project_dir, base_dir)?;

    block_export_sync(async {
        let store = Arc::new(SqlxStore::open(&paths.db_path, PoolConfig::default()).await?);
        let pool = store.write_pool_server();

        // BEGIN DEFERRED snapshot transaction
        sqlx::query("BEGIN DEFERRED").execute(pool).await?;

        // Build skip set INSIDE the transaction (ADR-008, SR-02)
        let skip_ids: HashSet<i64> = if skip_quarantined {
            sqlx::query_scalar::<_, i64>("SELECT id FROM entries WHERE status = 3")
                .fetch_all(pool)
                .await?
                .into_iter()
                .collect()
        } else {
            HashSet::new()  // empty set -- O(1) contains() no-ops
        };

        // Run export
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

        let _ = sqlx::query("COMMIT").execute(pool).await;
        result
    })
}
```

## do_export Signature Change

```
async fn do_export(
    pool: &SqlitePool,
    writer: &mut impl Write,
    skip_ids: &HashSet<i64>,
    skip_quarantined: bool,
) -> Result<(), Box<dyn Error>>
{
    write_header(pool, writer, skip_quarantined).await?;
    export_counters(pool, writer).await?;
    let skip_entries = export_entries(pool, writer, skip_ids).await?;
    let skip_tags = export_entry_tags(pool, writer, skip_ids).await?;
    let skip_co = export_co_access(pool, writer, skip_ids).await?;
    let skip_fe = export_feature_entries(pool, writer, skip_ids).await?;
    export_outcome_index(pool, writer).await?;
    export_agent_registry(pool, writer).await?;
    export_audit_log(pool, writer).await?;
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

    Ok(())
}
```

## write_header Extension

Add optional `skip_quarantined` field to the header when flag is active:

```
async fn write_header(
    pool: &SqlitePool,
    writer: &mut impl Write,
    skip_quarantined: bool,
) -> Result<(), Box<dyn Error>>
{
    // ... existing schema_version, entry_count, exported_at queries ...

    let mut map = Map::new();
    map.insert("_header", true);
    map.insert("schema_version", schema_version);
    map.insert("exported_at", exported_at);
    map.insert("entry_count", entry_count);
    map.insert("format_version", 2);

    // Optional: indicate this is a filtered export (R-24)
    if skip_quarantined {
        map.insert("skip_quarantined", true);
    }

    write_header_line(map, writer)?;
    Ok(())
}
```

## Existing Exporter Signature Changes

All 4 existing entry-referencing exporters gain `skip_ids: &HashSet<i64>` and return the skip count as `u64`:

### export_entries

```
async fn export_entries(
    pool: &SqlitePool,
    writer: &mut impl Write,
    skip_ids: &HashSet<i64>,
) -> Result<u64, Box<dyn Error>>
{
    let rows = sqlx::query("SELECT id, ... FROM entries ORDER BY id")
        .fetch_all(pool).await?;

    let mut skipped: u64 = 0;

    for row in &rows {
        let id: i64 = row.get::<i64, _>(0);

        // Skip quarantined entries (ADR-008)
        if skip_ids.contains(&id) {
            skipped += 1;
            continue;
        }

        // ... existing map construction and write_row ...
    }
    Ok(skipped)
}
```

### export_entry_tags

```
async fn export_entry_tags(
    pool: &SqlitePool,
    writer: &mut impl Write,
    skip_ids: &HashSet<i64>,
) -> Result<u64, Box<dyn Error>>
{
    let rows = sqlx::query("SELECT entry_id, tag FROM entry_tags ORDER BY entry_id, tag")
        .fetch_all(pool).await?;

    let mut skipped: u64 = 0;

    for row in &rows {
        let entry_id: i64 = row.get::<i64, _>(0);

        if skip_ids.contains(&entry_id) {
            skipped += 1;
            continue;
        }

        // ... existing map construction and write_row ...
    }
    Ok(skipped)
}
```

### export_co_access

```
async fn export_co_access(
    pool: &SqlitePool,
    writer: &mut impl Write,
    skip_ids: &HashSet<i64>,
) -> Result<u64, Box<dyn Error>>
{
    let rows = sqlx::query(
        "SELECT entry_id_a, entry_id_b, count, last_updated
         FROM co_access ORDER BY entry_id_a, entry_id_b"
    ).fetch_all(pool).await?;

    let mut skipped: u64 = 0;

    for row in &rows {
        let entry_id_a: i64 = row.get::<i64, _>(0);
        let entry_id_b: i64 = row.get::<i64, _>(1);

        // BOTH sides checked (R-19, FR-23)
        if skip_ids.contains(&entry_id_a) || skip_ids.contains(&entry_id_b) {
            skipped += 1;
            continue;
        }

        // ... existing map construction and write_row ...
    }
    Ok(skipped)
}
```

### export_feature_entries

```
async fn export_feature_entries(
    pool: &SqlitePool,
    writer: &mut impl Write,
    skip_ids: &HashSet<i64>,
) -> Result<u64, Box<dyn Error>>
{
    let rows = sqlx::query(
        "SELECT feature_id, entry_id FROM feature_entries ORDER BY feature_id, entry_id"
    ).fetch_all(pool).await?;

    let mut skipped: u64 = 0;

    for row in &rows {
        let entry_id: i64 = row.get::<i64, _>(1);  // entry_id is column index 1

        if skip_ids.contains(&entry_id) {
            skipped += 1;
            continue;
        }

        // ... existing map construction and write_row ...
    }
    Ok(skipped)
}
```

## New Exporter: export_graph_edges (skip-aware)

```
async fn export_graph_edges(
    pool: &SqlitePool,
    writer: &mut impl Write,
    skip_ids: &HashSet<i64>,
) -> Result<u64, Box<dyn Error>>
{
    let rows = sqlx::query(
        "SELECT source_id, target_id, relation_type, weight,
                created_at, created_by, source, bootstrap_only, metadata
         FROM graph_edges
         ORDER BY source_id, target_id, relation_type"
    ).fetch_all(pool).await?;

    let mut skipped: u64 = 0;

    for row in &rows {
        let source_id: i64 = row.get::<i64, _>(0);
        let target_id: i64 = row.get::<i64, _>(1);

        // BOTH sides checked (R-20, FR-24)
        if skip_ids.contains(&source_id) || skip_ids.contains(&target_id) {
            skipped += 1;
            continue;
        }

        // ... map construction with NaN-safe weight (ADR-003) and write_row ...
    }
    Ok(skipped)
}
```

## Unaffected Exporters

These functions do NOT receive `skip_ids` and their signatures are unchanged:
- `export_counters`
- `export_outcome_index`
- `export_agent_registry`
- `export_audit_log`
- `export_observations`
- `export_cycle_events`

Per ADR-008: observations and cycle_events reference sessions/cycles, not entry IDs. counters, outcome_index, agent_registry, and audit_log have no entry references.

## Return Type Changes for Existing Exporters

The 4 existing exporters (`export_entries`, `export_entry_tags`, `export_co_access`, `export_feature_entries`) change their return type from `Result<(), Box<dyn Error>>` to `Result<u64, Box<dyn Error>>` where the `u64` is the count of skipped rows. When `skip_ids` is empty, the skipped count is always 0.

## Error Handling

- `--confirm` validation is a pure argument check -- no DB access, instant fail path (ADR-009)
- Skip-set query errors propagate via `?`
- All existing export error paths unchanged
- When `skip_ids` is empty (default path), `contains()` is O(1) on empty HashSet -- zero behavioral change (AC-29)

## Key Test Scenarios

1. **--skip-quarantined without --confirm aborts** -- non-zero exit, error message mentions `--confirm` (FR-26, AC-30).
2. **--skip-quarantined without --confirm produces no output** -- no file created, no DB access.
3. **--skip-quarantined --confirm succeeds** -- export proceeds normally.
4. **--confirm without --skip-quarantined** -- silently ignored, full export produced (ADR-009).
5. **entries filtered** -- DB with 5 entries (3 active, 2 quarantined). Export with --skip-quarantined --confirm. Only 3 entries in output (FR-20, AC-23).
6. **entry_tags filtered** -- tags for quarantined entries absent, tags for active entries present (FR-21, AC-24).
7. **feature_entries filtered** -- feature_entries for quarantined entries absent (FR-22, AC-25).
8. **co_access dual-column check** -- 4 combinations: (a) neither quarantined -> present, (b) entry_id_a quarantined -> absent, (c) entry_id_b quarantined -> absent, (d) both quarantined -> absent (FR-23, AC-26, R-19).
9. **graph_edges dual-column check** -- 4 combinations: (a) neither quarantined -> present, (b) source_id quarantined -> absent, (c) target_id quarantined -> absent, (d) both quarantined -> absent (FR-24, AC-27, R-20).
10. **observations NOT filtered** -- all observations present even with --skip-quarantined (FR-25, AC-29).
11. **cycle_events NOT filtered** -- all cycle_events present even with --skip-quarantined (FR-25).
12. **counters, audit_log, outcome_index, agent_registry NOT filtered** -- row counts match DB (R-21).
13. **Default path unchanged** -- export without --skip-quarantined includes quarantined entries and all dependents (FR-28, AC-29).
14. **Skip summary reported** -- stderr includes skipped entry count and per-table dependent skip counts (FR-27, AC-28).
15. **Skip summary absent when flag inactive** -- no skip-related lines in stderr output.
16. **Zero quarantined entries with --skip-quarantined** -- empty skip set, all rows exported, skip counts 0.
17. **All entries quarantined** -- no entries exported, all dependent rows skipped, observations/cycle_events/counters/audit_log exported.
18. **Hash integrity preserved** -- export with --skip-quarantined --confirm, import without --skip-hash-validation succeeds (FR-29, AC-31).
19. **Header includes skip_quarantined metadata** -- parse header when flag active, verify skip_quarantined: true (R-24).
20. **Header does NOT include skip_quarantined when flag inactive** -- verify field absent or false.
21. **Skip set built inside DEFERRED transaction** -- code-level verification that SELECT runs after BEGIN DEFERRED (R-17).
22. **Round-trip with skip-quarantined** -- export with --skip-quarantined --confirm, import, re-export, verify no quarantined entry IDs appear anywhere (R-16).
