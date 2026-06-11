# Component: ProjectRegistry + register/list/delete lifecycle CLI

> Source file: `crates/unimatrix-server/src/projects.rs` *(new)*. CLI dispatch added to
> `crates/unimatrix-server/src/main.rs` (the C-10 pre-tokio sync subcommand block, ~L293).
> Requirements: FR-C4 (register/list/delete), FR-C3 (per-slug own DB+vector+hash-chain+
> analytics under `/data/.unimatrix/{slug}/`), FR-C5 (slug allowlist at the edge), C5/
> ADR-004 (register is server-side, store NEVER client-auto-created). LOCKED: D1 grammar
> (reused), D3 (list MAY include store-open status, NO network health surface), C-10
> (sync pre-tokio subcommand like health/version), D4 (delete = de-register only,
> `--purge` destroys loudly, re-register RE-ATTACHES), D5 (reserved-slug refusal at
> register — shared list with projects-config), D6 (register idempotence is two-state).

## Purpose

Operator-facing project lifecycle. `register <slug>` validates the slug (D1 charset AND
D5 reserved-segment refusal), then EITHER creates a fresh per-slug data tree
(`/data/.unimatrix/{slug}/` with its own DB, vector dir, hash chain, analytics) OR
re-attaches to a preserved one (D4/D6 restore path) — never client-auto-created. `list`
enumerates registered slugs (D3: MAY add a cheap store-open status field, operator-side
only, NO network surface). `delete <slug>` **de-registers only** — it PRESERVES the
on-disk data dir and hash chain (D4); `delete <slug> --purge` destroys the on-disk store
+ hash chain, and is LOUD (requires re-typing the slug to confirm). All are **pre-tokio
synchronous subcommands** (C-10), dispatched alongside `health`/`version`/`client-bundle`
(main.rs:293–361).

### The data dir is the source of truth (D4 mental model)

Two independent facts about a slug, never collapsed:
- **On-disk data dir** (`/data/.unimatrix/{slug}/`): the DB, vector index, **hash chain**,
  analytics. The hash chain is unrollbackable and sacred — destroying it is NEVER a default.
- **Routing registration** (the `[[projects]]` stanza the running server reads): whether
  the slug is currently routed. Operator-managed, restart-applied.

`delete` (default) removes ONLY routing intent and is non-destructive to the data dir.
`--purge` is the ONE operation that destroys the data dir + chain. `register` re-attaches
to a surviving data dir rather than clobbering it. De-register → re-register is a RESTORE.

## CLI surface (clap — mirror the existing `Command` enum, main.rs:84)

```rust
/// Project lifecycle (vnc-034 Wave 2, FR-C4). Sync pre-tokio subcommand (C-10).
/// register creates the per-slug store; list enumerates; delete removes. Operator-only;
/// a client NEVER auto-creates a project (C5/ADR-004).
Project {
    #[command(subcommand)]
    command: ProjectCommand,
},

enum ProjectCommand {
    /// Register a project: validate the slug (D1 charset + D5 reserved), then create a
    /// fresh /data/.unimatrix/{slug}/ OR re-attach to a preserved one (D4/D6 restore).
    Register { slug: String },
    /// List registered project slugs (D3: + optional cheap store-open status).
    List,
    /// De-register a project (D4): remove it from routing intent. By DEFAULT the on-disk
    /// data dir + hash chain are PRESERVED (non-destructive; re-register restores).
    /// --purge ALSO destroys the on-disk store + hash chain (loud: re-type the slug).
    Delete {
        slug: String,
        /// Destroy the on-disk data dir + hash chain too (NOT just de-register). The one
        /// operation that destroys integrity. Requires --confirm <slug> (re-typed name).
        #[arg(long)] purge: bool,
        /// Re-typed slug name confirming a --purge. Bare --purge is REFUSED; the operator
        /// must pass --confirm <slug> matching <slug> exactly. Ignored without --purge.
        #[arg(long)] confirm: Option<String>,
    },
}
```
> **D4 confirmation shape (gate-checked LOUDness):** `--purge` alone does NOT destroy.
> The operator must additionally pass `--confirm <slug>` where the value EQUALS the slug
> being purged. This is the "re-type the slug name" requirement — a bare flag is not
> enough to drop a hash chain. (No interactive TTY in the container, so the re-type is a
> required CLI value, not a prompt — mirrors the no-prompt constraint already noted for
> the old `--confirm` gate.) Default `delete` (no `--purge`) needs no confirmation because
> it is non-destructive.

