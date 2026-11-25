//! C1 write side (vnc-050 ADR-001): mint, tombstone revoke, listing.
//!
//! Only the `client-bundle` CLI (C7) writes; the server never does. Every
//! mutation runs under an exclusive `flock` on `{data_dir}/tokens.lock` and
//! lands via tmp (0600, created exclusively) + fsync + rename, so readers see
//! either the old or the new file, never a partial one. Error paths never
//! write, leaving the file byte-identical.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::time::{SystemTime, UNIX_EPOCH};

use fs2::FileExt;
use rand::TryRngCore;
use sha2::{Digest, Sha256};

use super::{FILE_VERSION, ModeWire, TokenFile, TokenRecord, TokenTable, is_label, parse_file};
use crate::error::ServerError;
use crate::http::principal::TokenMode;
use crate::http::router::ProjectSlug;

pub(crate) const TOKENS_TMP: &str = "tokens.json.tmp";
pub(crate) const TOKENS_LOCK: &str = "tokens.lock";

/// Non-secret listing row for one token (no hash).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EntryView {
    pub(crate) label: String,
    pub(crate) mode: TokenMode,
    pub(crate) allowed_slugs: Vec<ProjectSlug>,
    pub(crate) created_at: u64,
    pub(crate) revoked_at: Option<u64>,
}

fn io_err(what: &str, e: io::Error) -> ServerError {
    ServerError::Config(format!("tokens.json {what} failed: {e}"))
}

fn invalid(reason: &str) -> ServerError {
    ServerError::InvalidInput {
        field: "label".to_string(),
        reason: reason.to_string(),
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl From<TokenMode> for ModeWire {
    fn from(m: TokenMode) -> Self {
        match m {
            TokenMode::Read => ModeWire::Read,
            TokenMode::ReadWrite => ModeWire::ReadWrite,
        }
    }
}

/// Read + strictly validate the current file. Missing = empty v1 file.
fn read_current(table: &TokenTable) -> Result<TokenFile, ServerError> {
    let corrupt = || {
        ServerError::Config(
            "tokens.json is invalid; fix or restore it before minting/revoking".into(),
        )
    };
    match std::fs::read(table.path()) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(TokenFile {
            v: FILE_VERSION,
            tokens: Vec::new(),
        }),
        Err(e) => Err(io_err("read", e)),
        Ok(b) => {
            parse_file(&b).map_err(|_| corrupt())?;
            serde_json::from_slice::<TokenFile>(&b).map_err(|_| corrupt())
        }
    }
}

impl TokenTable {
    /// Run `f` on the current file under the exclusive lock; write its
    /// returned file (if any) atomically. Lock released on drop.
    fn with_lock<T>(
        &self,
        f: impl FnOnce(TokenFile) -> Result<(Option<TokenFile>, T), ServerError>,
    ) -> Result<T, ServerError> {
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .open(self.data_dir.join(TOKENS_LOCK))
            .map_err(|e| io_err("lock open", e))?;
        lock.lock_exclusive().map_err(|e| io_err("lock", e))?;
        let current = read_current(self)?;
        let (new_file, out) = f(current)?;
        if let Some(nf) = new_file {
            self.atomic_write(&nf)?;
        }
        drop(lock);
        Ok(out)
    }

    fn atomic_write(&self, file: &TokenFile) -> Result<(), ServerError> {
        let mut bytes = serde_json::to_vec_pretty(file)
            .map_err(|e| ServerError::Config(format!("tokens.json serialize failed: {e}")))?;
        bytes.push(b'\n');
        let tmp = self.data_dir.join(TOKENS_TMP);
        match std::fs::remove_file(&tmp) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => {
                return Err(io_err("stale tmp remove", e));
            }
            _ => {}
        }
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(|e| io_err("tmp create", e))?;
        #[cfg(test)]
        if let Some(hook) = super::AFTER_TMP_CREATE
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
        {
            hook(&tmp);
        }
        let res = f
            .write_all(&bytes)
            .and_then(|()| f.sync_all())
            .and_then(|()| std::fs::rename(&tmp, self.path()));
        if let Err(e) = res {
            let _ = std::fs::remove_file(&tmp);
            return Err(io_err("write", e));
        }
        if let Err(e) = File::open(&self.data_dir).and_then(|d| d.sync_all()) {
            tracing::warn!(error = %e, "tokens.json directory fsync failed");
        }
        Ok(())
    }

    /// Mint a new token bound to `slug`. Returns the raw 32 bytes; only the
    /// SHA-256 is stored. Labels are never reusable (tombstones included).
    pub(crate) fn mint(
        &self,
        label: &str,
        mode: TokenMode,
        slug: &ProjectSlug,
    ) -> Result<[u8; 32], ServerError> {
        if !is_label(label) {
            return Err(invalid("must match ^[a-z0-9][a-z0-9-]{0,62}$"));
        }
        let mut secret = [0u8; 32];
        rand::rngs::OsRng
            .try_fill_bytes(&mut secret)
            .map_err(|e| ServerError::Config(format!("OS RNG unavailable: {e}")))?;
        let sha256 = hex::encode(Sha256::digest(secret));
        self.with_lock(|mut file| {
            if file.tokens.iter().any(|r| r.label == label) {
                return Err(invalid("label already exists (labels are never reusable)"));
            }
            file.tokens.push(TokenRecord {
                label: label.to_string(),
                sha256,
                mode: mode.into(),
                allowed_slugs: vec![slug.as_str().to_string()],
                created_at: now_secs(),
                revoked_at: None,
            });
            Ok((Some(file), ()))
        })?;
        Ok(secret)
    }

    /// Tombstone-revoke `label` within `slug`. The record is kept.
    pub(crate) fn revoke(&self, label: &str, slug: &ProjectSlug) -> Result<(), ServerError> {
        self.with_lock(|mut file| {
            let rec = file
                .tokens
                .iter_mut()
                .find(|r| r.label == label)
                .ok_or_else(|| invalid("no token with that label"))?;
            if !rec.allowed_slugs.iter().any(|s| s == slug.as_str()) {
                return Err(invalid("no token with that label for this project"));
            }
            if rec.revoked_at.is_some() {
                return Err(invalid("token already revoked"));
            }
            rec.revoked_at = Some(now_secs());
            Ok((Some(file), ()))
        })
    }

    /// Tokens bound to `slug` (revoked included). Corrupt file -> `Err`.
    pub(crate) fn list_for_checked(
        &self,
        slug: &ProjectSlug,
    ) -> Result<Vec<EntryView>, ServerError> {
        let file = read_current(self)?;
        let mut out = Vec::new();
        for r in file.tokens {
            if !r.allowed_slugs.iter().any(|s| s == slug.as_str()) {
                continue;
            }
            let allowed_slugs = r
                .allowed_slugs
                .iter()
                .filter_map(|s| ProjectSlug::try_from(s.as_str()).ok())
                .collect();
            out.push(EntryView {
                label: r.label,
                mode: r.mode.into(),
                allowed_slugs,
                created_at: r.created_at,
                revoked_at: r.revoked_at,
            });
        }
        Ok(out)
    }

    /// Infallible listing; a corrupt file yields empty (logged). Test-only:
    /// the CLI uses [`Self::list_for_checked`] so corruption is reported.
    #[cfg(test)]
    pub(crate) fn list_for(&self, slug: &ProjectSlug) -> Vec<EntryView> {
        self.list_for_checked(slug).unwrap_or_else(|e| {
            tracing::error!(error = %e, "tokens.json list failed");
            Vec::new()
        })
    }
}

#[cfg(test)]
#[path = "write_tests.rs"]
mod tests;
