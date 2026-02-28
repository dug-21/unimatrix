## ADR-001: Use fs2 crate for advisory file locking

### Context

vnc-004 needs advisory file locking (flock) on the PID file to eliminate the TOCTOU race between reading the PID file and opening the database. The server crate uses `#![forbid(unsafe_code)]`, so raw `libc::flock` calls are not an option.

Three alternatives were considered:
1. **`fs2` crate** — provides `FileExt::lock_exclusive()` and `try_lock_exclusive()` as safe wrappers around platform-native locking (flock on Unix, LockFileEx on Windows).
2. **`libc` crate with unsafe** — direct `flock(2)` call. Requires removing `forbid(unsafe_code)` or adding `allow(unsafe_code)` blocks.
3. **Shell command wrapper** — similar to the existing `kill -0` pattern, invoke `flock` CLI. Fragile and not available on all platforms.

### Decision

Use the `fs2` crate. It provides safe, cross-platform file locking with minimal API surface (one trait extension on `std::fs::File`). It is well-maintained, has no transitive dependencies beyond `libc` (already an indirect dependency), and preserves `#![forbid(unsafe_code)]`.

### Consequences

- **Easier**: PidGuard can hold a `std::fs::File` and call `file.try_lock_exclusive()` — clean, safe Rust.
- **Easier**: Cross-platform support (Windows, macOS, Linux) without conditional compilation beyond what `fs2` handles internally.
- **Harder**: Adds one new direct dependency to `unimatrix-server`. Acceptable because `fs2` is small and focused.
- **Neutral**: The flock is automatically released when the file handle closes (process exit, drop, SIGKILL), which is the desired behavior.
