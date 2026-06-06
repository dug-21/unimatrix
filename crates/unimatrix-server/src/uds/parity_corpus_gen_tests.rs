//! Non-ignored guard tests for the parity-corpus generator (vnc-026).
//!
//! These run in the default test pass and protect the corpus contract without
//! touching `packages/`: R-02 arm coverage, volatile normalization rules, the
//! read_stdin 1 MiB cap mirror, and construction invariants of the size and
//! multibyte window-edge cases.

use super::*;

#[test]
fn test_generator_branch_coverage() {
    // R-02 scenario 2: a new arm key without a corpus case fails here.
    assert_coverage(&all_cases());
}

#[test]
fn test_cap_stdin_over_cap_truncates_to_boundary_or_empty() {
    let exact = "a".repeat(1_048_576);
    assert_eq!(cap_stdin(&exact).len(), 1_048_576);
    let over = "a".repeat(1_048_577);
    assert_eq!(cap_stdin(&over).len(), 1_048_576);
    // Cap landing mid-char mirrors read_to_string failure → empty buffer.
    let mut multi = "a".repeat(1_048_575);
    multi.push('é'); // 2-byte char straddling the cap
    assert_eq!(cap_stdin(&multi), "");
}

#[test]
fn test_normalize_volatile_timestamp_and_ppid_rewritten() {
    let mut v = serde_json::json!({
        "type": "RecordEvent",
        "event_type": "Stop",
        "session_id": "ppid-12345",
        "timestamp": 1_770_000_000u64,
        "payload": null
    });
    normalize_volatile(&mut v);
    assert_eq!(v["timestamp"], 0);
    assert_eq!(v["session_id"], "ppid-X");

    let mut keep = serde_json::json!({ "session_id": "ppid-x9", "timestamp": "not-a-number" });
    normalize_volatile(&mut keep);
    assert_eq!(keep["session_id"], "ppid-x9");
    assert_eq!(keep["timestamp"], "not-a-number");
}

#[test]
fn test_normalize_volatile_events_array_elements_rewritten() {
    let mut v = serde_json::json!({
        "type": "RecordEvents",
        "events": [
            { "session_id": "ppid-7", "timestamp": 99u64 },
            { "session_id": "sess-keep", "timestamp": 100u64 }
        ]
    });
    normalize_volatile(&mut v);
    assert_eq!(v["events"][0]["session_id"], "ppid-X");
    assert_eq!(v["events"][0]["timestamp"], 0);
    assert_eq!(v["events"][1]["session_id"], "sess-keep");
    assert_eq!(v["events"][1]["timestamp"], 0);
}

#[test]
fn test_normalize_volatile_process_cwd_rewritten() {
    let cwd = std::env::current_dir()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let mut v = serde_json::json!({ "type": "SessionRegister", "cwd": cwd });
    normalize_volatile(&mut v);
    assert_eq!(v["cwd"], "<process-cwd>");

    let mut keep = serde_json::json!({ "cwd": "/explicit/path" });
    normalize_volatile(&mut keep);
    assert_eq!(keep["cwd"], "/explicit/path");
}

#[test]
fn test_case_table_stdin_size_cases_exact_lengths() {
    let cases = all_cases();
    let exact = cases
        .iter()
        .find(|c| c.name == "stdin-exactly-1mib")
        .expect("stdin-exactly-1mib case present");
    assert_eq!(exact.stdin.len(), 1_048_576);
    let over = cases
        .iter()
        .find(|c| c.name == "stdin-over-1mib")
        .expect("stdin-over-1mib case present");
    assert_eq!(over.stdin.len(), 1_048_577);
}

#[test]
fn test_case_table_multibyte_window_edge_cuts_mid_char() {
    let cases = all_cases();
    let case = cases
        .iter()
        .find(|c| c.name == "sas-tail-multibyte-window-edge")
        .expect("sas-tail-multibyte-window-edge case present");
    let transcript = case.transcript.as_ref().expect("has transcript");
    let window = MAX_PRECOMPACT_BYTES * TAIL_MULTIPLIER;
    assert!(
        transcript.len() > window,
        "transcript must exceed the tail window"
    );
    let cut = transcript.len() - window;
    assert!(
        !transcript.is_char_boundary(cut),
        "tail-window cut must land inside a multi-byte char"
    );
}
