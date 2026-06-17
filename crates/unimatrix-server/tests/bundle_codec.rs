//! C1 connection-bundle parity — Rust oracle + drift guard (vnc-038, ADR-002).
//!
//! `encode_bundle`/`decode_bundle` are the **single oracle** for the `v:2` wire
//! form (`unimatrix-bundle:<base64url-nopad(canonical-json)>`,
//! `{v, mcp_url, observe_url, token, fp}`). This file:
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
//!    paste (R-03).
//! 3. Proves the **trust-boundary guard ordering** (R-03/NFR-08) and round-trip
//!    (R-03 sc.1) against the public decode API, plus the strict-reject matrix and
//!    the v:1 hard-cut (R-04).
//!
//! Synthetic 64-hex tokens only — NOT `sk-`-style provider secrets (lesson #4792).

use std::path::PathBuf;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};

use unimatrix_server::client_bundle::{
    BUNDLE_SCHEME, BUNDLE_VERSION, Bundle, BundleError, MAX_RAW_LEN, decode_bundle, encode_bundle,
};

/// Canonical fields of one golden row (matches the `v:2` [`Bundle`] shape).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct GoldenFields {
    v: u8,
    mcp_url: String,
    observe_url: String,
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
            v: 2,
            mcp_url: "https://cloud.example:8443/v1/alpha".to_string(),
            observe_url: "https://cloud.example:8443/v1/alpha/observe".to_string(),
            token: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
            fp: "sha256:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
                .to_string(),
        },
        GoldenFields {
            v: 2,
            mcp_url: "https://host.internal:9000/v1/beta".to_string(),
            observe_url: "https://host.internal:9000/v1/beta/observe".to_string(),
            token: "f".repeat(64),
            fp: format!("sha256:{}", "0".repeat(64)),
        },
        GoldenFields {
            v: 2,
            // IPv6 literal authority round-trips through the wire form unchanged.
            mcp_url: "https://[2001:db8::1]:8443/v1/gamma-2".to_string(),
            observe_url: "https://[2001:db8::1]:8443/v1/gamma-2/observe".to_string(),
            token: "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2".to_string(),
            fp: "sha256:1111111111111111111111111111111111111111111111111111111111111111"
                .to_string(),
        },
    ]
}

fn encode_fields(f: &GoldenFields) -> String {
    encode_bundle(f.v, &f.mcp_url, &f.observe_url, &f.token, &f.fp).expect("encode")
}

fn encode_json(json: &str) -> String {
    format!("{BUNDLE_SCHEME}{}", URL_SAFE_NO_PAD.encode(json.as_bytes()))
}

// ---- Canonical encode (R-03 / C1) ----

