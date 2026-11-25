//! C1 token-table read-path tests (vnc-050 test-plan/token-table.md).
//!
//! Also hosts the cumulative `tokens.json` fixture writer used by the C2
//! validator and step-0 HTTP tests.

use std::path::Path;
use std::time::{Duration, SystemTime};

use sha2::{Digest, Sha256};

use super::*;

/// One fixture row: (label, raw token, mode, slugs, revoked).
pub(crate) type FixtureRow<'a> = (&'a str, [u8; 32], &'a str, &'a [&'a str], bool);

/// Lowercase hex SHA-256 of a raw token.
pub(crate) fn hash_hex(raw: &[u8; 32]) -> String {
    hex::encode(Sha256::digest(raw))
}

/// Render a canonical tokens.json body.
pub(crate) fn render(rows: &[FixtureRow<'_>]) -> String {
    let tokens: Vec<serde_json::Value> = rows
        .iter()
        .map(|(label, raw, mode, slugs, revoked)| {
            serde_json::json!({
                "label": label,
                "sha256": hash_hex(raw),
                "mode": mode,
                "allowed_slugs": slugs,
                "created_at": 1_700_000_000u64,
                "revoked_at": if *revoked { Some(1_700_000_100u64) } else { None },
            })
        })
        .collect();
    serde_json::json!({ "v": 1, "tokens": tokens }).to_string()
}

/// Atomically (tmp + rename) write raw bytes as tokens.json.
pub(crate) fn write_raw(dir: &Path, body: &str) {
    let tmp = dir.join("tokens.json.fixture-tmp");
    std::fs::write(&tmp, body).expect("write tmp");
    std::fs::rename(&tmp, dir.join(TOKENS_FILE)).expect("rename");
}

/// Write a fixture table.
pub(crate) fn write_table(dir: &Path, rows: &[FixtureRow<'_>]) {
    write_raw(dir, &render(rows));
}

fn tok(b: u8) -> [u8; 32] {
    [b; 32]
}

fn parse(body: &str) -> Result<Vec<ParsedEntry>, ParseError> {
    parse_file(body.as_bytes())
}

#[test]
fn parse_valid_two_of_each_mode() {
    let body = render(&[
        ("r1", tok(1), "read", &["alpha"], false),
        ("r2", tok(2), "read", &["alpha", "beta"], false),
        ("w1", tok(3), "read-write", &["alpha"], false),
        ("w2", tok(4), "read-write", &["beta"], true),
    ]);
    let e = parse(&body).expect("valid");
    assert_eq!(e.len(), 4);
    assert_eq!(e[0].entry.mode, TokenMode::Read);
    assert_eq!(e[2].entry.mode, TokenMode::ReadWrite);
    assert_eq!(e[1].entry.allowed_slugs.len(), 2);
    assert!(e[3].entry.revoked);
    assert!(!e[0].entry.revoked);
}

#[test]
fn parse_rejects_unknown_key() {
    let top = r#"{"v":1,"tokens":[],"extra":1}"#;
    assert!(parse(top).is_err());
    let mut v: serde_json::Value =
        serde_json::from_str(&render(&[("a", tok(1), "read", &["alpha"], false)])).unwrap();
    v["tokens"][0]["note"] = serde_json::json!("x");
    assert!(parse(&v.to_string()).is_err());
}

#[test]
fn parse_rejects_duplicate_label() {
    let body = render(&[
        ("a", tok(1), "read", &["alpha"], false),
        ("a", tok(2), "read", &["alpha"], true),
    ]);
    assert!(parse(&body).is_err());
}

#[test]
fn parse_rejects_empty_allowed_slugs() {
    assert!(parse(&render(&[("a", tok(1), "read", &[], false)])).is_err());
    assert!(parse(&render(&[("a", tok(1), "read", &["Bad_Slug"], false)])).is_err());
}

#[test]
fn parse_rejects_bad_label_regex() {
    let long64 = "a".repeat(64);
    let long63 = "a".repeat(63);
    for bad in ["Upper", "-lead", long64.as_str(), ""] {
        assert!(
            parse(&render(&[(bad, tok(1), "read", &["alpha"], false)])).is_err(),
            "{bad:?} must be rejected"
        );
    }
    assert!(parse(&render(&[(&long63, tok(1), "read", &["alpha"], false)])).is_ok());
}

#[test]
fn parse_rejects_bad_sha256() {
    let good = hash_hex(&tok(1));
    for bad in [
        good.to_uppercase(),
        good[..63].to_string(),
        format!("{}g", &good[..63]),
    ] {
        let body = format!(
            r#"{{"v":1,"tokens":[{{"label":"a","sha256":"{bad}","mode":"read","allowed_slugs":["alpha"],"created_at":1,"revoked_at":null}}]}}"#
        );
        assert!(parse(&body).is_err());
    }
}

#[test]
fn parse_rejects_bad_mode_and_version() {
    let body = render(&[("a", tok(1), "admin", &["alpha"], false)]);
    assert!(parse(&body).is_err());
    assert!(parse(r#"{"v":2,"tokens":[]}"#).is_err());
}

#[test]
fn parse_empty_tokens_array_ok() {
    assert!(parse(r#"{"v":1,"tokens":[]}"#).expect("ok").is_empty());
}

#[test]
fn open_missing_file_empty_table() {
    let dir = tempfile::tempdir().unwrap();
    let t = TokenTable::open(dir.path());
    assert_eq!(t.lookup_ct(&tok(1)), None);
}

#[test]
fn lookup_hit_miss_and_tombstone() {
    let dir = tempfile::tempdir().unwrap();
    write_table(
        dir.path(),
        &[
            ("first", tok(1), "read", &["alpha"], false),
            ("mid", tok(2), "read-write", &["alpha"], true),
            ("last", tok(3), "read-write", &["beta"], false),
        ],
    );
    let t = TokenTable::open(dir.path());
    assert_eq!(t.lookup_ct(&tok(1)).map(|e| e.label), Some("first".into()));
    assert_eq!(
        t.lookup_ct(&tok(3)).map(|e| e.mode),
        Some(TokenMode::ReadWrite)
    );
    assert_eq!(t.lookup_ct(&tok(2)), None, "tombstone rejected");
    assert_eq!(t.lookup_ct(&tok(9)), None);
    for probe in [tok(1), tok(2), tok(3), tok(9)] {
        assert_eq!(t.lookup_ct_counted(&probe).1, 3, "full scan always");
    }
}

#[test]
fn snapshot_reloads_on_stamp_change() {
    let dir = tempfile::tempdir().unwrap();
    write_table(dir.path(), &[("a", tok(1), "read", &["alpha"], false)]);
    let t = TokenTable::open(dir.path());
    assert!(t.lookup_ct(&tok(2)).is_none());
    write_table(
        dir.path(),
        &[
            ("a", tok(1), "read", &["alpha"], false),
            ("b", tok(2), "read", &["alpha"], false),
        ],
    );
    assert_eq!(t.lookup_ct(&tok(2)).map(|e| e.label), Some("b".into()));
    std::fs::remove_file(dir.path().join(TOKENS_FILE)).unwrap();
    assert!(t.lookup_ct(&tok(1)).is_none(), "deleted file = empty table");
}

#[test]
fn same_mtime_same_len_rewrite_detected_by_inode() {
    let dir = tempfile::tempdir().unwrap();
    let fixed = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    let set_mtime = |p: &Path| {
        std::fs::File::options()
            .write(true)
            .open(p)
            .unwrap()
            .set_modified(fixed)
            .unwrap();
    };
    let path = dir.path().join(TOKENS_FILE);
    write_table(dir.path(), &[("a", tok(1), "read", &["alpha"], false)]);
    set_mtime(&path);
    let t = TokenTable::open(dir.path());
    assert!(t.lookup_ct(&tok(1)).is_some());
    let before = std::fs::metadata(&path).unwrap();
    // Same label length, same mode -> identical byte length; new inode via rename.
    write_table(dir.path(), &[("b", tok(1), "read", &["alpha"], false)]);
    set_mtime(&path);
    let after = std::fs::metadata(&path).unwrap();
    assert_eq!(before.len(), after.len());
    assert_eq!(before.modified().unwrap(), after.modified().unwrap());
    assert_eq!(t.lookup_ct(&tok(1)).map(|e| e.label), Some("b".into()));
}

#[test]
fn corrupt_file_fails_closed_never_last_good() {
    let dir = tempfile::tempdir().unwrap();
    let good = render(&[("a", tok(1), "read", &["alpha"], false)]);
    write_raw(dir.path(), &good);
    let t = TokenTable::open(dir.path());
    assert!(t.lookup_ct(&tok(1)).is_some());
    write_raw(dir.path(), "{not json");
    assert!(t.lookup_ct(&tok(1)).is_none(), "corrupt -> fail closed");
    assert!(t.lookup_ct(&tok(1)).is_none());
    write_raw(dir.path(), &good);
    assert!(t.lookup_ct(&tok(1)).is_some(), "fix -> recovers");
}

#[test]
fn revoked_token_not_resurrected_by_corrupt_file() {
    let dir = tempfile::tempdir().unwrap();
    write_table(dir.path(), &[("a", tok(1), "read", &["alpha"], false)]);
    let t = TokenTable::open(dir.path());
    assert!(t.lookup_ct(&tok(1)).is_some());
    write_table(dir.path(), &[("a", tok(1), "read", &["alpha"], true)]);
    assert!(t.lookup_ct(&tok(1)).is_none());
    write_raw(dir.path(), "garbage");
    assert!(
        t.lookup_ct(&tok(1)).is_none(),
        "corrupt phase never serves last-good"
    );
}