### main.rs dispatch (add to the C-10 sync block, alongside Command::ClientBundle ~L338)

```
Some(Command::Project { command }) =>
    # Sync path: NO tokio (C-10), like Health/Version/ClientBundle. Uses
    # block_export_sync internally for the async sqlx store open (mirrors Snapshot/Eval).
    return unimatrix_server::projects::run_project_command(command, cli.project_dir)
        .map_err(Into::into);
```
> register/delete touch the sqlx store (async `Store::open`). Follow the established
> pattern (Snapshot/Eval at main.rs:352–360): the sync subcommand wraps async work via
> `block_export_sync` internally — NOT a tokio runtime in the dispatch arm. Flagged
> OQ-CLI-1 to confirm the exact block-on helper name available to `projects.rs`.

## New types (in projects.rs)

```rust
/// Registry rooted at the cloud data base dir (/data/.unimatrix). The base dir, NOT the
/// path-hash data_dir — slugs are operator-declared and path-independent (A2/ADR-004).
pub struct ProjectRegistry { base_dir: PathBuf }

/// One row of `list` output. `store_open` is the D3 cheap operator-side status field:
/// Some(true/false) only if cheaply determinable (e.g. db file exists + opens), None if
/// the implementation chooses not to probe. NO network/HTTP health (D3).
pub struct ProjectStatus { pub slug: ProjectSlug, pub store_open: Option<bool> }
```

## Shared path helper (the SINGLE per-slug layout site — also used by project-router)

```
fn per_slug_data_dir(base: &Path, slug: &ProjectSlug) -> PathBuf:
    base.join(slug.as_str())
    # slug is ALREADY allowlist-validated (D1) — no `..`, `/`, `%`, etc. can exist in it,
    # so the join cannot escape /data/.unimatrix/{slug}/ (AC-W2-R6). NEVER call this with
    # a raw &str; the &ProjectSlug type is the proof the value passed the parse edge.
```
This helper and `per_slug_paths(base, slug) -> ProjectPaths`-shaped derivation are the
ONLY translation from slug to filesystem path in the whole feature. Both `projects.rs`
and the listener wiring (`build_project_entry`) import it.

## New / modified functions

### `run_project_command` (entry, sync)

```
fn run_project_command(cmd: ProjectCommand, project_dir: Option<PathBuf>) -> Result<(), ServerError>:
    base_dir = resolve_base_dir(project_dir)?          # the /data base, NOT the path-hash data_dir
    registry = ProjectRegistry { base_dir }
    match cmd:
        Register { slug } => registry.register(&slug)
        List              => registry.list_and_print()
        Delete { slug, purge, confirm } => registry.delete(&slug, purge, confirm.as_deref())
```
`resolve_base_dir`: derive `/data/.unimatrix` from `project_dir` (the container's
`--project-dir /data` per constraint C-5/NFR-11). The base is the parent of the
path-hash data_dirs; confirm the exact derivation against `ensure_data_directory`'s
base-dir semantics (engine/project.rs). Flagged OQ-CLI-2.

### `ProjectRegistry::register`

