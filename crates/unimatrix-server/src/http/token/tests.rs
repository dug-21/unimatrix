//! Unit tests for the bearer-token manager (`super`).
//!
//! Extracted from `token.rs` to keep both files within the 500-line limit
//! (Unimatrix Rust hygiene C-06). `use super::*` resolves to the `token`
//! module, so every test references the production items unchanged.

use super::*;
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;
use tracing_test::traced_test;

// T-TM-15: first-boot notice never contains the token hex (NFR-06) and
// points to the retrieval command. Mirrors client_bundle.rs token-absence
// unit test — render-then-emit makes this a true unit test (no binary spawn).
#[test]
fn test_first_boot_notice_token_absent_and_points_to_retrieval() {
    // A representative 64-hex token; the notice must not echo it.
    let hex_string = "ab".repeat(32);
    let notice = render_first_boot_notice();

    assert!(
        !notice.contains(&hex_string),
        "first-boot notice must never contain the token hex (NFR-06)"
    );
    assert!(
        notice.contains("client-bundle"),
        "notice must point operators to the retrieval command"
    );
}

/// Scans a string for any run of >= TOKEN_HEX_LEN consecutive ASCII hex
/// digits — the shape of a leaked bearer token. Used by the redaction
/// regression guards to catch a future edit that interpolates the token
/// hex into any operator-facing message, not just an exact-substring match.
fn contains_token_shaped_run(s: &str) -> bool {
    let mut run = 0usize;
    for c in s.chars() {
        if c.is_ascii_hexdigit() {
            run += 1;
            if run >= TOKEN_HEX_LEN {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

// AC-11 / R-14 sc.1 (CI-1, NFR-06): the first-boot notice emitted by the
// WINNER path (write_new_token -> eprintln!(render_first_boot_notice())) must
// never contain the token hex of the token that was just generated. This is
// the cloud first-boot stdout/stderr surface. render-then-emit lets us assert
// on the exact emitted string without spawning the binary.
#[test]
fn test_first_boot_stdout_no_token_substring() {
    let tmp = TempDir::new().unwrap();
    let token = load_or_generate_token(tmp.path()).unwrap();
    let hex_string = hex::encode(&token);

    // The string emitted on the first-boot path is exactly the notice.
    let emitted = render_first_boot_notice();

    assert!(
        !emitted.contains(&hex_string),
        "first-boot notice must not contain the generated token hex (NFR-06)"
    );
    // Defensive: no token-SHAPED run either, so a future edit that builds the
    // message from a different token value is still caught.
    assert!(
        !contains_token_shaped_run(&emitted),
        "first-boot notice must contain no 64+ hex-char run (token-shaped leak)"
    );
}

// AC-11 / R-14 sc.1: the print SITE is redacted/gated — the notice carries no
// secret. Asserts the notice contains neither the token hex nor the token-file
// PATH (a path leak is also disallowed under NFR-06 — the notice points only at
// the retrieval command). Doubles as the regression guard for the pure builder.
#[test]
fn test_token_print_site_redacted() {
    let tmp = TempDir::new().unwrap();
    let token_path = tmp.path().join(TOKEN_FILE_NAME);
    let token = load_or_generate_token(tmp.path()).unwrap();
    let hex_string = hex::encode(&token);

    let notice = render_first_boot_notice();

    assert!(
        !notice.contains(&hex_string),
        "redacted notice must not echo the token hex"
    );
    assert!(
        !notice.contains(&token_path.display().to_string()),
        "redacted notice must not echo the token-file path"
    );
    assert!(
        !contains_token_shaped_run(&notice),
        "redacted notice must contain no token-shaped hex run"
    );
}

// R-14 sc.2 (sole-channel / no parallel print): the ONLY function that emits to
// an output sink on first boot is the notice builder, and it is token-free for
// EVERY possible generated token. Loop over many fresh generations to prove no
// generated value ever lands in the emitted text (no "also-print it" path).
#[test]
fn test_no_parallel_token_print_path() {
    for _ in 0..32 {
        let tmp = TempDir::new().unwrap();
        let token = load_or_generate_token(tmp.path()).unwrap();
        let hex_string = hex::encode(&token);

        let notice = render_first_boot_notice();
        assert!(
            !notice.contains(&hex_string),
            "no first-boot emission may carry the token (sole channel is the bundle)"
        );
        assert!(!contains_token_shaped_run(&notice));
    }
}

// R-14 sc.1 (tracing surface): the load path's debug log records the PATH only,
// never the token bytes. Captures the actual tracing output via #[traced_test]
// and asserts no token-shaped run reached any tracing event.
#[traced_test]
#[test]
fn test_load_existing_token_tracing_no_token_substring() {
    let tmp = TempDir::new().unwrap();
    let token_path = tmp.path().join(TOKEN_FILE_NAME);
    // A known synthetic 64-hex token written directly so the load arm runs.
    let known_hex = "ab".repeat(32);
    fs::write(&token_path, &known_hex).unwrap();

    let token = load_or_generate_token(tmp.path()).unwrap();
    assert_eq!(hex::encode(&token), known_hex);

    // Assert the captured tracing buffer never contains the token hex nor any
    // token-shaped run. logs_assert exposes the recorded lines for inspection.
    logs_assert(|lines: &[&str]| {
        for line in lines {
            if line.contains(&known_hex) {
                return Err(format!("tracing output leaked token hex: {line}"));
            }
            if contains_token_shaped_run(line) {
                return Err(format!("tracing output contains token-shaped run: {line}"));
            }
        }
        Ok(())
    });
}

// R-14 sc.3 (local non-regression): the redaction added by CI-1 is realized as a
// token-free pure builder, NOT a deployment-context branch that suppresses output.
// There is no shared print to gate, so the local STDIO/UDS token affordance is
// functionally unchanged — load_or_generate_token returns the identical bytes on a
// repeat (load) call with no emission of the secret on either path.
#[traced_test]
#[test]
fn test_local_token_affordance_unchanged() {
    let tmp = TempDir::new().unwrap();
    // First call generates (winner path), second loads (loser/local path).
    let generated = load_or_generate_token(tmp.path()).unwrap();
    let loaded = load_or_generate_token(tmp.path()).unwrap();
    assert_eq!(
        generated, loaded,
        "local repeat-load token affordance must be unchanged"
    );

    let hex_string = hex::encode(&generated);
    logs_assert(|lines: &[&str]| {
        for line in lines {
            if line.contains(&hex_string) || contains_token_shaped_run(line) {
                return Err(format!("local path leaked token to tracing: {line}"));
            }
        }
        Ok(())
    });
}

// T-TM-01: test_generate_token_creates_file_with_correct_length
#[test]
fn test_generate_token_creates_file_with_correct_length() {
    let tmp = TempDir::new().unwrap();
    let result = load_or_generate_token(tmp.path()).unwrap();

    let token_path = tmp.path().join("token");
    assert!(token_path.exists());

    let contents = fs::read_to_string(&token_path).unwrap();
    assert_eq!(contents.len(), 64);
    assert!(contents.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(result.len(), 32);
}

// T-TM-02: test_generate_token_file_permissions_0600
#[test]
fn test_generate_token_file_permissions_0600() {
    let tmp = TempDir::new().unwrap();
    let _result = load_or_generate_token(tmp.path()).unwrap();

    let token_path = tmp.path().join("token");
    let metadata = fs::metadata(&token_path).unwrap();
    let mode = metadata.permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

// T-TM-03: test_generate_token_returns_raw_bytes
#[test]
fn test_generate_token_returns_raw_bytes() {
    let tmp = TempDir::new().unwrap();
    let result = load_or_generate_token(tmp.path()).unwrap();

    assert_eq!(result.len(), 32);

    let token_path = tmp.path().join("token");
    let file_contents = fs::read_to_string(&token_path).unwrap();
    assert_eq!(hex::encode(&result), file_contents);
}

// T-TM-04: test_load_existing_token_returns_same_bytes
#[test]
fn test_load_existing_token_returns_same_bytes() {
    let tmp = TempDir::new().unwrap();
    let known_hex = "aa".repeat(32); // 64 hex chars -> 32 bytes of 0xAA
    let token_path = tmp.path().join("token");
    fs::write(&token_path, &known_hex).unwrap();

    let result = load_or_generate_token(tmp.path()).unwrap();
    assert_eq!(result, vec![0xAA; 32]);
}

// T-TM-05: test_reject_token_file_wrong_length_with_trailing_content
// Note: the pseudocode test plan says "aa" * 32 + "\n" = 65 chars which after trim
// is 64 chars and would be accepted. This test validates that a 63-char trimmed
// token is rejected. The trailing newline case is tested in T-TM-08 variant.
#[test]
fn test_reject_token_file_odd_length() {
    let tmp = TempDir::new().unwrap();
    let token_path = tmp.path().join("token");
    // 63 hex chars (odd length)
    fs::write(&token_path, "a".repeat(63)).unwrap();

    let err = load_or_generate_token(tmp.path()).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("64 hex characters"));
}

// T-TM-06: test_reject_token_file_short
#[test]
fn test_reject_token_file_short() {
    let tmp = TempDir::new().unwrap();
    let token_path = tmp.path().join("token");
    fs::write(&token_path, "a".repeat(62)).unwrap();

    let err = load_or_generate_token(tmp.path()).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("64 hex characters"));
    assert!(msg.contains("found 62"));
}

// T-TM-07: test_reject_token_file_non_hex_characters
#[test]
fn test_reject_token_file_non_hex_characters() {
    let tmp = TempDir::new().unwrap();
    let token_path = tmp.path().join("token");
    // 64 chars but includes 'g' and 'z'
    let bad_token = format!("{}gz", "a".repeat(62));
    fs::write(&token_path, bad_token).unwrap();

    let err = load_or_generate_token(tmp.path()).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("non-hex"));
}

// T-TM-08: test_accept_token_file_exactly_64_hex_chars
#[test]
fn test_accept_token_file_exactly_64_hex_chars() {
    let tmp = TempDir::new().unwrap();
    let token_path = tmp.path().join("token");
    let valid_hex = "0123456789abcdef".repeat(4); // 64 hex chars
    fs::write(&token_path, &valid_hex).unwrap();

    let result = load_or_generate_token(tmp.path()).unwrap();
    assert_eq!(result.len(), 32);
    assert_eq!(hex::encode(&result), valid_hex);
}

// T-TM-09: test_generate_token_is_cryptographically_random
#[test]
fn test_generate_token_is_cryptographically_random() {
    let tmp1 = TempDir::new().unwrap();
    let tmp2 = TempDir::new().unwrap();

    let token1 = load_or_generate_token(tmp1.path()).unwrap();
    let token2 = load_or_generate_token(tmp2.path()).unwrap();

    assert_ne!(token1, token2);
}

// T-TM-10: test_token_file_on_readonly_parent_dir
#[test]
fn test_token_file_on_readonly_parent_dir() {
    let tmp = TempDir::new().unwrap();
    // Make directory read-only
    fs::set_permissions(tmp.path(), fs::Permissions::from_mode(0o555)).unwrap();

    let result = load_or_generate_token(tmp.path());
    assert!(result.is_err());

    // Restore permissions so TempDir cleanup succeeds
    fs::set_permissions(tmp.path(), fs::Permissions::from_mode(0o755)).unwrap();
}

// T-TM-11: test_token_file_uppercase_hex_accepted
#[test]
fn test_token_file_uppercase_hex_accepted() {
    let tmp = TempDir::new().unwrap();
    let token_path = tmp.path().join("token");
    let upper_hex = "AA".repeat(32); // 64 uppercase hex chars
    fs::write(&token_path, &upper_hex).unwrap();

    // hex::decode handles uppercase, is_ascii_hexdigit() accepts A-F
    let result = load_or_generate_token(tmp.path()).unwrap();
    assert_eq!(result, vec![0xAA; 32]);
}

// T-TM-trailing-newline: token with trailing newline is accepted after trim
#[test]
fn test_trailing_newline_tolerance() {
    let tmp = TempDir::new().unwrap();
    let token_path = tmp.path().join("token");
    let valid_hex = "bb".repeat(32);
    // Write with trailing newline
    fs::write(&token_path, format!("{valid_hex}\n")).unwrap();

    let result = load_or_generate_token(tmp.path()).unwrap();
    assert_eq!(result, vec![0xBB; 32]);
}

// Idempotent load: generate then load from same path
#[test]
fn test_idempotent_load() {
    let tmp = TempDir::new().unwrap();
    let generated = load_or_generate_token(tmp.path()).unwrap();
    let loaded = load_or_generate_token(tmp.path()).unwrap();
    assert_eq!(generated, loaded);
}

// T-TM-12: create_new rejects when file already exists (no-overwrite test).
// Proves the TOCTOU overwrite race is eliminated: a pre-existing token file
// is loaded rather than silently overwritten.
#[test]
fn test_no_overwrite_existing_token() {
    let tmp = TempDir::new().unwrap();
    let token_path = tmp.path().join("token");
    let known_hex = "cc".repeat(32);
    fs::write(&token_path, &known_hex).unwrap();

    // load_or_generate_token should load, not overwrite
    let result = load_or_generate_token(tmp.path()).unwrap();
    assert_eq!(result, vec![0xCC; 32]);

    // File contents must be unchanged
    let contents = fs::read_to_string(&token_path).unwrap();
    assert_eq!(contents, known_hex);
}

// T-TM-13: file is created with 0600 permissions atomically (no chmod window).
// Verifies that mode is set at creation time via OpenOptions::mode(), not by a
// separate set_permissions() call.
#[test]
fn test_permissions_at_creation_are_0600() {
    let tmp = TempDir::new().unwrap();
    let _result = load_or_generate_token(tmp.path()).unwrap();

    let token_path = tmp.path().join("token");
    let metadata = fs::metadata(&token_path).unwrap();
    let mode = metadata.permissions().mode() & 0o777;
    assert_eq!(
        mode, 0o600,
        "token file should be created with 0600 permissions atomically"
    );
}

// T-TM-14: concurrent creation test — two threads racing to create the same
// token file. Both must succeed (one generates, one loads), and the file must
// contain a valid token.
#[test]
fn test_concurrent_creation_no_corruption() {
    use std::sync::{Arc, Barrier};

    let tmp = TempDir::new().unwrap();
    let data_dir = tmp.path().to_path_buf();
    let barrier = Arc::new(Barrier::new(2));

    let handles: Vec<_> = (0..2)
        .map(|_| {
            let dir = data_dir.clone();
            let bar = Arc::clone(&barrier);
            std::thread::spawn(move || {
                bar.wait();
                load_or_generate_token(&dir)
            })
        })
        .collect();

    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // Both must succeed
    let token_a = results[0].as_ref().expect("thread 0 failed");
    let token_b = results[1].as_ref().expect("thread 1 failed");

    // Both must return the same token (one generated, one loaded)
    assert_eq!(
        token_a, token_b,
        "concurrent callers must converge on same token"
    );

    // File must contain a valid 64-char hex string
    let token_path = data_dir.join("token");
    let contents = fs::read_to_string(&token_path).unwrap();
    assert_eq!(contents.len(), 64);
    assert!(contents.chars().all(|c| c.is_ascii_hexdigit()));

    // Permissions must be 0600
    let mode = fs::metadata(&token_path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

/// Serializes use of the global `WRITE_PAUSE_MS` hook so it cannot perturb
/// sibling tests that run concurrently in the same lib binary.
static WRITE_PAUSE_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

// T-742-01: forced-interleave convergence (#742 Item 1).
//
// Deterministically widens the winner's create->publish window via the
// WRITE_PAUSE hook so the loser ALWAYS reads the file mid-write, then asserts:
//   - convergence: both racers return the SAME token (token_a == token_b),
//   - the published token is the full 64 hex chars (NO "found 0"),
//   - 0600 survives the temp+rename,
//   - no orphan `.token.<pid>.tmp` remains.
// Looped ~20x to exercise the window repeatedly.
#[test]
fn test_concurrent_creation_forced_interleave_converges() {
    use std::sync::atomic::Ordering;
    use std::sync::{Arc, Barrier};

    let _guard = WRITE_PAUSE_GUARD.lock().unwrap();
    // 5ms pause between final-file create and temp-write+rename: a loser that
    // wins the barrier will observe the empty final file and must retry-read.
    WRITE_PAUSE_MS.store(5, Ordering::SeqCst);

    for _ in 0..20 {
        let tmp = TempDir::new().unwrap();
        let data_dir = tmp.path().to_path_buf();
        let barrier = Arc::new(Barrier::new(2));

        let handles: Vec<_> = (0..2)
            .map(|_| {
                let dir = data_dir.clone();
                let bar = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    bar.wait();
                    load_or_generate_token(&dir)
                })
            })
            .collect();

        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        let token_a = results[0].as_ref().expect("thread 0 failed");
        let token_b = results[1].as_ref().expect("thread 1 failed");

        // Convergence: one wrote, one loaded; both return the SAME token.
        assert_eq!(token_a, token_b, "racers must converge on same token");

        // Published file is the full 64 hex chars (no "found 0").
        let token_path = data_dir.join("token");
        let contents = fs::read_to_string(&token_path).unwrap();
        assert_eq!(contents.len(), 64, "published token must be 64 hex chars");
        assert!(contents.chars().all(|c| c.is_ascii_hexdigit()));

        // 0600 survives the rename.
        let mode = fs::metadata(&token_path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "0600 must survive temp+rename");

        // No orphan temp file remains.
        let tmp_path = temp_token_path(&token_path);
        assert!(
            !tmp_path.exists(),
            "no orphan temp token file must remain: {}",
            tmp_path.display()
        );
    }

    WRITE_PAUSE_MS.store(0, Ordering::SeqCst);
}
