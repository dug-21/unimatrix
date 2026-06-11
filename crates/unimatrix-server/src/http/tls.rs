//! TLS configuration: rustls ServerConfig from PEM files (ADR-005).
//!
//! Constructs a `tokio_rustls::TlsAcceptor` from PEM-encoded certificate chain
//! and private key files. Returns `None` when TLS is disabled for
//! proxy-terminated deployments. Validates cert/key at startup and refuses to
//! start on invalid files when TLS is enabled.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;

use sha2::{Digest, Sha256};
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio_rustls::rustls::{self};

use crate::error::ServerError;
use crate::infra::config::TlsConfig;

/// Build a TLS acceptor from configuration.
///
/// Returns `Some(TlsAcceptor)` when TLS is enabled with valid cert/key,
/// `None` when TLS is disabled (proxy-terminated mode).
///
/// # Errors
///
/// Returns `ServerError::Config` when TLS is enabled but:
/// - `cert_path` or `key_path` is missing
/// - Certificate or key file cannot be read
/// - PEM data is invalid or empty
/// - Certificate and key do not match
pub fn build_tls_acceptor(config: &TlsConfig) -> Result<Option<TlsAcceptor>, ServerError> {
    if !config.is_enabled() {
        tracing::info!("TLS disabled \u{2014} binding plain HTTP (proxy-terminated mode)");
        return Ok(None);
    }

    // Both paths are guaranteed present by config validation (C7),
    // but defend against misuse with descriptive errors.
    let cert_path = config
        .cert_path
        .as_ref()
        .ok_or_else(|| ServerError::Config("TLS enabled but cert_path missing".to_string()))?;
    let key_path = config
        .key_path
        .as_ref()
        .ok_or_else(|| ServerError::Config("TLS enabled but key_path missing".to_string()))?;

    // Load certificate chain
    let certs = load_certs(cert_path)?;
    if certs.is_empty() {
        return Err(ServerError::Config(format!(
            "no certificates found in {}",
            cert_path.display()
        )));
    }

    // Load private key
    let key = load_private_key(key_path)?;

    // Build rustls ServerConfig with safe defaults (no client auth / no mTLS).
    // Use ring as the crypto provider (already a transitive dependency).
    let provider = rustls::crypto::ring::default_provider();
    let server_config = ServerConfig::builder_with_provider(Arc::new(provider))
        .with_safe_default_protocol_versions()
        .map_err(|e| ServerError::Config(format!("TLS protocol version error: {e}")))?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| ServerError::Config(format!("TLS configuration error: {e}")))?;

    let acceptor = TlsAcceptor::from(Arc::new(server_config));
    tracing::info!(
        "TLS enabled \u{2014} loaded cert from {}",
        cert_path.display()
    );

    Ok(Some(acceptor))
}

/// Load PEM-encoded certificate chain from a file.
fn load_certs(path: &Path) -> Result<Vec<CertificateDer<'static>>, ServerError> {
    let file = File::open(path).map_err(|e| {
        ServerError::Config(format!(
            "cannot read TLS certificate {}: {e}",
            path.display()
        ))
    })?;
    let mut reader = BufReader::new(file);

    rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ServerError::Config(format!("invalid PEM data in {}: {e}", path.display())))
}

/// Load the first PEM-encoded private key from a file.
///
/// Supports PKCS#8, RSA, and EC key formats via `rustls_pemfile::private_key`.
fn load_private_key(path: &Path) -> Result<PrivateKeyDer<'static>, ServerError> {
    let file = File::open(path)
        .map_err(|e| ServerError::Config(format!("cannot read TLS key {}: {e}", path.display())))?;
    let mut reader = BufReader::new(file);

    rustls_pemfile::private_key(&mut reader)
        .map_err(|e| ServerError::Config(format!("invalid PEM key in {}: {e}", path.display())))?
        .ok_or_else(|| ServerError::Config(format!("no private key found in {}", path.display())))
}

