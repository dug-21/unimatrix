## ADR-001: Chain-Verify Core Lives in `unimatrix-store`; Server `validate_hashes` and CLI Are Thin Callers

### Context

D-4 (SCOPE) requires a chain-verify that is a **transport-agnostic core function** so the CLI
can call it today and a future enterprise admin MCP tool (post-RBAC) is a thin wrapper over the
same core. SR-03 flags the crate home as the load-bearing open decision:

- The only existing verifier, `validate_hashes`, is **private**, lives in `unimatrix-server`
  (`import/mod.rs:396`), and runs **only at import**. It hand-rolls two checks (content-hash
  recompute + a weak "previous_hash references some known hash" existence check).
- The on-demand live-DB verify is genuinely new surface, not a refactor.
- The wrong home creates a dependency cycle or strands future callers.

Crate facts: `unimatrix-store` is a leaf — it depends on **no** other `unimatrix-*` crate.
`unimatrix-server` depends on `unimatrix-store`. Everything the verify logic needs already lives
in store: `compute_content_hash` (`hash.rs`), `EntryRecord` (`schema.rs`), `entry_from_row` /
`ENTRY_COLUMNS` / `query_all_entries` (`read.rs`), `query_supersession_chain` (`graph_queries.rs`).

### Decision

Place the chain-verify core in a **new `unimatrix-store` module, `chain_verify.rs`**, as a
**pure, I/O-free function** over an in-memory slice of entries:

```rust
pub fn verify_entries(entries: &[EntryRecord]) -> ChainReport;
```

with public result types `ChainReport { checked, skipped_legacy, violations }` (plus
`is_clean()` and a `Display`), `ChainViolation { entry_id, kind }`, and `ViolationKind`.

Each caller supplies the entry set from its own connection and interprets the report:

- **CLI (today, the only caller):** a new `unimatrix-server` module `verify.rs` with
  `run_verify(project_dir) -> Result<(), Box<dyn Error>>` — mirrors `run_import`'s sync-wrapper
  shape, resolves the db path via `project::ensure_data_directory`, opens
  `SqlxStore::open_readonly`, calls `store.query_all_entries()`, passes the slice to
  `verify_entries`, prints the report, and returns `Err` on a non-clean report so `fn main`
  exits non-zero. Exposed as a new `Command::Verify` variant dispatched in the pre-Tokio sync
  block of `main.rs`, exactly like `Command::Import`.
- **Import (existing caller):** `validate_hashes` is **refactored into a thin adapter** — it
  loads all rows from its in-flight transaction connection (using `ENTRY_COLUMNS` + `entry_from_row`)
  and calls the **same** `verify_entries`, mapping a non-clean report to its `Box<dyn Error>` and
  ROLLBACK. It stops hand-rolling its own checks.
- **Future MCP admin tool (deferred, NOT built in v1):** wraps `verify_entries` by handing it the
  entries it already holds. No new logic. This is why the core is pure and connection-agnostic.

Rejected alternative — **core in `unimatrix-server`**: would sit next to the import pipeline it
is merely *first* called from, would strand any future non-server consumer, and offers no
dependency-direction benefit (server already depends on store). Store is strictly better.

### Consequences

- **Easier:** one integrity oracle. `verify_entries` being pure and enforcing content-hash AND
  chain-link together means no caller can run half the check (AC-04). The core is trivially
  unit-testable on hand-built `Vec<EntryRecord>` fixtures — no DB needed. The future MCP tool is
  a genuine thin wrapper, satisfying D-4 by construction rather than by promise. No crate cycle.
- **Harder / cost:** `import/mod.rs`'s `validate_hashes` must be rewritten to load `EntryRecord`s
  and call the core (a modest refactor; it should shrink `import/mod.rs`, which is near the
  500-line ceiling). Import must load entries on its **transaction connection** (not a fresh pool)
  so it sees uncommitted rows — the core takes a slice precisely to stay connection-agnostic.
- **Watch:** the CLI loader must include **Deprecated** entries (predecessors are superseded);
  confirm `query_all_entries()` is not status-filtered, else use an all-status query. Verify
  tests must include a Deprecated predecessor.

Related: ADR-003 (the chain semantics the core implements). ADR-002 (the frozen-hash boundary the
content-hash recompute depends on).
