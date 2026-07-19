# vnc-048 Agent 5 (v2) — REWORK: AC-12 / R-03 S2 served-vector-from-`start`

Stage 3c gate gap, iteration 1. Added the one missing test proving the served-vector
outcome. Import logic unchanged — no defect found.

## 1. Files modified

- `crates/unimatrix-server/tests/import_integration.rs` — added `test_restore_sequence_serves_vector_search_from_start` + a local `insert_semantic_entry` helper + boot-path imports (`Arc`, `unimatrix_embed::{EmbedConfig, OnnxProvider, embed_entry}`, `unimatrix_vector::{VectorConfig, VectorIndex}`, `SLUG_VECTOR_META` const). +192 lines, single file.

No import/mod.rs (or any other) change — the import path was correct; the gap was a missing test, not a defect.

## 2. New test + the exact boot-path code it reuses

`test_restore_sequence_serves_vector_search_from_start`:

1. **Assembled restore sequence (operator's real invocations):** `seed_slug_store("aslug")` + insert a semantically-distinct 3-entry corpus (async-Rust/tokio, sourdough, alpine hiking) → `run_export_with_base(slug=Some("aslug"))` → `seed_slug_store("bslug")` (fresh register) → `run_import_with_base(slug=Some("bslug"), skip=false)`. This is the register → stop → import path; the live-PID gate is clear because no daemon is up.
2. **Faithful `start` simulation — reuses the daemon boot path, NOT a proxy:** mirrors `build_project_server` (`http_provision.rs:186-224`) against the POST-IMPORT on-disk `{base}/bslug/`:
   - `SqlxStore::open(&b_db, PoolConfig::default())` wrapped in `Arc` — the boot store-open (http_provision.rs:187).
   - probe `vector_dir.join("unimatrix-vector.meta")` — the boot meta probe (http_provision.rs:196-197).
   - `VectorIndex::load(Arc::clone(&boot_store), VectorConfig::default(), &b_vector_dir).await` — the exact boot-time per-slug vector load (http_provision.rs:203). No pre-import in-memory index is ever constructed, so none can be reused.
3. **Served query through the freshly-loaded index:** embed `"asynchronous runtime and concurrency in Rust with tokio"` with the SAME `OnnxProvider`/`EmbedConfig::default()` model `reconstruct_embeddings` uses, then `boot_index.search(&query, 3, 32)`. Asserts `results[0].entry_id == 1` (the restored async-runtime entry ranks top) and `boot_index.point_count() == 3` — proving the daemon serves the REBUILT index, not a stale/empty fallback. No `file_count`/presence assertion is used as the outcome; the only `file_count` call is a negative guard that the path-hash `vector/` was untouched (a precondition, not the outcome).

Why in-process rather than a full HTTP daemon: `build_project_server` lives in the `unimatrix-server` **binary** crate (it does `use unimatrix_server::...`), so a `tests/` integration test — which links the **library** crate — cannot call it. The narrowest faithful equivalent is the boot path's actual library-crate calls (`SqlxStore::open` + `VectorIndex::load` against `{slug}/vector`), which is exactly what boot runs; both are reused verbatim. This exercises the real load-then-serve path so the two-resolver / stale-index SR-10 failure modes could surface.

## 3. Test pass/fail (foreground evidence)

- `cargo test -p unimatrix-server --test import_integration test_restore_sequence_serves_vector_search_from_start` → **1 passed, 0 failed** (foreground). Import log shows the real sequence: `exported 3 entries`, `Embedding batch 1/1 (3/3)`, `Persisting vector index to .../bslug/vector`, `Hash validation: PASSED`.
- Full file: `cargo test -p unimatrix-server --test import_integration` → **27 passed, 0 failed** (26 pre-existing + 1 new).
- `cargo clippy -p unimatrix-server -- -D warnings` → clean (rc=0).
- `rustfmt --edition 2024` on my file only (workspace edition = 2024); no workspace-wide `cargo fmt` run.
- `git status --short` before commit: only `crates/unimatrix-server/tests/import_integration.rs` modified (the two untracked entries are the tester's report/testing dir — not mine, not touched). Committed as `18c50cdb`.

## 4. Import defect found / blocker

**None.** The import path already rebuilds the HNSW into `{slug}/vector` correctly (ADR-004) and persists VECTOR_MAP into the restored slug db, so the boot-time `VectorIndex::load` reconstructs a fully searchable index (`point_count == 3`, correct semantic ranking). The gap was purely a missing test; import logic was not changed.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` was not reachable in this rework thread (deferred MCP tool, not loaded); relied on the prior agent-5 briefing findings carried in the reports — #1162 (two-phase import: DB then embedding), #1146 (ADR-004 re-embed after commit), #5708 (live-PID import-gate test gotcha). The operative lessons here (#917/#918/#930 disk-state-proxy-vs-assembled-path; #4202 named-test-never-implemented) are already recorded and were the exact frame for this fix.
- Stored: nothing novel — the reusable gotcha this rework surfaced (an integration test in `tests/` cannot call `build_project_server` because it lives in the binary crate, so the faithful boot-path equivalent is the library-crate `SqlxStore::open` + `VectorIndex::load(store, config, &{slug}/vector)` pair) is a specific instance of the already-recorded disk-state-proxy-vs-served-path family, not a new cross-feature pattern. Promotable at retro only if the "prove served-vector-from-boot without standing up the daemon" shape recurs in the sibling-CLI slug work.
