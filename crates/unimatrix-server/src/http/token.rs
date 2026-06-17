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
#[path = "token/tests.rs"]
mod tests;
