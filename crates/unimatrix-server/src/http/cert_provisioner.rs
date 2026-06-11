//! CertProvisioner (vnc-034) — `load_or_generate_cert`.
//!
//! First-boot self-signed cert + key provisioner for the OSS fingerprint-pinning
//! trust model (ADR-002). On first boot, generates a self-signed leaf into
//! `{data_dir}/tls/{cert.pem,key.pem}` with production SANs (from
//! [`derive_public_url`](super::public_url::derive_public_url), C3), a bounded
//! validity window, and key mode `0600`. On every subsequent boot, the existing
//! pair is LOADED byte-identically — a regenerated cert would silently invalidate
//! every pinned client and the emitted bundle (R-07, SR-01).
//!
//! Mirrors `token::load_or_generate_token`'s generate-first +
//! `O_CREAT | O_EXCL` persistence (mode `0600`) so two concurrent first boots on
//! one volume converge on a single credential rather than racing to two distinct
//! certs (R-07).
//!
//! Promoted from the test-only `generate_simple_self_signed` helper that
//! previously lived in `tls.rs`: production params are explicit here (SANs from
//! C3, bounded validity, key 0600), never rcgen's unbounded test defaults (R-08).

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::net::IpAddr;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rcgen::{CertificateParams, KeyPair, SanType, date_time_ymd};

use crate::error::ServerError;

/// PEM byte buffer for the certificate (same shape as the prior test helper).
pub type CertPem = Vec<u8>;
/// PEM byte buffer for the private key.
pub type KeyPem = Vec<u8>;

/// Subdirectory of `data_dir` holding the TLS material.
const CERT_DIR_NAME: &str = "tls";
/// Public certificate file name.
const CERT_FILE_NAME: &str = "cert.pem";
/// Private key file name.
const KEY_FILE_NAME: &str = "key.pem";
/// Mode for the private key file: owner read/write only.
const KEY_FILE_MODE: u32 = 0o600;
/// Mode for the public certificate file: world-readable.
const CERT_FILE_MODE: u32 = 0o644;
/// Bounded, defined validity window (SR-01/R-08). Not rcgen's unbounded default.
const CERT_VALIDITY_DAYS: i64 = 825;

