# Component 8 — register CLI (Rust)

**File:** `crates/unimatrix-server/src/projects.rs`
**ADR:** ADR-007 (#5086) · **AC:** AC-02, AC-03, AC-04 · **Risk:** R-05, R-06

## Purpose

`register <slug>` writes the `[[projects]]` routing-intent stanza atomically instead of printing config instructions. Idempotent, re-attach-safe (open existing store, never genesis-clobber). Same command for project 1 and N. Distroless-safe (Rust std::fs only, no shell). Restart applies via the unchanged boot read.

## State Model (UNCHANGED semantics; outputs change)

```
data_exists = db_path(per_slug_data_dir(base_dir, slug)).exists()
is_routed   = is_registered_in_config(slug)        // reads [[projects]] from config.toml
State A: data_exists && is_routed   -> loud error, NO write, nothing to do
State B: data_exists && !is_routed  -> RE-ATTACH: open preserved store (never genesis), THEN write stanza
State C: !data_exists               -> GENESIS: create dir tree + genesis store, THEN write stanza
```

## Modified `register` (MODIFY projects.rs:264-337)

```
fn register(&self, raw_slug: &str) -> Result<(), ServerError>:
    slug = Self::validate_slug(raw_slug)?               // charset + reserved (Component 9), unchanged
    dir  = per_slug_data_dir(&self.base_dir, &slug)
    data_exists = db_path(&dir).exists()
    is_routed   = self.is_registered_in_config(&slug)

    if data_exists && is_routed:
        return Err(Config("project '{slug}' is already registered and routing; nothing to do"))  // State A, unchanged

    if data_exists:
        // State B — RESTORE / re-attach. OPEN the preserved store; NEVER genesis over it (R-05, hash chain sacred).
        block_projects_sync(Store::open(&db_path(&dir), PoolConfig::default()))
            .map_err(|e| Config("failed to re-attach preserved store for '{slug}': {e}"))?
        // CHANGED: was eprintln!("re-add to config.toml ... [[projects]] slug = ...") at :302-304.
        self.ensure_project_stanza(&slug)?              // ATOMIC write of routing intent (AC-03)
        println!("re-attached project '{slug}' to its preserved store at {dir}; routing intent written. Restart to apply.")
        return Ok(())

    // State C — fresh registration. Create the per-slug tree, then genesis store (store-first), then stanza.
    create_dir_all(&dir)?; create_dir_all(dir.join(PROJECT_VECTOR_DIR))?     // unchanged
    block_projects_sync(Store::open(&db_path(&dir), PoolConfig::default()))
        .map_err(|e| Config("failed to initialize store for '{slug}': {e}"))?   // genesis; reached ONLY when !data_exists
    // CHANGED: was eprintln!("add to config.toml ... [[projects]] slug = ...") at :335.
    self.ensure_project_stanza(&slug)?                  // ATOMIC write of routing intent (AC-02/03)
    println!("registered project '{slug}' at {dir}; routing intent written. Restart to apply.")
    Ok(())
```

> Ordering invariant (R-05): in BOTH State B and State C the store is opened/created BEFORE the stanza write, so a stanza never points at a missing store. State C genesis is structurally unreachable when `data_exists` (guarded by the branch), so re-register can never clobber a hash chain.

## New `ensure_project_stanza` (NEW — atomic, idempotent `[[projects]]` write, ADR-007)

```
fn ensure_project_stanza(&self, slug: &ProjectSlug) -> Result<(), ServerError>:
    config_path = self.config_data_dir.join("config.toml")
    // 1. READ existing config (preserve ALL other config). Missing file => start from a minimal doc.
    text = read_to_string(config_path).unwrap_or_default()
    doc  = parse TOML (toml_edit or the existing toml lib) -> map_err Config("config.toml is malformed: {e}")
    // 2. IDEMPOTENCY: if a [[projects]] stanza with slug == this slug already exists -> no-op, return Ok.
    if doc.projects contains { slug == slug.as_str() }: return Ok(())
    // 3. APPEND the stanza, preserving existing entries and unrelated config.
    doc.projects.push(ProjectStanza { slug: slug.to_string() })
    new_text = serialize(doc)
    // 4. ATOMIC WRITE (SR-07 / R-06): temp file in the SAME dir + fsync + atomic rename over config.toml.
    tmp = config_data_dir.join(format!(".config.toml.{pid}.tmp", pid=process::id()))
    write_all(tmp, new_text).map_err(cleanup_tmp_and Config(...))?
    fsync(tmp)                                              // durability before rename
    fs::rename(tmp, config_path).map_err(cleanup_tmp_and Config(...))?   // atomic on one fs
    Ok(())
    // Crash mid-write => the rename never happened => config.toml is the OLD complete file (R-06).
```

> Use the SAME TOML representation the boot read parses (`load_config_and_build_allowlist` / `validate_projects_config`). The slug is the `ProjectSlug` newtype (charset-constrained), so no TOML metacharacter can survive into the stanza (TOML-injection guard, security risk #3). Prefer `toml_edit` to preserve comments/formatting if already a dependency; otherwise round-trip the existing `toml` structs. No new external crate (Dependencies: none added) — use whichever TOML lib is already in `Cargo.toml`.

## Data Flow

- IN: `raw_slug` (operator-supplied).
- Side effects: per-slug data dir + store (State C) or re-attach (State B); atomic append to `config.toml` `[[projects]]`.
- OUT: `Ok(())` + a stdout line; routing intent applied on next restart (no live reload, NFR-05).

## Error Handling

- Invalid/reserved slug → `ServerError::Config`, loud (Component 9).
- Malformed existing `config.toml` → loud `Config` error, no write (don't clobber an unparseable file blindly).
- Write/rename failure → temp cleaned up, loud `Config` error; config.toml left intact.
- State A → loud error, no write.

## Key Test Scenarios (hints)

1. AC-02/03: from clean state, `register <slug>` creates dir + genesis store AND appends `[[projects]]`; assert NO instruction string printed; boot re-read makes it routable.
2. AC-04 (N=2): `register <slug2>` via the same command appends a second stanza; both routable after restart; first stanza intact.
3. R-05 re-attach: `register` twice → State B opens the preserved store; chain-head hash equal before/after; no second genesis; one `[[projects]]` entry (idempotent).
4. R-06 atomicity: simulate interruption mid-write → config.toml is the complete old OR complete new file, never partial.
5. R-06 preservation: register into a config with N existing stanzas + unrelated `[http]`/`[tls]` sections → all preserved, N+1 well-formed.
6. State A → loud error, no write.
7. TOML-injection: a slug is already charset-constrained; assert no metacharacter path (covered by `validate_slug`).