```
fn register(&self, raw_slug: &str) -> Result<(), ServerError>:
    # 1a. PARSE EDGE — D1 charset validation BEFORE any filesystem use (R-03). Reuse the
    #     merged allowlist; do NOT re-implement.
    slug = ProjectSlug::try_from(raw_slug).map_err(|_| ServerError::Config(
        format!("invalid project slug '{raw_slug}': must match ^[a-z0-9][a-z0-9-]{{0,62}}$ \
                 (lowercase alphanumeric and hyphen, 1-63 chars, no underscore)")))?

    # 1b. RESERVED-SLUG refusal (D5) — SEPARATE check from D1. A charset-valid slug equal
    #     to a reserved route segment (v1/health/observe/tools) is still rejected. `tools`
    #     is critical: it shadows the /v1/tools/... default-project alias (ADR-005). Uses
    #     the SHARED RESERVED_SLUGS / is_reserved_slug from projects-config — NOT a 2nd list.
    if is_reserved_slug(&slug):
        return Err(ServerError::Config(format!(
            "project slug '{slug}' is reserved (v1, health, observe, tools); 'tools' would \
             shadow the default-project alias /v1/tools/...")))

    dir = per_slug_data_dir(&self.base_dir, &slug)     # safe: validated slug

    # 2. TWO-STATE idempotence (D6 + D4 re-attach). The on-disk data dir and the routing
    #    registration are independent facts; branch on BOTH, never collapse to one msg.
    data_exists  = db_path(&dir).exists()              # the data dir / hash chain survives
    is_routed    = self.is_registered_in_config(&slug) # currently in [[projects]] routing intent
                                                       # (cheap config read; see helper note)

    if data_exists and is_routed:
        # State A: already registered AND routing -> LOUD ERROR. No silent re-register,
        # no clobber. (D6 first state.)
        return Err(ServerError::Config(format!(
            "project '{slug}' is already registered and routing; nothing to do")))

    if data_exists and not is_routed:
        # State B: data dir survives but the slug was de-registered (D4). This is the
        # RESTORE path — RE-ATTACH to the preserved store/hash chain. OPEN the existing
        # store; NEVER initialize a fresh one over it (that would start a new chain over
        # old data — the integrity violation D4 forbids). This is NOT an error. (D6 2nd state.)
        block_on(Store::open(&db_path(&dir), PoolConfig::default()))   # OPENS existing — no genesis
            .map_err(|e| ServerError::Config(format!(
                "failed to re-attach preserved store for '{slug}': {e}")))?
        # Re-open is non-destructive: Store::open on an existing DB attaches to the
        # existing schema + hash chain (it does NOT re-run genesis or truncate). Assert
        # this is the open-existing path, not create. Flagged OQ-CLI-7.
        print to stdout: "re-attached project '{slug}' to its preserved store at {dir}"
        print to stderr: "re-add to config.toml to resume routing:\n\n[[projects]]\nslug = \"{slug}\"\n"
        return Ok(())

    # State C: fresh registration — no data dir. Create the per-slug tree (FR-C3): own DB,
    #          vector index dir, analytics. Store::open on a fresh path initializes schema +
    #          the hash chain genesis. This is the ONLY creator of a project store
    #          (C5: never client-auto-created).
    create_dir_all(&dir)?                              # 0700-ish; mirror existing data-dir perms
    create_dir_all(&vector_dir(&dir))?
    block_on(Store::open(&db_path(&dir), PoolConfig::default()))   # FRESH: initializes DB + hash chain
        .map_err(|e| ServerError::Config(format!("failed to initialize store for '{slug}': {e}")))?
    # (vector index file is lazily built on first use, matching the daemon path; or
    #  pre-build an empty index here if the daemon requires a present meta file —
    #  match open_or_build_vector_index semantics. Flagged OQ-CLI-3.)

    print to stdout: "registered project '{slug}' at {dir}"
    print to stderr: "add to config.toml to enable routing:\n\n[[projects]]\nslug = \"{slug}\"\n"
    Ok(())
```
> **Re-attach is the integrity guarantee (D4).** States B and C both call `Store::open`,
> but on DIFFERENT preconditions: B opens an EXISTING db (attaches to the surviving chain),
> C opens a FRESH path (runs genesis). The implementer MUST NOT funnel both through a code
> path that truncates/re-initializes when the file exists — re-attach over preserved data
> must never start a new chain. If `Store::open` cannot distinguish, gate the genesis on
> `data_exists` explicitly. Flagged OQ-CLI-7.
>
> **`is_registered_in_config` (D6 helper).** "Currently routing" is read from the
> `[[projects]]` config (the routing source of truth), NOT from the data dir's existence
> (the data dir surviving is exactly State B). If the CLI does not parse config today,
> the minimal read is `load_config(project_dir)` and scanning `config.projects` for the
> slug. Confirm the config path the CLI sees matches the daemon's. Flagged OQ-CLI-8.
> Whether `register` should ALSO append the `[[projects]]` stanza to `config.toml`
> automatically vs. only print it is a UX call. Auto-append couples the CLI to config
> file I/O + format preservation; printing keeps it simple and explicit. The pseudocode
> prints (recommended); flagged OQ-CLI-4.

### `ProjectRegistry::list_and_print`

