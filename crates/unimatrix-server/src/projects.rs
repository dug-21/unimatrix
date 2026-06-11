//! Project lifecycle registry + CLI (vnc-034 Wave 2, FR-C3/FR-C4).
//!
//! Operator-facing `register` / `list` / `delete` over per-slug stores under
//! `{base}/.unimatrix/{slug}/`. All three are **pre-tokio synchronous**
//! subcommands (C-10) dispatched alongside `health` / `version` / `client-bundle`
//! in `main.rs`; async store work bridges via [`block_projects_sync`] (the same
//! current-thread runtime bridge `snapshot`/`eval` use), never a runtime in the
//! dispatch arm.
//!
//! ## The data dir is the source of truth (D4 mental model)
//!
//! Two independent facts about a slug, never collapsed:
//! - **On-disk data dir** (`{base}/{slug}/`): the DB, vector index, **hash chain**,
//!   analytics. The hash chain is unrollbackable and sacred — destroying it is
//!   NEVER a default.
//! - **Routing registration** (the `[[projects]]` stanza the running server reads):
//!   whether the slug is currently routed. Operator-managed, restart-applied.
//!
//! `delete` (default) removes ONLY routing intent and is non-destructive to the
//! data dir. `--purge` is the ONE operation that destroys the data dir + chain.
//! `register` re-attaches to a surviving data dir rather than clobbering it
//! (D4/D6 RESTORE path). De-register → re-register is a RESTORE.
//!
//! ## OQ-CLI-7 — re-attach vs genesis (the load-bearing integrity guarantee)
//!
//! `SqlxStore::open` on an EXISTING db runs idempotent migrations +
//! `create_tables_if_needed` only — it does NOT truncate or re-run a genesis that
//! clobbers existing rows, so the hash chain (the `entries` rows + their
//! `previous_hash` links) is preserved across an open. The re-attach (State B) and
//! `--purge`-then-re-register paths therefore preserve the chain head. As defence
//! in depth, the per-slug *directory* creation is gated explicitly on
//! `data_exists` so the "fresh" provisioning branch can never run over preserved
//! data even if `open` semantics ever change.

use std::path::{Path, PathBuf};

use clap::Subcommand;

use unimatrix_core::Store;
use unimatrix_store::PoolConfig;

use crate::error::ServerError;
use crate::http::ProjectSlug;
use crate::infra::config::is_reserved_slug;
use crate::project;

/// Database file name within a per-slug data dir (matches the single-project layout
/// and `http_provision::build_project_server`).
const PROJECT_DB_NAME: &str = "unimatrix.db";
/// Vector index subdirectory within a per-slug data dir.
const PROJECT_VECTOR_DIR: &str = "vector";

/// Project lifecycle subcommands (vnc-034 Wave 2, FR-C4). Sync pre-tokio (C-10).
///
/// `register` creates (or re-attaches) the per-slug store; `list` enumerates;
/// `delete` de-registers (and `--purge` destroys). Operator-only — a client NEVER
/// auto-creates a project (C5 / ADR-004).
#[derive(Debug, Subcommand)]
pub enum ProjectCommand {
    /// Register a project: validate the slug (D1 charset + D5 reserved), then
    /// create a fresh `{base}/{slug}/` OR re-attach to a preserved one (D4/D6).
    Register {
        /// Operator-declared project slug.
        slug: String,
    },
    /// List registered project slugs (D3: + a local store-open status field).
    List,
    /// De-register a project (D4): remove it from routing intent. By DEFAULT the
    /// on-disk data dir + hash chain are PRESERVED (non-destructive; re-register
    /// restores). `--purge` ALSO destroys the data dir + hash chain (loud:
    /// re-type the slug via `--confirm <slug>`).
    Delete {
        /// Project slug to de-register (or purge).
        slug: String,
        /// Destroy the on-disk data dir + hash chain too (NOT just de-register).
        /// The one operation that destroys integrity. Requires `--confirm <slug>`.
        #[arg(long)]
        purge: bool,
        /// Re-typed slug name confirming a `--purge`. A bare `--purge` is REFUSED;
        /// the operator must pass `--confirm <slug>` matching `<slug>` exactly.
        /// Ignored without `--purge`.
        #[arg(long)]
        confirm: Option<String>,
    },
}

/// One row of `list` output (vnc-034 D3).
///
/// `store_open` is the operator-side status field, derived ONLY from local
/// filesystem state (db-file presence/readability) — NEVER a network/HTTP probe.
#[derive(Debug)]
pub struct ProjectStatus {
    /// The validated slug (a dir under `base` whose name parses as a `ProjectSlug`).
    pub slug: ProjectSlug,
    /// Local store-open signal: `Some(true)` if the db file is present and readable,
    /// `Some(false)` if the dir exists but the db is missing/unreadable.
    pub store_open: Option<bool>,
}

