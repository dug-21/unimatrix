# Agent Report: nan-007-agent-1-architect

## Status: Complete

## Outputs

### ARCHITECTURE.md
`/workspaces/unimatrix/product/features/nan-007/architecture/ARCHITECTURE.md`

### ADR Files

| File | Unimatrix ID | Title |
|------|-------------|-------|
| `architecture/ADR-001-vacuum-into-rusqlite-sync.md` | #2584 | VACUUM INTO via rusqlite (Synchronous, Pre-Tokio) |
| `architecture/ADR-002-analytics-mode-suppression.md` | #2585 | EvalServiceLayer Suppresses Analytics Queue at Construction |
| `architecture/ADR-003-test-support-feature-for-eval.md` | #2586 | eval Module Uses test-support Feature to Access kendall_tau |
| `architecture/ADR-004-no-new-eval-crate.md` | #2587 | Eval Infrastructure Lives in unimatrix-server, Not a New Crate |
| `architecture/ADR-005-nested-eval-subcommand-clap4.md` | #2588 | Nested eval Subcommand via Clap 4.x Inner Enum |

---

## Open Question Answers

### 1. VACUUM INTO: sync vs async
**Answer: rusqlite synchronous.**
`VACUUM INTO` is a single DDL statement. The snapshot subcommand is dispatched pre-tokio (C-10). rusqlite is already a transitive dependency. Creating a tokio runtime via `block_export_sync` for a one-shot DDL call adds overhead with no benefit. The WAL-mode isolation guarantee means snapshot against a live daemon's database is safe without stopping the daemon. (ADR-001)

### 2. Nested eval subcommand structure in clap 4.x
**Answer: Confirmed. `Command::Eval { command: EvalCommand }` with inner `#[command(subcommand)]` field.**
Clap 4.x natively supports nested subcommand enums. The entire `Eval` dispatch arm is placed in the sync block before the tokio runtime (C-10 satisfied). The inner `EvalCommand::Run` uses `block_export_sync` for its async sqlx needs — exactly as `export.rs` does. The `snapshot` subcommand is at the top-level `Command` enum (same level as `export`), not nested under `eval`. (ADR-005)

### 3. Hook socket path
**Answer: `ProjectPaths.socket_path` = `{data_dir}/unimatrix.sock`.**
Confirmed in `crates/unimatrix-engine/src/project.rs`. The hook IPC socket is `socket_path` (NOT `mcp_socket_path`). The MCP UDS socket is `mcp_socket_path` = `{data_dir}/unimatrix-mcp.sock`. No new `ProjectPaths` field is required. `UnimatrixHookClient` accepts the socket path as a constructor argument; callers supply it from `ProjectPaths.socket_path`. The `daemon_server` pytest fixture already exposes both paths.

---

## Key Design Decisions

1. **Analytics suppression via `AnalyticsMode::Suppressed`** (SR-07, High risk): `EvalServiceLayer` never spawns the drain task. Uses a raw read-only pool — not `SqlxStore::open()`. The `?mode=ro` SQLite enforcement is a secondary layer. Type-level declaration prevents accidental analytics writes during eval replay.

2. **`test-support` feature for kendall_tau access** (ADR-003): The `test_scenarios` module is gated by `#[cfg(any(test, feature = "test-support"))]`. The eval runner is production code that needs these metrics. Enabling the feature on `unimatrix-engine` in `unimatrix-server/Cargo.toml` is the correct approach — no duplication.

3. **rusqlite for VACUUM INTO** (ADR-001): Single DDL statement, pre-tokio path, no runtime needed. Consistent with C-10.

4. **No new crate** (ADR-004): Module tree in `crates/unimatrix-server/src/eval/`. Single-binary principle maintained.

5. **D1–D4 / D5–D6 acceptance separation** (SR-04): Offline eval (no daemon) and live simulation (daemon required) are explicitly separate acceptance groups. D5/D6 daemon fixture issues cannot block D1–D4 acceptance.

6. **SR-01 resolved**: `rmcp` is already pinned to `=0.16.0` exact version in `crates/unimatrix-server/Cargo.toml`. No Cargo.toml change needed. A UDS `serve()` smoke test provides compile-time breakage detection.

---

## Open Questions for Implementer

None — all architect open questions are resolved. The implementation brief can proceed.
