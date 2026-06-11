//! C1/C2 cross-stack fingerprint parity — Rust oracle + drift guard (ADR-002, ADR-006, SR-02).
//!
//! `fingerprint_leaf_der` is the **single oracle** for the cert-fingerprint wire
//! contract. This file:
//!
//! 1. **Generates** the committed golden corpus
//!    (`tests/fixtures/c1c2-parity/fingerprint-golden.json`) from the oracle —
//!    `test_generate_c2_fingerprint_golden`, `#[ignore]` (run explicitly to regen).
//! 2. **Guards** it in normal CI — `test_c2_fingerprint_golden_is_stable` re-derives
//!    `fp` for every committed row and asserts byte-equality. A hex-casing or
//!    DER-handling change fails HERE, not at a user's connect (R-02).
//! 3. Proves **served-cert linkage** — the fingerprint over the leaf DER extracted
//!    from a provisioned cert's PEM equals an independent SHA-256 (AC-W1-S4).
//!
//! The committed corpus is the source of truth consumed byte-identically by the JS
//! client test (#725). The JS golden is NEVER hand-written — it is derived from this
//! corpus (SR-02). Synthetic DERs only — no real-provider-shaped secrets (lesson #4792).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use unimatrix_server::http::{fingerprint_leaf_der, leaf_der_from_pem};

/// One parity row: a DER (hex) and its canonical fingerprint.
///
/// `der_hex` is lowercase hex of the synthetic leaf DER bytes; the JS test hex-decodes
/// it, computes `sha256`, and asserts the `sha256:`-prefixed lowercase hex equals `fp`.
#[derive(Debug, Serialize, Deserialize)]
struct ParityRow {
    der_hex: String,
    fp: String,
}

/// Absolute path to the committed golden corpus.
fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/c1c2-parity/fingerprint-golden.json")
}

/// Deterministic synthetic leaf-DER byte vectors.
///
/// These are NOT real certificates — `fingerprint_leaf_der` hashes raw bytes, so the
/// contract is exercised by fixed, reproducible byte patterns. Using deterministic
/// bytes (not rcgen's random keys) keeps the corpus stable across regenerations.
/// The set spans: empty, a DER-shaped header, ASCII, all-byte-values, multi-KB.
fn synthetic_ders() -> Vec<Vec<u8>> {
    // Deterministic pseudo-DER ~1 KB (a simple LCG, no randomness).
    let mut lcg: u32 = 0x1234_5678;
    let mut lcg_buf = Vec::with_capacity(1024);
    for _ in 0..1024 {
        lcg = lcg.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        lcg_buf.push((lcg >> 24) as u8);
    }

    vec![
        // 1. Empty input (defined behavior — sha256 of empty).
        Vec::new(),
        // 2. Minimal DER-shaped bytes (SEQUENCE tag + length).
        vec![0x30, 0x03, 0x02, 0x01, 0x00],
        // 3. ASCII marker.
        b"unimatrix-c2-parity".to_vec(),
        // 4. Every byte value 0x00..=0xFF once.
        (0u16..=255).map(|b| b as u8).collect(),
        // 5. Deterministic pseudo-DER ~1 KB.
        lcg_buf,
        // 6. Multi-KB block (no truncation in the hash).
        vec![0xA5u8; 4096],
    ]
}

// ---- Format correctness (R-02) ----

/// SHA-256 of the empty input — the canonical empty-digest vector.
const SHA256_EMPTY: &str =
    "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

#[test]
fn test_fingerprint_has_sha256_prefix() {
    let fp = fingerprint_leaf_der(b"any bytes");
    assert!(fp.starts_with("sha256:"), "must start with sha256: -> {fp}");
}

#[test]
fn test_fingerprint_is_64_lowercase_hex() {
    let fp = fingerprint_leaf_der(b"some DER bytes");
    let body = fp.strip_prefix("sha256:").expect("prefix present");
    assert_eq!(body.len(), 64, "body must be 64 hex chars -> {body}");
    assert!(
        body.chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
        "body must be lowercase hex only -> {body}"
    );
    assert!(!fp.contains("0x"), "no 0x marker -> {fp}");
    assert_eq!(fp.matches(':').count(), 1, "exactly one colon -> {fp}");
}