/// Registry rooted at the per-slug base dir (`{...}/.unimatrix`). The base dir,
/// NOT the path-hash `data_dir` — slugs are operator-declared and
/// path-independent (A2 / ADR-004).
#[derive(Debug)]
pub struct ProjectRegistry {
    /// Base dir under which each slug owns a `{base}/{slug}/` tree.
    base_dir: PathBuf,
    /// The daemon's path-hash `data_dir` — where `config.toml` (the routing source
    /// of truth) lives. Used to read "currently routing" state (D6).
    config_data_dir: PathBuf,
}

/// The SINGLE per-slug path-join site for the whole feature (mirrors
/// `http_provision::build_project_server`).
///
/// `slug` is ALREADY allowlist-validated (D1) — no `..`, `/`, `%`, etc. can exist
/// in it, so the join cannot escape `{base}/{slug}/` (AC-W2-R6). NEVER call this
/// with a raw `&str`; the `&ProjectSlug` type is the proof the value passed the
/// parse edge.
fn per_slug_data_dir(base: &Path, slug: &ProjectSlug) -> PathBuf {
    base.join(slug.as_str())
}

/// The per-slug db path within its data dir.
fn db_path(dir: &Path) -> PathBuf {
    dir.join(PROJECT_DB_NAME)
}

/// Bridge a sync subcommand to the async `Store::open` via a current-thread
/// runtime (mirrors `snapshot`/`eval`; C-09). No outer runtime is assumed —
/// these subcommands dispatch pre-tokio (C-10).
fn block_projects_sync<F, T>(fut: F) -> Result<T, ServerError>
where
    F: std::future::Future<Output = Result<T, ServerError>>,
{
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| ServerError::Config(format!("failed to build runtime: {e}")))?;
    rt.block_on(fut)
}

/// Entry point for the `project` subcommand (sync, pre-tokio — C-10).
///
/// Resolves the per-slug base dir (the `.unimatrix` parent of the path-hash
/// data_dir) and the daemon's config data_dir, then dispatches.
pub fn run_project_command(
    cmd: ProjectCommand,
    project_dir: Option<PathBuf>,
) -> Result<(), ServerError> {
    let registry = ProjectRegistry::resolve(project_dir.as_deref())?;
    match cmd {
        ProjectCommand::Register { slug } => registry.register(&slug),
        ProjectCommand::List => registry.list_and_print(),
        ProjectCommand::Delete {
            slug,
            purge,
            confirm,
        } => registry.delete(&slug, purge, confirm.as_deref()),
    }
}

