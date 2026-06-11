//! C1 connection-bundle codec — `client-bundle` sync subcommand (vnc-034, ADR-001).
//!
//! Encodes the connection bundle `{v, base_url, token, fp}` into the LOCKED wire
//! form `unimatrix-bundle:<base64url-nopad(canonical-json)>` and emits it for the
//! operator to paste into `unimatrix init --remote <bundle>`.
//!
//! This is the **build-first encoder** (the C1 oracle): the Rust side is the only
//! encoder; the JS client only decodes (remote-client.md), so the canonical form
//! and the parity corpus are stable. [`decode_bundle`] mirrors the exact JS
//! guard-ordering algorithm so round-trip + corpus tests pin both stacks together.
//!
//! ## Output contract (HARD — FR-A5b / NFR-06)
//!
//! - **stdout** = the opaque `unimatrix-bundle:…` blob ONLY. One line, pipeable,
//!   zero contamination. The token lives ONLY inside this base64url blob.
//! - **stderr** = human echo of `base_url` + `cert-fingerprint` ONLY. The token is
//!   NEVER printed to stderr, and never to any log line.
//!
//! `run_client_bundle` is a sync, pre-tokio subcommand (C-10), dispatched in the
//! `main.rs` sync block alongside `health`/`version`. No tokio runtime, no token
//! in any trace.

use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::Serialize;

use crate::error::ServerError;
use crate::http::public_url::{Env, derive_public_url};
use crate::http::{fingerprint_leaf_der, leaf_der_from_pem};
use crate::project;

/// Literal scheme prefix of the wire form (ADR-001). The client rejects anything
/// without it.
pub const BUNDLE_SCHEME: &str = "unimatrix-bundle:";

/// Bundle schema version. The client rejects unknown major versions.
pub const BUNDLE_VERSION: u8 = 1;

/// 4 KB cap on the RAW pasted string (bytes), enforced BEFORE decode/parse.
/// Belt-and-suspenders DoS pre-filter; the strict schema is the load-bearing guard.
pub const MAX_RAW_LEN: usize = 4096;

/// Token file name within the data directory (mirrors `http::token`).
const TOKEN_FILE_NAME: &str = "token";

/// Expected hex-encoded token length (32 bytes -> 64 hex chars).
const TOKEN_HEX_LEN: usize = 64;

/// Canonical bundle payload.
///
/// Field declaration order IS the canonical JSON key order (`v, base_url, token,
/// fp`) — `serde_json` (with no map reordering) serializes struct fields in
/// declaration order, guaranteeing a stable, fixture-stable wire form (ADR-001).
/// Never build this from a `HashMap` (key order undefined).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Bundle {
    /// Schema version (always [`BUNDLE_VERSION`]).
    pub v: u8,
    /// Public base URL, e.g. `"https://cloud.example:8443"`. Must be `https://`.
    pub base_url: String,
    /// 64 lowercase-hex bearer token. Carried ONLY inside the encoded blob.
    pub token: String,
    /// Cert fingerprint `"sha256:<64hex>"` over the served leaf DER (C2).
    pub fp: String,
}

/// Decode-side error variants (the shared trust-boundary contract, mirrored in JS).
///
/// No variant carries the token — it never appears in an error message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BundleError {
    /// Raw pasted string exceeds [`MAX_RAW_LEN`] (rejected on length BEFORE decode).
    TooLong,
    /// Missing or wrong `unimatrix-bundle:` scheme prefix.
    BadScheme,
    /// Body is not valid base64url (no-pad).
    BadBase64,
    /// Decoded bytes are not valid JSON.
    BadJson,
    /// Strict-schema rejection (load-bearing guard). Carries a token-free reason.
    Schema(String),
}

impl std::fmt::Display for BundleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BundleError::TooLong => write!(
                f,
                "bundle exceeds {MAX_RAW_LEN}-byte cap; not a valid unimatrix-bundle"
            ),
            BundleError::BadScheme => {
                write!(f, "bundle missing '{BUNDLE_SCHEME}' scheme prefix")
            }
            BundleError::BadBase64 => write!(f, "bundle payload is not valid base64url"),
            BundleError::BadJson => write!(f, "bundle payload is not valid JSON"),
            BundleError::Schema(reason) => write!(f, "bundle schema rejected: {reason}"),
        }
    }
}

