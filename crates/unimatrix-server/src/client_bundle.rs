//! C1 connection-bundle codec — `client-bundle` sync subcommand (vnc-038, ADR-002).
//!
//! Encodes the `v:2` connection bundle `{v, mcp_url, observe_url, token, fp}` into
//! the LOCKED wire form `unimatrix-bundle:<base64url-nopad(canonical-json)>` and
//! emits it for the operator to paste into `unimatrix init --bundle <bundle>`.
//!
//! ## The dumb-client invariant (ADR-001/002)
//!
//! The server is the SOLE authority on route shape. This encoder composes BOTH the
//! MCP and observe endpoint URLs from one route-grammar helper
//! ([`compose_route_urls`]) — the SAME grammar `parse_project_key` routes by — so a
//! bundle URL can never disagree with the live route. The client posts these
//! finished URLs verbatim and composes no paths.
//!
//! This is the **build-first encoder** (the C1 oracle): the Rust side is the only
//! encoder; the JS client only decodes (bundle.js), so the canonical form and the
//! parity corpus are stable. [`decode_bundle`] mirrors the exact JS guard-ordering
//! algorithm so round-trip + corpus tests pin both stacks together.
//!
//! ## Output contract (HARD — ADR-008 / NFR-06)
//!
//! - **stdout** = the opaque `unimatrix-bundle:…` blob ONLY. One line, pipeable,
//!   zero contamination. The token lives ONLY inside this base64url blob — it is
//!   the SOLE token-delivery channel (ADR-008).
//! - **stderr** = human echo of `mcp_url` + `observe_url` + `cert-fingerprint`
//!   ONLY. The token is NEVER printed to stderr, and never to any log line.
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
use crate::http::{ProjectSlug, fingerprint_leaf_der, leaf_der_from_pem};
use crate::project;

/// Literal scheme prefix of the wire form (ADR-001). The client rejects anything
/// without it.
pub const BUNDLE_SCHEME: &str = "unimatrix-bundle:";

/// Bundle schema version. The client rejects unknown major versions.
///
/// `v:2` (vnc-038, ADR-002): the bundle carries server-composed `mcp_url` +
/// `observe_url` instead of a bare `base_url`. A `v:1` bundle fails closed on both
/// sides with a re-issue message (R-04) — there is NO compat arm.
pub const BUNDLE_VERSION: u8 = 2;

/// 4 KB cap on the RAW pasted string (bytes), enforced BEFORE decode/parse.
/// Belt-and-suspenders DoS pre-filter; the strict schema is the load-bearing guard.
pub const MAX_RAW_LEN: usize = 4096;

/// Token file name within the data directory (mirrors `http::token`).
const TOKEN_FILE_NAME: &str = "token";

/// Expected hex-encoded token length (32 bytes -> 64 hex chars).
const TOKEN_HEX_LEN: usize = 64;

/// Canonical bundle payload (`v:2`, ADR-002).
///
/// Field declaration order IS the canonical JSON key order (`v, mcp_url,
/// observe_url, token, fp`) — `serde_json` (with no map reordering) serializes
/// struct fields in declaration order, guaranteeing a stable, fixture-stable wire
/// form (ADR-002). Never build this from a `HashMap` (key order undefined).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Bundle {
    /// Schema version (always [`BUNDLE_VERSION`]).
    pub v: u8,
    /// Server-composed MCP root URL, e.g. `"https://cloud.example:8443/v1/alpha"`.
    /// Must be `https://`. Posted by the client verbatim (ADR-001).
    pub mcp_url: String,
    /// Server-composed observe URL, e.g.
    /// `"https://cloud.example:8443/v1/alpha/observe"`. Must be `https://`. Posted
    /// by the client verbatim (ADR-001).
    pub observe_url: String,
    /// 64 lowercase-hex bearer token. Carried ONLY inside the encoded blob (the
    /// sole token-delivery channel, ADR-008).
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

/// Compose the per-slug MCP and observe URLs from the public base (ADR-002).
///
/// This is the SINGLE route-grammar owner on the encode path. It mirrors the route
/// grammar `parse_project_key` resolves by (`/v1/{slug}/...` and
/// `/v1/{slug}/observe`), so the bundle URLs can NEVER disagree with the live
/// route. `public_base` is normalized defensively (a single trailing `/` stripped)
/// so the composed URLs never carry a doubled separator.
fn compose_route_urls(public_base: &str, slug: &ProjectSlug) -> (String, String) {
    let base = public_base.trim_end_matches('/');
    let mcp_url = format!("{base}/v1/{slug}");
    let observe_url = format!("{base}/v1/{slug}/observe");
    (mcp_url, observe_url)
}

/// Run the `client-bundle <slug>` subcommand: emit the per-project connection bundle.
///
/// Sync, no tokio (C-10). Resolves the same `data_dir` the listener uses, validates
/// the `<slug>` at the edge, reads the bearer token + served leaf cert, fingerprints
/// the leaf, derives the public base-url, composes the per-slug MCP + observe URLs
/// (ADR-002), encodes the `v:2` bundle, and emits it per the stdout/stderr split.
///
/// The `<slug>` is mandatory: there is NO default-aliased bundle (ADR-001/004). An
/// absent or invalid slug is a loud [`ServerError::Config`], never a silent default.
///
/// # Errors
///
/// Returns [`ServerError`] (naming the path/slug + a fix) when the slug is invalid
/// or the data directory, token, or cert cannot be read/validated. No panic, no
/// `.unwrap()`.
pub fn run_client_bundle(project_dir: Option<PathBuf>, slug_arg: &str) -> Result<(), ServerError> {
    let slug = ProjectSlug::try_from(slug_arg).map_err(|_| {
        ServerError::Config(format!(
            "client-bundle requires a registered <slug> matching ^[a-z0-9][a-z0-9-]{{0,62}}$; \
             got {slug_arg:?}. Register a project first (unimatrix register <slug>)."
        ))
    })?;

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

    let public_base = derive_public_url(&Env::from_process()).base_url;
    let (mcp_url, observe_url) = compose_route_urls(&public_base, &slug);

    let blob = encode_bundle(BUNDLE_VERSION, &mcp_url, &observe_url, &token_hex, &fp)?;

    emit_bundle(&blob, &mcp_url, &observe_url, &fp);
    Ok(())
}

