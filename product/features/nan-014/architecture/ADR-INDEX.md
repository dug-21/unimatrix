# ADR Index — nan-014

ADR content lives in Unimatrix (category `decision`), not in this file. This file is the index.

**List every ADR for this feature:**
`context_lookup({"category": "decision", "topic": "nan-014", "agent_id": "<your agent type>"})`

**Read one ADR in full:**
`context_get({"id": <Unimatrix ID, integer>, "format": "markdown", "agent_id": "<your agent type>"})`
`context_get` resolves a superseded ID to its current version, so an older ID still returns the live decision.

Only uni-architect creates, corrects, or deprecates ADRs, or edits this file. If you find a conflict, raise it to the scrum master.

Legacy on-disk `ADR-00N-*.md` files in this directory predate the Unimatrix-only convention; Unimatrix is authoritative where they differ. ADR-002's file was removed (stale since GH #1031).

| ADR | Unimatrix ID | Title | Decision (one line) | Status |
|-----|--------------|-------|---------------------|--------|
| ADR-001 | 4569 | Foreground Mode as Direct tokio_main_daemon Call | `--foreground` calls `tokio_main_daemon` directly; `--daemon` adds setsid first | active |
| ADR-002 | 5927 (supersedes 5925 ← 4570) | ORT Supply Chain Verification via SHA-256 Gate | ORT lib from pinned PyPI wheel via `pip download --require-hashes --no-deps --only-binary=:all: --platform manylinux_2_27_<arch> --python-version 3.10`; lib located by glob, fail closed; curl+sha256 of pinned wheel URL is escalation-only fallback (bugfix-1031 D-001, D-003) | active |
| ADR-003 | 4581 | Health Check via UDS Socket Connect | Sync `UnixStream::connect` to the MCP socket; exit 0 healthy, 1 unhealthy | active |
| ADR-004 | 4572 | Container CI Jobs Independent of Binary/npm Release Jobs | Container jobs have no `needs` link to binary/npm jobs | active |
| ADR-005 | 4573 | Container Data Path Resolution via --project-dir /data + HOME=/data | All data under `/data`; config via `UNIMATRIX_CONFIG` | active |
| ADR-006 | 4574 | cargo-chef Version Pinning with --locked | `cargo install cargo-chef --version <pin> --locked` | active |
| ADR-007 | 4575 | PidGuard Self-PID Guard Against Container Restart Self-Termination | Stale PID equal to own PID: reclaim without SIGTERM | active |