impl ProjectRegistry {
    /// Resolve the registry from the operator's `--project-dir`.
    ///
    /// `ensure_data_directory` yields the path-hash `data_dir`
    /// (`{base}/.unimatrix/{hash}/`); the per-slug base is its parent
    /// (`{base}/.unimatrix`) — the SAME base the listener wiring uses
    /// (`paths.data_dir.parent()`), so register and route agree on layout.
    fn resolve(project_dir: Option<&Path>) -> Result<Self, ServerError> {
        let paths = project::ensure_data_directory(project_dir, None)
            .map_err(|e| ServerError::ProjectInit(e.to_string()))?;
        let base_dir = paths
            .data_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| paths.data_dir.clone());
        Ok(ProjectRegistry {
            base_dir,
            config_data_dir: paths.data_dir,
        })
    }

    /// Construct a registry over an explicit base dir + config data dir (tests).
    #[cfg(test)]
    fn with_dirs(base_dir: PathBuf, config_data_dir: PathBuf) -> Self {
        ProjectRegistry {
            base_dir,
            config_data_dir,
        }
    }

    /// Validate `raw_slug` (D1 charset) then reject reserved segments (D5).
    ///
    /// Two SEPARATE checks: a charset-valid slug equal to a reserved route segment
    /// (`v1`/`health`/`observe`/`tools`) is still rejected. `tools` is critical —
    /// it shadows the `/v1/tools/...` default-project alias (ADR-005).
    fn validate_slug(raw_slug: &str) -> Result<ProjectSlug, ServerError> {
        // 1. Charset allowlist (D1) — the shared Wave-1 newtype, NOT a reimpl.
        let slug = ProjectSlug::try_from(raw_slug).map_err(|_| {
            ServerError::Config(format!(
                "invalid project slug '{raw_slug}': must match \
                 ^[a-z0-9][a-z0-9-]{{0,62}}$ (lowercase alphanumeric and hyphen, \
                 1-63 chars, no underscore)"
            ))
        })?;

        // 2. Reserved-slug refusal (D5) — SEPARATE check, shared list from config.
        if is_reserved_slug(&slug) {
            return Err(ServerError::Config(format!(
                "project slug '{slug}' is reserved (v1, health, observe, tools); \
                 'tools' would shadow the default-project alias /v1/tools/..."
            )));
        }

        Ok(slug)
    }

    /// The slugs currently declared in the daemon's `[[projects]]` routing config
    /// (the routing source of truth — `data_dir/config.toml`).
    ///
    /// Each raw stanza `slug` is re-validated through the shared `ProjectSlug`
    /// allowlist; charset-invalid or reserved entries (which the daemon itself
    /// would reject at load) are skipped here rather than failing the lifecycle
    /// command. An absent/unparseable config yields an empty list. Reading config
    /// — NOT directory presence — is the discriminator: path-hash data_dirs are
    /// siblings of slug dirs under `.unimatrix` and have charset-valid names, so a
    /// directory scan would mis-classify them as projects. Config is authoritative.
    fn configured_slugs(&self) -> Vec<ProjectSlug> {
        let path = self.config_data_dir.join("config.toml");
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Vec::new();
        };
        // Parse ONLY the `[[projects]]` array; ignore everything else so a
        // malformed unrelated section never blocks the lifecycle command.
        #[derive(serde::Deserialize)]
        struct RoutingProbe {
            #[serde(default)]
            projects: Vec<ProbeEntry>,
        }
        #[derive(serde::Deserialize)]
        struct ProbeEntry {
            slug: String,
        }
        match toml::from_str::<RoutingProbe>(&text) {
            Ok(probe) => probe
                .projects
                .iter()
                .filter_map(|e| ProjectSlug::try_from(e.slug.as_str()).ok())
                .filter(|s| !is_reserved_slug(s))
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Is `slug` currently in the daemon's `[[projects]]` routing config (D6)?
    ///
    /// "Currently routing" is read from config (the routing source of truth), NOT
    /// from the data dir's existence (the data dir surviving is exactly State B).
    fn is_registered_in_config(&self, slug: &ProjectSlug) -> bool {
        self.configured_slugs().iter().any(|s| s == slug)
    }

    /// Register a project (D1/D5 validate; D6 two-state; D4 re-attach).
    fn register(&self, raw_slug: &str) -> Result<(), ServerError> {
        let slug = Self::validate_slug(raw_slug)?;
        let dir = per_slug_data_dir(&self.base_dir, &slug);

        // D6 two-state: the data dir and routing registration are INDEPENDENT
        // facts; branch on BOTH, never collapse to one message.
        let data_exists = db_path(&dir).exists();
        let is_routed = self.is_registered_in_config(&slug);

        if data_exists && is_routed {
            // State A: already registered AND routing -> LOUD ERROR. No silent
            // re-register, no clobber.
            return Err(ServerError::Config(format!(
                "project '{slug}' is already registered and routing; nothing to do \
                 (it is in [[projects]] and its store exists)"
            )));
        }

        if data_exists {
            // State B: data dir survives but the slug was de-registered (D4). The
            // RESTORE path — RE-ATTACH to the preserved store/hash chain. OPEN the
            // existing store; NEVER initialize a fresh one over it.
            let db = db_path(&dir);
            let slug_label = slug.to_string();
            block_projects_sync(async move {
                Store::open(&db, PoolConfig::default())
                    .await
                    .map(|_| ())
                    .map_err(|e| {
                        ServerError::Config(format!(
                            "failed to re-attach preserved store for '{slug_label}': {e}"
                        ))
                    })
            })?;
            println!(
                "re-attached project '{slug}' to its preserved store at {}",
                dir.display()
            );
            eprintln!(
                "re-add to config.toml to resume routing:\n\n[[projects]]\nslug = \"{slug}\"\n"
            );
            return Ok(());
        }

        // State C: fresh registration — no data dir. Create the per-slug tree
        // (FR-C3) and initialize the store (genesis). This branch is reached ONLY
        // when !data_exists, so genesis can never run over preserved data.
        std::fs::create_dir_all(&dir).map_err(|e| {
            ServerError::Config(format!(
                "failed to create data dir for '{slug}' at {}: {e}",
                dir.display()
            ))
        })?;
        std::fs::create_dir_all(dir.join(PROJECT_VECTOR_DIR)).map_err(|e| {
            ServerError::Config(format!("failed to create vector dir for '{slug}': {e}"))
        })?;

        let db = db_path(&dir);
        let slug_label = slug.to_string();
        block_projects_sync(async move {
            Store::open(&db, PoolConfig::default())
                .await
                .map(|_| ())
                .map_err(|e| {
                    ServerError::Config(format!(
                        "failed to initialize store for '{slug_label}': {e}"
                    ))
                })
        })?;

        println!("registered project '{slug}' at {}", dir.display());
        eprintln!("add to config.toml to enable routing:\n\n[[projects]]\nslug = \"{slug}\"\n");
        Ok(())
    }

    /// List registered slugs with a local store-open status field (D3).
    fn list_and_print(&self) -> Result<(), ServerError> {
        for status in self.scan_registered()? {
            let mut line = status.slug.as_str().to_string();
            if let Some(open) = status.store_open {
                line.push_str(if open {
                    "  [store: ok]"
                } else {
                    "  [store: unavailable]"
                });
            }
            println!("{line}");
        }
        Ok(())
    }

    /// Enumerate REGISTERED (routed) projects with a local store-open status (D3).
    ///
    /// "Registered" is read from the `[[projects]]` config — the routing source of
    /// truth — NOT from a directory scan. A directory scan cannot distinguish a
    /// per-slug dir from a path-hash `data_dir` (both are siblings under
    /// `.unimatrix` with charset-valid names), so config is authoritative. For
    /// each configured slug, `store_open` is the operator-side status: `Some(true)`
    /// if its db file is present + readable, `Some(false)` if the dir/db is missing
    /// (e.g. deleted out-of-band). Local filesystem ONLY — NO network/HTTP probe.
    fn scan_registered(&self) -> Result<Vec<ProjectStatus>, ServerError> {
        let mut out: Vec<ProjectStatus> = self
            .configured_slugs()
            .into_iter()
            .map(|slug| {
                let db = db_path(&per_slug_data_dir(&self.base_dir, &slug));
                // D3 cheap status: db-file presence + readability, local-only.
                let store_open = Some(std::fs::File::open(&db).is_ok());
                ProjectStatus { slug, store_open }
            })
            .collect();

        out.sort_by(|a, b| a.slug.as_str().cmp(b.slug.as_str()));
        Ok(out)
    }

    /// De-register (default) or purge (`--purge --confirm <slug>`) a project (D4).
    fn delete(
        &self,
        raw_slug: &str,
        purge: bool,
        confirm: Option<&str>,
    ) -> Result<(), ServerError> {
        // Parse edge again (R-03) — validate before any path join. Charset only
        // (reserved slugs were never registrable, so no data dir exists for them;
        // a reserved name here is simply an invalid target).
        let slug = ProjectSlug::try_from(raw_slug).map_err(|_| {
            ServerError::Config(format!(
                "invalid project slug '{raw_slug}': must match \
                 ^[a-z0-9][a-z0-9-]{{0,62}}$"
            ))
        })?;
        let dir = per_slug_data_dir(&self.base_dir, &slug);

        if !purge {
            // D4 DEFAULT: DE-REGISTER ONLY — non-destructive. PRESERVE the on-disk
            // data dir (DB, vector, HASH CHAIN, analytics). Drop routing intent
            // only; the surviving data dir is what makes register's State-B
            // re-attach possible. No --confirm needed: nothing destructive happens.
            if !dir.exists() {
                return Err(ServerError::Config(format!(
                    "project '{slug}' has no on-disk data at {} — nothing to de-register",
                    dir.display()
                )));
            }
            println!(
                "de-registered project '{slug}' (data preserved at {})",
                dir.display()
            );
            eprintln!(
                "remove the matching [[projects]] stanza from config.toml and restart;\n\
                 data is retained — `project register {slug}` re-attaches it,\n\
                 `project delete {slug} --purge --confirm {slug}` destroys it permanently"
            );
            return Ok(());
        }

        // D4 --purge: DESTROY the on-disk store + hash chain. LOUD. The ONE
        // operation that destroys integrity (drops the unrollbackable chain).
        // Require the operator to RE-TYPE the slug via --confirm <slug>; a bare
        // --purge is NOT enough, and the value must EQUAL the slug exactly.
        if confirm != Some(slug.as_str()) {
            return Err(ServerError::Config(format!(
                "refusing to purge project '{slug}': re-type the slug to confirm.\n\
                 this PERMANENTLY destroys {} including its hash chain (unrollbackable).\n\
                 run: project delete {slug} --purge --confirm {slug}",
                dir.display()
            )));
        }

        if !dir.exists() {
            // Nothing on disk to purge — loud no-op error so the operator is not
            // misled into thinking a chain was destroyed.
            return Err(ServerError::Config(format!(
                "project '{slug}' has no on-disk data to purge at {}",
                dir.display()
            )));
        }

        std::fs::remove_dir_all(&dir).map_err(|e| {
            ServerError::Config(format!(
                "failed to purge project '{slug}' at {}: {e}",
                dir.display()
            ))
        })?;
        println!("purged project '{slug}' — data dir and hash chain permanently destroyed");
        eprintln!("remove the matching [[projects]] stanza from config.toml and restart");
        Ok(())
    }
}

#[cfg(test)]
mod tests;