/// Load an existing TLS cert/key pair, or generate a self-signed one on first boot.
///
/// Returns the PEM byte buffers `(cert, key)`. Side effect on first boot:
/// `{data_dir}/tls/{cert,key}.pem` written to disk, the key with mode `0600`.
///
/// Idempotent: when both files already exist they are loaded verbatim and never
/// regenerated (R-07), so an operator-mounted override pair is honored, not
/// overwritten (FR-A3). A partial state (exactly one file present) is a loud
/// error — never a silent half-state serving a mismatched pair.
///
/// # Errors
///
/// - [`ServerError::ProjectInit`] when `{data_dir}/tls` cannot be created or the
///   key cannot be written — the message names the path and the UID-65532
///   writability fix (R-11, FR-A9). No panic, no `.unwrap()`.
/// - [`ServerError::Config`] when existing material is empty/unreadable, exactly
///   one of the pair is present (incomplete material), or rcgen fails.
pub fn load_or_generate_cert(
    data_dir: &Path,
    sans: &[String],
) -> Result<(CertPem, KeyPem), ServerError> {
    let tls_dir = data_dir.join(CERT_DIR_NAME);
    let cert_path = tls_dir.join(CERT_FILE_NAME);
    let key_path = tls_dir.join(KEY_FILE_NAME);

    // Step 0: ensure tls/ exists; fail loud-and-actionable if /data is unwritable.
    fs::create_dir_all(&tls_dir).map_err(|e| {
        ServerError::ProjectInit(format!(
            "cannot create TLS directory {}: {e}. Ensure /data is writable by UID 65532 \
             (see container bind-mount docs).",
            tls_dir.display()
        ))
    })?;

    let cert_exists = cert_path.exists();
    let key_exists = key_path.exists();

    // Step 1: both present -> LOAD verbatim (idempotence + operator override).
    if cert_exists && key_exists {
        return load_existing(&cert_path, &key_path);
    }

    // Step 2: partial state (exactly one present) -> fail loud, never regenerate.
    if cert_exists != key_exists {
        let (present, missing) = if cert_exists {
            (CERT_FILE_NAME, KEY_FILE_NAME)
        } else {
            (KEY_FILE_NAME, CERT_FILE_NAME)
        };
        return Err(ServerError::Config(format!(
            "incomplete TLS material in {}: {present} present but {missing} missing — \
             remove both to regenerate, or supply both (override).",
            tls_dir.display()
        )));
    }

    // Step 3: neither present -> generate. The KEY is the atomic ownership claim:
    // write it first with O_CREAT|O_EXCL|0600, so exactly one racing boot wins. Only
    // the winner then writes the matching cert, guaranteeing the cert/key on disk are
    // a pair (R-07). A loser observes AlreadyExists and loads the winner's pair.
    let (cert_pem, key_pem) = generate_self_signed_production(sans)?;

    match write_excl_mode(&key_path, &key_pem, KEY_FILE_MODE) {
        Ok(()) => {
            // We own the credential — publish the matching cert.
            write_with_mode(&cert_path, &cert_pem, CERT_FILE_MODE).map_err(|e| {
                // Roll back our key so no half-state (key without cert) persists.
                let _ = fs::remove_file(&key_path);
                ServerError::ProjectInit(format!(
                    "cannot write TLS certificate {}: {e}. Ensure /data is writable by UID 65532.",
                    cert_path.display()
                ))
            })?;
            Ok((cert_pem, key_pem))
        }
        // A racing first boot won the EXCL key create: adopt the winner's pair.
        // The winner publishes the cert immediately after claiming the key; retry
        // the load briefly to close the tiny key-then-cert window.
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            load_existing_with_retry(&cert_path, &key_path)
        }
        Err(e) => Err(ServerError::ProjectInit(format!(
            "cannot write TLS key {}: {e}. Ensure /data is writable by UID 65532.",
            key_path.display()
        ))),
    }
}

