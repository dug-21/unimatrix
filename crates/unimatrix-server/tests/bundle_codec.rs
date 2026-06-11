//! C1 connection-bundle parity — Rust oracle + drift guard (ADR-001, ADR-006, SR-09).
//!
//! `encode_bundle`/`decode_bundle` are the **single oracle** for the C1 wire form
//! (`unimatrix-bundle:<base64url-nopad(canonical-json)>`). This file:
//!
//! 1. **Generates** the committed golden corpus
//!    (`tests/fixtures/c1c2-parity/bundle-golden.json`) from the oracle —
//!    `test_generate_c1_bundle_golden`, `#[ignore]` (run explicitly to regen). Each
//!    row carries `{fields, wire}`; the JS decoder test consumes `row.wire` and
//!    asserts the decoded fields equal `row.fields` (SR-02 — JS golden NEVER
//!    hand-written).
//! 2. **Guards** it in normal CI — `test_c1_bundle_golden_is_stable` re-encodes every
//!    row's fields and asserts byte-equality with the committed `wire`. A canonical
//!    key-order, base64url-alphabet, or escaping change fails HERE, not at a user's
//!    paste (R-05).
//! 3. Proves the **trust-boundary guard ordering** (AC-W1-C9/C10) and round-trip
//!    (R-05.3) against the public decode API.
//!
//! Synthetic 64-hex tokens only — NOT `sk-`-style provider secrets (lesson #4792).

use std::path::PathBuf;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};

use unimatrix_server::client_bundle::{
    BUNDLE_SCHEME, BUNDLE_VERSION, Bundle, BundleError, MAX_RAW_LEN, decode_bundle, encode_bundle,
};

/// Canonical fields of one golden row (matches the [`Bundle`] shape).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct GoldenFields {
    v: u8,
    base_url: String,
    token: String,
    fp: String,
}

/// One parity row: the canonical fields and their encoded wire form.
#[derive(Debug, Serialize, Deserialize)]
struct BundleRow {
    fields: GoldenFields,
    wire: String,
}

/// Absolute path to the committed golden corpus.
fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/c1c2-parity/bundle-golden.json")
}

/// Deterministic synthetic bundle field-sets. Synthetic 64-hex tokens only.
fn synthetic_fields() -> Vec<GoldenFields> {
    vec![
        GoldenFields {
            v: 1,
            base_url: "https://cloud.example:8443".to_string(),
            token: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
            fp: "sha256:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
                .to_string(),
        },
        GoldenFields {
            v: 1,
            base_url: "https://host.internal:9000".to_string(),
            token: "f".repeat(64),
            fp: format!("sha256:{}", "0".repeat(64)),
        },
        GoldenFields {
            v: 1,
            // IPv6 literal authority round-trips through the wire form unchanged.
            base_url: "https://[2001:db8::1]:8443".to_string(),
            token: "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2".to_string(),
            fp: "sha256:1111111111111111111111111111111111111111111111111111111111111111"
                .to_string(),
        },
    ]
}

fn encode_fields(f: &GoldenFields) -> String {
    encode_bundle(f.v, &f.base_url, &f.token, &f.fp).expect("encode")
}

fn encode_json(json: &str) -> String {
    format!("{BUNDLE_SCHEME}{}", URL_SAFE_NO_PAD.encode(json.as_bytes()))
}

// ---- Canonical encode (R-02 / C1) ----

#[test]
fn test_bundle_encode_canonical_field_order() {
    let f = &synthetic_fields()[0];
    let wire = encode_fields(f);
    let body = wire.strip_prefix(BUNDLE_SCHEME).expect("scheme");
    let json_bytes = URL_SAFE_NO_PAD.decode(body).expect("base64url");
    let json = String::from_utf8(json_bytes).expect("utf8");
    let expected = format!(
        "{{\"v\":1,\"base_url\":\"{}\",\"token\":\"{}\",\"fp\":\"{}\"}}",
        f.base_url, f.token, f.fp
    );
    assert_eq!(
        json, expected,
        "canonical key order must be v,base_url,token,fp"
    );
    assert!(!json.contains(": "), "no insignificant whitespace");
}

#[test]
fn test_bundle_wire_has_scheme_prefix() {
    let wire = encode_fields(&synthetic_fields()[0]);
    assert!(wire.starts_with("unimatrix-bundle:"), "wire: {wire}");
}

#[test]
fn test_bundle_base64url_no_padding() {
    let wire = encode_fields(&synthetic_fields()[0]);
    let body = wire.strip_prefix(BUNDLE_SCHEME).expect("scheme");
    assert!(!body.contains('='), "no '=' padding: {body}");
    assert!(
        !body.contains('+') && !body.contains('/'),
        "url-safe alphabet only: {body}"
    );
}

// ---- Round-trip (R-05.3) ----

#[test]
fn test_bundle_roundtrip_encode_decode_identical() {
    for f in synthetic_fields() {
        let wire = encode_fields(&f);
        let decoded = decode_bundle(&wire).expect("decode");
        assert_eq!(decoded.v, f.v);
        assert_eq!(decoded.base_url, f.base_url);
        assert_eq!(decoded.token, f.token);
        assert_eq!(decoded.fp, f.fp);
    }
}

