# vnc-004 Acceptance Criteria Map

| AC-ID | Description | Verification Method | Verification Detail | Status |
|-------|-------------|--------------------|--------------------|--------|
| AC-01 | No `process::exit` in server code | grep | `grep -r "process::exit" crates/unimatrix-server/src/` returns no matches | PENDING |
| AC-02 | PID file always cleaned up on normal exit, error exit, and panic | test | Unit tests: PidGuard drop removes file on normal scope exit; PidGuard drop removes file when main returns Err; PidGuard drop removes file during panic unwind | PENDING |
| AC-03 | Stale PID detection verifies process identity — never SIGTERMs a non-unimatrix process | test | Unit tests: `is_unimatrix_process` returns false for non-unimatrix PIDs; `handle_stale_pid_file` does not call SIGTERM when PID is alive but not unimatrix-server | PENDING |
| AC-04 | Advisory file lock prevents TOCTOU race — two simultaneous startups don't both proceed past PID check | test | Unit test: second `PidGuard::acquire` on same path fails immediately with error (not blocks) | PENDING |
| AC-05 | Zombie server detection — server exits gracefully if stdio transport is broken but process lingers | test | Integration test: session timeout triggers graceful shutdown (vector dump + compact + PID cleanup) | PENDING |
| AC-06 | No panics from lock poisoning — `CategoryAllowlist` recovers instead of crashing | test | Unit test: poison RwLock via panic-in-write, then verify validate/add_category/list_categories all succeed | PENDING |