/// Load both files, retrying briefly while the racing winner finishes writing the
/// cert it publishes right after claiming the key (bounded, never blocks a real
/// missing-file error indefinitely).
fn load_existing_with_retry(
    cert_path: &Path,
    key_path: &Path,
) -> Result<(CertPem, KeyPem), ServerError> {
    const MAX_ATTEMPTS: u32 = 50; // ~500ms ceiling
    for attempt in 0..MAX_ATTEMPTS {
        if cert_path.exists() {
            return load_existing(cert_path, key_path);
        }
        if attempt + 1 < MAX_ATTEMPTS {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    // Cert never appeared — surface the real error from a final read attempt.
    load_existing(cert_path, key_path)
}

/// Load both PEM files verbatim, asserting they are non-empty and re-asserting
/// the key mode `0600` (defensive: an override may have arrived looser).
fn load_existing(cert_path: &Path, key_path: &Path) -> Result<(CertPem, KeyPem), ServerError> {
    let cert_pem = fs::read(cert_path).map_err(|e| {
        ServerError::Config(format!(
            "cannot read TLS certificate {}: {e}",
            cert_path.display()
        ))
    })?;
    let key_pem = fs::read(key_path).map_err(|e| {
        ServerError::Config(format!("cannot read TLS key {}: {e}", key_path.display()))
    })?;

    if cert_pem.is_empty() {
        return Err(ServerError::Config(format!(
            "existing TLS certificate is empty: {}",
            cert_path.display()
        )));
    }
    if key_pem.is_empty() {
        return Err(ServerError::Config(format!(
            "existing TLS key is empty: {}",
            key_path.display()
        )));
    }

    best_effort_chmod_key(key_path);
    tracing::debug!(
        "loaded existing TLS material from {}",
        cert_path.parent().unwrap_or(cert_path).display()
    );
    Ok((cert_pem, key_pem))
}

/// Re-assert key mode `0600` on load. Best-effort: a non-unix target or a
/// read-only override mount cannot be chmodded — that is not fatal.
fn best_effort_chmod_key(key_path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = fs::set_permissions(key_path, fs::Permissions::from_mode(KEY_FILE_MODE)) {
            tracing::debug!(
                "could not re-assert 0600 on {} (likely a read-only override): {e}",
                key_path.display()
            );
        }
    }
    #[cfg(not(unix))]
    {
        let _ = key_path;
    }
}

/// Generate a self-signed leaf with production params (R-08): SANs from C3,
/// bounded validity, single self-signed leaf (no CA chain).
fn generate_self_signed_production(sans: &[String]) -> Result<(CertPem, KeyPem), ServerError> {
    let mut params = CertificateParams::new(sans.to_vec())
        .map_err(|e| ServerError::Config(format!("TLS cert params: {e}")))?;

    // Classify each SAN: IP literals as IpAddress, everything else as DnsName.
    let mut san_types: Vec<SanType> = Vec::with_capacity(sans.len());
    for s in sans {
        let san = match s.parse::<IpAddr>() {
            Ok(ip) => SanType::IpAddress(ip),
            Err(_) => {
                let dns = s
                    .clone()
                    .try_into()
                    .map_err(|e| ServerError::Config(format!("invalid DNS SAN {s:?}: {e}")))?;
                SanType::DnsName(dns)
            }
        };
        san_types.push(san);
    }
    params.subject_alt_names = san_types;

    set_validity_window(&mut params)?;

    let key_pair =
        KeyPair::generate().map_err(|e| ServerError::Config(format!("TLS key generation: {e}")))?;
    let cert = params
        .self_signed(&key_pair)
        .map_err(|e| ServerError::Config(format!("TLS self-sign: {e}")))?;

    Ok((
        cert.pem().into_bytes(),
        key_pair.serialize_pem().into_bytes(),
    ))
}

/// Set `not_before`/`not_after` to a bounded, date-granular UTC window:
/// `not_before` = today (UTC midnight), `not_after` = today + `CERT_VALIDITY_DAYS`.
///
/// Day granularity keeps the window deterministic and lets us avoid naming the
/// `time` crate (an rcgen transitive, not a direct dep): we derive Y-M-D from
/// the Unix epoch day and feed rcgen's re-exported [`date_time_ymd`], assigning
/// its `OffsetDateTime` results straight into the params fields by inference.
fn set_validity_window(params: &mut CertificateParams) -> Result<(), ServerError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| ServerError::Config(format!("system clock before Unix epoch: {e}")))?;
    let epoch_day = (now.as_secs() / 86_400) as i64;

    let (ny, nm, nd) = civil_from_days(epoch_day);
    let (ay, am, ad) = civil_from_days(epoch_day + CERT_VALIDITY_DAYS);

    params.not_before = date_time_ymd(ny, nm, nd);
    params.not_after = date_time_ymd(ay, am, ad);
    Ok(())
}

/// Convert a count of days since the Unix epoch (1970-01-01) to a proleptic
/// Gregorian `(year, month, day)` via Howard Hinnant's `civil_from_days`
/// algorithm. Pure integer arithmetic — total, never panics.
fn civil_from_days(z: i64) -> (i32, u8, u8) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u8; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u8; // [1, 12]
    let year = (y + if m <= 2 { 1 } else { 0 }) as i32;
    (year, m, d)
}

/// Write `bytes` to `path` with the given Unix mode, truncating if it exists.
/// Used for the public certificate (mode `0644`).
fn write_with_mode(path: &Path, bytes: &[u8], mode: u32) -> io::Result<()> {
    let mut opts = OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(mode);
    }
    #[cfg(not(unix))]
    {
        let _ = mode;
    }
    let mut file = opts.open(path)?;
    if let Err(e) = file.write_all(bytes) {
        let _ = fs::remove_file(path);
        return Err(e);
    }
    Ok(())
}

/// Atomically create `path` with `O_CREAT | O_EXCL` and the given mode, then
/// write `bytes`. Returns `AlreadyExists` when a concurrent creator won the
/// race. Cleans up a 0-byte file if the write fails. Used for the private key
/// (mode `0600`) so the mode is set at creation — no chmod window.
fn write_excl_mode(path: &Path, bytes: &[u8], mode: u32) -> io::Result<()> {
    let mut file = create_excl(path, mode)?;
    if let Err(e) = file.write_all(bytes) {
        let _ = fs::remove_file(path);
        return Err(e);
    }
    Ok(())
}

