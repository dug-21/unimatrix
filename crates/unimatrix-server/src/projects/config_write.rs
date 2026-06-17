//! Atomic, idempotent `[[projects]]` routing-intent write for `register` (ADR-007).
//!
//! `register <slug>` WRITES the `[[projects]] slug = "<slug>"` stanza to
//! `config.toml` instead of printing hand-edit instructions. The write is:
//!
//! - **Atomic** (SR-07 / R-06): a temp file in the SAME dir + `fsync` + atomic
//!   `rename` over `config.toml`. A crash before the rename leaves the OLD complete
//!   file; after it, the NEW complete file — never a partial/corrupt config.
//! - **Idempotent**: a stanza for `slug` already present is a no-op (re-register
//!   never duplicates a stanza).
//! - **Additive / preserving**: a read-modify-write round-trip of the whole TOML
//!   document keeps every other section and existing stanza intact.
//! - **Distroless-safe** (C-05): `std::fs` + the in-tree `toml` lib only — no shell.
//!
//! The `slug` is the `ProjectSlug` newtype (charset-constrained
//! `^[a-z0-9][a-z0-9-]{0,62}$`), so no TOML metacharacter (quote, newline, bracket)
//! can survive into the stanza value — the write side carries no injection surface.

use std::path::Path;

use crate::error::ServerError;
use crate::http::ProjectSlug;

/// Temp-file prefix for the atomic config write. Same-dir sibling of `config.toml`
/// so the rename is intra-filesystem (atomic). Dotted + PID-tagged so it is hidden
/// and collision-free across concurrent provisioning invocations.
const TMP_PREFIX: &str = ".config.toml.";

/// Ensure a `[[projects]] slug = "<slug>"` stanza exists in `config.toml` under
/// `config_data_dir`, written atomically. Idempotent and additive (ADR-007 / R-06).
///
/// A missing `config.toml` starts from an empty document (NOT an error) so the first
/// `register` on a clean deployment writes the first stanza exactly as the Nth does.
/// A malformed existing `config.toml` is a loud error — it is never blindly clobbered.
pub(super) fn ensure_project_stanza(
    config_data_dir: &Path,
    slug: &ProjectSlug,
) -> Result<(), ServerError> {
    let config_path = config_data_dir.join("config.toml");

    // 1. READ the existing config (preserve ALL other config).
    let text = std::fs::read_to_string(&config_path).unwrap_or_default();
    let mut doc: toml::Value = if text.trim().is_empty() {
        toml::Value::Table(toml::value::Table::new())
    } else {
        toml::from_str(&text).map_err(|e| {
            ServerError::Config(format!(
                "config.toml at {} is malformed; refusing to write over it: {e}",
                config_path.display()
            ))
        })?
    };

    // 2. IDEMPOTENCY + read-modify-write: append the stanza only if absent.
    let table = doc.as_table_mut().ok_or_else(|| {
        ServerError::Config(format!(
            "config.toml at {} is not a TOML table",
            config_path.display()
        ))
    })?;
    let projects = table
        .entry("projects")
        .or_insert_with(|| toml::Value::Array(Vec::new()));
    let array = projects.as_array_mut().ok_or_else(|| {
        ServerError::Config(
            "config.toml `projects` is not an array-of-tables; refusing to write".to_string(),
        )
    })?;
    let already_present = array.iter().any(|entry| {
        entry
            .get("slug")
            .and_then(toml::Value::as_str)
            .is_some_and(|s| s == slug.as_str())
    });
    if already_present {
        // Idempotent no-op: re-register of an already-listed slug duplicates
        // nothing and rewrites nothing (R-05/R-06).
        return Ok(());
    }
    let mut stanza = toml::value::Table::new();
    stanza.insert(
        "slug".to_string(),
        toml::Value::String(slug.as_str().to_string()),
    );
    array.push(toml::Value::Table(stanza));

    // 3. SERIALIZE the full document (all prior sections + stanzas preserved).
    let new_text = toml::to_string_pretty(&doc)
        .map_err(|e| ServerError::Config(format!("failed to serialize config.toml: {e}")))?;

    // 4. ATOMIC WRITE (R-06): temp + fsync + rename.
    atomic_write(config_data_dir, &config_path, new_text.as_bytes())
}

/// Atomically replace `target` with `bytes`: write to a uniquely-named temp file in
/// `dir` (the SAME directory as `target`), `fsync` it (durability before the
/// rename), then `rename` over `target` (atomic on a single filesystem). The temp
/// file is cleaned up on any failure so a crashed write never litters a partial
/// sibling.
fn atomic_write(dir: &Path, target: &Path, bytes: &[u8]) -> Result<(), ServerError> {
    use std::io::Write as _;

    let tmp = dir.join(format!("{TMP_PREFIX}{}.tmp", std::process::id()));

    let write_result = (|| -> std::io::Result<()> {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.flush()?;
        f.sync_all()?; // fsync: durable on disk BEFORE the rename
        Ok(())
    })();

    if let Err(e) = write_result {
        let _ = std::fs::remove_file(&tmp);
        return Err(ServerError::Config(format!(
            "failed to write temp config at {}: {e}",
            tmp.display()
        )));
    }

    std::fs::rename(&tmp, target).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        ServerError::Config(format!(
            "failed to atomically install config at {}: {e}",
            target.display()
        ))
    })
}