// ---- Guard 1: length cap BEFORE decode (AC-W1-C10, load-bearing order) ----

#[test]
fn test_bundle_length_cap_before_decode() {
    // Over-cap AND not valid base64url ('!'): MUST reject on LENGTH, proving the cap
    // ran before the base64 decode (the parser-DoS guard).
    let raw = format!("{BUNDLE_SCHEME}{}", "!".repeat(MAX_RAW_LEN));
    assert!(raw.len() > MAX_RAW_LEN);
    assert_eq!(
        decode_bundle(&raw).unwrap_err(),
        BundleError::TooLong,
        "must be the length-cap reject, not BadBase64/BadJson"
    );
}

#[test]
fn test_bundle_at_exactly_cap_boundary() {
    // At exactly MAX_RAW_LEN: passes the length gate (then fails a later guard).
    let at_cap = "x".repeat(MAX_RAW_LEN);
    assert_eq!(at_cap.len(), MAX_RAW_LEN);
    assert_ne!(decode_bundle(&at_cap).unwrap_err(), BundleError::TooLong);
    // At MAX_RAW_LEN + 1: rejected on length.
    let over = "x".repeat(MAX_RAW_LEN + 1);
    assert_eq!(decode_bundle(&over).unwrap_err(), BundleError::TooLong);
}

// ---- Guard 2/3/4: scheme, base64, json ----

#[test]
fn test_bundle_reject_bad_scheme_prefix() {
    assert_eq!(
        decode_bundle("not-a-bundle:abc").unwrap_err(),
        BundleError::BadScheme
    );
    assert_eq!(decode_bundle("abc").unwrap_err(), BundleError::BadScheme);
}

#[test]
fn test_bundle_reject_non_base64url_body() {
    let raw = format!("{BUNDLE_SCHEME}not valid base64!!");
    assert_eq!(decode_bundle(&raw).unwrap_err(), BundleError::BadBase64);
}

#[test]
fn test_bundle_reject_valid_base64url_invalid_json() {
    let raw = format!(
        "{BUNDLE_SCHEME}{}",
        URL_SAFE_NO_PAD.encode(b"this is not json{")
    );
    assert_eq!(decode_bundle(&raw).unwrap_err(), BundleError::BadJson);
}

#[test]
fn test_bundle_reject_truncated_payload() {
    let wire = encode_fields(&synthetic_fields()[0]);
    let truncated = &wire[..wire.len() - 5];
    let err = decode_bundle(truncated).unwrap_err();
    assert!(matches!(err, BundleError::BadBase64 | BundleError::BadJson));
}

// ---- Guard 5: strict schema (AC-W1-C9, load-bearing) ----

const TEST_TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const TEST_FP: &str = "sha256:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
const TEST_BASE_URL: &str = "https://cloud.example:8443";

#[test]
fn test_bundle_strict_schema_reject_missing_field() {
    let raw = encode_json(&format!(
        "{{\"v\":1,\"base_url\":\"{TEST_BASE_URL}\",\"token\":\"{TEST_TOKEN}\"}}"
    ));
    assert!(matches!(
        decode_bundle(&raw).unwrap_err(),
        BundleError::Schema(_)
    ));
}

#[test]
fn test_bundle_strict_schema_reject_extra_field() {
    let raw = encode_json(&format!(
        "{{\"v\":1,\"base_url\":\"{TEST_BASE_URL}\",\"token\":\"{TEST_TOKEN}\",\"fp\":\"{TEST_FP}\",\"slug\":\"x\"}}"
    ));
    assert!(matches!(
        decode_bundle(&raw).unwrap_err(),
        BundleError::Schema(_)
    ));
}

#[test]
fn test_bundle_strict_schema_reject_wrong_type() {
    // v as string
    let raw = encode_json(&format!(
        "{{\"v\":\"1\",\"base_url\":\"{TEST_BASE_URL}\",\"token\":\"{TEST_TOKEN}\",\"fp\":\"{TEST_FP}\"}}"
    ));
    assert!(matches!(
        decode_bundle(&raw).unwrap_err(),
        BundleError::Schema(_)
    ));
    // token as number
    let raw2 = encode_json(&format!(
        "{{\"v\":1,\"base_url\":\"{TEST_BASE_URL}\",\"token\":12345,\"fp\":\"{TEST_FP}\"}}"
    ));
    assert!(matches!(
        decode_bundle(&raw2).unwrap_err(),
        BundleError::Schema(_)
    ));
}

#[test]
fn test_bundle_reject_unknown_major_version() {
    let raw = encode_json(&format!(
        "{{\"v\":2,\"base_url\":\"{TEST_BASE_URL}\",\"token\":\"{TEST_TOKEN}\",\"fp\":\"{TEST_FP}\"}}"
    ));
    assert!(matches!(
        decode_bundle(&raw).unwrap_err(),
        BundleError::Schema(_)
    ));
}

