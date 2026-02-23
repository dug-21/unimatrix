# vnc-001 Acceptance Criteria Map

| AC-ID | Description | Verification Method | Verification Detail | Status |
|-------|-------------|--------------------|--------------------|--------|
| AC-01 | `unimatrix-server` binary crate compiles with `cargo build` | shell | `cargo build -p unimatrix-server` exits 0 | PENDING |
| AC-02 | Server completes MCP initialize handshake with ServerInfo containing name, version, instructions | test | Integration test: pipe initialize request to server stdin, verify response JSON contains serverInfo with name="unimatrix", non-empty version, non-empty instructions | PENDING |
| AC-03 | Instructions field contains behavioral guidance text per ASS-006 | test | Unit test: `server.get_info().instructions` contains "search for relevant patterns" | PENDING |
| AC-04 | Project root detected by walking up to `.git/` directory | test | Unit test: create temp dir tree with `.git/` at level 2, detect from level 4, verify correct root | PENDING |
| AC-05 | Project hash is deterministic SHA-256 first 16 hex chars | test | Unit test: `compute_project_hash(path)` called twice returns identical 16-char hex string | PENDING |
| AC-06 | Auto-initialization creates data directory, database, vector directory on first run | test | Integration test: run `ensure_data_directory()` in temp dir, verify `{hash}/unimatrix.redb` and `{hash}/vector/` exist | PENDING |
| AC-07 | AGENT_REGISTRY table exists with AgentRecord schema | test | Unit test: open Store, write AgentRecord to AGENT_REGISTRY, read back, verify round-trip | PENDING |
| AC-08 | Default agents "human" (Privileged) and "system" (System) bootstrapped on first run | test | Unit test: create registry, call bootstrap_defaults(), verify both agents present with correct trust levels | PENDING |
| AC-09 | Unknown agent_id auto-enrolls as Restricted with [Read, Search] | test | Unit test: `resolve_or_enroll("unknown-agent")` returns Restricted with Read+Search capabilities | PENDING |
| AC-10 | AUDIT_LOG table exists with monotonic IDs | test | Unit test: log 10 events, read back IDs, verify strictly increasing sequence starting from 1 | PENDING |
| AC-11 | Agent identity extracted from tool params and threaded through pipeline | test | Integration test: call tool stub with agent_id="test-agent", verify audit event contains agent_id="test-agent" | PENDING |
| AC-12 | Graceful shutdown calls dump() and compact() | test | Integration test: start server, insert data, trigger shutdown, verify vector dump files exist and store file is valid | PENDING |
| AC-13 | Four tool stubs registered with correct names and schemas | test | Integration test: send tools/list, verify 4 tools with names context_search, context_lookup, context_store, context_get | PENDING |
| AC-14 | Server errors map to ErrorData with actionable messages | test | Unit test: map each ServerError variant via `Into<ErrorData>`, verify code and message content | PENDING |
| AC-15 | Server uses unimatrix-core async wrappers | grep | `grep -r "AsyncEntryStore" crates/unimatrix-server/src/` finds usage in server.rs | PENDING |
| AC-16 | Data directory at ~/.unimatrix/{hash}/unimatrix.redb and ~/.unimatrix/{hash}/vector/ | test | Integration test: verify ProjectPaths fields after ensure_data_directory() | PENDING |
| AC-17 | Store::open() creates 10 tables | test | Unit test: open Store, verify all 10 tables accessible in read transaction | PENDING |
| AC-18 | Server handles full MCP lifecycle | test | Integration test: initialize -> tool call -> shutdown, all succeed without error | PENDING |
| AC-19 | All server code uses forbid(unsafe_code), edition 2024, MSRV 1.89 | shell | `grep -r 'forbid(unsafe_code)' crates/unimatrix-server/src/lib.rs` + `cargo build` with MSRV | PENDING |