#[test]
fn test_encode_bundle_v2_composes_both_urls() {
    let f = &synthetic_fields()[0];
    let wire = encode_fields(f);
    let body = wire.strip_prefix(BUNDLE_SCHEME).expect("scheme");
    let json_bytes = URL_SAFE_NO_PAD.decode(body).expect("base64url");
    let json = String::from_utf8(json_bytes).expect("utf8");
    let expected = format!(
        "{{\"v\":2,\"mcp_url\":\"{}\",\"observe_url\":\"{}\",\"token\":\"{}\",\"fp\":\"{}\"}}",
        f.mcp_url, f.observe_url, f.token, f.fp
    );
    assert_eq!(
        json, expected,
        "canonical key order must be v,mcp_url,observe_url,token,fp"
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

// ---- Round-trip (R-03 sc.1) ----

#[test]
fn test_decode_bundle_v2_round_trip() {
    for f in synthetic_fields() {
        let wire = encode_fields(&f);
        let decoded = decode_bundle(&wire).expect("decode");
        assert_eq!(decoded.v, f.v);
        assert_eq!(decoded.mcp_url, f.mcp_url);
        assert_eq!(decoded.observe_url, f.observe_url);
        assert_eq!(decoded.token, f.token);
        assert_eq!(decoded.fp, f.fp);
    }
}

// ---- Guard 1: length cap BEFORE decode (R-03 sc.3 / NFR-08) ----

#[test]
fn test_max_raw_len_runs_first() {
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

// ---- Guard 5: strict schema (R-03 sc.2, load-bearing) ----

const TEST_TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const TEST_FP: &str = "sha256:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
const TEST_MCP_URL: &str = "https://cloud.example:8443/v1/alpha";
const TEST_OBSERVE_URL: &str = "https://cloud.example:8443/v1/alpha/observe";

#[test]
fn test_reject_missing_key() {
    // Missing observe_url (4 keys) → reject.
    let raw = encode_json(&format!(
        "{{\"v\":2,\"mcp_url\":\"{TEST_MCP_URL}\",\"token\":\"{TEST_TOKEN}\",\"fp\":\"{TEST_FP}\"}}"
    ));
    assert!(matches!(
        decode_bundle(&raw).unwrap_err(),
        BundleError::Schema(_)
    ));
}

#[test]
fn test_reject_extra_key() {
    // 6th key → reject.
    let raw = encode_json(&format!(
        "{{\"v\":2,\"mcp_url\":\"{TEST_MCP_URL}\",\"observe_url\":\"{TEST_OBSERVE_URL}\",\"token\":\"{TEST_TOKEN}\",\"fp\":\"{TEST_FP}\",\"slug\":\"x\"}}"
    ));
    assert!(matches!(
        decode_bundle(&raw).unwrap_err(),
        BundleError::Schema(_)
    ));
}

#[test]
fn test_reject_wrong_type_key() {
    // v as string.
    let raw = encode_json(&format!(
        "{{\"v\":\"2\",\"mcp_url\":\"{TEST_MCP_URL}\",\"observe_url\":\"{TEST_OBSERVE_URL}\",\"token\":\"{TEST_TOKEN}\",\"fp\":\"{TEST_FP}\"}}"
    ));
    assert!(matches!(
        decode_bundle(&raw).unwrap_err(),
        BundleError::Schema(_)
    ));
    // mcp_url as number.
    let raw2 = encode_json(&format!(
        "{{\"v\":2,\"mcp_url\":123,\"observe_url\":\"{TEST_OBSERVE_URL}\",\"token\":\"{TEST_TOKEN}\",\"fp\":\"{TEST_FP}\"}}"
    ));
    assert!(matches!(
        decode_bundle(&raw2).unwrap_err(),
        BundleError::Schema(_)
    ));
}

#[test]
fn test_reject_non_https_url() {
    // non-https mcp_url.
    let raw = encode_json(&format!(
        "{{\"v\":2,\"mcp_url\":\"http://h/v1/alpha\",\"observe_url\":\"{TEST_OBSERVE_URL}\",\"token\":\"{TEST_TOKEN}\",\"fp\":\"{TEST_FP}\"}}"
    ));
    assert!(matches!(
        decode_bundle(&raw).unwrap_err(),
        BundleError::Schema(_)
    ));
    // non-https observe_url (ftp://).
    let raw2 = encode_json(&format!(
        "{{\"v\":2,\"mcp_url\":\"{TEST_MCP_URL}\",\"observe_url\":\"ftp://h/v1/alpha/observe\",\"token\":\"{TEST_TOKEN}\",\"fp\":\"{TEST_FP}\"}}"
    ));
    assert!(matches!(
        decode_bundle(&raw2).unwrap_err(),
        BundleError::Schema(_)
    ));
}

#[test]
fn test_reject_unknown_major_version() {
    // v: 3 → forward-compat reject.
    let raw = encode_json(&format!(
        "{{\"v\":3,\"mcp_url\":\"{TEST_MCP_URL}\",\"observe_url\":\"{TEST_OBSERVE_URL}\",\"token\":\"{TEST_TOKEN}\",\"fp\":\"{TEST_FP}\"}}"
    ));
    assert!(matches!(
        decode_bundle(&raw).unwrap_err(),
        BundleError::Schema(_)
    ));
}

#[test]
fn test_bundle_field_format_validation() {
    // uppercase token is not lowercase-hex.
    let raw = encode_json(&format!(
        "{{\"v\":2,\"mcp_url\":\"{TEST_MCP_URL}\",\"observe_url\":\"{TEST_OBSERVE_URL}\",\"token\":\"{}\",\"fp\":\"{TEST_FP}\"}}",
        "A".repeat(64)
    ));
    assert!(matches!(
        decode_bundle(&raw).unwrap_err(),
        BundleError::Schema(_)
    ));
    // malformed fp prefix.
    let raw2 = encode_json(&format!(
        "{{\"v\":2,\"mcp_url\":\"{TEST_MCP_URL}\",\"observe_url\":\"{TEST_OBSERVE_URL}\",\"token\":\"{TEST_TOKEN}\",\"fp\":\"md5:abc\"}}"
    ));
    assert!(matches!(
        decode_bundle(&raw2).unwrap_err(),
        BundleError::Schema(_)
    ));
}

#[test]
fn test_bundle_token_never_in_error_message() {
    let leaky = "deadbeef".repeat(8); // 64 hex; we make it 65 to force a schema reject
    let raw = encode_json(&format!(
        "{{\"v\":2,\"mcp_url\":\"{TEST_MCP_URL}\",\"observe_url\":\"{TEST_OBSERVE_URL}\",\"token\":\"{leaky}X\",\"fp\":\"{TEST_FP}\"}}"
    ));
    let err = decode_bundle(&raw).unwrap_err();
    assert!(
        !err.to_string().contains(&leaky),
        "token must not leak into error"
    );
}

// ---- v:1 hard-cut (R-04) ----

#[test]
fn test_reject_v1_shaped_bundle() {
    // A well-formed v:1 artifact ({v:1, base_url, token, fp}) presented to the v:2
    // decode → loud reject. No base_url acceptance path survives.
    let raw = encode_json(&format!(
        "{{\"v\":1,\"base_url\":\"https://cloud.example:8443\",\"token\":\"{TEST_TOKEN}\",\"fp\":\"{TEST_FP}\"}}"
    ));
    assert!(
        matches!(decode_bundle(&raw).unwrap_err(), BundleError::Schema(_)),
        "a v:1 bundle must fail closed under v:2 decode"
    );
}

#[test]
fn test_no_v1_fallback_decode_path() {
    // Exactly one version arm (v == 2). Every other major fails closed — assert v:1
    // and v:3 (with otherwise-valid v:2 shape) both reject, proving no compat arm.
    for bad_v in [0u8, 1, 3, 4] {
        let raw = encode_json(&format!(
            "{{\"v\":{bad_v},\"mcp_url\":\"{TEST_MCP_URL}\",\"observe_url\":\"{TEST_OBSERVE_URL}\",\"token\":\"{TEST_TOKEN}\",\"fp\":\"{TEST_FP}\"}}"
        ));
        assert!(
            matches!(decode_bundle(&raw).unwrap_err(), BundleError::Schema(_)),
            "v:{bad_v} must fail closed (only v==2 accepted)"
        );
    }
}

// ---- Parser robustness corpus (R-03) ----

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

// ---- C1 golden oracle + drift guard (ADR-002, SR-02) ----

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
            let wire = encode_bundle(
                fields.v,
                &fields.mcp_url,
                &fields.observe_url,
                &fields.token,
                &fields.fp,
            )
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
/// wire form is byte-identical. The load-bearing C1 regression test (R-03).
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
            &row.fields.mcp_url,
            &row.fields.observe_url,
            &row.fields.token,
            &row.fields.fp,
        )
        .expect("encode");
        assert_eq!(reencoded, row.wire, "row {i}: encoder drift");

        // Decode committed wire -> must equal committed fields (decoder/parity).
        let decoded: Bundle = decode_bundle(&row.wire)
            .unwrap_or_else(|e| panic!("row {i}: committed wire failed to decode: {e}"));
        assert_eq!(decoded.v, row.fields.v, "row {i}: v");
        assert_eq!(decoded.mcp_url, row.fields.mcp_url, "row {i}: mcp_url");
        assert_eq!(
            decoded.observe_url, row.fields.observe_url,
            "row {i}: observe_url"
        );
        assert_eq!(decoded.token, row.fields.token, "row {i}: token");
        assert_eq!(decoded.fp, row.fields.fp, "row {i}: fp");

        // Wire shape: scheme prefix + no-pad url-safe base64.
        let body = row.wire.strip_prefix(BUNDLE_SCHEME).expect("scheme");
        assert!(!body.contains('=') && !body.contains('+') && !body.contains('/'));
        assert_eq!(row.fields.v, BUNDLE_VERSION, "row {i}: version");
    }
}