#[test]
fn test_fingerprint_matches_known_sha256_vector() {
    assert_eq!(fingerprint_leaf_der(b""), SHA256_EMPTY);
    // "abc" -> known NIST SHA-256 vector.
    assert_eq!(
        fingerprint_leaf_der(b"abc"),
        "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

/// SR-02 silent-break vector: hashing DER must differ from hashing PEM bytes.
#[test]
fn test_fingerprint_hashes_der_not_pem() {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
        .expect("self-signed cert");
    let cert_pem = cert.cert.pem().into_bytes();
    let der = leaf_der_from_pem(&cert_pem).expect("extract DER");

    let fp_der = fingerprint_leaf_der(&der);
    let fp_pem = fingerprint_leaf_der(&cert_pem); // naive PEM-bytes hash
    assert_ne!(
        fp_der, fp_pem,
        "DER fingerprint must differ from PEM-bytes hash"
    );
}

#[test]
fn test_fingerprint_deterministic() {
    let der = b"deterministic input";
    assert_eq!(fingerprint_leaf_der(der), fingerprint_leaf_der(der));
}

#[test]
fn test_fingerprint_empty_der_is_defined() {
    assert_eq!(fingerprint_leaf_der(&[]), SHA256_EMPTY);
}

#[test]
fn test_fingerprint_large_der_no_truncation() {
    let big = vec![0xABu8; 8192];
    let fp = fingerprint_leaf_der(&big);
    assert_eq!(fp.strip_prefix("sha256:").expect("prefix").len(), 64);
}

// ---- leaf_der_from_pem ----

#[test]
fn test_leaf_der_from_pem_extracts_der() {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
        .expect("self-signed cert");
    let cert_pem = cert.cert.pem().into_bytes();
    let der = leaf_der_from_pem(&cert_pem).expect("extract DER");
    assert!(!der.is_empty(), "extracted DER must be non-empty");
    // DER (SEQUENCE) starts with 0x30; PEM text starts with '-' (0x2d).
    assert_eq!(der[0], 0x30, "extracted bytes must be DER, not PEM text");
}

#[test]
fn test_leaf_der_from_pem_garbage_returns_err() {
    assert!(
        leaf_der_from_pem(b"not a pem").is_err(),
        "garbage PEM must error"
    );
}

#[test]
fn test_leaf_der_from_pem_empty_returns_err() {
    assert!(leaf_der_from_pem(b"").is_err(), "empty PEM must error");
}

/// Oracle: regenerate the committed golden corpus.
///
/// `#[ignore]` so it does not write files in normal CI. Run explicitly to regenerate:
/// `cargo test -p unimatrix-server --test fingerprint_parity generate -- --ignored`.
#[test]
#[ignore = "oracle regen — writes the committed corpus; run explicitly"]
fn test_generate_c2_fingerprint_golden() {
    let rows: Vec<ParityRow> = synthetic_ders()
        .into_iter()
        .map(|der| ParityRow {
            der_hex: hex::encode(&der),
            fp: fingerprint_leaf_der(&der),
        })
        .collect();

    let path = golden_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create fixtures dir");
    }
    let json = serde_json::to_string_pretty(&rows).expect("serialize corpus");
    std::fs::write(&path, format!("{json}\n")).expect("write golden corpus");
    eprintln!("wrote {} rows to {}", rows.len(), path.display());
}

/// Drift guard (normal CI): re-derive `fp` for every committed row and assert equality.
///
/// This is the load-bearing C2 regression test — a casing or DER-handling change fails
/// here, loudly, instead of silently breaking pinning at a user's connect (R-02).
#[test]
fn test_c2_fingerprint_golden_is_stable() {
    let path = golden_path();
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "missing golden corpus {} ({e}) — regenerate with `--ignored test_generate_c2_fingerprint_golden`",
            path.display()
        )
    });
    let rows: Vec<ParityRow> = serde_json::from_str(&raw).expect("parse golden corpus");

    assert!(!rows.is_empty(), "golden corpus must not be empty");

    for (i, row) in rows.iter().enumerate() {
        let der = hex::decode(&row.der_hex)
            .unwrap_or_else(|e| panic!("row {i}: der_hex not valid hex ({e})"));
        let recomputed = fingerprint_leaf_der(&der);
        assert_eq!(
            recomputed, row.fp,
            "row {i}: oracle drift — committed fp {} != recomputed {recomputed}",
            row.fp
        );
        // Shape guard: canonical lowercase form.
        let body = row.fp.strip_prefix("sha256:").expect("sha256: prefix");
        assert_eq!(body.len(), 64, "row {i}: fp body must be 64 hex chars");
        assert!(
            body.chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
            "row {i}: fp body must be lowercase hex"
        );
    }
}

/// Served-cert linkage (AC-W1-S4): the fingerprint over the leaf DER extracted from a
/// provisioned cert's PEM equals an independent SHA-256 over that same leaf DER.
///
/// Proves the bundle pins the *served* cert (extracted from the acceptor's PEM), not a
/// stale on-disk one. The fingerprint compute is owned here; cert provisioning is the
/// cert-provisioner component's concern (next wave) — this uses rcgen directly.
#[test]
fn test_bundle_fp_equals_served_leaf_der_fingerprint() {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
        .expect("self-signed cert");
    let cert_pem = cert.cert.pem().into_bytes();

    let der = leaf_der_from_pem(&cert_pem).expect("extract leaf DER from served PEM");

    // Independent SHA-256 over the leaf DER -> canonical form.
    let expected = format!("sha256:{}", hex::encode(Sha256::digest(&der)));
    let fp = fingerprint_leaf_der(&der);

    assert_eq!(
        fp, expected,
        "bundle fp must equal served leaf DER fingerprint"
    );
}
