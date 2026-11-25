//! vnc-050 C5 boot gate: an unrecognized `UNIMATRIX_LEGACY_TOKEN_ENABLED`
//! value fails boot (non-zero exit naming the variable) instead of falling
//! back to compiled defaults, which would re-enable the legacy token.

use std::process::{Command, Stdio};

const VAR: &str = "UNIMATRIX_LEGACY_TOKEN_ENABLED";

fn boot(value: &str) -> (Option<i32>, String) {
    let home = tempfile::tempdir().expect("home");
    let project = home.path().join("p");
    std::fs::create_dir_all(&project).expect("project dir");
    let out = Command::new(env!("CARGO_BIN_EXE_unimatrix"))
        .arg("--project-dir")
        .arg(&project)
        .args(["serve", "--stdio"])
        .env("HOME", home.path())
        .env(VAR, value)
        .env_remove("UNIMATRIX_CONFIG")
        .env_remove("RUST_LOG")
        .stdin(Stdio::null())
        .output()
        .expect("spawn unimatrix");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn bad_env_value_fails_boot() {
    for bad in ["yes", "TRUE "] {
        let (code, stderr) = boot(bad);
        assert_ne!(code, Some(0), "{bad:?}: {stderr}");
        assert!(
            stderr.contains(&format!("{VAR} must be one of true|1|false|0")),
            "{bad:?}: {stderr}"
        );
    }
}

#[test]
fn valid_env_value_passes_config_gate() {
    let (_, stderr) = boot("false");
    assert!(!stderr.contains(&format!("{VAR} must be")), "{stderr}");
}