impl std::error::Error for BundleError {}

/// Run the `client-bundle` subcommand: emit the connection bundle.
///
/// Sync, no tokio (C-10). Resolves the same `data_dir` the listener uses, reads
/// the bearer token + served leaf cert, fingerprints the leaf, derives the public
/// base-url, encodes the bundle, and emits it per the stdout/stderr split.
///
/// # Errors
///
/// Returns [`ServerError`] (naming the path + a fix) when the data directory,
/// token, or cert cannot be read/validated. No panic, no `.unwrap()`.
pub fn run_client_bundle(project_dir: Option<PathBuf>) -> Result<(), ServerError> {
    let paths = project::ensure_data_directory(project_dir.as_deref(), None)
        .map_err(|e| ServerError::ProjectInit(e.to_string()))?;
    let data_dir = paths.data_dir;

    let token_hex = read_token_hex(&data_dir)?;

    let cert_path = data_dir.join("tls").join("cert.pem");
    let cert_pem = fs::read(&cert_path).map_err(|e| {
        ServerError::Config(format!(
            "cannot read cert {}: {e}. Run the server once to provision TLS, or check the data volume.",
            cert_path.display()
        ))
    })?;
    let der = leaf_der_from_pem(&cert_pem)?;
    let fp = fingerprint_leaf_der(&der);

    let base_url = derive_public_url(&Env::from_process()).base_url;

    let blob = encode_bundle(BUNDLE_VERSION, &base_url, &token_hex, &fp)?;

    emit_bundle(&blob, &base_url, &fp);
    Ok(())
}

/// Emit the bundle per the HARD output contract (FR-A5b / NFR-06).
///
/// stdout = the opaque blob ONLY. stderr = base-url + fp echo, TOKEN OMITTED.
/// The exact text is built by [`render_output`] so the contract (blob-only stdout,
/// token-absent stderr) is unit-testable without capturing process fds.
fn emit_bundle(blob: &str, base_url: &str, fp: &str) {
    let (stdout_line, stderr_block) = render_output(blob, base_url, fp);
    // stdout: the opaque blob, nothing else (pipeable). The token is inside it only.
    println!("{stdout_line}");
    // stderr: human echo — base-url + cert-fingerprint ONLY. Token never printed.
    eprint!("{stderr_block}");
}

/// Build the exact `(stdout, stderr)` text the subcommand emits — the testable
/// core of the FR-A5b / NFR-06 output contract.
///
/// - stdout is EXACTLY the opaque blob (one line, no prose, pipeable).
/// - stderr is the base-url + cert-fingerprint echo ONLY; the token is never
///   placed in it. Each stderr line ends in `\n`.
fn render_output(blob: &str, base_url: &str, fp: &str) -> (String, String) {
    let mut stderr = String::new();
    stderr.push_str("unimatrix connection bundle (paste into: unimatrix init --remote <bundle>)\n");
    stderr.push_str(&format!("  base-url : {base_url}\n"));
    stderr.push_str(&format!("  cert-fp  : {fp}\n"));
    if base_url.contains("<EDIT-ME>") {
        stderr.push_str(
            "  WARNING  : UNIMATRIX_PUBLIC_URL is unset — base-url is a placeholder. \
             Set it and re-run before distributing this bundle.\n",
        );
    }
    (blob.to_string(), stderr)
}

/// Read and validate the bearer token (64 lowercase hex) from `{data_dir}/token`.
///
/// Read-only: never generates, never prints (the generate path in `http::token`
/// prints to stdout, which would contaminate the bundle blob). The token is NOT
/// logged here.
///
/// # Errors
///
/// [`ServerError::Config`] when the token file is missing or malformed.
fn read_token_hex(data_dir: &Path) -> Result<String, ServerError> {
    let token_path = data_dir.join(TOKEN_FILE_NAME);
    let content = fs::read_to_string(&token_path).map_err(|e| {
        ServerError::Config(format!(
            "cannot read token {}: {e}. Run the server once to provision it, or check the data volume.",
            token_path.display()
        ))
    })?;
    let trimmed = content.trim_end();
    if trimmed.len() != TOKEN_HEX_LEN || !trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ServerError::Config(format!(
            "token file {} must contain exactly {TOKEN_HEX_LEN} hex characters",
            token_path.display()
        )));
    }
    // Canonicalize to lowercase hex (the wire form / TOKEN_RE is lowercase).
    Ok(trimmed.to_ascii_lowercase())
}

