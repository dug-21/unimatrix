# vnc-048 — Per-Slug Backup/Restore for Personal-Cloud Deployments

Tracked under GH Issue #953 (opened as a bugfix, escalated to a design session at the fix-approach checkpoint).

## Problem Statement

In a personal-cloud (multi-project HTTP) deployment, an operator cannot back up or restore a project's knowledge. Both operator CLIs resolve their target database exclusively through the single-project SHA-256 path-hash scheme (`{base}/.unimatrix/<hash16>/unimatrix.db`), while the runtime writes every project's data through the literal-slug scheme (`{base}/.unimatrix/<slug>/unimatrix.db`). A 16-hex hash segment can never equal an operator slug, so no `--project-dir` value reaches a slug store.

The failure is silent in both directions:
- **Export** opens (and, via `SqlxStore::open`, *auto-creates*) the near-empty path-hash store and emits a file with audit-log rows and zero knowledge entries. It reports success.
- **Import** resolves the same hash path and restores into a store nothing routes to. It reports success.

Who is affected: personal-cloud operators — the deployment shape the `personal-cloud` goal (#5673) names as the destination. The runtime arc (per-slug routing, isolated stores, per-slug config) is `delivery:proven`; the operator CLI never followed it to multi-project. This is a trailing-edge capability gap, not an incident.

Why now: fixing export alone delivers zero user-facing outcome — the issue's own stated Impact ("data migration between personal-cloud instances is effectively broken") stays true after an export-only fix merges. Export half is not a smaller version of the outcome; it is none of it.

## Goals

1. An operator can export a named per-slug project's knowledge from a personal-cloud deployment to a portable JSONL file containing that project's actual corpus.
2. An operator can restore that file into a named per-slug project on a personal-cloud deployment (same or different instance), and the restored project serves that knowledge — including vector search — after the daemon restarts.
3. The export→import round-trip is proven by a test that seeds through the **runtime's** literal-slug layout and reads through the **CLI's** resolver, so the two resolvers can actually disagree.
4. Every failure in the new paths is loud, names the fully resolved absolute path, and states the next action. No path may auto-create a store or silently target the wrong one.
5. Single-project / local (`--project-dir`-only, no `--slug`) behavior is byte-for-byte unchanged.

## Non-Goals

- **The `--skip-quarantined` asymmetry.** It filters `entries`/`entry_tags`/`co_access`/`feature_entries`/`graph_edges` but never `audit_log`. This is **correct as designed and will not be touched** — audit logs must never be altered; an export that filtered its own audit log would be the defect. Decided by the human; not re-litigated here. (Consequence: an audit-rows-only export is a legitimate possible output, which is part of why the empty-store failure was silent. This argues for fail-loud, not for changing the filter.)
- **A new base-resolution mechanism.** Base is derived from the existing `paths.data_dir.parent()`, the same derivation `main.rs:1287`, `ProjectRegistry::resolve`, and `http_provision::build_project_server` already use. No `--base` flag, no env var. The issue body's claim that one is needed is over-stated (verified by the investigator). A second configuration scheme for the same value is the single thing this design most refuses.
- **Slug-awareness for the rest of the operator CLI** (`verify`, `snapshot`, `eval`, `health`, `stop`, `client-bundle`). All are hash-only resolvers with the same gap. Out of scope, but this feature establishes the `--slug` pattern they will copy. Whether "the operator CLI is slug-aware" becomes one tracked item rather than five future bug reports is a human call, not filed here.
- **Restoring over a slug store that already has audit history.** See Constraints C-5 — this fails loud and stays out of scope.
- **Live-daemon import.** See Constraints C-4 and OQ-1 — `import --slug` refuses outright while any daemon PID is live (`register → stop → import → start`). Making import *safe* under a live daemon (locking, daemon-mediated import, index invalidation) is a separate design problem.
- **Backup as disaster recovery.** The goal's success criteria already define cloud DR as "backup = volume snapshot; recovery = restore + restart". What is broken here is **per-project portability** ("owning your knowledge") — a volume snapshot cannot deliver that. This feature does not change the DR story.
- **Version skew** (an exporter/importer binary newer than the daemon that migrates the live DB). Pre-existing hazard; the operational mitigation (exec into the container, so the CLI *is* the daemon's binary) already applies.

## Background Research

### The two resolvers (verified line-by-line)

| Scheme | Join site | Used by |
|---|---|---|
| Path-hash | `unimatrix-engine/src/project.rs:163` — `data_dir = unimatrix_base.join(&project_hash)` | every CLI subcommand, incl. export/import |
| Literal slug | `http_provision.rs:172-174` and `projects.rs:123-125` — `base.join(slug.as_str())` | the runtime, `project register/list/delete` |

Both live under the **same** `.unimatrix` base; per-slug dirs are siblings of the path-hash dirs (Unimatrix pattern #4972). In-container `HOME=/data` (Dockerfile:132), so the base is `/data/.unimatrix` and `data_dir.parent()` resolves it correctly with no new mechanism. This is the known two-resolvers trap (lesson #5507): the resolvers only disagree when a test seeds through one and reads through the other, and no test does.

### Import is NOT a mirror image — the vector-rebuild answer

The prior agents' open question ("is import's `paths.vector_dir` rebuild a small delta or its own design problem?") resolves as **both, and the design problem is the one that matters**.

**Mechanically the delta is small.** The per-slug layout is *relatively identical* to the hash layout: `{slug_dir}/unimatrix.db` + `{slug_dir}/vector`, exactly mirroring `{data_dir}/unimatrix.db` + `{data_dir}/vector` (`PROJECT_VECTOR_DIR = "vector"`, `projects.rs:55` / `http_provision.rs:109`; `ensure_data_directory`, `project.rs:163-166`). Import's Phase 10 (`import/mod.rs:226`) passes `&paths.vector_dir` to `reconstruct_embeddings`, which dumps a fresh HNSW there (`embed_reconstruct.rs:110`). Redirecting it to `slug_dir.join("vector")` is one line, and `project register` already creates that dir (`projects.rs:330-332`). Import needs no vector logic it does not already have.

**But import exposes a hazard export does not: import is a WRITE against a store the daemon holds open, and the daemon will clobber the rebuilt index.**

- `build_project_server` loads each slug's vector index **at boot only** (`http_provision.rs:196-224`).
- At shutdown the daemon dumps every per-slug index back to its own dir: `for (index, dir) in &handles.per_slug_vectors { index.dump(dir) }` (`infra/shutdown.rs:189-197`, #823).

So: import into a live daemon's slug → import rebuilds `{slug}/vector` on disk → the daemon still holds its stale boot-time index in memory → on the next shutdown it **overwrites the freshly rebuilt index with the stale one**. The DB rows survive; vector search silently reverts. Restore appears to work and does not.

The supported restore sequence avoids this entirely by taking the daemon down across the import — `project register` already tells the operator a restart is needed (it prints "Restart to apply"):

```
project register <slug>   # creates {base}/<slug>/{unimatrix.db,vector}, writes [[projects]] stanza
stop                      # daemon releases every per-slug store; the live-PID gate (OQ-1) now clears
import --slug <slug> -i dump.jsonl   # no live daemon holds the store — safe to write + rebuild index
start                     # boots, loads the rebuilt index
```

This ordering is the actual product outcome. It is why `--slug` import is not merely "export's flag on the other command."

Import's existing pre-flight already has the right signal but the wrong strength: the PID check (`import/mod.rs:268-273`) is warning-only per SR-07, and the PID file lives in the **path-hash** `data_dir` — which is correct, because it is the *daemon's* PID, and one daemon serves all slugs. For `--slug` import the predicate is **live-PID-only**: a present, live daemon PID hard-errors the import. The `[[projects]]` half of the earlier predicate is dropped — config is read only at boot (#5079), so "slug in `[[projects]]`" cannot prove the daemon holds the store (register writes the stanza *before* the next restart), and gating on it would refuse the documented `register → stop → import → start` sequence. See OQ-1.

### Fresh slug stores are audit-empty (verified — C-5 / OQ-2 basis)

The restore target's collision-freedom rests on a freshly-registered slug store having a **zero-row `audit_log`**. Verified by tracing `project register` (State C, fresh — `projects.rs:322-346`) → `Store::open` (`store/src/db.rs:61`):

- `migrate_if_needed` (`store/src/migration.rs:42-58`) returns `Ok(())` immediately when the DB has no `entries` table — the fresh case — writing nothing.
- `create_tables_if_needed` (`store/src/db.rs:700`+) creates `audit_log` empty (`db.rs:989-1006`), installs only the `audit_log_no_update` / `audit_log_no_delete` **BEFORE UPDATE/DELETE** triggers (`db.rs:1023-1037`) — neither can insert a row — and its only INSERTs are `INSERT OR IGNORE INTO counters` (`db.rs:1272`+). No `INSERT INTO audit_log`, no "project created" audit event, no AFTER-INSERT trigger. (The runtime `audit_log` INSERT — `audit.rs:46`, `write_audit_event` — is a mutation-time primitive, never called by `Store::open`.)

So a fresh slug store's `audit_log` is provably zero rows. `insert_audit_log` (`import/inserters.rs:157-177`) is a plain explicit-`event_id` INSERT; against the fresh (audit-empty) supported target it **cannot collide**. C-5 therefore holds **as a constraint** (fail-loud on a non-empty target), not as a redesign — OQ-2 and AC-10 stand as written.

### Capability #5586 (BACKUP-RESTORE) — honest assessment

```
done_when: A backup then restore reproduces the full corpus with hash validation confirming losslessness.
proven_by: cargo test backup/restore round-trip across all tables with hash validation
           (nxs-012 + nan-001/002); NOTE — not a release-gate round-trip
delivered_by: nxs-012, nan-001, nan-002   |   tagged: delivery:proven
```

What **is** covered, genuinely: table-level losslessness. All 11 tables round-trip; 26 entry columns; f64 bit-exact confidence; JSON-in-TEXT emitted raw, not re-encoded; NULL vs empty-string distinction; content-hash and chain-link validation via the single `chain_verify` oracle inside the in-flight transaction, with ROLLBACK on a non-clean report. That work is real and is not in question.

What is **not** covered: **which database**. Every existing test seeds and reads through the same `ensure_data_directory` hash path (`make_project_dir()` in `import/mod.rs:513-524`; `tests/export_integration.rs` via `run_export_with_base`). The two resolvers are never made to disagree, so the suite could not have caught this — exactly lesson #5507. The `done_when` says "the full corpus"; on the cloud shape it reproduces *a* corpus that is not the project's.

Honest reading: `#5586` is proven for **local single-project** and **unproven for the cloud shape the goal calls the destination**. The `proven_by` does not name its resolver, which is what let a resolver-blind test read as full proof. Recommendation (the vision session owns the tag, not this one): flip `delivery:proven → partial`, tighten `proven_by` to name the resolver and shape, restore on the AC-09 seam evidence below, and only for the shape that evidence covers. Do not restore it on an export-only fix.

### Prior knowledge queried

`context_briefing` (vnc-048) surfaced the `personal-cloud` capability cluster (#5582/#5589/#5591/#5594) and container/config placement patterns (#4626, #5248). Prior agents' `context_get` results carried forward: #4972 (per-slug dirs are siblings of the path-hash dir under one base; hash dir names are charset-valid slugs), #5507 (two path resolvers disagree when e2e-testing sync subcommands — the root pattern here), #80 (ADR: project isolation via path hash), #1192 / #4577 (sync pre-tokio subcommand procedure + pattern), #5270 (hash-vs-slug data-placement symptom).

## Proposed Approach

Add `--slug <name>` to both `Export` and `Import`. When present, the target store is resolved by the **runtime's** scheme; when absent, behavior is untouched.

Rationale for the key choices — all three are reuse decisions, and each exists to avoid inventing a second scheme:

1. **One join site.** Reuse `per_slug_data_dir(base, &ProjectSlug)` (raise to `pub(crate)`). Its doc comment states the `&ProjectSlug` type *is* the proof of validation ("NEVER call this with a raw `&str`"). A second join site breaks that contract.
2. **One validation edge.** `ProjectSlug::try_from` (charset) + `is_reserved_slug` (reserved), kept as two separate checks, via the existing `ProjectRegistry::validate_slug` marked `pub(crate)` — the smaller diff over lifting it to a free function, same single-edge property, body unchanged.
3. **One base derivation.** `paths.data_dir.parent()`, `Option` handled with the same fallback the existing call sites use (`main.rs:1287`, `projects.rs:181-185`) — **no `unwrap`/`expect`**.

Ordering in both commands: validate slug → resolve `slug_dir` → **existence check on the db file** → open. The existence check must precede `SqlxStore::open` and never rely on it; `open` auto-creates and migrates, which would re-stage this exact defect in a new costume.

The gate is **file existence, not config registration**. `project list` is deliberately config-driven (#4972), but export must not copy that: the highest-value export is the one taken from a de-registered project before `project delete --purge` destroys it, and a registration gate would refuse exactly that. Accepted consequence — `--slug` means "a store dir under the base", not "a registered project", and a 16-hex hash dir name is a charset-valid slug that would resolve a real store. Document in help text; do not gate.

Import additionally derives `vector_dir = slug_dir/"vector"` and keeps `pid_path` from the path-hash `paths` (it is the daemon's PID, base-scoped, and remains correct in slug mode).

The silence that cost the operator time is closed by reporting, not by a heuristic: export prints a one-line count summary to stderr (`exported N entries, M audit rows → <path>`). Not a behavior change — it reports what already happened — and "exported 0 entries" is self-diagnosing for every future cause of a sparse export, including the `--skip-quarantined` asymmetry that is correctly staying as-is. Preferred over a sibling-slug-dir scan, which is a heuristic and reads as creep.

## Acceptance Criteria

- **AC-01**: `unimatrix --project-dir <dir> export --slug <slug> -o <file>` resolves `{base}/<slug>/unimatrix.db` (where `base` = `data_dir.parent()`) and exports exactly that store's corpus.
- **AC-02**: `unimatrix --project-dir <dir> import --slug <slug> -i <file>` restores into `{base}/<slug>/unimatrix.db` and rebuilds the HNSW index into `{base}/<slug>/vector`.
- **AC-03**: Export or import with `--slug` naming a slug with no store at the resolved path fails loud, creates nothing (no store, no dirs, no output file), and the error names the slug, the **fully resolved absolute path** tried, and the next action.
- **AC-04**: Export or import with a charset-invalid (`Foo!`, `a_b`, 64+ chars) or reserved (`v1`, `health`, `observe`, `tools`) slug is rejected at the CLI edge before any filesystem or DB access.
- **AC-05**: Invocations without `--slug` are behaviorally unchanged for export and import (single-project / local path byte-for-byte identical).
- **AC-06**: Export prints a one-line count summary to stderr naming entries exported, audit rows exported, and the resolved output path.
- **AC-07**: `--slug` help text on both commands states: base is derived from `--project-dir`, the in-container invocation is the expected posture, and `--slug` means "a store dir under the base," not "a registered project." Import's `--slug` help additionally carries a one-line pointer to the README restore procedure (OQ-3).
- **AC-08**: Export against a live daemon's slug store succeeds (read-only alongside WAL + `busy_timeout`); no locking is added.
- **AC-09** *(the seam test — the reason this shipped broken)*: a test seeds `{base}/<slug>/unimatrix.db` via the **`http_provision` literal-slug layout** with known entries, then `run_export_with_base(..., slug=Some("foo"), ...)` emits exactly those entries — and, with the hash store seeded differently, emits **none** of the hash store's contents. The two resolvers must be able to disagree.
- **AC-10**: Full round-trip test: seed a slug store via the literal-slug layout → export `--slug` → import `--slug` into a second, freshly-registered slug → the restored corpus matches the source across all tables and passes hash/chain validation.
- **AC-11**: Boundary guard: exporting **without** `--slug` from a base containing a populated slug dir yields only the hash store's data — documented, not silently reinterpreted.
- **AC-12**: The `register → stop → import --slug → start` sequence is documented in the README as the canonical restore procedure (OQ-3), and the restored slug serves vector search after `start` (the rebuilt index is the one the daemon loads).
- **AC-13**: `import --slug` hard-errors when a live daemon PID is present (OQ-1, live-PID-only predicate), naming the resolved PID path and the `stop → import → start` remedy; covered by a test asserting the refusal.

## Constraints

- **C-1 (hard)**: Base MUST be `paths.data_dir.parent()` with `Option` handled via the existing fallback idiom, no `unwrap`. `ensure_data_directory` builds `data_dir = unimatrix_base.join(&project_hash)`, so `parent()` is the `.unimatrix` base *by construction* in all four deploy shapes (in-container, local dev, `*_with_base` test hook, host bind-mount). No new base surface.
- **C-2 (hard)**: `per_slug_data_dir` is the only join site; `&ProjectSlug` (never `&str`) crosses into it. Traversal is closed **structurally**, not at runtime: `ProjectSlug::try_from` (`http/router/seam.rs:96-118`) enforces 1..=63 bytes, first char `[a-z0-9]`, rest `[a-z0-9-]` — so `.`, `/`, `\`, `%`, whitespace, NUL and uppercase are unrepresentable, making `..`, `%2e`, `%2f` and absolute paths impossible. ASCII-only makes the byte bound exact (no multi-byte bypass).
- **C-3 (hard)**: Existence check strictly before `SqlxStore::open` on both paths. `open` → `migrate_if_needed` (`store/src/db.rs:82`) is a **write** that auto-creates. An auto-created empty store *is this bug in a new costume*.
- **C-4 (hard, import)**: Import must not leave a live daemon holding a stale in-memory vector index over the rebuilt one — `infra/shutdown.rs:189-197` dumps `per_slug_vectors` at shutdown and would clobber it. Resolved per OQ-1: the **live-PID-only** refusal makes the shutdown-clobber **structurally unreachable** (import cannot run while any daemon is up) rather than conditionally avoided by operator discipline.
- **C-5 (hard, import)**: `insert_audit_log` (`import/inserters.rs:162-165`) is a plain `INSERT` with explicit `event_id`, and `drop_all_data` **cannot** clear `audit_log` (append-only triggers, vnc-014 / schema v25; audit history is preserved across import resets per ADR-005). Restoring into a slug store that has accumulated audit rows therefore hits a UNIQUE collision — even with `--force`. The supported restore target is a **freshly-registered (audit-empty) slug store**; anything else must fail loud with an actionable message, not a raw SQLite error.
- **C-6**: `ensure_data_directory` still creates and chmods the path-hash `data_dir` + `vector/` before its `db_path` is discarded in slug mode. Accepted — `ProjectRegistry::resolve` already does exactly this, and it is the price of one base derivation. Do not "optimize" it away by deriving the base another way.
- **C-7**: Host-side `--slug` (outside the container, against a bind-mounted data dir) resolves the host `$HOME` base and misses. Identical posture to `project list/register` today; fails loud with the resolved path (which is what distinguishes it from a typo — the single most likely operator mistake). Not a new mechanism; one line of help text.
- **C-8**: Both commands stay sync pre-tokio subcommands (procedure #1192 / pattern #4577); import keeps its multi-thread runtime (`block_in_place` in `embed_reconstruct` panics on `current_thread` — GH#554).
- **C-9**: Signature changes to `run_export`/`run_export_with_base`/`run_import`/`run_import_with_base` touch `main.rs:556-567` and the two integration test files only — mechanical, no shared runtime path modified.
- **C-10**: Rust workspace rules apply: no `.unwrap()` in non-test code, `cargo fmt`/`clippy -D warnings` clean, max 500 lines/file.

## Open Questions

- **OQ-1 (RESOLVED — hard error, live-PID-only)**: `import --slug` HARD-ERRORS when a live daemon PID is present, full stop — no warning-past. The predicate is **live-PID-only**; the "AND slug in `[[projects]]`" half is dropped, because config is read only at boot (#5079) so a stanza cannot prove the daemon holds the store, and gating on it would refuse the documented sequence. This reverses SR-07's warning-only stance for this one flag; a `--force`-style override is out of scope. The supported sequence `register → stop → import --slug → start` never trips the gate. Human decision at the scope gate.
- **OQ-2 (RESOLVED — refuse pre-flight)**: `import --slug` refuses pre-flight when the destination `audit_log` is non-empty (C-5), with the actionable message "restore targets a fresh slug; run `project register <new-slug>` and import there" — never the raw SQLite UNIQUE error. Confirmed safe: a freshly-registered slug store has a **zero-row** `audit_log` (verified — see Background Research), so the supported target passes pre-flight and the explicit-`event_id` import INSERT cannot collide. Human decision at the scope gate.
- **OQ-3 (RESOLVED — README canonical)**: The restore procedure's canonical home is the README; `--slug` help text carries a one-line pointer to it (AC-07/AC-12). The sequence is load-bearing for the outcome and must not be discoverable only by reading `project register`'s output. Human decision at the scope gate.
- **OQ-4 (RESOLVED — no import summary)**: No count summary is added to import — it already prints per-table counts via `print_summary`. AC-06 (the stderr summary) stays export-only; export had nothing before. Human decision at the scope gate.
- **OQ-5** *(not this session's call; recorded for the vision session)*: `#5586` `delivery:proven → partial` retag, `proven_by` tightened to name the resolver and shape, restored on AC-09/AC-10 evidence only.

## Tracking

GH Issue #953 — https://github.com/dug-21/unimatrix/issues/953. Opened as the export-half bug, escalated to design session vnc-048 by the human at the fix-approach checkpoint; the bugfix cycle was closed with no branch and no code changed. Issue carries the investigator's verified diagnosis, the architect's APPROVED WITH NOTES review, the product review, and the human's scope-change comment.