/// Canonical algorithm prefix for the cert-fingerprint wire form (C2, ADR-002).
///
/// The full fingerprint is `FP_PREFIX` followed by 64 lowercase hex characters.
pub const FP_PREFIX: &str = "sha256:";

/// Compute the canonical cert-fingerprint over a served leaf certificate's DER bytes.
///
/// Returns `"sha256:" + lowercase_hex(sha256(der))` — exactly 7 + 64 characters.
/// This is the **single oracle** for the C1/C2 cross-stack parity contract (ADR-002,
/// SR-02): the JS client recomputes `sha256(cert.raw)` in `checkServerIdentity` and
/// constant-form-compares to this value.
///
/// Contract (LOCKED, ADR-002):
/// - **DER in, not PEM.** Callers MUST pass the raw DER bytes (the `CertificateDer`
///   rustls serves), never PEM text — use [`leaf_der_from_pem`] to extract them.
/// - **Leaf only**, not a chain. Callers holding a chain pass `chain[0]`.
/// - **Lowercase hex, always.** `hex::encode` yields lowercase; comparison downstream
///   is case-sensitive on this canonical form.
///
/// Total function — it hashes bytes and cannot fail.
pub fn fingerprint_leaf_der(der: &[u8]) -> String {
    let digest = Sha256::digest(der);
    let mut out = String::with_capacity(FP_PREFIX.len() + 64);
    out.push_str(FP_PREFIX);
    out.push_str(&hex::encode(digest));
    out
}

