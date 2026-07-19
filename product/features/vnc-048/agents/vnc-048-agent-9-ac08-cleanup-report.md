# vnc-048 Agent 9 — AC-08 test + stale-lint cleanup

Pre-merge polish (non-gating, human-requested). Two changes in the `unimatrix-server` crate.

## 1. Files modified

- `crates/unimatrix-server/tests/export_integration.rs` — added `seed_slug_store` helper + the AC-08 test.
- `crates/unimatrix-server/src/projects/slug_store.rs` — removed two stale `#[allow(dead_code)]`; removed the now-unused `slug_dir` field; adapted three unit-test assertions.

No other files touched (`git status --short` shows only these two).

## 2. AC-08 test

**Test name:** `test_export_slug_readonly_under_wal_writer`

**Concurrent-writer condition it reuses (genuine, not a proxy):**
- Seeds the per-slug store via the runtime literal-slug layout (`seed_slug_store` → `{base}/<slug>/unimatrix.db`), distinct code from the CLI read path (`run_export_with_base(slug=Some(..))` → `resolve_slug_store`).
- Holds a **second live `SqlxStore`/pool** open on the same db across the export. `SqlxStore::open` applies `journal_mode = WAL` + `busy_timeout = 10s` per connection (`unimatrix-store/src/pool_config.rs:143-148`) — byte-for-byte how `build_project_server` holds each per-slug store at boot for the daemon's lifetime. Standing up a full HTTP daemon in a cargo integration test is infeasible; this concurrent-open-connection-in-WAL is the narrowest faithful equivalent, as the spawn brief permits.
- Drives a **background thread performing continuous INSERTs** through a cloned `write_pool_server()` handle (guarded by an `AtomicBool` stop flag) so the export reads *alongside* an active writer, not an idle handle or a closed store.
- Asserts the export **succeeds** (no lock error) and emits the seeded corpus (ids 101/102/103, disjoint from the writer's ids), proving read-only coexistence with no locking added.

**Why it is reliable** (the non-obvious part): export wraps reads in `BEGIN DEFERRED` (ADR-001) but never writes inside that transaction. `SQLITE_BUSY_SNAPSHOT` — which `busy_timeout` cannot resolve (Unimatrix #2130) — only arises when a DEFERRED reader later tries to write after another connection committed. Because export is pure-read, the concurrent writer cannot push it into `BUSY_SNAPSHOT`. No blocker; AC-08 is exercisable without a daemon.

## 3. dead_code allows removed + clippy

- Removed `#[allow(dead_code)]` from `SlugStorePaths` (struct) and `resolve_slug_store` (fn). Both are now genuinely used: `resolve_slug_store` is called by `export.rs:100` and `import/mod.rs:149`; `db_path` consumed by both callers; `vector_dir` consumed by import (`import/mod.rs:150`).
- **Field-unused finding (reported, not hidden):** removing the struct-level allow surfaced `warning: field slug_dir is never read`. `slug_dir` is genuinely dead in production — only `db_path`/`vector_dir` are consumed by callers, and `slug_dir` was read only by the struct's own `#[cfg(test)]` unit tests. Per the brief ("do not silence blindly — wire it or report it; a truly-unused field is a design smell worth flagging, not hiding"), I **removed the field** rather than re-add an allow, and adapted the three unit-test assertions to assert derivation via `db_path` (whose parent is the slug dir, so base-derivation correctness is still fully proven). Flagging for awareness: `SlugStorePaths` now carries two fields, not the three the pseudocode drafted.
- **Clippy (owned files clean):**
  - `cargo clippy -p unimatrix-server --lib -- -D warnings` → clean (covers `slug_store.rs`, the non-test path where the `slug_dir` warning appeared).
  - `cargo clippy -p unimatrix-server --test export_integration -- -D warnings` → clean.
- **Pre-existing, out-of-scope clippy failure:** `cargo clippy -p unimatrix-server --tests` fails with two `clippy::manual_repeat_n` errors in `src/mcp/response/verbosity.rs:192,208` (`repeat().take()`). Confirmed present on HEAD with my changes stashed — a clippy 1.95.0 toolchain-drift lint, unrelated to this task and in a file I do not own. Left untouched. One-line fix if desired: `std::iter::repeat_n(two_byte, N)`.

## 4. Test pass/fail (foreground evidence)

```
cargo test -p unimatrix-server --test export_integration
running 22 tests
test test_export_slug_readonly_under_wal_writer ... ok
...
test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## 5. Blocker

None. AC-08 is exercisable without a full HTTP daemon (see reliability note above).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing (vnc-048) — surfaced #2130 (SQLITE_BUSY_SNAPSHOT is not recoverable by busy_timeout; requires txn restart; caused by concurrent DEFERRED WAL writes), #329/#2130/#2147-2153 (SqlxStore dual-pool WAL: 1 writer, ≤8 readers), #1097 (export BEGIN DEFERRED snapshot, ADR-001), #5193 (asserting a WAL store grew from a committed write). Applied: confirmed export's read-only DEFERRED transaction cannot hit BUSY_SNAPSHOT under the concurrent writer, making the AC-08 test reliable.
- Stored: entry #5709 "Faithful AC-08 test: export read-only coexistence under a live WAL writer without a daemon" via context_store (pattern, unimatrix-server) — the daemon-free faithful-equivalent test construction + the BUSY_SNAPSHOT reliability rationale.