/// Encode a canonical bundle into the `unimatrix-bundle:<base64url-nopad>` wire form.
///
/// Field order is fixed by the [`Bundle`] struct declaration order (`v, base_url,
/// token, fp`); base64url is RFC 4648 §5, URL-safe alphabet, no padding.
///
/// # Errors
///
/// [`ServerError::Config`] only if canonical JSON serialization fails (it cannot
/// for these owned `String`/`u8` fields, but the error is propagated, not unwrapped).
pub fn encode_bundle(
    v: u8,
    base_url: &str,
    token_hex: &str,
    fp: &str,
) -> Result<String, ServerError> {
    let bundle = Bundle {
        v,
        base_url: base_url.to_string(),
        token: token_hex.to_string(),
        fp: fp.to_string(),
    };
    let json = serde_json::to_string(&bundle)
        .map_err(|e| ServerError::Config(format!("bundle JSON encode failed: {e}")))?;
    let b64 = URL_SAFE_NO_PAD.encode(json.as_bytes());
    Ok(format!("{BUNDLE_SCHEME}{b64}"))
}

/// Decode + strict-validate a pasted bundle (the shared trust-boundary algorithm).
///
/// This mirrors the JS production decoder (remote-client.md) byte-for-byte in
/// guard ordering. The Rust side is used for round-trip + corpus parity tests.
///
/// Guard ordering is non-negotiable (ADR-001, FR-B9):
/// 1. LENGTH CAP — on the RAW string, BEFORE any decode/parse (DoS pre-filter).
/// 2. scheme prefix.
/// 3. base64url-decode (no pad).
/// 4. JSON parse.
/// 5. STRICT SCHEMA (load-bearing): exactly `{v, base_url, token, fp}`, correct shapes.
///
/// An over-cap raw string that is NOT valid base64url MUST still reject on
/// **length** (GUARD 1), not on a decode error (AC-W1-C10).
pub fn decode_bundle(raw: &str) -> Result<Bundle, BundleError> {
    // GUARD 1 — length cap FIRST, on the raw bytes, before any decode/parse.
    if raw.len() > MAX_RAW_LEN {
        return Err(BundleError::TooLong);
    }

    // GUARD 2 — scheme prefix.
    let body = raw
        .strip_prefix(BUNDLE_SCHEME)
        .ok_or(BundleError::BadScheme)?;

    // GUARD 3 — base64url-decode (no pad).
    let bytes = URL_SAFE_NO_PAD
        .decode(body)
        .map_err(|_| BundleError::BadBase64)?;

    // GUARD 4 — JSON parse (to a generic Value so we can enforce EXACT keys).
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| BundleError::BadJson)?;

    // GUARD 5 — STRICT SCHEMA (load-bearing).
    validate_schema(value)
}

