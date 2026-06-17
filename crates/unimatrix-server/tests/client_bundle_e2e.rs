//! End-to-end `client-bundle` subcommand integration — drives the REAL `unimatrix`
//! binary over its process boundary (vnc-034 Wave-1, #726 / C1 / C2).
//!
//! The component tests in `bundle_codec.rs`, `cert_provisioner.rs`, and
//! `fingerprint_parity.rs` prove the codec, provisioner, and oracle in isolation.
//! THIS file proves the contract that is only visible through the compiled binary's
//! real stdout/stderr fds — the thing a unit test on `render_output` cannot:
//!
//! - **AC-W1-S4 / AC-CT-C2** — the `fp` the emitted bundle carries equals an
//!   INDEPENDENT SHA-256 of the leaf DER of the cert on the data volume the server
//!   serves (the bundle pins the served cert, end-to-end through the subcommand).
//! - **AC-W1-S5 / AC-W1-S5b / NFR-06** — the bearer token appears in NEITHER real
//!   stdout NOR real stderr; stdout is the opaque blob ONLY; stderr echoes base-url +
//!   fingerprint only. Captured from the actual child process, not a string helper.
//! - **R-05.3** — the captured stdout blob decodes (via the production `decode_bundle`)
//!   back to `{base_url, token, fp}` — round-trip through the wire form.
//! - **AC-CT-ROT** — rotating the cert on the volume (regenerate) yields a NEW `fp`;
//!   a client still pinned to the OLD `fp` would mismatch. This is the server half of
//!   the rotation contract whose client-side diagnosable rejection lives in the JS
//!   `remote-client.test.js` (`test_pin_mismatch_rejects_with_diagnosable_error`).
//!
//! Hermetic: `HOME` is pointed at a TempDir so `ensure_data_directory` (base = None →
//! `$HOME/.unimatrix/{hash}`) resolves inside the sandbox; nothing leaks into the real
//! home. `UNIMATRIX_PUBLIC_URL` is set so the base-url is concrete (not the placeholder).
//!
//! The cert is provisioned via the SAME production `load_or_generate_cert` the listener
//! uses, so the leaf DER under test is the one the server would serve (SR-01 / AC-W1-S4).

#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};
use tempfile::TempDir;

use unimatrix_server::client_bundle::{Bundle, decode_bundle};
use unimatrix_server::http::{fingerprint_leaf_der, leaf_der_from_pem, load_or_generate_cert};

/// 64 lowercase-hex synthetic bearer token (NOT a provider-shaped secret — lesson #4792).
const SYNTH_TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

const PUBLIC_URL: &str = "https://cloud.example:8443";

/// The registered slug the `client-bundle <slug>` subcommand composes URLs for
/// (vnc-038 ADR-002 — the bundle is per-slug; there is no default-aliased bundle).
const SLUG: &str = "alpha";

/// Server-composed `v:2` URLs the emitted bundle MUST carry (PUBLIC_URL + the
/// `/v1/{slug}` route grammar, mirroring `compose_route_urls`). The client posts
/// these verbatim (ADR-001) — the e2e proof that the bundle is the SOLE route
/// authority.
fn expected_mcp_url() -> String {
    format!("{PUBLIC_URL}/v1/{SLUG}")
}
fn expected_observe_url() -> String {
    format!("{PUBLIC_URL}/v1/{SLUG}/observe")
}