```
fn list_and_print(&self) -> Result<(), ServerError>:
    slugs = self.scan_registered()?                    # enumerate dirs under base that hold a DB
    for status in slugs:
        line = status.slug.as_str()
        # D3: store_open is operator-side + CHEAP only. NO network probe, NO HTTP health.
        if let Some(open) = status.store_open:
            line += if open { "  [store: ok]" } else { "  [store: unavailable]" }
        print line to stdout
    Ok(())

fn scan_registered(&self) -> Result<Vec<ProjectStatus>, ServerError>:
    out = Vec::new()
    for entry in read_dir(&self.base_dir)?:            # each subdir is a candidate slug dir
        name = entry.file_name() as str
        # Only surface dirs whose name is a VALID slug AND that contain a DB. A dir whose
        # name isn't a valid slug (e.g. a path-hash data_dir) is NOT a registered project —
        # skip it. This also means list never emits an un-validatable name.
        slug = match ProjectSlug::try_from(name): Ok(s) => s, Err(_) => continue
        if !db_path(&per_slug_data_dir(&self.base_dir, &slug)).exists(): continue
        # D3 cheap status: file presence is free; "opens" requires a real open. Prefer the
        # cheapest signal (db file present + readable) to avoid opening N stores just to list.
        store_open = Some(db_file_readable(&db_path(...)))     # cheap; OR None to skip entirely
        out.push(ProjectStatus { slug, store_open })
    Ok(out)
```
> **D3 boundary (gate-checked):** `store_open` is derived ONLY from local filesystem
> state (file presence/readability) or at most a local store open — NEVER an
> over-the-wire/per-slug HTTP health call. There is NO `--list-slugs` network endpoint
> and NO per-slug `/health/{slug}`. Adding either reopens the ADR-004/OQ-B rejection and
> breaches AC-W1-S6. If the cheap signal is not actually cheap/meaningful, set
> `store_open = None` and omit the field — `list` then prints slugs only. Flagged OQ-CLI-5.

### `ProjectRegistry::delete`

```
fn delete(&self, raw_slug: &str, purge: bool, confirm: Option<&str>) -> Result<(), ServerError>:
    slug = ProjectSlug::try_from(raw_slug).map_err(|_| ServerError::Config(
        format!("invalid project slug '{raw_slug}'")))?     # parse edge again (R-03)

    dir = per_slug_data_dir(&self.base_dir, &slug)          # safe: validated slug

    if not purge:
        # ── D4 DEFAULT: DE-REGISTER ONLY — non-destructive. ──────────────────────────
        # PRESERVE the on-disk data dir (DB, vector, HASH CHAIN, analytics). We only drop
        # routing intent. The data dir surviving is what makes register's State-B re-attach
        # (restore) possible. The hash chain is never touched on a default delete.
        # No --confirm needed: nothing destructive happens.
        print to stdout: "de-registered project '{slug}' (data preserved at {dir})"
        print to stderr: "remove the matching [[projects]] stanza from config.toml and restart;\n\
                          data is retained — `project register {slug}` re-attaches it,\n\
                          `project delete {slug} --purge --confirm {slug}` destroys it permanently"
        return Ok(())

    # ── D4 --purge: DESTROY the on-disk store + hash chain. LOUD. ────────────────────
    # This is the ONE operation that destroys integrity (drops the unrollbackable chain).
    # Require the operator to RE-TYPE the slug via --confirm <slug>; a bare --purge is NOT
    # enough. The re-typed value must EQUAL the slug exactly.
    if confirm != Some(slug.as_str()):
        return Err(ServerError::Config(format!(
            "refusing to purge project '{slug}': re-type the slug to confirm.\n\
             this PERMANENTLY destroys {dir} including its hash chain (unrollbackable).\n\
             run: project delete {slug} --purge --confirm {slug}")))

    if !dir.exists():
        # Nothing on disk to purge. Treat as a loud no-op error so the operator isn't
        # misled into thinking a chain was destroyed (it never existed / already purged).
        return Err(ServerError::Config(format!(
            "project '{slug}' has no on-disk data to purge at {dir}")))

    remove_dir_all(&dir)?                                    # destroy the whole per-slug tree + chain
    print to stdout: "purged project '{slug}' — data dir and hash chain permanently destroyed"
    print to stderr: "remove the matching [[projects]] stanza from config.toml and restart"
    Ok(())
```
> **D4 split, restated:** default `delete` = de-register (preserve); `--purge` = destroy.
> Default never removes the data dir, so it needs no destructive confirmation and the chain
> is always recoverable via re-register. `--purge` is the only path that calls
> `remove_dir_all`, and it is gated on the re-typed slug (`--confirm <slug>`), not a bare
> flag. De-register → register is the RESTORE round-trip; purge is the irreversible exit.
>
> Neither variant touches a running server's in-memory resolver map (built at boot;
> register/delete/purge take effect on restart). This matches the "config + restart"
> operator model and keeps the CLI free of IPC to the daemon. Flagged OQ-CLI-6 (live
> reload — recommend NO for Wave 2; restart is the model).