/// Extract the leaf certificate's DER bytes from PEM, for fingerprinting the served cert.
///
/// `client-bundle` and the listener wiring hold PEM (from `load_or_generate_cert`);
/// rustls serves DER. To fingerprint the *served* leaf (AC-W1-S4 — the bundle `fp` must
/// equal the served cert, not a stale on-disk one), this extracts DER from the same PEM
/// the acceptor loads, reusing the existing `rustls_pemfile` parse path.
///
/// # Errors
///
/// Returns `ServerError::Config` when the PEM is invalid or contains no certificate.
pub fn leaf_der_from_pem(cert_pem: &[u8]) -> Result<Vec<u8>, ServerError> {
    let mut reader = BufReader::new(cert_pem);
    let certs = rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ServerError::Config(format!("invalid cert PEM: {e}")))?;
    let leaf = certs
        .first()
        .ok_or_else(|| ServerError::Config("no certificate in PEM".to_string()))?;
    Ok(leaf.as_ref().to_vec())
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::path::PathBuf;

    use tempfile::NamedTempFile;

    use super::*;

    /// Generate a self-signed certificate and key PEM pair using rcgen.
    fn generate_self_signed() -> (Vec<u8>, Vec<u8>) {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
            .expect("cert generation");
        let cert_pem = cert.cert.pem().into_bytes();
        let key_pem = cert.key_pair.serialize_pem().into_bytes();
        (cert_pem, key_pem)
    }

    /// Write bytes to a temp file and return the file (keeps it alive).
    fn write_temp(data: &[u8]) -> NamedTempFile {
        let mut f = NamedTempFile::new().expect("temp file");
        f.write_all(data).expect("write");
        f.flush().expect("flush");
        f
    }

    /// Extract the error from a build_tls_acceptor result.
    /// Needed because TlsAcceptor does not implement Debug,
    /// so Result::unwrap_err() cannot be used.
    fn expect_err(result: Result<Option<TlsAcceptor>, ServerError>) -> ServerError {
        match result {
            Err(e) => e,
            Ok(_) => panic!("expected Err, got Ok"),
        }
    }

    // T-TLS-01: valid cert+key returns TlsAcceptor
    #[test]
    fn test_valid_cert_and_key_returns_tls_acceptor() {
        let (cert_pem, key_pem) = generate_self_signed();
        let cert_file = write_temp(&cert_pem);
        let key_file = write_temp(&key_pem);

        let config = TlsConfig {
            enabled: Some(true),
            cert_path: Some(cert_file.path().to_path_buf()),
            key_path: Some(key_file.path().to_path_buf()),
        };

        let result = build_tls_acceptor(&config);
        match &result {
            Err(e) => panic!("expected Ok, got Err: {e}"),
            Ok(opt) => assert!(opt.is_some(), "expected Some(TlsAcceptor)"),
        }
    }

    // T-TLS-02: missing cert_path returns error
    #[test]
    fn test_missing_cert_path_returns_error() {
        let (_, key_pem) = generate_self_signed();
        let key_file = write_temp(&key_pem);

        let config = TlsConfig {
            enabled: Some(true),
            cert_path: None,
            key_path: Some(key_file.path().to_path_buf()),
        };

        let result = build_tls_acceptor(&config);
        assert!(result.is_err());
        let msg = format!("{}", expect_err(result));
        assert!(
            msg.contains("cert_path"),
            "error should mention cert_path: {msg}"
        );
    }

    // T-TLS-03: missing key_path returns error
    #[test]
    fn test_missing_key_path_returns_error() {
        let (cert_pem, _) = generate_self_signed();
        let cert_file = write_temp(&cert_pem);

        let config = TlsConfig {
            enabled: Some(true),
            cert_path: Some(cert_file.path().to_path_buf()),
            key_path: None,
        };

        let result = build_tls_acceptor(&config);
        assert!(result.is_err());
        let msg = format!("{}", expect_err(result));
        assert!(
            msg.contains("key_path"),
            "error should mention key_path: {msg}"
        );
    }

    // T-TLS-04: invalid PEM cert returns error
    #[test]
    fn test_invalid_pem_cert_returns_error() {
        let (_, key_pem) = generate_self_signed();
        let cert_file = write_temp(b"not a valid PEM");
        let key_file = write_temp(&key_pem);

        let config = TlsConfig {
            enabled: Some(true),
            cert_path: Some(cert_file.path().to_path_buf()),
            key_path: Some(key_file.path().to_path_buf()),
        };

        let result = build_tls_acceptor(&config);
        assert!(result.is_err());
        let msg = format!("{}", expect_err(result));
        assert!(
            msg.contains("no certificates found") || msg.contains("invalid PEM"),
            "error should mention cert issue: {msg}"
        );
    }

    // T-TLS-05: invalid PEM key returns error
    #[test]
    fn test_invalid_pem_key_returns_error() {
        let (cert_pem, _) = generate_self_signed();
        let cert_file = write_temp(&cert_pem);
        let key_file = write_temp(b"not a valid PEM");

        let config = TlsConfig {
            enabled: Some(true),
            cert_path: Some(cert_file.path().to_path_buf()),
            key_path: Some(key_file.path().to_path_buf()),
        };

        let result = build_tls_acceptor(&config);
        assert!(result.is_err());
        let msg = format!("{}", expect_err(result));
        assert!(
            msg.contains("no private key found") || msg.contains("invalid PEM key"),
            "error should mention key issue: {msg}"
        );
    }

    // T-TLS-06: mismatched cert/key returns error
    #[test]
    fn test_mismatched_cert_key_returns_error() {
        let (cert_pem_a, _) = generate_self_signed();
        let (_, key_pem_b) = generate_self_signed();
        let cert_file = write_temp(&cert_pem_a);
        let key_file = write_temp(&key_pem_b);

        let config = TlsConfig {
            enabled: Some(true),
            cert_path: Some(cert_file.path().to_path_buf()),
            key_path: Some(key_file.path().to_path_buf()),
        };

        let result = build_tls_acceptor(&config);
        assert!(result.is_err(), "mismatched cert/key should fail");
        let msg = format!("{}", expect_err(result));
        assert!(
            msg.contains("TLS configuration error"),
            "error should mention TLS config issue: {msg}"
        );
    }

    // T-TLS-07: nonexistent cert file returns error
    #[test]
    fn test_nonexistent_cert_file_returns_error() {
        let (_, key_pem) = generate_self_signed();
        let key_file = write_temp(&key_pem);

        let config = TlsConfig {
            enabled: Some(true),
            cert_path: Some(PathBuf::from("/nonexistent/cert.pem")),
            key_path: Some(key_file.path().to_path_buf()),
        };

        let result = build_tls_acceptor(&config);
        assert!(result.is_err());
        let msg = format!("{}", expect_err(result));
        assert!(
            msg.contains("cannot read TLS certificate"),
            "error should mention read failure: {msg}"
        );
    }

    // T-TLS-08: TLS disabled returns None
    #[test]
    fn test_tls_disabled_returns_none() {
        let config = TlsConfig {
            enabled: Some(false),
            cert_path: None,
            key_path: None,
        };

        let result = build_tls_acceptor(&config);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none(), "disabled TLS should return None");
    }

    // T-TLS-08 variant: default config (no paths, no enabled) returns None
    #[test]
    fn test_default_config_returns_none() {
        let config = TlsConfig::default();

        let result = build_tls_acceptor(&config);
        assert!(result.is_ok());
        assert!(
            result.unwrap().is_none(),
            "default config should return None"
        );
    }

    // T-TLS-09: cert with IP SAN accepted
    #[test]
    fn test_cert_with_ip_san() {
        use std::net::IpAddr;

        let mut params = rcgen::CertificateParams::new(vec![]).expect("params");
        params
            .subject_alt_names
            .push(rcgen::SanType::IpAddress(IpAddr::V4(
                std::net::Ipv4Addr::new(127, 0, 0, 1),
            )));
        let key_pair = rcgen::KeyPair::generate().expect("key pair");
        let cert = params.self_signed(&key_pair).expect("self-signed");

        let cert_pem = cert.pem().into_bytes();
        let key_pem = key_pair.serialize_pem().into_bytes();

        let cert_file = write_temp(&cert_pem);
        let key_file = write_temp(&key_pem);

        let config = TlsConfig {
            enabled: Some(true),
            cert_path: Some(cert_file.path().to_path_buf()),
            key_path: Some(key_file.path().to_path_buf()),
        };

        let result = build_tls_acceptor(&config);
        match &result {
            Err(e) => panic!("IP SAN cert should be accepted, got Err: {e}"),
            Ok(opt) => assert!(opt.is_some(), "expected Some(TlsAcceptor)"),
        }
    }

    // Nonexistent key file returns descriptive error
    #[test]
    fn test_nonexistent_key_file_returns_error() {
        let (cert_pem, _) = generate_self_signed();
        let cert_file = write_temp(&cert_pem);

        let config = TlsConfig {
            enabled: Some(true),
            cert_path: Some(cert_file.path().to_path_buf()),
            key_path: Some(PathBuf::from("/nonexistent/key.pem")),
        };

        let result = build_tls_acceptor(&config);
        assert!(result.is_err());
        let msg = format!("{}", expect_err(result));
        assert!(
            msg.contains("cannot read TLS key"),
            "error should mention read failure: {msg}"
        );
    }

    // FingerprintComputer (C2, ADR-002) unit + oracle/parity tests live in
    // tests/fingerprint_parity.rs (integration), keeping this source file <500 lines.

    // Empty cert file (valid PEM structure but no certs) returns error
    #[test]
    fn test_empty_cert_file_returns_error() {
        let (_, key_pem) = generate_self_signed();
        let cert_file = write_temp(b"");
        let key_file = write_temp(&key_pem);

        let config = TlsConfig {
            enabled: Some(true),
            cert_path: Some(cert_file.path().to_path_buf()),
            key_path: Some(key_file.path().to_path_buf()),
        };

        let result = build_tls_acceptor(&config);
        assert!(result.is_err());
        let msg = format!("{}", expect_err(result));
        assert!(
            msg.contains("no certificates found"),
            "error should mention empty certs: {msg}"
        );
    }
}
