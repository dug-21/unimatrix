# Implementation Brief: vnc-004 Server Process Reliability

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/vnc-004/SCOPE.md |
| Scope Risk Assessment | product/features/vnc-004/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/vnc-004/architecture/ARCHITECTURE.md |
| Specification | product/features/vnc-004/specification/SPECIFICATION.md |
| Risk Strategy | product/features/vnc-004/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-004/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| pid-guard | pseudocode/pid-guard.md | test-plan/pid-guard.md |
| error-path | pseudocode/error-path.md | test-plan/error-path.md |
| session-timeout | pseudocode/session-timeout.md | test-plan/session-timeout.md |
| poison-recovery | pseudocode/poison-recovery.md | test-plan/poison-recovery.md |

## Goal

Eliminate cascading process lifecycle failures in the MCP server by replacing `process::exit(1)` with proper error propagation, implementing RAII-based PID file management with advisory locking and process identity verification, bounding session lifetime with a timeout, and recovering from lock poisoning. After this feature, the server reliably starts, recovers from crashes, and releases resources when connections drop.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| flock implementation | Use `fs2` crate for safe advisory locking | Architecture | architecture/ADR-001-flock-implementation.md |
| Session timeout strategy | Use `tokio::time::timeout` on session future (30 min default) | Architecture | architecture/ADR-002-session-timeout-strategy.md |
| Poison recovery | Use `unwrap_or_else(\|e\| e.into_inner())` | Architecture | N/A (straightforward pattern) |

## Files to Create/Modify

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-server/Cargo.toml` | Modify | Add `fs2` dependency |
| `crates/unimatrix-server/src/pidfile.rs` | Modify | Add `PidGuard` struct, `is_unimatrix_process()`, update `handle_stale_pid_file()` |
| `crates/unimatrix-server/src/error.rs` | Modify | Add `DatabaseLocked(PathBuf)` variant with Display and ErrorData impls |
| `crates/unimatrix-server/src/main.rs` | Modify | Use PidGuard, replace `process::exit(1)`, add session timeout constant and wrapper |
| `crates/unimatrix-server/src/shutdown.rs` | Modify | Keep PID removal as belt-and-suspenders logging (PidGuard is primary cleanup) |
| `crates/unimatrix-server/src/categories.rs` | Modify | Replace 3x `.expect("category lock poisoned")` with `.unwrap_or_else(\|e\| e.into_inner())` |

## Data Structures

### PidGuard (new)

```rust
/// RAII guard for PID file lifecycle: flock + PID write + cleanup on drop.
pub struct PidGuard {
    /// Open file handle holding the exclusive advisory lock.
    file: std::fs::File,
    /// Path to the PID file (for removal on drop).
    path: PathBuf,
}

impl PidGuard {
    /// Acquire exclusive flock on PID file, write current PID.
    /// Returns Err if lock is held by another process or I/O fails.
    pub fn acquire(path: &Path) -> io::Result<Self> { ... }
}

impl Drop for PidGuard {
    fn drop(&mut self) {
        // Remove PID file, log warning on failure. Lock released by file close.
    }
}
```

### ServerError::DatabaseLocked (new variant)

```rust
pub enum ServerError {
    // ... existing variants ...
    /// Database is locked by another process after exhausting retries.
    DatabaseLocked(PathBuf),
}
```

## Function Signatures

### New functions

```rust
// pidfile.rs
pub fn is_unimatrix_process(pid: u32) -> bool;

impl PidGuard {
    pub fn acquire(path: &Path) -> io::Result<Self>;
}
```

### Modified functions

```rust
// main.rs — open_store_with_retry changes return type behavior
fn open_store_with_retry(db_path: &Path) -> Result<Arc<Store>, Box<dyn std::error::Error>>;
// Previously: calls process::exit(1) on failure
// After: returns Err(ServerError::DatabaseLocked(path))
```

### Unchanged but affected

```rust
// pidfile.rs — handle_stale_pid_file keeps signature, changes internals
pub fn handle_stale_pid_file(pid_path: &Path, terminate_timeout: Duration) -> io::Result<bool>;
// Now uses is_unimatrix_process() instead of is_process_alive() for identity check

// shutdown.rs — graceful_shutdown keeps signature
// PID file removal step becomes belt-and-suspenders (PidGuard is primary)
```

## Constants

```rust
// main.rs
const SESSION_IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60); // 30 minutes
```

## Constraints

- `#![forbid(unsafe_code)]` — no unsafe code in server crate
- No MCP protocol changes, no schema changes, no new redb tables
- Backward compatible — happy path behavior unchanged
- Only one new dependency: `fs2`
- All fixes in existing modules (no new source files)

## Dependencies

| Crate | Version | New? | Purpose |
|-------|---------|------|---------|
| `fs2` | latest stable | Yes | Safe flock wrappers for advisory file locking |

## NOT in Scope

- Agent permission/capability errors (#46)
- Config Externalization (original vnc-004 in product vision — needs re-ID)
- Windows process identity support
- Automatic server restart (MCP client handles this)
- Dynamic session timeout configuration (fixed constant for now)
- Watchdog or stdio health monitoring (ADR-002: simple timeout chosen instead)

## Alignment Status

- **Vision Alignment**: PASS
- **Milestone Fit**: WARN — Feature ID vnc-004 conflicts with Config Externalization in product vision. Recommend re-ID Config Externalization as vnc-005.
- **Scope**: PASS (no gaps, no additions)
- **Architecture**: PASS
- **Risk Completeness**: PASS
