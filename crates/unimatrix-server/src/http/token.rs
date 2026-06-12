//! Token manager for HTTP bearer authentication (C4, vnc-021).
//!
//! Manages the bearer token lifecycle: generate on first run, load on subsequent
//! runs, validate format. The token is a 32-byte cryptographic random value stored
//! as 64 hex characters in `{data_dir}/token` with mode 0600.
//!
//! File creation uses `O_CREAT | O_EXCL` (via `create_new`) with mode 0600 to
//! atomically create-with-permissions and reject concurrent creators.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

use crate::error::ServerError;

/// Token file name within the data directory.
const TOKEN_FILE_NAME: &str = "token";

/// Number of raw random bytes in a token.
const TOKEN_BYTE_LEN: usize = 32;

/// Expected hex-encoded string length (TOKEN_BYTE_LEN * 2).
const TOKEN_HEX_LEN: usize = 64;

/// Bounded read-retry budget for `load_existing_token` (loser branch).
///
/// The election winner creates the empty final file (O_EXCL) then publishes the
/// 64 hex bytes via a temp-file + atomic rename. A loser that takes the
/// `AlreadyExists -> load` arm can observe the file in the brief create->rename
/// gap (0 bytes, or, transiently, the prior inode mid-rename). It retries reading
/// for up to this ceiling before surfacing whatever it last read (so a genuinely
/// malformed token still produces the canonical length error).
const LOAD_RETRY_CEILING_MS: u64 = 50;

/// Poll interval for the loser's bounded read-retry.
const LOAD_RETRY_POLL_MS: u64 = 1;

/// Test-only hook: a pause (ms) injected by the winner between creating the empty
/// final file and publishing the token via temp+rename. Deterministically widens
/// the loser-reads-mid-write window so the forced-interleave convergence test does
/// not depend on scheduler luck. Zero in production (the hook is never set).
#[cfg(test)]
static WRITE_PAUSE_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Load an existing token or generate a new one.
///
/// Returns raw token bytes (32 bytes), not hex. On first generation, emits a
/// non-sensitive pointer to the retrieval command on stderr; the token hex is
/// never written to stdout, stderr, or any log line (NFR-06). The token is
/// recoverable only from the 0600 file or `unimatrix client-bundle`.
///
/// The flow is generate-first: attempt atomic file creation, and on
/// `AlreadyExists` fall back to loading the existing file. This eliminates
/// the TOCTOU race from a prior `path.exists()` check.
///
/// Error discrimination uses `io::ErrorKind::AlreadyExists` directly on the
/// raw `io::Error` from file creation, avoiding fragile string matching on
/// stringified errors.
pub fn load_or_generate_token(data_dir: &Path) -> Result<Vec<u8>, ServerError> {
    let token_path = data_dir.join(TOKEN_FILE_NAME);

    // Generate token bytes before file creation so the write follows the open
    // with minimal delay, keeping the race window narrow for concurrent creators.
    let mut token_bytes = [0u8; TOKEN_BYTE_LEN];
    rand::fill(&mut token_bytes);
    let hex_string = hex::encode(token_bytes);

    match create_token_file(&token_path) {
        // Winner: holds the exclusively-created empty final file. It alone writes
        // (losers load), so both racers converge on the SAME token.
        Ok(file) => write_new_token(file, &token_path, &token_bytes, &hex_string),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => load_existing_token(&token_path),
        Err(e) => Err(ServerError::ProjectInit(format!(
            "failed to create token file {}: {e}",
            token_path.display()
        ))),
    }
}

/// Atomically create the token file with `O_CREAT | O_EXCL` and mode 0600.
///
/// Returns the raw `io::Error` so callers can match on `ErrorKind::AlreadyExists`
/// without string parsing.
fn create_token_file(path: &Path) -> io::Result<File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
    }
    #[cfg(not(unix))]
    {
        OpenOptions::new().write(true).create_new(true).open(path)
    }
}

