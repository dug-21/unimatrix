# Component 3 — Import slug branch + pre-flight gates + vector redirect (`unimatrix-server/src/import/mod.rs`)

## Purpose

Add `--slug` to `import`: restore into the runtime's slug store and rebuild HNSW into
`slug_dir/vector`, guarded by two new pre-flight refusals (live-PID hard-error ADR-003;
non-empty-`audit_log` ADR-005). DB + vector targets come from `SlugStorePaths`; **PID stays
base-scoped `paths.pid_path`** (ADR-004). Import is a destructive write — every gate fires
before any write.

## Signature changes (add `slug` param; thread through all four)

```
pub fn run_import(project_dir, input, slug: Option<&str>, skip_hash_validation, force)          # +slug
pub fn run_import_with_base(project_dir, input, slug: Option<&str>, skip_hash_validation, force, base_dir)  # +slug
fn  run_import_inner(project_dir, input, slug, skip_hash_validation, force, base_dir)            # +slug
async fn run_import_async(project_dir, input, slug, skip_hash_validation, force, base_dir)       # +slug
```

`run_import_inner` and `run_import_async` just forward `slug` unchanged (the runtime-flavor
branching in `run_import_inner` is untouched — C-8/NFR-5: multi-thread runtime kept, GH#554).
Call sites to update (C-9): `main.rs:556-567` + the import integration test file only.

## Slug-mode target resolution (in `run_import_async`, Phase 1)

Compute the DB + vector targets ONCE, up front, then use them everywhere the code currently
uses `paths.db_path` / `paths.vector_dir`. PID is never redirected.

```
FUNCTION run_import_async(project_dir, input, slug, skip_hash_validation, force, base_dir):

    # Phase 1: setup — path-hash paths (C-6 still creates hash dir)
    paths = project::ensure_data_directory(project_dir, base_dir)?

    # NEW — resolve targets. Two path sources in slug mode; do NOT tidy into one (ADR-004).
    (db_target, vector_target) =
        MATCH slug:
            Some(raw) =>
                slug_store = projects::resolve_slug_store(&paths, raw)?    # validate→base→join→EXISTS (C-3)
                (slug_store.db_path, slug_store.vector_dir)
            None =>
                (paths.db_path.clone(), paths.vector_dir.clone())          # unchanged path-hash flow (AC-05)

    # NEW — live-PID hard-error gate (ADR-003) — structural, pre-open, slug mode ONLY.
    #        Placed before open so a live daemon refuses before any DB touch.
    IF slug.is_some():
        preflight_live_pid_refusal(&paths.pid_path)?      # reads paths.pid_path — base-scoped daemon PID

    # Phase 1b: open the TARGET store (slug or hash). Reached only post-existence-gate (C-3).
    store = Arc::new(SqlxStore::open(&db_target, PoolConfig::default()).await?)
    pool  = store.write_pool_server()

    # Phase 2: header parse (unchanged)
    ...
    header = parse_header(first_line)?

    # Phase 3: pre-flight — existing checks + NEW non-empty-audit gate (slug mode)
    db_schema_version = SELECT value FROM counters WHERE name='schema_version'
    check_preflight(pool, force, &paths, slug.is_some()).await?    # signature extended; see below

    # Phase 4..12: UNCHANGED except the vector target in Phase 10
    ... header validation, force-drop, BEGIN IMMEDIATE, ingest, hash-validate, COMMIT ...

    # Phase 10: vector redirect (ADR-004/AC-02/AC-12)
    crate::embed_reconstruct::reconstruct_embeddings(&store, &vector_target)?   # was &paths.vector_dir

    # Phase 11-12: provenance + print_summary (UNCHANGED; no import count summary — OQ-4/FR-14)
    ...
    Ok(())
```

## New gate 1: live-PID hard-error (ADR-003, AC-13, C-4)

```
FUNCTION preflight_live_pid_refusal(pid_path: &Path) -> Result<(), Box<dyn Error>>:
    IF let Some(pid) = read_pid_file(pid_path):            # infra/pidfile.rs:93
        IF is_process_alive(pid) AND is_unimatrix_process(pid):   # pidfile.rs:113 & :141 — LIVENESS, not file presence
            RETURN Err(format!(
                "a live unimatrix daemon (pid {pid}) is running; its PID file is {abs}. \
                 Importing into a live slug would be clobbered at shutdown. \
                 Run: stop → import --slug … → start.",
                pid = pid, abs = pid_path.display()))     # names resolved PID path + remedy (AC-13)
    RETURN Ok(())
```

- Predicate is **live-PID-only** (ADR-003): a `[[projects]]` stanza is NOT consulted — config
  is boot-only, and `register` writes the stanza before restart, so the canonical
  `register → stop → import → start` must not be blocked by a stanza (R-11 S2).
- Use `is_unimatrix_process` to narrow to unimatrix binaries (guards against a reused OS PID
  false-refuse). A stale/dead PID file must NOT block (R-11 coverage).
- No `--force` override exists for this refusal.
- Slug mode only. No-slug import keeps its existing warning-only PID behavior unchanged
  (AC-05) — see `check_preflight` below.

## New gate 2: non-empty-`audit_log` refusal (ADR-005, AC-10/FR-13, C-5)

Extend `check_preflight` to take a `slug_mode: bool` and add the audit gate (slug mode only,
after the existing entry-count/force check, before any write). Also downgrade the no-op PID
warning appropriately.

```
FUNCTION check_preflight(pool, force, paths, slug_mode) -> Result<(), Box<dyn Error>>:

    # (existing) empty-DB / --force check — unchanged; operates on the TARGET store's pool
    entry_count = SELECT COUNT(*) FROM entries
    IF entry_count > 0 AND NOT force:
        RETURN Err("database is not empty ({entry_count} entries). Use --force … or a fresh --project-dir.")

    # NEW — non-empty-audit pre-flight refusal (slug mode only). append-only audit_log
    #        cannot be cleared by drop_all_data (schema v25 triggers), so a non-empty target
    #        would hit a raw SQLite UNIQUE on the explicit-event_id INSERT. Refuse loud first.
    IF slug_mode:
        audit_rows = SELECT COUNT(*) FROM audit_log
        IF audit_rows > 0:
            RETURN Err(format!(
                "restore target already has {audit_rows} audit rows at {abs}; restore requires \
                 a freshly-registered (audit-empty) slug. Run `project register <new-slug>` and \
                 import there.",
                audit_rows = audit_rows, abs = paths /* resolved DB path — see note */))
            # NEVER surface the raw SQLite UNIQUE error (OQ-2). Fires BEFORE drop_all_data/insert.

    # PID check (existing lines 268-273): in slug mode the LIVE-PID hard-error already ran
    # pre-open (preflight_live_pid_refusal). Keep the warning-only branch for NO-slug mode
    # unchanged so AC-05 holds:
    IF NOT slug_mode:
        IF paths.pid_path.exists():
            eprintln!("WARNING: PID file exists at {} …", paths.pid_path.display())
    # (in slug mode, do NOT re-warn — the hard gate already covered it)

    RETURN Ok(())
```

> **OPEN QUESTION 4 — resolved path in the audit-gate message.** `check_preflight` currently
> receives `&paths` (path-hash). The audit message should name the *slug* db path
> (`db_target`), not the hash path. Simplest: pass the resolved `db_target: &Path` into
> `check_preflight` (or move the audit gate into `run_import_async` right after open where
> `db_target` is in scope). Recommend the latter — keeps `check_preflight`'s signature change
> minimal (`slug_mode: bool` only) and names the correct resolved path. Implementer decides;
> the message MUST name the resolved slug db path.

## Gate ordering (all before any write — R-02/R-07/R-11)

```
1. resolve_slug_store   → validate slug (AC-04) + existence gate (AC-03)   [pre-open, funnel]
2. live-PID refusal     → hard-error if live daemon (AC-13)                 [pre-open, structural]
3. SqlxStore::open(db_target)                                               [reached only post-1&2]
4. entry-count/--force check (existing)                                     [post-open DB query]
5. non-empty-audit refusal (AC-10)                                          [post-open DB query, pre-write]
6. drop_all_data (only if --force) / ingest / hash / COMMIT                 [writes]
7. reconstruct_embeddings(&store, &vector_target)                           [vector rebuild]
```

Cheapest structural gates (validate, existence, PID) first; DB-query gates after open; writes
last (ADR-005 ordering).

## State machine

Import is a linear pipeline (Phases 1–12). The new gates insert as additional pre-write guard
phases; no new lifecycle states. On any gate `Err`, the function returns before Phase 6, so no
`drop_all_data`, no ingest, no vector write occurs (R-14 "creates nothing").

## Data flow

- **Input:** JSONL `input`, `slug`, `force`, `skip_hash_validation`.
- **Slug mode:** `raw` → funnel → `db_target` (restore destination) + `vector_target` (HNSW
  rebuild destination); `paths.pid_path` (daemon PID, base-scoped) → live-PID gate.
- **No-slug:** `paths.db_path` + `paths.vector_dir`; warning-only PID (unchanged).
- **Output:** restored DB at `db_target`; fresh HNSW under `vector_target`; per-table counts
  via `print_summary` (unchanged).

## Error handling

| Condition | Gate | Message names | AC |
|---|---|---|---|
| Invalid/reserved slug | funnel `validate_slug` | raw slug + charset | AC-04 |
| Missing slug store | funnel existence gate | absolute db_target | AC-03 |
| Live daemon PID | `preflight_live_pid_refusal` | resolved PID path + `stop→import→start` | AC-13 |
| Non-empty audit target | `check_preflight` audit gate | resolved slug db path + "register fresh slug" | AC-10/FR-13 |
| DB not empty (no --force) | existing check | entry count | (unchanged) |

Never surface a raw SQLite UNIQUE error to the operator (C-5). No `unwrap`/`expect`.

## Key test scenarios (hints)

- **AC-10 round-trip (top weight):** seed slug A via literal-slug layout (all tables) →
  `export --slug A` → `import --slug B` into a second freshly-registered (audit-empty) slug →
  diff all tables A vs B; f64 confidence bit-exact, raw JSON-in-TEXT, NULL vs empty preserved,
  `chain_verify` clean. Crosses two different slugs (A→A is insufficient).
- **AC-12 served-vector-from-`start` (gate non-negotiable):** full `register → stop →
  import --slug → start`, then a served vector query returns restored hits — proves the daemon
  loaded the rebuilt `{slug}/vector` index, not disk state.
- **AC-13 live-PID:** live unimatrix PID at base-scoped `pid_path` → import refuses, message
  names PID path + remedy, `{slug}/vector` not written. Stale/dead PID (or stanza-only, no live
  daemon) → import proceeds (R-11 S2).
- **AC-02 vector redirect:** post-import, fresh HNSW under `{base}/<slug>/vector`; nothing
  written to the hash `vector/`.
- **FR-13 audit refusal:** import into a slug whose `audit_log` has rows → pre-flight refusal
  before `drop_all_data`, actionable message, no raw UNIQUE; `--force` does not bypass.
- **AC-03 missing store / AC-05 parity:** as in export.
