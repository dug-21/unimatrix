//! Unit tests for the PRIVATE helpers in [`crate::client_bundle`] (output split,
//! token reader, route-url composition, field validators).
//!
//! Public-API tests (encode/decode/guard ordering/schema) + the C1 golden oracle
//! live in `tests/bundle_codec.rs` (integration), mirroring the FingerprintComputer
//! oracle precedent (`tests/fingerprint_parity.rs`). Split into this sibling file
//! (via `#[path]`) so `client_bundle.rs` stays under the 500-line cap (C-06).

use super::*;

const TEST_TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const TEST_FP: &str = "sha256:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
const TEST_MCP_URL: &str = "https://cloud.example:8443/v1/alpha";
const TEST_OBSERVE_URL: &str = "https://cloud.example:8443/v1/alpha/observe";

// --- compose_route_urls (route grammar owner, ADR-002) ---

#[test]
fn test_compose_route_urls_uses_route_grammar() {
    let slug = ProjectSlug::try_from("alpha").unwrap();
    let (mcp, obs) = compose_route_urls("https://h", &slug);
    assert_eq!(mcp, "https://h/v1/alpha");
    assert_eq!(obs, "https://h/v1/alpha/observe");
}

#[test]
fn test_compose_route_urls_strips_trailing_slash() {
    let slug = ProjectSlug::try_from("alpha").unwrap();
    let (mcp, obs) = compose_route_urls("https://h:8443/", &slug);
    assert_eq!(mcp, "https://h:8443/v1/alpha");
    assert_eq!(obs, "https://h:8443/v1/alpha/observe");
}

// --- stdout/stderr split + token redaction (AC-11, NFR-06) ---

#[test]
fn test_client_bundle_stdout_is_opaque_blob_only() {
    let blob = encode_bundle(
        BUNDLE_VERSION,
        TEST_MCP_URL,
        TEST_OBSERVE_URL,
        TEST_TOKEN,
        TEST_FP,
    )
    .unwrap();
    let (stdout, _stderr) = render_output(&blob, TEST_MCP_URL, TEST_OBSERVE_URL, TEST_FP);
    // stdout is EXACTLY the blob — no prose, no extra lines, pipeable.
    assert_eq!(stdout, blob);
    assert!(stdout.starts_with("unimatrix-bundle:"));
    assert!(!stdout.contains('\n'), "stdout blob must be a single line");
}

#[test]
fn test_client_bundle_stderr_echoes_urls_and_fp_only() {
    let blob = encode_bundle(
        BUNDLE_VERSION,
        TEST_MCP_URL,
        TEST_OBSERVE_URL,
        TEST_TOKEN,
        TEST_FP,
    )
    .unwrap();
    let (_stdout, stderr) = render_output(&blob, TEST_MCP_URL, TEST_OBSERVE_URL, TEST_FP);
    assert!(stderr.contains(TEST_MCP_URL), "stderr must echo mcp-url");
    assert!(
        stderr.contains(TEST_OBSERVE_URL),
        "stderr must echo observe-url"
    );
    assert!(
        stderr.contains(TEST_FP),
        "stderr must echo cert fingerprint"
    );
}

#[test]
fn test_client_bundle_token_absent_from_stdout_and_stderr() {
    // LOAD-BEARING (ADR-008): the token hex must appear in NEITHER stdout NOR
    // stderr — it lives ONLY inside the base64url blob payload on stdout.
    let blob = encode_bundle(
        BUNDLE_VERSION,
        TEST_MCP_URL,
        TEST_OBSERVE_URL,
        TEST_TOKEN,
        TEST_FP,
    )
    .unwrap();
    let (stdout, stderr) = render_output(&blob, TEST_MCP_URL, TEST_OBSERVE_URL, TEST_FP);
    assert!(
        !stdout.contains(TEST_TOKEN),
        "token must not be plaintext in stdout"
    );
    assert!(
        !stderr.contains(TEST_TOKEN),
        "token must not appear in stderr"
    );
    // But it IS recoverable from inside the encoded blob (sanity check).
    let decoded = decode_bundle(&stdout).unwrap();
    assert_eq!(decoded.token, TEST_TOKEN);
}

#[test]
fn test_client_bundle_edit_me_placeholder_visible_on_stderr() {
    let mcp = "https://<EDIT-ME>:8443/v1/alpha";
    let obs = "https://<EDIT-ME>:8443/v1/alpha/observe";
    let blob = encode_bundle(BUNDLE_VERSION, mcp, obs, TEST_TOKEN, TEST_FP).unwrap();
    let (_stdout, stderr) = render_output(&blob, mcp, obs, TEST_FP);
    assert!(
        stderr.contains("<EDIT-ME>"),
        "placeholder must be visible on stderr"
    );
    assert!(
        stderr.contains("WARNING"),
        "placeholder must carry a warning"
    );
}

#[test]
fn test_run_client_bundle_rejects_invalid_slug() {
    // Invalid slug → loud Config error; no blob emitted (ADR-001/004).
    let err = run_client_bundle(None, "badslug!").unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("slug"),
        "error must name the slug problem: {msg}"
    );
    // The error path returns BEFORE any data-dir / token / cert read.
}

// --- read_token_hex (token file handling) ---

#[test]
fn test_read_token_hex_valid_lowercases_and_trims() {
    let tmp = tempfile::TempDir::new().unwrap();
    let upper = "AB".repeat(32); // 64 uppercase hex
    std::fs::write(tmp.path().join("token"), format!("{upper}\n")).unwrap();
    let got = read_token_hex(tmp.path()).unwrap();
    assert_eq!(got, upper.to_ascii_lowercase());
    assert!(is_token(&got));
}

#[test]
fn test_read_token_hex_missing_file_errors() {
    let tmp = tempfile::TempDir::new().unwrap();
    let err = read_token_hex(tmp.path()).unwrap_err();
    assert!(format!("{err}").contains("token"));
}

#[test]
fn test_read_token_hex_wrong_length_errors() {
    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp.path().join("token"), "abc").unwrap();
    assert!(read_token_hex(tmp.path()).is_err());
}

// --- field validators (is_token / is_fingerprint) ---

#[test]
fn test_is_token_and_is_fingerprint() {
    assert!(is_token(TEST_TOKEN));
    assert!(!is_token(&"A".repeat(64)), "uppercase is not lowercase-hex");
    assert!(!is_token(&"a".repeat(63)), "wrong length rejects");
    assert!(is_fingerprint(TEST_FP));
    assert!(!is_fingerprint("md5:abc"), "wrong algo prefix rejects");
    assert!(!is_fingerprint(TEST_TOKEN), "missing prefix rejects");
}
