//! C1 writer tests (vnc-050 test-plan/token-table.md: writers, R-05.5, R-12).

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::{Arc, Mutex};

use sha2::{Digest, Sha256};

use super::super::{TOKENS_FILE, TokenTable};
use super::*;
use crate::http::principal::TokenMode;
use crate::http::router::ProjectSlug;

fn slug(s: &str) -> ProjectSlug {
    ProjectSlug::try_from(s).unwrap()
}

fn file_digest(dir: &Path) -> Vec<u8> {
    Sha256::digest(std::fs::read(dir.join(TOKENS_FILE)).unwrap()).to_vec()
}

fn table() -> (tempfile::TempDir, TokenTable) {
    let d = tempfile::tempdir().unwrap();
    let t = TokenTable::open(d.path());
    (d, t)
}

fn mode_of(p: &Path) -> u32 {
    std::fs::metadata(p).unwrap().permissions().mode() & 0o777
}

#[test]
fn mint_returns_32_bytes_and_stores_sha256_only() {
    let (d, t) = table();
    let raw = t.mint("ci", TokenMode::Read, &slug("alpha")).unwrap();
    let body = std::fs::read_to_string(d.path().join(TOKENS_FILE)).unwrap();
    assert!(body.contains(&hex::encode(Sha256::digest(raw))));
    assert!(!body.contains(&hex::encode(raw)));
    assert!(body.contains(r#""revoked_at": null"#), "{body}");
    let hit = t.lookup_ct(&raw).expect("minted token resolves");
    assert_eq!(hit.label, "ci");
    assert_eq!(hit.mode, TokenMode::Read);
}

#[test]
fn mint_rejects_bad_label() {
    let (d, t) = table();
    for bad in ["", "-x", "UP", &"a".repeat(64)] {
        assert!(
            t.mint(bad, TokenMode::Read, &slug("alpha")).is_err(),
            "{bad}"
        );
    }
    assert!(!d.path().join(TOKENS_FILE).exists());
}

#[test]
fn mint_existing_label_fails() {
    let (d, t) = table();
    t.mint("ci", TokenMode::Read, &slug("alpha")).unwrap();
    let before = file_digest(d.path());
    assert!(t.mint("ci", TokenMode::ReadWrite, &slug("beta")).is_err());
    assert_eq!(file_digest(d.path()), before);
}

#[test]
fn mint_revoked_label_fails() {
    let (d, t) = table();
    t.mint("ci", TokenMode::Read, &slug("alpha")).unwrap();
    t.revoke("ci", &slug("alpha")).unwrap();
    let before = file_digest(d.path());
    assert!(t.mint("ci", TokenMode::Read, &slug("alpha")).is_err());
    assert_eq!(file_digest(d.path()), before);
}

#[test]
fn revoke_sets_revoked_at_keeps_record() {
    let (_d, t) = table();
    let raw = t.mint("ci", TokenMode::ReadWrite, &slug("alpha")).unwrap();
    let before = t.list_for(&slug("alpha"));
    t.revoke("ci", &slug("alpha")).unwrap();
    // Revocation at the real read boundary: next lookup rejects.
    assert_eq!(t.lookup_ct(&raw), None);
    let after = t.list_for(&slug("alpha"));
    assert_eq!(after.len(), 1);
    let (b, a) = (&before[0], &after[0]);
    assert_eq!(
        (&a.label, a.mode, &a.allowed_slugs, a.created_at),
        (&b.label, b.mode, &b.allowed_slugs, b.created_at)
    );
    assert!(b.revoked_at.is_none() && a.revoked_at.is_some());
}

#[test]
fn mint_after_revoke_preserves_tombstone() {
    let (_d, t) = table();
    let old = t.mint("old", TokenMode::Read, &slug("alpha")).unwrap();
    t.revoke("old", &slug("alpha")).unwrap();
    t.mint("new", TokenMode::Read, &slug("alpha")).unwrap();
    let rows = t.list_for(&slug("alpha"));
    assert!(
        rows.iter()
            .any(|r| r.label == "old" && r.revoked_at.is_some())
    );
    assert_eq!(t.lookup_ct(&old), None);
}

#[test]
fn revoke_unknown_or_already_revoked_or_other_slug_errors() {
    let (d, t) = table();
    t.mint("ci", TokenMode::Read, &slug("alpha")).unwrap();
    let before = file_digest(d.path());
    assert!(t.revoke("nope", &slug("alpha")).is_err());
    let other = t.revoke("ci", &slug("beta")).unwrap_err().to_string();
    assert!(
        !other.contains("alpha"),
        "does not reveal other slugs: {other}"
    );
    assert_eq!(file_digest(d.path()), before);
    t.revoke("ci", &slug("alpha")).unwrap();
    let before = file_digest(d.path());
    assert!(t.revoke("ci", &slug("alpha")).is_err());
    assert_eq!(file_digest(d.path()), before);
}

#[test]
fn list_for_slug_filters_and_shows_revoked() {
    let (_d, t) = table();
    t.mint("a1", TokenMode::Read, &slug("alpha")).unwrap();
    t.mint("a2", TokenMode::ReadWrite, &slug("alpha")).unwrap();
    t.mint("b1", TokenMode::Read, &slug("beta")).unwrap();
    t.revoke("a2", &slug("alpha")).unwrap();
    let mut rows = t.list_for(&slug("alpha"));
    rows.sort_by(|x, y| x.label.cmp(&y.label));
    let labels: Vec<_> = rows.iter().map(|r| r.label.as_str()).collect();
    assert_eq!(labels, ["a1", "a2"]);
    assert!(rows[1].revoked_at.is_some() && rows[0].revoked_at.is_none());
}

#[test]
fn corrupt_file_blocks_writers_and_list_checked() {
    let (d, t) = table();
    std::fs::write(d.path().join(TOKENS_FILE), b"{not json").unwrap();
    assert!(t.mint("ci", TokenMode::Read, &slug("alpha")).is_err());
    assert!(t.list_for_checked(&slug("alpha")).is_err());
    assert!(t.list_for(&slug("alpha")).is_empty());
    assert_eq!(
        std::fs::read(d.path().join(TOKENS_FILE)).unwrap(),
        b"{not json"
    );
}

#[test]
fn twenty_parallel_mints_all_present() {
    let d = tempfile::tempdir().unwrap();
    let dir = Arc::new(d.path().to_path_buf());
    let handles: Vec<_> = (0..20)
        .map(|i| {
            let dir = Arc::clone(&dir);
            // Separate TokenTable per thread: contention is resolved by flock only.
            std::thread::spawn(move || {
                TokenTable::open(&dir)
                    .mint(&format!("t{i}"), TokenMode::Read, &slug("alpha"))
                    .unwrap()
            })
        })
        .collect();
    let raws: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let t = TokenTable::open(d.path());
    assert_eq!(t.list_for_checked(&slug("alpha")).unwrap().len(), 20);
    for raw in raws {
        assert!(t.lookup_ct(&raw).is_some());
    }
}

#[test]
fn reader_never_sees_partial_file() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().to_path_buf();
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let reader = {
        let (path, stop) = (path.clone(), Arc::clone(&stop));
        std::thread::spawn(move || {
            let mut reads = 0usize;
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                if let Ok(b) = std::fs::read(path.join(TOKENS_FILE)) {
                    assert!(
                        serde_json::from_slice::<serde_json::Value>(&b).is_ok(),
                        "partial read"
                    );
                    reads += 1;
                }
            }
            reads
        })
    };
    let t = TokenTable::open(&path);
    for i in 0..200 {
        t.mint(&format!("t{i}"), TokenMode::Read, &slug("alpha"))
            .unwrap();
    }
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(reader.join().unwrap() > 0);
}

