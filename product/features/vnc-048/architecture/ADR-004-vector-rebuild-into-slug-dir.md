## ADR-004: Import rebuilds HNSW into `slug_dir/vector`; the PID path stays base-scoped path-hash

### Context
Import is not a mirror image of export: it is a WRITE that must also rebuild the HNSW index, and the restored slug must serve *vector* search after the daemon restarts (Goal 2, SR-10, AC-12). Import's Phase 10 (`import/mod.rs:226`) passes `&paths.vector_dir` to `reconstruct_embeddings`, which dumps a fresh HNSW there (`embed_reconstruct.rs:110`). In slug mode `paths.vector_dir` is the path-hash `{hash}/vector` — the wrong directory; the daemon loads `{slug}/vector` at boot. The per-slug layout is *relatively identical* to the hash layout (`PROJECT_VECTOR_DIR = "vector"`; `{slug_dir}/unimatrix.db` + `{slug_dir}/vector` mirrors `{data_dir}/unimatrix.db` + `{data_dir}/vector`), so the fix is a redirect, not new vector logic. Separately, the PID path must *not* be redirected — it is the daemon's, not the store's.

### Decision
In slug mode, import derives its write targets from `SlugStorePaths` (ADR-001) and keeps the PID from the path-hash `paths`:

- **DB restore** targets `SlugStorePaths.db_path` (`{base}/<slug>/unimatrix.db`) — AC-02.
- **HNSW rebuild** targets `SlugStorePaths.vector_dir` (`{base}/<slug>/vector`), passed to `reconstruct_embeddings` in place of `&paths.vector_dir`. `project register` already creates that dir (`projects.rs:330-332`), so the rebuild writes where the daemon will load — AC-02/AC-12. This is the index the daemon loads at the next `start`; the restore outcome is proven from `start` onward (vector search served post-restart), not from rows-on-disk (SR-10).
- **`pid_path` stays `paths.pid_path`** — the path-hash, base-scoped daemon PID (ADR-003 depends on this). It is correct in slug mode precisely because one daemon serves all slugs; it must not be redirected to `slug_dir`.

Without `--slug`, both targets remain the path-hash `paths.*` (AC-05, unchanged).

### Consequences
Easier: the restored slug serves vector search after `start` because the rebuilt index is the one the daemon loads — the outcome holds from the operator's actual invocation, not merely from disk state (SR-10 closed). The redirect is one argument; import needs no vector logic it does not already have.

Harder: import now reads two path sources in slug mode — DB/vector from `SlugStorePaths`, PID from `paths` — an agent must not "tidy" this into one source (redirecting PID to `slug_dir` would break the ADR-003 live-PID gate, since the daemon's PID is base-scoped). The correctness of `slug_dir/vector` being loadable rests on `register` having created it; the non-empty-audit pre-flight (ADR-005) already constrains the target to a freshly-registered slug, so the dir exists.
