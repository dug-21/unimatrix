# Test Plan: authentication

## Risks Covered

| Risk | Severity | Test Coverage |
|------|----------|--------------|
| R-10 | High | UID verification bypass -- same-user succeeds, different-user rejected |
| R-11 | Medium | Process lineage false negative -- advisory check, various cmdline formats |

## Unit Tests

Location: `crates/unimatrix-engine/src/auth.rs` (within `#[cfg(test)]` module)

### UID Verification (R-10)

1. **test_verify_uid_same_user**: `verify_uid(PeerCredentials { uid: 1000, .. }, 1000)` -> `Ok(())`.

2. **test_verify_uid_different_user**: `verify_uid(PeerCredentials { uid: 1001, .. }, 1000)` -> `Err(AuthError::UidMismatch { peer_uid: 1001, server_uid: 1000 })`.

3. **test_verify_uid_root**: `verify_uid(PeerCredentials { uid: 0, .. }, 1000)` -> `Err(AuthError::UidMismatch)` (root is still a different UID).

### Process Lineage (R-11, Linux only)

Tests gated behind `#[cfg(target_os = "linux")]`:

4. **test_verify_lineage_unimatrix_server**: cmdline containing `unimatrix-server` as filename -> `Ok(())`.

5. **test_verify_lineage_with_full_path**: cmdline `"/home/user/.cargo/bin/unimatrix-server"` -> passes (file_name extraction).

6. **test_verify_lineage_target_release**: cmdline `"target/release/unimatrix-server"` -> passes.

7. **test_verify_lineage_other_binary**: cmdline `"/usr/bin/some-other-binary"` -> `Err(AuthError::LineageFailed)`.

8. **test_verify_lineage_empty_cmdline**: Empty bytes -> `Err(AuthError::LineageFailed)`.

9. **test_verify_lineage_process_gone**: Non-existent PID -> `Err(AuthError::LineageFailed)`.

10. **test_verify_lineage_substring_no_match**: cmdline `"not-unimatrix-server-tool"` -> fails (exact filename match, not substring).

### Combined Authentication

11. **test_authenticate_same_uid**: Real UDS pair. `authenticate_connection(stream, our_uid)` -> `Ok(PeerCredentials)` with matching UID.

12. **test_authenticate_extracts_credentials**: Verify returned `PeerCredentials` has expected UID and GID.

## Integration Tests

13. **test_real_uds_peer_credentials**: UDS listener in tempdir. Connect. `extract_peer_credentials` returns correct UID.

14. **test_full_authenticate_real_uds**: Full `authenticate_connection` succeeds for same-user connection.

## Assertions

- Matching UIDs: `Ok(())`
- Mismatched UIDs: `Err(AuthError::UidMismatch)` with both UIDs
- Lineage pass: `Ok(())`
- Lineage fail: `Err(AuthError::LineageFailed)` (not panic)
- Real UDS: `PeerCredentials.uid` matches `std::process::Command::new("id").arg("-u")` output