/// Strict-schema guard: exactly the four keys, correct types/shapes (load-bearing).
fn validate_schema(value: serde_json::Value) -> Result<Bundle, BundleError> {
    let obj = value
        .as_object()
        .ok_or_else(|| BundleError::Schema("payload is not a JSON object".to_string()))?;

    // Exactly four keys — missing OR extra rejects.
    if obj.len() != 4 {
        return Err(BundleError::Schema(format!(
            "expected exactly 4 keys (v, base_url, token, fp), found {}",
            obj.len()
        )));
    }
    for key in obj.keys() {
        if !matches!(key.as_str(), "v" | "base_url" | "token" | "fp") {
            return Err(BundleError::Schema(format!("unexpected key '{key}'")));
        }
    }

    // v == BUNDLE_VERSION (unknown major rejects — forward-compat).
    let v = obj
        .get("v")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| BundleError::Schema("'v' must be an integer".to_string()))?;
    if v != u64::from(BUNDLE_VERSION) {
        return Err(BundleError::Schema(format!(
            "unsupported bundle version {v} (expected {BUNDLE_VERSION})"
        )));
    }

    let base_url = obj
        .get("base_url")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| BundleError::Schema("'base_url' must be a string".to_string()))?;
    if !base_url.starts_with("https://") {
        return Err(BundleError::Schema(
            "'base_url' must be https://".to_string(),
        ));
    }

    let token = obj
        .get("token")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| BundleError::Schema("'token' must be a string".to_string()))?;
    if !is_token(token) {
        // Reason carries no token value (never echo a malformed credential).
        return Err(BundleError::Schema(
            "'token' must be 64 lowercase hex characters".to_string(),
        ));
    }

    let fp = obj
        .get("fp")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| BundleError::Schema("'fp' must be a string".to_string()))?;
    if !is_fingerprint(fp) {
        return Err(BundleError::Schema(
            "'fp' must match sha256:<64 lowercase hex>".to_string(),
        ));
    }

    Ok(Bundle {
        v: BUNDLE_VERSION,
        base_url: base_url.to_string(),
        token: token.to_string(),
        fp: fp.to_string(),
    })
}

/// `^[0-9a-f]{64}$` — 64 lowercase hex characters.
fn is_token(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// `^sha256:[0-9a-f]{64}$`.
fn is_fingerprint(s: &str) -> bool {
    match s.strip_prefix("sha256:") {
        Some(hex) => is_token(hex),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for the PRIVATE helpers (output split, token reader, validators).
    //! Public-API tests (encode/decode/guard ordering/schema) + the C1 golden
    //! oracle live in `tests/bundle_codec.rs` (integration), mirroring the
    //! FingerprintComputer oracle precedent (`tests/fingerprint_parity.rs`) and
    //! keeping this source file under the 500-line cap.

    use super::*;

    const TEST_TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const TEST_FP: &str = "sha256:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
    const TEST_BASE_URL: &str = "https://cloud.example:8443";

    // --- stdout/stderr split + token redaction (AC-W1-S5b, NFR-06) ---

    #[test]
    fn test_client_bundle_stdout_is_opaque_blob_only() {
        let blob = encode_bundle(BUNDLE_VERSION, TEST_BASE_URL, TEST_TOKEN, TEST_FP).unwrap();
        let (stdout, _stderr) = render_output(&blob, TEST_BASE_URL, TEST_FP);
        // stdout is EXACTLY the blob — no prose, no extra lines, pipeable.
        assert_eq!(stdout, blob);
        assert!(stdout.starts_with("unimatrix-bundle:"));
        assert!(!stdout.contains('\n'), "stdout blob must be a single line");
    }

    #[test]
    fn test_client_bundle_stderr_echoes_base_url_and_fp_only() {
        let blob = encode_bundle(BUNDLE_VERSION, TEST_BASE_URL, TEST_TOKEN, TEST_FP).unwrap();
        let (_stdout, stderr) = render_output(&blob, TEST_BASE_URL, TEST_FP);
        assert!(stderr.contains(TEST_BASE_URL), "stderr must echo base-url");
        assert!(
            stderr.contains(TEST_FP),
            "stderr must echo cert fingerprint"
        );
    }

    #[test]
    fn test_client_bundle_token_absent_from_stdout_and_stderr() {
        // LOAD-BEARING: the token hex must appear in NEITHER stdout NOR stderr —
        // it lives ONLY inside the base64url blob payload on stdout.
        let blob = encode_bundle(BUNDLE_VERSION, TEST_BASE_URL, TEST_TOKEN, TEST_FP).unwrap();
        let (stdout, stderr) = render_output(&blob, TEST_BASE_URL, TEST_FP);
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
        let placeholder_url = "https://<EDIT-ME>:8443";
        let blob = encode_bundle(BUNDLE_VERSION, placeholder_url, TEST_TOKEN, TEST_FP).unwrap();
        let (_stdout, stderr) = render_output(&blob, placeholder_url, TEST_FP);
        assert!(
            stderr.contains("<EDIT-ME>"),
            "placeholder must be visible on stderr"
        );
        assert!(
            stderr.contains("WARNING"),
            "placeholder must carry a warning"
        );
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
}