## Data flow

```
`unimatrix project register alpha`
   │ ProjectSlug::try_from("alpha")   [D1 charset parse edge, before any FS]
   │ is_reserved_slug(alpha)? no      [D5 reserved refusal — separate check]
   ▼ per_slug_data_dir(/data/.unimatrix, alpha) = /data/.unimatrix/alpha
   ▼ two-state branch (D6):
   │   State A  data+routing -> LOUD ERROR (already registered)
   │   State B  data, no route -> RE-ATTACH: Store::open(existing) — preserve chain (D4 restore)
   │   State C  no data -> CREATE: Store::open(fresh) — genesis
   ▼ stdout: registered/re-attached; stderr: the [[projects]] stanza to add
operator adds [[projects]] slug="alpha" to config.toml, restarts
   ▼ load_config validates (projects-config.md) -> ProjectRouter routes /v1/alpha/... (project-router.md)

`unimatrix project delete alpha`            (D4 default: DE-REGISTER)
   ▼ data dir + hash chain PRESERVED; stderr tells operator to drop the [[projects]] stanza
   ▼ later `project register alpha` -> State B re-attach (RESTORE round-trip)

`unimatrix project delete alpha --purge --confirm alpha`   (D4: DESTROY, loud)
   │ confirm == slug? yes  -> remove_dir_all(dir): data dir + hash chain destroyed (irreversible)
   │ confirm missing/≠slug -> REFUSED (re-type the slug)
```

## Error handling

- Invalid slug (register/delete) → `ServerError::Config` with the D1 grammar in the
  message → non-zero exit. No `.unwrap()`, no panic (NFR-03).
- Reserved slug at register (D5) → `ServerError::Config` naming the reserved set + the
  `tools` shadow risk. Separate from the D1-charset message.
- register, State A (data + routing) → `ServerError::Config("already registered and
  routing")` — loud, no clobber, no silent re-register (D6).
- register, State B (data, de-registered) → NOT an error: re-attach + Ok (D4/D6 restore).
- delete default (de-register) → never errors on the data dir; always non-destructive.
- delete `--purge` without `--confirm <slug>` (or a mismatched confirm) → `ServerError::
  Config` refusal, NOTHING destroyed (D4 loud gate).
