//! vnc-050 C7 — `client-bundle --headless | --list | --revoke` through the built
//! binary (R-05/R-09/R-12/R-14, AC-16/AC-17). Exit codes and file effects are
//! observed on the real process; nothing calls the library directly.
//!
//! Synthetic 64-hex legacy token only (lesson #4792).

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};

use sha2::{Digest, Sha256};
use tempfile::TempDir;

use unimatrix_server::client_bundle::decode_bundle;
use unimatrix_server::http::load_or_generate_cert;

const LEGACY_TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const PUBLIC_URL: &str = "https://cloud.example:8443";

/// A provisioned, git-rooted project with `alpha` + `beta` registered.
struct Env {
    _tmp: TempDir,
    home: PathBuf,
    project: PathBuf,
    data_dir: PathBuf,
}

impl Env {
    fn new() -> Self {
        let tmp = TempDir::new().unwrap();
        let home = tmp.path().to_path_buf();
        let project = home.join("proj");
        fs::create_dir_all(project.join(".git")).unwrap();
        let canonical = project.canonicalize().unwrap();
        let digest = Sha256::digest(canonical.to_string_lossy().as_bytes());
        let hash = format!("{digest:x}")[..16].to_string();
        let data_dir = home.join(".unimatrix").join(hash);
        fs::create_dir_all(&data_dir).unwrap();
        fs::write(data_dir.join("token"), LEGACY_TOKEN).unwrap();
        let sans: Vec<String> = ["localhost", "cloud.example"].map(String::from).to_vec();
        load_or_generate_cert(&data_dir, &sans).unwrap();
        fs::write(
            data_dir.join("config.toml"),
            "[[projects]]\nslug = \"alpha\"\n\n[[projects]]\nslug = \"beta\"\n",
        )
        .unwrap();
        Env {
            _tmp: tmp,
            home,
            project,
            data_dir,
        }
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_unimatrix"))
            .arg("--project-dir")
            .arg(&self.project)
            .arg("client-bundle")
            .args(args)
            .env("HOME", &self.home)
            .env("UNIMATRIX_PUBLIC_URL", PUBLIC_URL)
            .env_remove("RUST_LOG")
            .output()
            .expect("spawn unimatrix")
    }

    fn tokens(&self) -> PathBuf {
        self.data_dir.join("tokens.json")
    }

    fn tokens_bytes(&self) -> Option<Vec<u8>> {
        fs::read(self.tokens()).ok()
    }

    fn mint(&self, slug: &str, mode: &str, label: &str, extra: &[&str]) -> Output {
        let mut a = vec![slug, "--headless", "--mode", mode, "--label", label];
        a.extend_from_slice(extra);
        self.run(&a)
    }
}