/// Emit the bundle per the HARD output contract (ADR-008 / NFR-06).
///
/// stdout = the opaque blob ONLY. stderr = mcp-url + observe-url + fp echo, TOKEN
/// OMITTED. The exact text is built by [`render_output`] so the contract (blob-only
/// stdout, token-absent stderr) is unit-testable without capturing process fds.
fn emit_bundle(blob: &str, mcp_url: &str, observe_url: &str, fp: &str) {
    let (stdout_line, stderr_block) = render_output(blob, mcp_url, observe_url, fp);
    // stdout: the opaque blob, nothing else (pipeable). The token is inside it only.
    println!("{stdout_line}");
    // stderr: human echo — URLs + cert-fingerprint ONLY. Token never printed.
    eprint!("{stderr_block}");
}

/// Build the exact `(stdout, stderr)` text the subcommand emits — the testable
/// core of the ADR-008 / NFR-06 output contract.
///
/// - stdout is EXACTLY the opaque blob (one line, no prose, pipeable).
/// - stderr is the mcp-url + observe-url + cert-fingerprint echo ONLY; the token is
///   never placed in it. Each stderr line ends in `\n`.
fn render_output(blob: &str, mcp_url: &str, observe_url: &str, fp: &str) -> (String, String) {
    let mut stderr = String::new();
    stderr.push_str("unimatrix connection bundle (paste into: unimatrix init --bundle <bundle>)\n");
    stderr.push_str(&format!("  mcp-url     : {mcp_url}\n"));
    stderr.push_str(&format!("  observe-url : {observe_url}\n"));
    stderr.push_str(&format!("  cert-fp     : {fp}\n"));
    if mcp_url.contains("<EDIT-ME>") {
        stderr.push_str(
            "  WARNING     : UNIMATRIX_PUBLIC_URL is unset — the URLs carry a placeholder. \
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

/// Encode a canonical `v:2` bundle into the `unimatrix-bundle:<base64url-nopad>`
/// wire form.
///
/// Field order is fixed by the [`Bundle`] struct declaration order (`v, mcp_url,
/// observe_url, token, fp`); base64url is RFC 4648 §5, URL-safe alphabet, no
/// padding.
///
/// # Errors
///
/// [`ServerError::Config`] only if canonical JSON serialization fails (it cannot
/// for these owned `String`/`u8` fields, but the error is propagated, not unwrapped).
pub fn encode_bundle(
    v: u8,
    mcp_url: &str,
    observe_url: &str,
    token_hex: &str,
    fp: &str,
) -> Result<String, ServerError> {
    let bundle = Bundle {
        v,
        mcp_url: mcp_url.to_string(),
        observe_url: observe_url.to_string(),
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
/// 5. STRICT SCHEMA (load-bearing): exactly `{v, mcp_url, observe_url, token, fp}`,
///    `v == 2`, both URLs `https://`, correct shapes.
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

/// Strict-schema guard: exactly the five `v:2` keys, correct types/shapes
/// (load-bearing). A `v:1`-shaped payload fails closed here (R-04) — there is no
/// `base_url` acceptance path and no compat arm.
fn validate_schema(value: serde_json::Value) -> Result<Bundle, BundleError> {
    let obj = value
        .as_object()
        .ok_or_else(|| BundleError::Schema("payload is not a JSON object".to_string()))?;

    // Exactly five keys — missing OR extra rejects.
    if obj.len() != 5 {
        return Err(BundleError::Schema(format!(
            "expected exactly 5 keys (v, mcp_url, observe_url, token, fp), found {}",
            obj.len()
        )));
    }
    for key in obj.keys() {
        if !matches!(
            key.as_str(),
            "v" | "mcp_url" | "observe_url" | "token" | "fp"
        ) {
            return Err(BundleError::Schema(format!("unexpected key '{key}'")));
        }
    }

    // v == BUNDLE_VERSION (unknown major rejects — forward-compat; v:1 fails here).
    let v = obj
        .get("v")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| BundleError::Schema("'v' must be an integer".to_string()))?;
    if v != u64::from(BUNDLE_VERSION) {
        return Err(BundleError::Schema(format!(
            "unsupported bundle version {v} (expected {BUNDLE_VERSION}); re-issue the bundle with `unimatrix client-bundle <slug>`"
        )));
    }

    let mcp_url = obj
        .get("mcp_url")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| BundleError::Schema("'mcp_url' must be a string".to_string()))?;
    if !mcp_url.starts_with("https://") {
        return Err(BundleError::Schema(
            "'mcp_url' must be https://".to_string(),
        ));
    }

    let observe_url = obj
        .get("observe_url")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| BundleError::Schema("'observe_url' must be a string".to_string()))?;
    if !observe_url.starts_with("https://") {
        return Err(BundleError::Schema(
            "'observe_url' must be https://".to_string(),
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
        mcp_url: mcp_url.to_string(),
        observe_url: observe_url.to_string(),
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

/// Unit tests for private helpers — split into a sibling file (via `#[path]`) to
/// keep this source file under the 500-line cap (C-06).
#[cfg(test)]
#[path = "client_bundle_tests.rs"]
mod tests;
