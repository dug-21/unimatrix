//! C7 scoped-token actions of `client-bundle` (vnc-050 ADR-001/005): headless
//! mint, `--list`, `--revoke`. All output is stderr except the minted bundle blob
//! (stdout). No secret or hash is ever printed outside that blob.

use std::path::Path;

use super::{Trust, emit_for, resolve_fp};
use crate::error::ServerError;
use crate::http::ProjectSlug;
use crate::http::principal::TokenMode;
use crate::http::token_table::TokenTable;
use crate::projects::slug_store::is_registered_slug;

/// `--headless`: mint a scoped token bound to `slug` and emit it in a bundle.
///
/// Order: registration check, then the cert fingerprint (pin trust), THEN mint, so
/// an unregistered slug or a missing cert never leaves an orphan record.
pub(super) fn run_mint(
    data_dir: &Path,
    slug: &ProjectSlug,
    mode: TokenMode,
    label: &str,
    trust: Trust,
) -> Result<(), ServerError> {
    ensure_registered(data_dir, slug)?;
    let fp = resolve_fp(data_dir, trust)?;
    let secret = TokenTable::open(data_dir).mint(label, mode, slug)?;
    let token_hex = hex::encode(secret);
    emit_for(slug, &token_hex, fp.as_deref(), Some((label, mode))).map_err(|e| {
        ServerError::Config(format!(
            "token '{label}' was minted but the bundle could not be emitted ({e}); \
             revoke it with --revoke {label}"
        ))
    })
}

fn ensure_registered(data_dir: &Path, slug: &ProjectSlug) -> Result<(), ServerError> {
    if is_registered_slug(data_dir, slug) {
        Ok(())
    } else {
        Err(ServerError::Config(format!(
            "project '{slug}' is not registered; run `unimatrix project register {slug}` first"
        )))
    }
}

/// `--list`: stderr table of the slug's scoped tokens (revoked included).
pub(super) fn run_list(data_dir: &Path, slug: &ProjectSlug) -> Result<(), ServerError> {
    let rows = TokenTable::open(data_dir).list_for_checked(slug)?;
    eprintln!(
        "{:<32} {:<10} {:<24} {:<20} STATE",
        "LABEL", "MODE", "SLUGS", "CREATED"
    );
    for r in &rows {
        let slugs: Vec<&str> = r.allowed_slugs.iter().map(ProjectSlug::as_str).collect();
        let state = match r.revoked_at {
            Some(t) => format!("revoked {}", rfc3339(t)),
            None => "active".to_string(),
        };
        eprintln!(
            "{:<32} {:<10} {:<24} {:<20} {state}",
            r.label,
            r.mode.as_wire(),
            slugs.join(","),
            rfc3339(r.created_at)
        );
    }
    if rows.is_empty() {
        eprintln!("(no scoped tokens for {slug})");
    }
    Ok(())
}

/// `--revoke`: tombstone the slug's token `label`. Errors leave the file unchanged.
pub(super) fn run_revoke(
    data_dir: &Path,
    slug: &ProjectSlug,
    label: &str,
) -> Result<(), ServerError> {
    TokenTable::open(data_dir).revoke(label, slug)?;
    eprintln!("revoked token '{label}' for {slug}; next request using it gets 401");
    Ok(())
}

/// Unix seconds → `YYYY-MM-DDTHH:MM:SSZ` (UTC, civil-from-days).
pub(super) fn rfc3339(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem / 60) % 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::rfc3339;

    #[test]
    fn test_rfc3339_epoch_and_known_dates() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(rfc3339(1_700_000_000), "2023-11-14T22:13:20Z");
    }
}