#[test]
fn test_bundle_field_format_validation() {
    // non-https base_url
    let raw = encode_json(&format!(
        "{{\"v\":1,\"base_url\":\"http://x:8443\",\"token\":\"{TEST_TOKEN}\",\"fp\":\"{TEST_FP}\"}}"
    ));
    assert!(matches!(
        decode_bundle(&raw).unwrap_err(),
        BundleError::Schema(_)
    ));
    // uppercase token is not lowercase-hex
    let raw2 = encode_json(&format!(
        "{{\"v\":1,\"base_url\":\"{TEST_BASE_URL}\",\"token\":\"{}\",\"fp\":\"{TEST_FP}\"}}",
        "A".repeat(64)
    ));
    assert!(matches!(
        decode_bundle(&raw2).unwrap_err(),
        BundleError::Schema(_)
    ));
    // malformed fp prefix
    let raw3 = encode_json(&format!(
        "{{\"v\":1,\"base_url\":\"{TEST_BASE_URL}\",\"token\":\"{TEST_TOKEN}\",\"fp\":\"md5:abc\"}}"
    ));
    assert!(matches!(
        decode_bundle(&raw3).unwrap_err(),
        BundleError::Schema(_)
    ));
}

#[test]
fn test_bundle_token_never_in_error_message() {
    let leaky = "deadbeef".repeat(8); // 64 hex; we make it 65 to force a schema reject
    let raw = encode_json(&format!(
        "{{\"v\":1,\"base_url\":\"{TEST_BASE_URL}\",\"token\":\"{leaky}X\",\"fp\":\"{TEST_FP}\"}}"
    ));
    let err = decode_bundle(&raw).unwrap_err();
    assert!(
        !err.to_string().contains(&leaky),
        "token must not leak into error"
    );
}

// ---- Parser robustness corpus (R-05) ----

#[test]
fn test_bundle_parser_never_crashes_on_corpus() {
    let big = "A".repeat(MAX_RAW_LEN + 100);
    let empty_obj = encode_json("{}");
    let arr = encode_json("[1,2,3]");
    let null = encode_json("null");
    let corpus: [&str; 8] = [
        "",
        "unimatrix-bundle:",
        "unimatrix-bundle:!!!",
        "garbage",
        &big,
        &empty_obj,
        &arr,
        &null,
    ];
    for input in corpus {
        assert!(decode_bundle(input).is_err(), "should reject: {input:?}");
    }
}

// ---- C1 golden oracle + drift guard (ADR-006, SR-02) ----

/// Oracle: regenerate the committed golden corpus.
///
/// `#[ignore]` so it does not write files in normal CI. Run explicitly:
/// `cargo test -p unimatrix-server --test bundle_codec generate -- --ignored`.
#[test]
#[ignore = "oracle regen — writes the committed corpus; run explicitly"]
fn test_generate_c1_bundle_golden() {
    let rows: Vec<BundleRow> = synthetic_fields()
        .into_iter()
        .map(|fields| {
            let wire = encode_bundle(fields.v, &fields.base_url, &fields.token, &fields.fp)
                .expect("encode");
            BundleRow { fields, wire }
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

/// Drift guard (normal CI): re-encode every committed row's fields and assert the
/// wire form is byte-identical. The load-bearing C1 regression test (R-05).
#[test]
fn test_c1_bundle_golden_is_stable() {
    let path = golden_path();
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "missing golden corpus {} ({e}) — regenerate with `--ignored test_generate_c1_bundle_golden`",
            path.display()
        )
    });
    let rows: Vec<BundleRow> = serde_json::from_str(&raw).expect("parse golden corpus");
    assert!(!rows.is_empty(), "golden corpus must not be empty");

    for (i, row) in rows.iter().enumerate() {
        // Re-encode -> must equal committed wire (encoder stability).
        let reencoded = encode_bundle(
            row.fields.v,
            &row.fields.base_url,
            &row.fields.token,
            &row.fields.fp,
        )
        .expect("encode");
        assert_eq!(reencoded, row.wire, "row {i}: encoder drift");

        // Decode committed wire -> must equal committed fields (decoder/parity).
        let decoded: Bundle = decode_bundle(&row.wire)
            .unwrap_or_else(|e| panic!("row {i}: committed wire failed to decode: {e}"));
        assert_eq!(decoded.v, row.fields.v, "row {i}: v");
        assert_eq!(decoded.base_url, row.fields.base_url, "row {i}: base_url");
        assert_eq!(decoded.token, row.fields.token, "row {i}: token");
        assert_eq!(decoded.fp, row.fields.fp, "row {i}: fp");

        // Wire shape: scheme prefix + no-pad url-safe base64.
        let body = row.wire.strip_prefix(BUNDLE_SCHEME).expect("scheme");
        assert!(!body.contains('=') && !body.contains('+') && !body.contains('/'));
        assert_eq!(row.fields.v, BUNDLE_VERSION, "row {i}: version");
    }
}
