# Component 2 — Export slug branch + stderr summary (`unimatrix-server/src/export.rs`)

## Purpose

Add `--slug` support to `export`: in slug mode, open the store the funnel resolves instead of
the path-hash store; in both modes, print a one-line stderr count summary (ADR-006/AC-06). No
second resolver — export calls `resolve_slug_store` and nothing else new.

## Signature changes (Integration Surface — add `slug` param, keep order/position stable)

```
pub fn run_export(
    project_dir: Option<&Path>,
    output: Option<&Path>,
    slug: Option<&str>,          # NEW
    skip_quarantined: bool,
    confirm: bool,
) -> Result<(), Box<dyn std::error::Error>>

pub fn run_export_with_base(
    project_dir: Option<&Path>,
    output: Option<&Path>,
    base_dir: &Path,
    slug: Option<&str>,          # NEW
    skip_quarantined: bool,
    confirm: bool,
) -> Result<(), Box<dyn std::error::Error>>

fn run_export_inner(
    project_dir: Option<&Path>,
    output: Option<&Path>,
    base_dir: Option<&Path>,
    slug: Option<&str>,          # NEW
    skip_quarantined: bool,
    confirm: bool,
) -> Result<(), Box<dyn std::error::Error>>
```

> Param placement: put `slug` right after `output` (before `skip_quarantined`) so the summary
> concern and the store-selection concern read together. Implementer may choose a consistent
> position; whatever is chosen, update `main.rs:556-567` and the export integration test file
> (C-9) — those are the ONLY other call sites.

## Modified function body: `run_export_inner`

```
FUNCTION run_export_inner(project_dir, output, base_dir, slug, skip_quarantined, confirm):

    # (unchanged) ADR-009 --confirm guard BEFORE any DB access
    IF skip_quarantined AND NOT confirm: RETURN Err("... add --confirm ...")

    # 1. Resolve path-hash paths (unchanged; C-6: still creates+chmods hash dir/vector)
    paths = project::ensure_data_directory(project_dir, base_dir)?

    # 2. NEW — select the DB to open (the ONLY new branch)
    open_db_path =
        MATCH slug:
            Some(raw) =>
                slug_store = projects::resolve_slug_store(&paths, raw)?   # funnel: validate→base→join→EXISTS
                             # ^ existence gate has already proven the db file exists (C-3),
                             #   so the open below never auto-creates a slug store (R-02).
                slug_store.db_path
            None =>
                paths.db_path.clone()      # byte-for-byte unchanged path-hash flow (AC-05/AC-11)

    # 3. Bridge async→sync exactly as today (block_export_sync; C-8/NFR-5)
    RETURN block_export_sync(async {

        # 4. Open target DB. In slug mode this is reached ONLY post-existence-gate.
        store = Arc::new(SqlxStore::open(&open_db_path, PoolConfig::default()).await?)
        pool  = store.write_pool_server()

        # 5. BEGIN DEFERRED snapshot (unchanged)
        query("BEGIN DEFERRED").execute(pool).await?

        # 6. Build skip set inside txn (unchanged)
        skip_ids = IF skip_quarantined { SELECT id FROM entries WHERE status = 3 } ELSE { empty }

        # 7. Run export, capturing counts for the summary (see change to do_export below)
        counts =
            IF Some(path) = output:
                file = File::create(path)?; writer = BufWriter::new(file)
                do_export(pool, &mut writer, &skip_ids, skip_quarantined).await
            ELSE:
                lock = stdout().lock(); writer = BufWriter::new(lock)
                do_export(pool, &mut writer, &skip_ids, skip_quarantined).await

        # 8. COMMIT (read-only DEFERRED; unchanged)
        let _ = query("COMMIT").execute(pool).await

        counts        # propagate Result<ExportCounts, _>
    })
    .and_then(|counts| {
        # 9. NEW — stderr count summary (AC-06, both modes). Runs only on export success.
        emit_export_summary(&counts, output, &open_db_path)
        Ok(())
    })
```

> Structural note: `block_export_sync` currently returns `Result<(), _>`. To carry counts out
> for the summary, either (a) have the async block return `Result<ExportCounts, _>` and widen
> `block_export_sync`'s generic to `Result<T, _>`, or (b) print the summary INSIDE the async
> block after COMMIT. Option (b) keeps `block_export_sync` untouched and is preferred (fewer
> signature ripples). Pseudocode above shows the emit outside for clarity; implementer picks
> one — **OPEN QUESTION 2**.

