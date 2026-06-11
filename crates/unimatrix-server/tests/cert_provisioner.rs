//! CertProvisioner public-API behavior (vnc-034, `load_or_generate_cert`).
//!
//! Split out of `src/http/cert_provisioner.rs` to keep that source file under
//! 500 lines (mirrors the FingerprintComputer split into `fingerprint_parity.rs`).
//! Covers the public contract: idempotence (R-07), operator override (FR-A3),
//! key mode 0600 (R-08), fail-loud provisioning (R-11), concurrent first boot
//! (R-07), and the `TlsConfig` acceptor seam (AC-CT-C6).

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use tempfile::TempDir;
use unimatrix_server::http::{build_tls_acceptor, load_or_generate_cert};
use unimatrix_server::infra::config::TlsConfig;

/// The production SAN vector — `derive_public_url("https://cloud.example:8443").sans`.
/// Hardcoded here (the public_url module is crate-internal); the exact derivation
/// is asserted in the in-module unit test `test_cert_san_set_...`.
fn sans() -> Vec<String> {
    ["localhost", "127.0.0.1", "0.0.0.0", "cloud.example"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

fn key_mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

fn tls_dir(base: &Path) -> std::path::PathBuf {
    base.join("tls")
}

// --- R-07: idempotence ---

#[test]
fn test_first_call_generates_both_files() {
    let tmp = TempDir::new().unwrap();
    load_or_generate_cert(tmp.path(), &sans()).unwrap();
    assert!(tls_dir(tmp.path()).join("cert.pem").exists());
    assert!(tls_dir(tmp.path()).join("key.pem").exists());
}

#[test]
fn test_second_call_loads_byte_identical_no_rewrite() {
    let tmp = TempDir::new().unwrap();
    let (cert1, key1) = load_or_generate_cert(tmp.path(), &sans()).unwrap();

    let cert_path = tls_dir(tmp.path()).join("cert.pem");
    let mtime1 = fs::metadata(&cert_path).unwrap().modified().unwrap();

    let (cert2, key2) = load_or_generate_cert(tmp.path(), &sans()).unwrap();
    assert_eq!(cert1, cert2, "cert must be byte-identical across calls");
    assert_eq!(key1, key2, "key must be byte-identical across calls");

    let mtime2 = fs::metadata(&cert_path).unwrap().modified().unwrap();
    assert_eq!(mtime1, mtime2, "cert file must not be rewritten on load");
}

// --- R-08: key mode 0600 at creation ---

#[test]
fn test_key_written_mode_0600() {
    let tmp = TempDir::new().unwrap();
    load_or_generate_cert(tmp.path(), &sans()).unwrap();
    assert_eq!(key_mode(&tls_dir(tmp.path()).join("key.pem")), 0o600);
}

// --- FR-A3: operator override honored, not overwritten ---

#[test]
fn test_operator_override_honored_not_overwritten() {
    let tmp = TempDir::new().unwrap();
    // First boot generates an "operator" pair we then treat as the mounted override.
    let other = TempDir::new().unwrap();
    let (op_cert, op_key) =
        load_or_generate_cert(other.path(), &["op.example".to_string()]).unwrap();

    let dir = tls_dir(tmp.path());
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("cert.pem"), &op_cert).unwrap();
    fs::write(dir.join("key.pem"), &op_key).unwrap();

    let (cert, key) = load_or_generate_cert(tmp.path(), &sans()).unwrap();
    assert_eq!(cert, op_cert, "operator cert must be returned unchanged");
    assert_eq!(key, op_key, "operator key must be returned unchanged");
    assert_eq!(
        fs::read(dir.join("cert.pem")).unwrap(),
        op_cert,
        "not overwritten"
    );
}

// --- Partial state: loud error, never silent regeneration ---

#[test]
fn test_partial_state_cert_only_errors_loud() {
    let tmp = TempDir::new().unwrap();
    let dir = tls_dir(tmp.path());
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("cert.pem"), b"dummy").unwrap();

    let err = load_or_generate_cert(tmp.path(), &sans()).unwrap_err();
    assert!(format!("{err}").contains("incomplete TLS material"));
    assert!(
        !dir.join("key.pem").exists(),
        "key must NOT be silently generated"
    );
}

#[test]
fn test_partial_state_key_only_errors_loud() {
    let tmp = TempDir::new().unwrap();
    let dir = tls_dir(tmp.path());
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("key.pem"), b"dummy").unwrap();

    let err = load_or_generate_cert(tmp.path(), &sans()).unwrap_err();
    assert!(format!("{err}").contains("incomplete TLS material"));
    assert!(
        !dir.join("cert.pem").exists(),
        "cert must NOT be silently generated"
    );
}

// --- R-11: fail-loud provisioning, no panic ---

#[test]
fn test_unwritable_data_dir_returns_actionable_error() {
    let tmp = TempDir::new().unwrap();
    fs::set_permissions(tmp.path(), fs::Permissions::from_mode(0o555)).unwrap();

    let result = load_or_generate_cert(tmp.path(), &sans());
    // Restore before asserting so TempDir cleanup succeeds.
    fs::set_permissions(tmp.path(), fs::Permissions::from_mode(0o755)).unwrap();

    let err = result.expect_err("read-only /data must error, not panic");
    let msg = format!("{err}");
    assert!(
        msg.contains("UID 65532"),
        "actionable msg names the UID fix: {msg}"
    );
    assert!(
        msg.contains(&tls_dir(tmp.path()).display().to_string()),
        "actionable msg names the path: {msg}"
    );
}

// --- Concurrency (R-07): two first boots converge, no corruption ---

#[test]
fn test_concurrent_first_boot_converges() {
    use std::sync::{Arc, Barrier};

    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().to_path_buf();
    let san = Arc::new(sans());
    let barrier = Arc::new(Barrier::new(4));

    let handles: Vec<_> = (0..4)
        .map(|_| {
            let d = dir.clone();
            let s = Arc::clone(&san);
            let b = Arc::clone(&barrier);
            std::thread::spawn(move || {
                b.wait();
                load_or_generate_cert(&d, &s)
            })
        })
        .collect();

    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let first = results[0].as_ref().expect("thread 0 must succeed");
    for (i, r) in results.iter().enumerate() {
        let pair = r
            .as_ref()
            .unwrap_or_else(|e| panic!("thread {i} failed: {e}"));
        assert_eq!(
            pair, first,
            "all concurrent boots must converge on one pair"
        );
    }
    assert_eq!(key_mode(&tls_dir(&dir).join("key.pem")), 0o600);
}

// --- AC-CT-C6: provisioned cert loads through the build_tls_acceptor seam ---

#[test]
fn test_provisioned_cert_builds_tls_acceptor() {
    let tmp = TempDir::new().unwrap();
    load_or_generate_cert(tmp.path(), &sans()).unwrap();

    let cfg = TlsConfig {
        enabled: Some(true),
        cert_path: Some(tls_dir(tmp.path()).join("cert.pem")),
        key_path: Some(tls_dir(tmp.path()).join("key.pem")),
    };
    match build_tls_acceptor(&cfg) {
        Ok(opt) => assert!(opt.is_some(), "provisioned cert must build an acceptor"),
        Err(e) => panic!("provisioned cert rejected by acceptor: {e}"),
    }
}
