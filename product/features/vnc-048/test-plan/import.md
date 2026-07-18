# Test Plan — import/mod.rs (`run_import` / `run_import_with_base` slug branch + gates + vector redirect)

Signature change: add `slug: Option<&str>` to `run_import` / `run_import_with_base`
(`import/mod.rs:54,68`). Branch to `resolve_slug_store`; **live-PID hard-error gate** (ADR-003);
**non-empty-audit pre-flight refusal** (ADR-005); redirect `reconstruct_embeddings` to
`slug_dir/"vector"` (ADR-004). Tests live in `crates/unimatrix-server/tests/import_integration.rs`.
Keep the multi-thread runtime (`block_in_place`/GH#554, C-8) — reuse the existing `open_store`
`block_in_place` idiom and `#[tokio::test(flavor = "multi_thread")]`.
Risks: **R-03 (Critical, top weight)**, R-04, R-07, R-11, R-02, R-14, R-09.

## AC-10 — full round-trip A→B across all tables (TOP weight)

- `test_import_slug_roundtrip_all_tables_two_slugs` — the mandatory shape:
  1. Seed slug store **A** via `seed_slug_store` (runtime literal-slug layout) with rows exercising
     **every table**: entries (all 26 columns via `insert_full_entry`), `entry_tags`, `co_access`,
     `feature_entries`, `graph_edges`, `audit_log`, `counters`.
  2. `run_export_with_base(slug=Some("A"))` → dump.
  3. `run_import_with_base(slug=Some("B"))` into a **second, freshly-registered** slug **B**
     (audit-empty target so the explicit-`event_id` INSERT cannot collide — C-5 basis).
  4. Diff **all** tables A vs B.
  **Crosses two different slugs (A→B, not A→A)** — an A→A round-trip does not prove the resolver
  distinguishes source from destination.
- `test_import_slug_roundtrip_type_fidelity` — f64 confidence **bit-exact**; JSON-in-TEXT emitted raw
  (not re-encoded); NULL vs empty-string preserved; content-hash + chain-link `chain_verify` reports
  clean on B. (SR-08 hash validity.)

## R-03 / AC-12 — served vector search from `start` (TOP weight, gate non-negotiable)

**CLI-sequence functional test** — drives the compiled binary, not a helper. Requires
`cargo build --release` first (Stage 3c).

- `test_restore_sequence_serves_vector_search_from_start` —
  1. `project register <slug>` → creates `{base}/<slug>/{unimatrix.db,vector}` + `[[projects]]`.
  2. `stop` → daemon releases per-slug stores; live-PID gate clears.
  3. `import --slug <slug> -i dump.jsonl` → DB restored into `{base}/<slug>/unimatrix.db`, HNSW
     rebuilt into `{base}/<slug>/vector`.
  4. `start` → daemon boots and loads the rebuilt index.
  5. Issue a **served vector search** against the restored slug (through the running daemon's query
     surface); assert it returns the restored corpus's semantic hits.
  **Proven from `start` onward, NOT from disk state.** A stat that `{slug}/vector` holds a fresh HNSW
  file (AC-02) is necessary but does NOT discharge SR-10 — the daemon must be shown to load the
  *rebuilt* index, not a stale one.

## AC-02 — restore target + vector redirect (necessary, not sufficient for R-03)

- `test_import_slug_restores_into_slug_db_and_vector` — post-import: DB rows land in
  `{base}/<slug>/unimatrix.db`; a **fresh HNSW file exists under `{base}/<slug>/vector`**; and
  **nothing** is written to the path-hash `vector/`. Asserts the `reconstruct_embeddings` redirect
  target is `slug_dir/"vector"` (integration risk: `reconstruct_embeddings` must receive
  `slug_dir/"vector"`, not `paths.vector_dir`).

## R-11 / AC-13 — live-PID hard-error (live-only predicate)

- `test_import_slug_live_pid_hard_errors_no_vector_write` — write a **live** PID to the base-scoped
  path-hash `pid_path` (a PID that `is_process_alive`+`is_unimatrix_process` accept — use the test
  process's own PID or a controlled child). `run_import_with_base(slug=..)` → refusal; message names
  the **resolved PID path** + `stop → import → start` remedy; assert **no write to `{slug}/vector`**
  (clobber path never entered — R-03 S1 structural unreachability).
- `test_import_slug_stale_pid_does_not_block` — a **dead/stale** PID at `pid_path` → import proceeds
  (predicate is live-PID-only via `is_process_alive`/`is_unimatrix_process`).
- `test_import_slug_projects_stanza_without_daemon_does_not_block` — a `[[projects]]` stanza written
  by `register` but **no live daemon** → import proceeds (else the canonical sequence would be
  refused). The `[[projects]]` config half is NOT consulted (FR-12).
- PID path stays base-scoped in slug mode (FR-11): assert the gate consults `paths.pid_path` (the one
  daemon's PID), never a per-slug path.

## R-07 — non-empty-audit pre-flight refusal (AC-10/FR-13, C-5)

- `test_import_slug_nonempty_audit_refuses_preflight` — target slug store whose `audit_log` already
  has rows → refusal **before** `drop_all_data`/insert; message directs "register a fresh slug";
  assert the raw **SQLite UNIQUE** error is **never** surfaced (`drop_all_data` cannot clear the
  append-only `audit_log`, schema v25 triggers).
- `test_import_slug_no_force_bypass` — confirm no `--force` bypasses the non-empty-audit refusal (no
  such override exists).
- Supported target sanity: `test_import_slug_fresh_audit_empty_slug_succeeds` — importing into a
  freshly-registered empty slug (0 rows, audit-empty) succeeds (the supported target).

## R-02 / R-14 — missing store fails loud, creates nothing (AC-03)

- `test_import_slug_missing_store_fails_loud_fs_unchanged` — valid slug, no `unimatrix.db` at the
  resolved path → `Err`, error contains the fully-resolved absolute path, and FS unchanged: no db,
  no `vector/`, no `-wal`/`-shm`, no partial import artifacts under `X/<slug>`. `open` (auto-create +
  migrate = a write) is never reached before the existence gate.

## R-08 — validation at the CLI edge (AC-04)

- `test_import_slug_invalid_rejected_no_fs_touch` — representative charset-invalid + reserved + one
  traversal case driven through `run_import_with_base(slug=Some(bad))`; rejection before any FS/DB,
  zero side effects. (Exhaustive set unit-tested in resolve_slug_store.md.)

## R-09 — no-`--slug` fallthrough parity (AC-05)

- `test_import_no_slug_resolved_path_is_path_hash_data_dir` — property: `slug=None` → resolved path
  == path-hash `data_dir` (funnel not entered).
- Existing `import_integration.rs` suite passes unchanged; update only call sites for the new param
  (C-9). WARN-1 stderr check applies here too if any import test asserts stderr emptiness.

## Integration risks (from Risk Strategy) — explicit assertions

- **Funnel ↔ `ensure_data_directory` coupling (C-6):** slug-mode import neither fails because of, nor
  depends on the *contents* of, the incidentally-created path-hash `data_dir`/`vector/`; and that
  hash dir is not mistaken for the slug store (assert import wrote to `{slug}/` only).
- **Sync pre-tokio + multi-thread runtime (C-8):** the AC-10 / AC-02 imports run to completion
  through `reconstruct_embeddings` (a `current_thread` regression panics only there — GH#554).

## Edge cases

- Import into freshly-registered empty slug (supported target) — succeeds.
- Slug dir exists but `unimatrix.db` absent → missing store (existence gate on db file).
- Max/min slug length with a real registered slug present.
</content>