## Count summary — where the numbers come from

The summary line is `exported N entries, M audit rows → <path>` (ADR-006/FR-8), reporting what
was actually written (post-filter). Sources:

- `do_export` already returns per-table skip/write counts internally (`export_entries` returns
  a `u64` of written entries; `export_audit_log` currently returns `()`).
- **Change:** make `do_export` return a small struct so the summary reflects emitted rows:

```
struct ExportCounts { entries: u64, audit_rows: u64 }

# do_export: accumulate and return
async fn do_export(pool, writer, skip_ids, skip_quarantined) -> Result<ExportCounts, _> {
    write_header(...)
    entries = export_entries(pool, writer, skip_ids).await?     # already returns written count
    ...
    audit_rows = export_audit_log(pool, writer).await?          # CHANGE: return written u64 (was ())
    ...
    Ok(ExportCounts { entries, audit_rows })
}
```

`export_entries` returns entries actually written (total minus skipped) — that is the correct
"exported N entries" number, and it makes a `--skip-quarantined` sparse export self-diagnosing
(`exported 0 entries`). Do NOT change the `--skip-quarantined` / `audit_log` filter asymmetry
(NFR-9) — only surface the counts.

## `emit_export_summary`

```
FUNCTION emit_export_summary(counts, output, resolved_db_path):
    dest = MATCH output: Some(p) => p.display() ; None => "stdout"
    eprintln!("exported {} entries, {} audit rows → {}",
              counts.entries, counts.audit_rows, dest)
    # stderr ONLY — stdout/JSONL piping unaffected (ADR-006). Resolved path is the OUTPUT
    # target (where the dump went). The resolved SOURCE db path (open_db_path) is already
    # named in any failure; on success the operator cares about the output destination.
```

> **OPEN QUESTION 3:** FR-8/AC-06 wording is "resolved output path". When `output` is `None`
> (stdout), print `→ stdout`. Confirm with tester whether AC-06 also wants the source db path
> echoed in slug mode; spec text says output path. Defaulting to output path per FR-8.

## State machine

None. Linear: guard → resolve → open → snapshot-read → commit → summary.

## Data flow

- **Input:** CLI args + `slug`.
- **No-slug:** `paths.db_path` → open → export → summary naming output. Identical to today
  except the added stderr line (WARN-1 reconciled: AC-05 byte-for-byte covers file+stdout+exit
  code, NOT stderr).
- **Slug:** `raw` → `resolve_slug_store` → `SlugStorePaths.db_path` → open → export → summary.
- **Output:** JSONL to file/stdout (unchanged) + stderr summary.

## Error handling

- `resolve_slug_store` errors (invalid/reserved slug AC-04; missing store AC-03) propagate via
  `?` before any open — fail loud, create nothing.
- Existing open/txn/write errors unchanged. Summary is emitted only on export success, so a
  failed export never prints a misleading count.
- Live daemon slug store (AC-08): export opens read-only alongside WAL + `busy_timeout`; no
  locking added — unchanged behavior, succeeds.

## Key test scenarios (hints)

- **AC-09 disagreement seam (top weight):** `run_export_with_base(project_dir, base=X,
  slug=Some("foo"), ...)`; seed `X/foo/unimatrix.db` via the `http_provision` literal-slug
  layout with entry set A, seed `X/<hash>/unimatrix.db` differently with disjoint non-empty
  set B. Assert emitted rows == A and ∩(emitted, B) == ∅. Same fixture, `slug=None` → emits B
  (proves the paths genuinely diverge; N=1 same-path is ceremonial #4974).
- **AC-06 summary:** capture stderr, assert it contains entry count, audit-row count, and the
  output path; assert stdout unaffected. Sparse case: 0 entries + audit rows → `exported 0
  entries, M audit rows`.
- **AC-03 missing store:** `export --slug ghost` → non-zero, error names absolute db_path, no
  file/dir created.
- **AC-05 parity:** existing export suite passes; no-slug resolved open path == `paths.db_path`.
- **AC-08 live-daemon read:** export a slug store under a simulated WAL writer → succeeds.