/// Production SAN vector for `cloud.example` (matches `derive_public_url`).
fn sans() -> Vec<String> {
    ["localhost", "127.0.0.1", "0.0.0.0", "cloud.example"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// Replicates `compute_project_hash`: `sha256(canonical_root)[..16]` lowercase hex.
fn project_hash(project_root: &Path) -> String {
    let canonical = project_root
        .canonicalize()
        .expect("canonicalize project root");
    let digest = Sha256::digest(canonical.to_string_lossy().as_bytes());
    format!("{digest:x}")[..16].to_string()
}

/// The data dir the binary will resolve for `--project-dir <root>` under `$HOME=home`.
fn resolved_data_dir(home: &Path, project_root: &Path) -> PathBuf {
    home.join(".unimatrix").join(project_hash(project_root))
}

/// A git-rooted project dir (so `detect_project_root` accepts it without walking up).
fn make_project(tmp: &Path) -> PathBuf {
    let root = tmp.join("proj");
    fs::create_dir_all(root.join(".git")).expect("create .git");
    root
}

/// Provision token + served cert into the resolved data dir, exactly as a first server
/// boot would. Returns (data_dir, served_leaf_der).
fn provision(home: &Path, project_root: &Path) -> (PathBuf, Vec<u8>) {
    let data_dir = resolved_data_dir(home, project_root);
    fs::create_dir_all(&data_dir).expect("create data dir");

    // Token file: the http::token format (hex, optional trailing newline tolerated).
    fs::write(data_dir.join("token"), SYNTH_TOKEN).expect("write token");

    // Served cert via the production provisioner (writes tls/{cert,key}.pem, key 0600).
    let (cert_pem, _key_pem) =
        load_or_generate_cert(&data_dir, &sans()).expect("provision served cert");
    let der = leaf_der_from_pem(&cert_pem).expect("extract served leaf DER");
    (data_dir, der)
}

/// Run the real `unimatrix client-bundle` subcommand against a provisioned data dir.
/// Returns (stdout, stderr) as captured from the child process.
fn run_client_bundle(home: &Path, project_root: &Path) -> (String, String) {
    let exe = env!("CARGO_BIN_EXE_unimatrix");
    // `--project-dir` is a top-level Cli arg (parsed BEFORE the subcommand).
    let out = Command::new(exe)
        .arg("--project-dir")
        .arg(project_root)
        .arg("client-bundle")
        // vnc-038 ADR-002: the bundle is per-slug; the subcommand requires the
        // registered <slug> (no default-aliased bundle).
        .arg(SLUG)
        .env("HOME", home)
        .env("UNIMATRIX_PUBLIC_URL", PUBLIC_URL)
        // Keep any developer RUST_LOG out of the captured stderr assertion surface.
        .env_remove("RUST_LOG")
        .output()
        .expect("spawn unimatrix client-bundle");

    assert!(
        out.status.success(),
        "client-bundle must exit 0; status={:?}\nstderr={}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    (
        String::from_utf8(out.stdout).expect("stdout utf8"),
        String::from_utf8(out.stderr).expect("stderr utf8"),
    )
}

/// Extract the single `unimatrix-bundle:` line from captured stdout.
fn blob_line(stdout: &str) -> String {
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(
        lines.len(),
        1,
        "stdout must be EXACTLY one non-empty line (the opaque blob); got: {stdout:?}"
    );
    let line = lines[0];
    assert!(
        line.starts_with("unimatrix-bundle:"),
        "stdout line must be the bundle blob; got: {line}"
    );
    line.to_string()
}

// ===========================================================================
// AC-W1-S4 / AC-CT-C2 — served-cert == bundle fp (end-to-end through the binary)
// ===========================================================================

#[test]
fn test_e2e_bundle_fp_equals_served_leaf_der() {
    let tmp = TempDir::new().unwrap();
    let project = make_project(tmp.path());
    let (_data_dir, served_der) = provision(tmp.path(), &project);

    let (stdout, _stderr) = run_client_bundle(tmp.path(), &project);
    let bundle: Bundle = decode_bundle(&blob_line(&stdout)).expect("decode emitted blob");

    // Independent SHA-256 over the served leaf DER -> canonical sha256:<hex>.
    let expected = format!("sha256:{}", hex::encode(Sha256::digest(&served_der)));
    assert_eq!(
        bundle.fp, expected,
        "emitted bundle fp must equal an independent fingerprint of the SERVED leaf DER"
    );
    // And it must equal the production oracle over the same DER (no second compute path).
    assert_eq!(bundle.fp, fingerprint_leaf_der(&served_der));
    // vnc-038 ADR-002: the bundle carries server-composed `v:2` URLs (mcp_url +
    // observe_url) built from PUBLIC_URL + the `/v1/{slug}` grammar — no bare
    // `base_url`. These are what the client posts verbatim (ADR-001).
    assert_eq!(bundle.v, 2, "emitted bundle is v:2 (no v:1)");
    assert_eq!(
        bundle.mcp_url,
        expected_mcp_url(),
        "mcp_url is PUBLIC_URL + /v1/{SLUG}"
    );
    assert_eq!(
        bundle.observe_url,
        expected_observe_url(),
        "observe_url is PUBLIC_URL + /v1/{SLUG}/observe"
    );
}

// ===========================================================================
// AC-W1-S5 / AC-W1-S5b / NFR-06 — token absent from real stdout AND stderr
// ===========================================================================

#[test]
fn test_e2e_token_absent_from_stdout_and_stderr() {
    let tmp = TempDir::new().unwrap();
    let project = make_project(tmp.path());
    provision(tmp.path(), &project);

    let (stdout, stderr) = run_client_bundle(tmp.path(), &project);

    // The token NEVER appears verbatim in EITHER stream — it lives only inside the
    // base64url blob (which is not the literal hex). Captured from the real fds.
    assert!(
        !stdout.contains(SYNTH_TOKEN),
        "token hex must NOT appear verbatim in stdout"
    );
    assert!(
        !stderr.contains(SYNTH_TOKEN),
        "token hex must NOT appear in stderr"
    );

    // stdout is the opaque blob ONLY (one line). stderr carries the URL + fp echo.
    let blob = blob_line(&stdout);
    assert!(
        stderr.contains(&expected_mcp_url()),
        "stderr echoes the mcp-url"
    );
    assert!(
        stderr.contains(&expected_observe_url()),
        "stderr echoes the observe-url"
    );

    // The blob DOES encode the token (round-trip proves it is carried, just not leaked).
    let bundle = decode_bundle(&blob).expect("decode blob");
    assert_eq!(
        bundle.token, SYNTH_TOKEN,
        "token round-trips inside the blob only"
    );
    assert!(
        stderr.contains(&bundle.fp),
        "stderr echoes the cert fingerprint"
    );
}

// ===========================================================================
// R-05.3 — round-trip: real emitted blob decodes back to the canonical fields
// ===========================================================================

#[test]
fn test_e2e_emitted_blob_round_trips() {
    let tmp = TempDir::new().unwrap();
    let project = make_project(tmp.path());
    let (_data_dir, served_der) = provision(tmp.path(), &project);

    let (stdout, _stderr) = run_client_bundle(tmp.path(), &project);
    let bundle = decode_bundle(&blob_line(&stdout)).expect("decode emitted blob");

    // vnc-038 ADR-002: the real emitted blob round-trips to the canonical `v:2`
    // fields {v, mcp_url, observe_url, token, fp}.
    assert_eq!(bundle.v, 2);
    assert_eq!(bundle.mcp_url, expected_mcp_url());
    assert_eq!(bundle.observe_url, expected_observe_url());
    assert_eq!(bundle.token, SYNTH_TOKEN);
    assert_eq!(bundle.fp, fingerprint_leaf_der(&served_der));
}

// ===========================================================================
// AC-CT-ROT (server half) — rotate cert (regenerate) → NEW fp; old pin would mismatch
// ===========================================================================

#[test]
fn test_e2e_rotation_changes_fp_old_pin_would_mismatch() {
    let tmp = TempDir::new().unwrap();
    let project = make_project(tmp.path());
    let (data_dir, _served_der) = provision(tmp.path(), &project);

    // Bundle #1 — the pin a client would currently hold.
    let (stdout1, _e1) = run_client_bundle(tmp.path(), &project);
    let fp_old = decode_bundle(&blob_line(&stdout1)).expect("decode #1").fp;

    // Rotate: delete the served cert/key so the next bundle run re-provisions a fresh
    // leaf (the operator runbook's "rotate cert" step — a new key => a new DER => new fp).
    let tls = data_dir.join("tls");
    fs::remove_file(tls.join("cert.pem")).expect("remove cert");
    fs::remove_file(tls.join("key.pem")).expect("remove key");
    let (cert_pem2, _k2) = load_or_generate_cert(&data_dir, &sans()).expect("re-provision");
    let der2 = leaf_der_from_pem(&cert_pem2).expect("new leaf DER");

    // Bundle #2 — after rotation, WITHOUT the operator re-pinning yet.
    let (stdout2, _e2) = run_client_bundle(tmp.path(), &project);
    let bundle2 = decode_bundle(&blob_line(&stdout2)).expect("decode #2");

    assert_ne!(
        bundle2.fp, fp_old,
        "rotation must produce a NEW fingerprint (a client pinned to the old fp mismatches)"
    );
    assert_eq!(
        bundle2.fp,
        fingerprint_leaf_der(&der2),
        "new bundle fp pins the freshly-served leaf DER (re-bundle restores reconnect)"
    );
}