/// Open `path` with `O_CREAT | O_EXCL` and the given mode.
fn create_excl(path: &Path, mode: u32) -> io::Result<File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .open(path)
    }
    #[cfg(not(unix))]
    {
        let _ = mode;
        OpenOptions::new().write(true).create_new(true).open(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::public_url::{Env, derive_public_url};
    use crate::http::tls::leaf_der_from_pem;

    /// SAN vector matching `derive_public_url("https://cloud.example:8443").sans`.
    fn public_sans() -> Vec<String> {
        let getter = |k: &str| {
            if k == "UNIMATRIX_PUBLIC_URL" {
                Some("https://cloud.example:8443".to_string())
            } else {
                None
            }
        };
        derive_public_url(&Env::new(&getter)).sans
    }

    // --- R-08: production cert params ---

    // test_cert_san_set_includes_public_url_host_and_local_set (FR-A4)
    #[test]
    fn test_cert_san_set_includes_public_url_host_and_local_set() {
        let sans = public_sans();
        assert_eq!(
            sans,
            vec![
                "localhost".to_string(),
                "127.0.0.1".to_string(),
                "0.0.0.0".to_string(),
                "cloud.example".to_string(),
            ]
        );
        let (cert_pem, _key) = generate_self_signed_production(&sans).unwrap();
        let der = leaf_der_from_pem(&cert_pem).unwrap();

        // DNS SANs appear verbatim as ASCII in the DER SAN extension.
        for dns in ["localhost", "cloud.example"] {
            assert!(
                der.windows(dns.len()).any(|w| w == dns.as_bytes()),
                "DNS SAN {dns} missing from cert DER"
            );
        }
        // IP SANs are 4 raw octets; 127.0.0.1 and 0.0.0.0 are present.
        assert!(
            der.windows(4).any(|w| w == [127, 0, 0, 1]),
            "IP SAN 127.0.0.1 missing from cert DER"
        );
        // No stray test SAN leaked in: "example.com" was never a SAN here.
        assert!(!der.windows(11).any(|w| w == b"example.com"));
    }

    // test_cert_validity_is_production_period
    #[test]
    fn test_cert_validity_is_production_period() {
        let mut params = CertificateParams::new(vec!["localhost".to_string()]).unwrap();
        set_validity_window(&mut params).unwrap();
        let span = params.not_after - params.not_before;
        assert_eq!(
            span.whole_days(),
            CERT_VALIDITY_DAYS,
            "validity window must be the bounded production period, not rcgen's default"
        );
        // not_before <= today <= not_after at generation. `today` is built via the
        // same rcgen re-export so its type is inferred — no direct `time` dep named.
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let (y, m, d) = civil_from_days((secs / 86_400) as i64);
        let today = date_time_ymd(y, m, d);
        assert!(params.not_before <= today, "not_before must be <= today");
        assert!(params.not_after >= today, "not_after must be >= today");
    }

    // test_cert_is_leaf_self_signed_no_chain
    #[test]
    fn test_cert_is_leaf_self_signed_no_chain() {
        let (cert_pem, _key) = generate_self_signed_production(&public_sans()).unwrap();
        let count = cert_pem
            .windows(b"BEGIN CERTIFICATE".len())
            .filter(|w| *w == b"BEGIN CERTIFICATE")
            .count();
        assert_eq!(count, 1, "self-signed leaf, no CA chain");
        // The leaf DER is recoverable for fingerprinting (feeds AC-W1-S4).
        assert!(!leaf_der_from_pem(&cert_pem).unwrap().is_empty());
    }

    // --- civil_from_days correctness (private helper) ---

    #[test]
    fn test_civil_from_days_known_epochs() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(18_993), (2022, 1, 1));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
    }

    // Public-API behavior (idempotence, override, fail-loud, concurrency, seam)
    // lives in tests/cert_provisioner.rs to keep this source file under 500 lines,
    // mirroring the FingerprintComputer split into tests/fingerprint_parity.rs.
}