#[test]
fn file_and_tmp_mode_0600() {
    let (d, t) = table();
    let dir = d.path().to_path_buf();
    let seen: Arc<Mutex<Vec<u32>>> = Arc::default();
    {
        let (dir, seen) = (dir.clone(), Arc::clone(&seen));
        // Global hook: only record tmp files under this test's dir.
        *super::super::AFTER_TMP_CREATE.lock().unwrap() = Some(Box::new(move |p: &Path| {
            if p.starts_with(&dir) {
                seen.lock().unwrap().push(mode_of(p));
            }
        }));
    }
    let r = t.mint("ci", TokenMode::Read, &slug("alpha"));
    *super::super::AFTER_TMP_CREATE.lock().unwrap() = None;
    r.unwrap();
    assert_eq!(*seen.lock().unwrap(), vec![0o600]);
    assert_eq!(mode_of(&dir.join(TOKENS_FILE)), 0o600);
    assert_eq!(mode_of(&dir.join(TOKENS_LOCK)), 0o600);
}

#[test]
fn no_tmp_left_after_write_and_lock_file_created_if_missing() {
    let (d, t) = table();
    std::fs::write(d.path().join(TOKENS_TMP), b"stale").unwrap();
    t.mint("ci", TokenMode::Read, &slug("alpha")).unwrap();
    let mut names: Vec<_> = std::fs::read_dir(d.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(names, [TOKENS_FILE, TOKENS_LOCK]);
}