fn out(o: &Output) -> (String, String) {
    (
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

fn assert_ok(o: &Output) {
    assert!(o.status.success(), "expected success: {:?}", out(o));
}

/// The single stdout blob line.
fn blob(o: &Output) -> String {
    let (stdout, _) = out(o);
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines.len(), 1, "stdout must be one blob line: {stdout:?}");
    lines[0].to_string()
}

#[test]
fn invalid_combos_exit_2_no_io() {
    let env = Env::new();
    let combos: &[&[&str]] = &[
        &["alpha", "--mode", "read"],
        &["alpha", "--label", "ci"],
        &["alpha", "--headless", "--label", "ci"],
        &["alpha", "--headless", "--mode", "read"],
        &["alpha", "--list", "--trust", "ca"],
        &[
            "alpha",
            "--revoke",
            "x",
            "--headless",
            "--mode",
            "read",
            "--label",
            "x",
        ],
        &["alpha", "--list", "--revoke", "x"],
        &["alpha", "--headless", "--mode", "admin", "--label", "ci"],
        &["alpha", "--headless", "--mode", "read", "--label", "BAD"],
        &["alpha", "--trust", "none"],
    ];
    for argv in combos {
        let o = env.run(argv);
        assert_eq!(o.status.code(), Some(2), "{argv:?}: {:?}", out(&o));
        assert!(!env.tokens().exists(), "{argv:?} created tokens.json");
    }
    assert_ok(&env.mint("alpha", "read", "seed", &[]));
    let before = env.tokens_bytes();
    let mtime = fs::metadata(env.tokens()).unwrap().modified().unwrap();
    for argv in combos {
        assert_eq!(env.run(argv).status.code(), Some(2), "{argv:?}");
    }
    assert_eq!(env.tokens_bytes(), before);
    assert_eq!(
        fs::metadata(env.tokens()).unwrap().modified().unwrap(),
        mtime
    );
}

#[test]
fn headless_mint_stdout_blob_only() {
    let env = Env::new();
    let o = env.mint("alpha", "read", "ci-bot", &[]);
    assert_ok(&o);
    let b = decode_bundle(&blob(&o)).expect("decode");
    assert_eq!(b.v, 2);
    assert_eq!(b.mcp_url, format!("{PUBLIC_URL}/v1/alpha"));
    assert_ne!(
        b.token, LEGACY_TOKEN,
        "headless bundle carries the minted token"
    );
    let (_, stderr) = out(&o);
    assert!(!stderr.contains(&b.token), "secret on stderr");
    assert!(stderr.contains("label       : ci-bot") && stderr.contains("mode        : read"));
    let file = String::from_utf8(env.tokens_bytes().unwrap()).unwrap();
    assert!(!file.contains(&b.token), "secret in tokens.json");
    let raw = hex::decode(&b.token).unwrap();
    assert!(
        file.contains(&hex::encode(Sha256::digest(raw))),
        "hash stored"
    );
}

#[test]
fn trust_ca_emits_fp_null() {
    let env = Env::new();
    let o = env.mint("alpha", "read", "ca-bot", &["--trust", "ca"]);
    assert_ok(&o);
    assert_eq!(decode_bundle(&blob(&o)).unwrap().fp, None);
    assert!(out(&o).1.contains("trust       : ca"));
    // Legacy form with --trust ca: legacy token + fp null.
    let o = env.run(&["alpha", "--trust", "ca"]);
    assert_ok(&o);
    let b = decode_bundle(&blob(&o)).unwrap();
    assert_eq!((b.fp, b.token.as_str()), (None, LEGACY_TOKEN));
}

#[test]
fn trust_pin_default_emits_fp() {
    let env = Env::new();
    let o = env.mint("alpha", "read-write", "pin-bot", &[]);
    assert_ok(&o);
    let fp = decode_bundle(&blob(&o)).unwrap().fp.expect("pinned");
    let hex = fp.strip_prefix("sha256:").unwrap();
    assert!(hex.len() == 64 && hex.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f')));
}

#[test]
fn mint_unregistered_slug_errors_no_write() {
    let env = Env::new();
    let o = env.mint("gamma", "read", "ci", &[]);
    assert_eq!(o.status.code(), Some(1), "{:?}", out(&o));
    assert!(out(&o).1.contains("not registered"));
    assert!(out(&o).0.is_empty());
    assert!(!env.tokens().exists());
}

#[test]
fn mint_missing_cert_pin_leaves_no_orphan() {
    let env = Env::new();
    fs::remove_file(env.data_dir.join("tls").join("cert.pem")).unwrap();
    let o = env.mint("alpha", "read", "ci", &[]);
    assert!(!o.status.success());
    assert!(!env.tokens().exists(), "pin failure must precede mint");
}

#[test]
fn list_after_two_mints() {
    let env = Env::new();
    let a = env.mint("alpha", "read", "reader", &[]);
    let b = env.mint("alpha", "read-write", "writer", &[]);
    assert_ok(&a);
    assert_ok(&b);
    assert_ok(&env.mint("beta", "read", "other", &[]));
    let o = env.run(&["alpha", "--list"]);
    assert_ok(&o);
    let (stdout, stderr) = out(&o);
    assert!(stdout.is_empty(), "list writes nothing to stdout");
    for needle in ["reader", "writer", "read-write", "alpha", "active"] {
        assert!(stderr.contains(needle), "missing {needle}: {stderr}");
    }
    assert!(!stderr.contains("other"), "beta token listed for alpha");
    for o in [&a, &b] {
        let tok = decode_bundle(&blob(o)).unwrap().token;
        let hash = hex::encode(Sha256::digest(hex::decode(&tok).unwrap()));
        assert!(!stderr.contains(&tok) && !stderr.contains(&hash));
    }
}

#[test]
fn revoke_then_list_shows_revoked_and_remint_fails() {
    let env = Env::new();
    assert_ok(&env.mint("alpha", "read", "ci", &[]));
    let o = env.run(&["alpha", "--revoke", "ci"]);
    assert_ok(&o);
    assert!(out(&o).1.contains("401"));
    assert!(out(&env.run(&["alpha", "--list"])).1.contains("revoked "));
    let before = env.tokens_bytes();
    let o = env.mint("alpha", "read", "ci", &[]);
    assert_eq!(o.status.code(), Some(1));
    assert!(out(&o).0.is_empty());
    assert_eq!(
        env.tokens_bytes(),
        before,
        "re-mint must not touch the file"
    );
}

#[test]
fn remint_active_label_fails_byte_identical() {
    let env = Env::new();
    assert_ok(&env.mint("alpha", "read", "ci", &[]));
    let before = env.tokens_bytes();
    assert_eq!(env.mint("beta", "read", "ci", &[]).status.code(), Some(1));
    assert_eq!(env.tokens_bytes(), before);
}

#[test]
fn revoke_other_slug_label_errors() {
    let env = Env::new();
    assert_ok(&env.mint("beta", "read", "b-tok", &[]));
    let before = env.tokens_bytes();
    assert_eq!(
        env.run(&["alpha", "--revoke", "b-tok"]).status.code(),
        Some(1)
    );
    assert_eq!(env.tokens_bytes(), before);
}

#[test]
fn revoke_unknown_or_twice_errors() {
    let env = Env::new();
    assert_ok(&env.mint("alpha", "read", "ci", &[]));
    let before = env.tokens_bytes();
    assert_eq!(
        env.run(&["alpha", "--revoke", "nope"]).status.code(),
        Some(1)
    );
    assert_eq!(env.tokens_bytes(), before);
    assert_ok(&env.run(&["alpha", "--revoke", "ci"]));
    let before = env.tokens_bytes();
    assert_eq!(env.run(&["alpha", "--revoke", "ci"]).status.code(), Some(1));
    assert_eq!(env.tokens_bytes(), before);
}

#[test]
fn twenty_parallel_cli_mints() {
    let env = Env::new();
    let exe = env!("CARGO_BIN_EXE_unimatrix");
    let children: Vec<_> = (0..20)
        .map(|i| {
            Command::new(exe)
                .arg("--project-dir")
                .arg(&env.project)
                .args(["client-bundle", "alpha", "--headless", "--mode", "read"])
                .arg("--label")
                .arg(format!("p-{i}"))
                .args(["--trust", "ca"])
                .env("HOME", &env.home)
                .env("UNIMATRIX_PUBLIC_URL", PUBLIC_URL)
                .env_remove("RUST_LOG")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap()
        })
        .collect();
    for mut c in children {
        assert!(c.wait().unwrap().success());
    }
    let v: serde_json::Value = serde_json::from_slice(&env.tokens_bytes().unwrap()).unwrap();
    assert_eq!(v["tokens"].as_array().unwrap().len(), 20);
    let mode = fs::metadata(env.tokens()).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    assert!(!env.data_dir.join("tokens.json.tmp").exists());
}

#[test]
fn existing_interactive_mode_unchanged() {
    let env = Env::new();
    let o = env.run(&["alpha"]);
    assert_ok(&o);
    let b = decode_bundle(&blob(&o)).unwrap();
    assert_eq!(b.token, LEGACY_TOKEN);
    assert!(b.fp.is_some());
    let (_, stderr) = out(&o);
    let expected = format!(
        "unimatrix connection bundle (paste into: unimatrix init --bundle <bundle>)\n  \
         mcp-url     : {PUBLIC_URL}/v1/alpha\n  observe-url : {PUBLIC_URL}/v1/alpha/observe\n  \
         cert-fp     : {}\n",
        b.fp.unwrap()
    );
    assert_eq!(
        stderr, expected,
        "legacy stderr must be byte-identical to pre-vnc-050"
    );
    assert!(
        !env.tokens().exists(),
        "legacy form never touches tokens.json"
    );
}