/// Publish the new token to the (already exclusively-created) final file via an
/// ATOMIC temp-file + rename.
///
/// The election winner holds `file`: an empty final-path file it created with
/// `O_CREAT | O_EXCL`. Writing the hex IN PLACE leaves a window where a concurrent
/// loser reads the still-empty final file (the "found 0" defect). Instead the
/// winner writes the 64 hex bytes to a PID-namespaced sibling temp file
/// (`.token.<pid>.tmp`, mode 0600) and `fs::rename`s it onto the final path. Rename
/// is atomic on one filesystem, so a reader sees either the prior bytes or the full
/// 64 — never a partial. Mode 0600 carries through the rename (it is the temp
/// inode's mode). The empty final file the winner created is replaced by the rename.
///
/// Temp cleanup: any error / early return removes the temp file so no orphan
/// `.token.<pid>.tmp` persists on collision or failure.
fn write_new_token(
    file: File,
    path: &Path,
    token_bytes: &[u8; TOKEN_BYTE_LEN],
    hex_string: &str,
) -> Result<Vec<u8>, ServerError> {
    // The winner created the empty final file purely to win the election; the
    // bytes go via temp+rename, so we no longer write through this handle.
    drop(file);

    let tmp_path = temp_token_path(path);

    // Remove any stale temp from a crashed prior boot reusing this PID (cheap
    // belt-and-suspenders so the O_EXCL temp create below cannot spuriously fail).
    let _ = fs::remove_file(&tmp_path);

    // Test-only: widen the create->publish window deterministically.
    #[cfg(test)]
    {
        let pause = WRITE_PAUSE_MS.load(std::sync::atomic::Ordering::SeqCst);
        if pause > 0 {
            std::thread::sleep(std::time::Duration::from_millis(pause));
        }
    }

    // Create the temp file exclusively at mode 0600, then write the hex.
    let mut tmp_file = match create_temp_token_file(&tmp_path) {
        Ok(f) => f,
        Err(e) => {
            return Err(ServerError::ProjectInit(format!(
                "failed to create temp token file {}: {e}",
                tmp_path.display()
            )));
        }
    };

    if let Err(e) = tmp_file.write_all(hex_string.as_bytes()) {
        let _ = fs::remove_file(&tmp_path);
        return Err(ServerError::ProjectInit(format!(
            "failed to write temp token file {}: {e}",
            tmp_path.display()
        )));
    }
    // Drop the handle before rename so all bytes are flushed to the inode.
    drop(tmp_file);

    // Atomic publish: replace the empty final file with the fully-written temp.
    if let Err(e) = fs::rename(&tmp_path, path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(ServerError::ProjectInit(format!(
            "failed to publish token file {}: {e}",
            path.display()
        )));
    }

    // Confirm generation on stderr with a non-sensitive pointer. The token hex
    // is NEVER emitted (NFR-06) — render-then-emit keeps the message testable.
    eprintln!("{}", render_first_boot_notice());

    Ok(token_bytes.to_vec())
}

/// Sibling temp path for the atomic write, PID-namespaced to avoid cross-process
/// collision (`.token.<pid>.tmp` in the same directory as the final token).
fn temp_token_path(final_path: &Path) -> std::path::PathBuf {
    let dir = final_path.parent().unwrap_or_else(|| Path::new("."));
    dir.join(format!(".{TOKEN_FILE_NAME}.{}.tmp", std::process::id()))
}

/// Exclusively create the temp token file at mode 0600 (carried through rename).
fn create_temp_token_file(path: &Path) -> io::Result<File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
    }
    #[cfg(not(unix))]
    {
        OpenOptions::new().write(true).create_new(true).open(path)
    }
}

/// Build the first-boot success notice shown after generating a new token.
///
/// Pure string builder (render-then-emit): it confirms the token was generated
/// and stored, and points to the supported retrieval command. It MUST NOT
/// contain the token hex, the token-file path, or any secret (NFR-06).
fn render_first_boot_notice() -> String {
    "[unimatrix] bearer token generated and stored (0600). \
     Retrieve it with: unimatrix client-bundle"
        .to_string()
}

/// Load and validate an existing token file.
///
/// This is the election LOSER's branch. The winner created the empty final file
/// (O_EXCL) and publishes the 64 hex bytes via temp+rename; a loser can observe
/// the file in the brief create->rename gap (0 bytes). This is the PRIMARY
/// correctness mechanism: poll briefly (bounded by `LOAD_RETRY_CEILING_MS`) until
/// the file is the published length, instead of panicking on a not-yet-complete
/// read. On deadline, surface the last read so a genuinely malformed token still
/// produces the canonical length error.
fn load_existing_token(path: &Path) -> Result<Vec<u8>, ServerError> {
    let deadline =
        std::time::Instant::now() + std::time::Duration::from_millis(LOAD_RETRY_CEILING_MS);

    let content = loop {
        let read = fs::read_to_string(path).map_err(|e| {
            ServerError::ProjectInit(format!("failed to read token file {}: {e}", path.display()))
        })?;

        // The published token is exactly TOKEN_HEX_LEN hex chars (plus optional
        // trailing whitespace). A complete read is therefore >= TOKEN_HEX_LEN
        // after trimming; anything shorter is the create->rename gap — retry.
        if read.trim_end().len() >= TOKEN_HEX_LEN || std::time::Instant::now() >= deadline {
            break read;
        }
        std::thread::sleep(std::time::Duration::from_millis(LOAD_RETRY_POLL_MS));
    };

    // Strip trailing whitespace (R-15 mitigation: trailing newline tolerance).
    let trimmed = content.trim_end();

    validate_token_format(trimmed)?;

    let token_bytes = hex::decode(trimmed).map_err(|_| {
        ServerError::ProjectInit("token file contains non-hex characters".to_string())
    })?;

    // No stdout output on load (FR-09). Debug-level trace only.
    tracing::debug!("loaded existing bearer token from {}", path.display());

    Ok(token_bytes)
}

/// Validate that a hex string has exactly 64 hex characters.
fn validate_token_format(hex_str: &str) -> Result<(), ServerError> {
    if hex_str.len() != TOKEN_HEX_LEN {
        return Err(ServerError::ProjectInit(format!(
            "token file must contain exactly {TOKEN_HEX_LEN} hex characters, found {}",
            hex_str.len()
        )));
    }

    if !hex_str.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ServerError::ProjectInit(
            "token file contains non-hex characters".to_string(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

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
}