- delete `--purge --confirm <slug>` with no on-disk data → `ServerError::Config` loud
  no-op (so the operator isn't misled that a chain was destroyed).
- FS errors (create/remove/open) → wrapped in `ServerError`, loud + actionable.

## Key test scenarios (hints — not the test plan)

1. **AC-W2-R4 lifecycle:** `register alpha` creates `/data/.unimatrix/alpha/` with a DB;
   `list` shows `alpha`; `delete alpha` de-registers (data PRESERVED — assert the dir and
   DB still exist); `delete alpha --purge --confirm alpha` removes the tree; the dir is gone.
2. **C5 no-auto-create:** asserting register is the ONLY store creator — a slug routed
   without prior register fails (covered in project-router OQ-PR-5); the CLI is the creator.
3. **AC-W2-R6 / R-03:** `register "../etc"`, `register "a/b"`, `register "a%2fb"`,
   `register "Alpha"`, `register "a_b"` (underscore — drifted charset MUST reject), a
   64-char slug (over the 63 bound MUST reject), `register ""` all rejected at the parse
   edge, BEFORE any dir is created (assert no directory was created on rejection).
4. **D5 reserved refusal at register:** `register tools`, `register v1`, `register health`,
   `register observe` each rejected with the reserved message — and assert each is
   charset-valid (`ProjectSlug::try_from` Ok) so the rejection is the SEPARATE reserved
   check, not D1. Assert NO directory was created. `tools` message names the
   `/v1/tools/...` shadow.
5. **D6 two-state idempotence:**
   (a) State A — `register alpha`, add to config (routing), second `register alpha` →
       LOUD "already registered and routing", no clobber of the DB/hash chain.
   (b) State B — `register alpha`, `delete alpha` (de-register), `register alpha` again →
       NOT an error: re-attaches; assert it OPENED the existing DB (same hash-chain head /
       no new genesis), did NOT create a fresh store. Distinct message from State A.
6. **D4 delete/purge/re-attach integrity (the locked edge):**
   (a) `delete alpha` (default) → de-register; assert data dir + DB + hash chain PRESERVED.
   (b) `delete alpha --purge` (bare, no `--confirm`) → REFUSED; assert nothing destroyed.
   (c) `delete alpha --purge --confirm beta` (mismatched re-type) → REFUSED; nothing destroyed.
   (d) `delete alpha --purge --confirm alpha` → destroys the dir + chain (assert gone).
   (e) RESTORE round-trip: write a known entry to alpha's store, `delete alpha`,
       `register alpha`, assert the entry (and the hash-chain head) survived — re-attach,
       not a fresh chain.
7. **D3 list status:** with `store_open` enabled, a present DB shows `[store: ok]`; the
   field is derived from LOCAL fs only — assert no network call is made. With status
   disabled, `list` prints bare slugs. Assert there is NO `--list-slugs` flag exposing a
   network/HTTP surface and no per-slug HTTP endpoint (AC-W1-S6 / D3).
8. **C-10:** `project` runs with no tokio runtime in the dispatch arm (sync, pre-tokio,
   like health/version) — assert it works before the listener starts.
9. **single path-join site:** the slug→path translation goes only through
   `per_slug_data_dir`; grep asserts no other `base.join(<str>)` on slug input.

## Out of scope (Wave 2)

- No live daemon reload of the `ProjectRouter` map (restart is the model — OQ-CLI-6).
- No per-slug network health/listing surface (D3 — split if ever wanted, authenticated +
  out-of-band per ADR-004/OQ-B).
- No config-overlay (D2).
- No auto-append/auto-remove of the `[[projects]]` stanza in config.toml (printed
  guidance only — OQ-CLI-4; de-register/purge tell the operator to drop the stanza).
- No interactive prompt for `--purge` (no TTY in container); the re-typed `--confirm
  <slug>` value IS the confirmation.

## Open questions / gaps (flagged, not guessed)

- **OQ-CLI-1 (sync→async bridge):** confirm the block-on helper `projects.rs` uses for
  `Store::open` (the `block_export_sync` pattern used by Snapshot/Eval at main.rs:352).
- **OQ-CLI-2 (base dir derivation):** confirm `/data/.unimatrix` base-dir derivation from
  `--project-dir /data` vs `ensure_data_directory` semantics (path-hash data_dir is a
  CHILD of this base; slugs are siblings of the hash dirs, both under `.unimatrix`).
- **OQ-CLI-3 (vector index init):** does register pre-build an empty vector index/meta
  file, or is lazy-on-first-use sufficient? Match `open_or_build_vector_index` semantics.
- **OQ-CLI-4 (config append):** register PRINTS the `[[projects]]` stanza (recommended)
  vs auto-appends to config.toml. Pseudocode prints.
- **OQ-CLI-5 (D3 status cheapness):** confirm the cheapest meaningful `store_open` signal
  (db-file-readable vs a real open); if not cheap, omit the field (set None).
- **OQ-CLI-6 (live reload):** Wave 2 uses register/delete + restart (recommended); live
  daemon map reload is follow-up.
- **OQ-CLI-7 (re-attach vs genesis — D4 integrity):** confirm `Store::open` on an EXISTING
  db attaches to the surviving schema + hash chain and does NOT re-run genesis or truncate.
  Register State B (re-attach) and `--purge`-then-re-register MUST preserve the chain head.
  If `Store::open` cannot distinguish create-vs-open, gate genesis explicitly on
  `data_exists`. This is the load-bearing integrity guarantee — do not guess.
- **OQ-CLI-8 (routing-state read — D6):** `is_registered_in_config` reads "currently
  routing" from `[[projects]]` config (not the data dir). Confirm the config path the CLI
  sees matches the daemon's, and the cheapest read (full `load_config` vs a targeted parse).
