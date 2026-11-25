//! C7 `client-bundle` flag surface (vnc-050 ADR-005).
//!
//! One command, four shapes: legacy bundle, headless mint, `--list`, `--revoke`.
//! clap enforces the flag matrix first (usage error, exit 2, before any I/O);
//! [`ClientBundleArgs::action`] re-checks it purely so the dispatcher can never
//! reach an I/O path with an invalid combination.

use crate::http::principal::TokenMode;
use crate::http::token_table::is_label;

/// Emit the per-project client connection bundle (vnc-038 C1/ADR-002, vnc-050 ADR-005).
///
/// Without flags: reads the provisioned bearer token + served leaf cert, composes
/// the per-slug MCP + observe URLs, and prints the opaque `unimatrix-bundle:` `v:2`
/// blob to stdout (pipeable) with a token-redacted echo on stderr.
///
/// `--headless --mode M --label L` mints a scoped token bound to `<slug>` and emits
/// it inside the bundle (shown once). `--trust ca` emits `fp: null` (CA trust, no
/// pin). `--list` / `--revoke L` manage the slug's scoped tokens. Sync, pre-tokio.
#[derive(clap::Args, Debug, Clone, PartialEq, Eq)]
pub struct ClientBundleArgs {
    /// Registered project slug the bundle targets (`^[a-z0-9][a-z0-9-]{0,62}$`).
    pub slug: String,
    /// Mint a scoped headless token and emit it in the bundle (requires --mode, --label).
    #[arg(long, requires_all = ["mode", "label"], conflicts_with_all = ["list", "revoke"])]
    pub headless: bool,
    /// Scoped token mode (with --headless).
    #[arg(long, value_parser = ["read", "read-write"], requires = "headless")]
    pub mode: Option<String>,
    /// Scoped token label, unique forever (with --headless).
    #[arg(long, requires = "headless")]
    pub label: Option<String>,
    /// Server trust: `pin` (default, cert fingerprint) or `ca` (fp null).
    #[arg(long, value_parser = ["pin", "ca"], conflicts_with_all = ["list", "revoke"])]
    pub trust: Option<String>,
    /// List the slug's scoped tokens (stderr table; no secrets).
    #[arg(long, conflicts_with_all = ["headless", "mode", "label", "trust", "revoke"])]
    pub list: bool,
    /// Tombstone-revoke the slug's scoped token with this label.
    #[arg(
        long,
        value_name = "LABEL",
        conflicts_with_all = ["headless", "mode", "label", "trust", "list"]
    )]
    pub revoke: Option<String>,
}

/// Bundle trust mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Trust {
    /// Pin the served leaf cert fingerprint.
    Pin,
    /// No pin; the server cert must chain to a trusted CA (`fp: null`).
    Ca,
}

/// The validated command shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Action {
    Legacy {
        trust: Trust,
    },
    Mint {
        mode: TokenMode,
        label: String,
        trust: Trust,
    },
    List,
    Revoke {
        label: String,
    },
}

/// Flag-matrix violation (exit 2; never a `ServerError`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UsageError(pub(crate) String);

/// One-line usage printed with every [`UsageError`].
pub(crate) const USAGE: &str = "usage: unimatrix client-bundle <slug> \
    [--headless --mode read|read-write --label <L>] [--trust pin|ca] | --list | --revoke <L>";

fn usage(msg: &str) -> UsageError {
    UsageError(msg.to_string())
}

impl ClientBundleArgs {
    /// Re-check the flag matrix (clap enforces it first). Pure; no I/O.
    pub(crate) fn action(&self) -> Result<Action, UsageError> {
        let trust = match self.trust.as_deref() {
            None | Some("pin") => Trust::Pin,
            Some("ca") => Trust::Ca,
            Some(_) => return Err(usage("--trust must be pin or ca")),
        };
        let extras = self.mode.is_some() || self.label.is_some() || self.trust.is_some();
        match (self.headless, self.list, self.revoke.as_ref()) {
            (true, false, None) => {
                let mode = self
                    .mode
                    .as_deref()
                    .and_then(TokenMode::parse_wire)
                    .ok_or_else(|| usage("--headless requires --mode read|read-write"))?;
                let label = self
                    .label
                    .clone()
                    .ok_or_else(|| usage("--headless requires --label <name>"))?;
                if !is_label(&label) {
                    return Err(usage("--label must match ^[a-z0-9][a-z0-9-]{0,62}$"));
                }
                Ok(Action::Mint { mode, label, trust })
            }
            (false, true, None) if extras => Err(usage("--list takes no --mode/--label/--trust")),
            (false, true, None) => Ok(Action::List),
            (false, false, Some(_)) if extras => {
                Err(usage("--revoke takes no --mode/--label/--trust"))
            }
            (false, false, Some(l)) => Ok(Action::Revoke { label: l.clone() }),
            (false, false, None) if self.mode.is_some() || self.label.is_some() => {
                Err(usage("--mode/--label require --headless"))
            }
            (false, false, None) => Ok(Action::Legacy { trust }),
            _ => Err(usage(
                "--headless, --list and --revoke are mutually exclusive",
            )),
        }
    }
}

#[cfg(test)]
#[path = "args_tests.rs"]
mod tests;
