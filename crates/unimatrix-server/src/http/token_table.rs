//! Scoped token table `{data_dir}/tokens.json` (vnc-050 C1, ADR-001).
//!
//! Read side here (strict parse, per-request stat-stamp snapshot,
//! constant-time full scan); writers in `write`. Hashes at rest (SHA-256 of the raw 32 bytes);
//! revoked records are tombstones that still participate in the scan. A file
//! that exists but does not parse fails closed — no last-good snapshot.

use std::collections::HashSet;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::{Choice, ConditionallySelectable, ConstantTimeEq};

use crate::http::principal::TokenMode;
use crate::http::router::ProjectSlug;

pub(crate) const TOKENS_FILE: &str = "tokens.json";
const FILE_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TokenFile {
    pub(crate) v: u32,
    pub(crate) tokens: Vec<TokenRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TokenRecord {
    pub(crate) label: String,
    pub(crate) sha256: String,
    pub(crate) mode: ModeWire,
    pub(crate) allowed_slugs: Vec<String>,
    pub(crate) created_at: u64,
    pub(crate) revoked_at: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ModeWire {
    Read,
    ReadWrite,
}

impl From<ModeWire> for TokenMode {
    fn from(m: ModeWire) -> Self {
        match m {
            ModeWire::Read => TokenMode::Read,
            ModeWire::ReadWrite => TokenMode::ReadWrite,
        }
    }
}

/// A resolved (non-secret) table entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScopedEntry {
    pub(crate) label: String,
    pub(crate) mode: TokenMode,
    pub(crate) allowed_slugs: Vec<ProjectSlug>,
    pub(crate) revoked: bool,
}

#[derive(Debug)]
struct ParsedEntry {
    hash: [u8; 32],
    entry: ScopedEntry,
}

/// Parse failure. Carries only a reason, never a hash value.
#[derive(Debug)]
pub(crate) struct ParseError(String);

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
struct Stamp {
    mtime_ns: i128,
    len: u64,
    inode: u64,
}

#[derive(Debug)]
enum SnapshotState {
    Missing,
    Loaded(Vec<ParsedEntry>),
    Corrupt,
}

#[derive(Debug)]
struct Snapshot {
    stamp: Option<Stamp>,
    state: SnapshotState,
}

/// Label grammar `^[a-z0-9][a-z0-9-]{0,62}$` — identical to `ProjectSlug`.
pub(crate) fn is_label(s: &str) -> bool {
    ProjectSlug::try_from(s).is_ok()
}

fn decode_lower_hex_32(s: &str) -> Result<[u8; 32], ParseError> {
    let ok = s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if !ok {
        return Err(ParseError("sha256 must be 64 lowercase hex chars".into()));
    }
    let mut out = [0u8; 32];
    hex::decode_to_slice(s, &mut out).map_err(|_| ParseError("sha256 is not hex".into()))?;
    Ok(out)
}

fn parse_file(bytes: &[u8]) -> Result<Vec<ParsedEntry>, ParseError> {
    let file: TokenFile = serde_json::from_slice(bytes)
        .map_err(|e| ParseError(format!("malformed tokens.json: {e}")))?;
    if file.v != FILE_VERSION {
        return Err(ParseError(format!("unsupported version {}", file.v)));
    }
    let mut seen: HashSet<String> = HashSet::new();
    let mut entries = Vec::with_capacity(file.tokens.len());
    for rec in file.tokens {
        if !is_label(&rec.label) {
            return Err(ParseError("invalid label".into()));
        }
        if !seen.insert(rec.label.clone()) {
            return Err(ParseError(format!("duplicate label {}", rec.label)));
        }
        if rec.allowed_slugs.is_empty() {
            return Err(ParseError(format!("empty allowed_slugs for {}", rec.label)));
        }
        let slugs = rec
            .allowed_slugs
            .iter()
            .map(|s| ProjectSlug::try_from(s.as_str()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ParseError(format!("invalid slug for {}", rec.label)))?;
        let hash = decode_lower_hex_32(&rec.sha256)?;
        entries.push(ParsedEntry {
            hash,
            entry: ScopedEntry {
                label: rec.label,
                mode: rec.mode.into(),
                allowed_slugs: slugs,
                revoked: rec.revoked_at.is_some(),
            },
        });
    }
    Ok(entries)
}

/// Persistent scoped-token store with a stat-stamped snapshot.
#[derive(Debug)]
pub(crate) struct TokenTable {
    data_dir: PathBuf,
    snap: RwLock<Snapshot>,
}

impl TokenTable {
    /// Open the table under `data_dir`. Never fails: a missing file is an
    /// empty table; a corrupt one fails closed (logged).
    pub(crate) fn open(data_dir: &Path) -> Self {
        let t = TokenTable {
            data_dir: data_dir.to_path_buf(),
            snap: RwLock::new(Snapshot {
                stamp: None,
                state: SnapshotState::Missing,
            }),
        };
        t.refresh();
        t
    }

    fn path(&self) -> PathBuf {
        self.data_dir.join(TOKENS_FILE)
    }

    fn stat_stamp(&self) -> Result<Option<Stamp>, io::Error> {
        match std::fs::metadata(self.path()) {
            Ok(m) => Ok(Some(Stamp {
                mtime_ns: i128::from(m.mtime()) * 1_000_000_000 + i128::from(m.mtime_nsec()),
                len: m.len(),
                inode: m.ino(),
            })),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Reload the snapshot iff the file stamp changed.
    fn refresh(&self) {
        let stamp = match self.stat_stamp() {
            Ok(s) => s,
            Err(e) => {
                let mut w = self.snap.write().unwrap_or_else(|p| p.into_inner());
                let already = w.stamp.is_none() && matches!(w.state, SnapshotState::Corrupt);
                *w = Snapshot {
                    stamp: None,
                    state: SnapshotState::Corrupt,
                };
                if !already {
                    tracing::error!(error = %e, "tokens.json stat failed; scoped tokens fail closed");
                }
                return;
            }
        };
        {
            let r = self.snap.read().unwrap_or_else(|p| p.into_inner());
            let stat_err_state = r.stamp.is_none() && matches!(r.state, SnapshotState::Corrupt);
            if r.stamp == stamp && !stat_err_state {
                return;
            }
        }
        let mut w = self.snap.write().unwrap_or_else(|p| p.into_inner());
        let stat_err_state = w.stamp.is_none() && matches!(w.state, SnapshotState::Corrupt);
        if w.stamp == stamp && !stat_err_state {
            return; // another thread reloaded
        }
        *w = match stamp {
            None => Snapshot {
                stamp: None,
                state: SnapshotState::Missing,
            },
            Some(st) => {
                let parsed = std::fs::read(self.path())
                    .map_err(|e| ParseError(format!("read failed: {e}")))
                    .and_then(|b| parse_file(&b));
                match parsed {
                    Ok(entries) => Snapshot {
                        stamp: Some(st),
                        state: SnapshotState::Loaded(entries),
                    },
                    Err(e) => {
                        // Logged once per stamp: only reached on a stamp change.
                        tracing::error!(reason = %e, "tokens.json invalid; scoped tokens fail closed");
                        Snapshot {
                            stamp: Some(st),
                            state: SnapshotState::Corrupt,
                        }
                    }
                }
            }
        };
    }

    /// Single scan loop body: constant-time compare against EVERY entry,
    /// tombstones included. No early exit; `visits` is a plain counter.
    fn scan(&self, presented: &[u8; 32], visits: &mut usize) -> Option<ScopedEntry> {
        self.refresh();
        let digest: [u8; 32] = Sha256::digest(presented).into();
        let r = self.snap.read().unwrap_or_else(|p| p.into_inner());
        let entries = match &r.state {
            SnapshotState::Loaded(e) => e,
            SnapshotState::Missing | SnapshotState::Corrupt => return None,
        };
        let mut found = Choice::from(0u8);
        let mut idx: u64 = 0;
        for (i, pe) in entries.iter().enumerate() {
            let eq = pe.hash.ct_eq(&digest);
            idx = u64::conditional_select(&idx, &(i as u64), eq);
            found |= eq;
            *visits += 1;
        }
        if !bool::from(found) {
            return None;
        }
        let e = entries.get(usize::try_from(idx).ok()?)?.entry.clone();
        // Decided after the full scan; caller maps to the identical 401.
        if e.revoked { None } else { Some(e) }
    }

    /// Constant-time lookup of a presented raw token.
    pub(crate) fn lookup_ct(&self, presented: &[u8; 32]) -> Option<ScopedEntry> {
        self.scan(presented, &mut 0)
    }

    /// Test seam: lookup plus the number of entries the scan visited.
    #[cfg(test)]
    pub(crate) fn lookup_ct_counted(&self, presented: &[u8; 32]) -> (Option<ScopedEntry>, usize) {
        let mut visits = 0;
        let r = self.scan(presented, &mut visits);
        (r, visits)
    }
}

mod write;
#[allow(unused_imports)] // rationale: consumed by C7 `client-bundle tokens list` (W2).
pub(crate) use write::EntryView;

/// Test seam (OVERVIEW §Test-Only Seams): called by `atomic_write` right
/// after the tmp file is created, before any bytes are written.
#[cfg(test)]
#[allow(clippy::type_complexity)]
pub(crate) static AFTER_TMP_CREATE: std::sync::Mutex<Option<Box<dyn Fn(&Path) + Send>>> =
    std::sync::Mutex::new(None);

#[cfg(test)]
pub(crate) mod tests;
