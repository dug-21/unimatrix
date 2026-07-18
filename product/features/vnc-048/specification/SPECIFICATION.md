# vnc-048 Specification — Per-Slug Backup/Restore for Personal-Cloud

Source: `product/features/vnc-048/SCOPE.md` (approved; OQ-1..OQ-4 RESOLVED) and `SCOPE-RISK-ASSESSMENT.md` (SR-01..SR-11). GH Issue #953.

## Objective

Add `--slug <name>` to the operator `export` and `import` CLI subcommands so a personal-cloud operator can back up and restore a named per-slug project's actual knowledge corpus. Slug mode resolves the target store through the **runtime's** literal-slug scheme (`{base}/<slug>/unimatrix.db`), not the CLI's path-hash scheme, closing the silent two-resolvers gap. Every failure in the new paths is loud, names the fully-resolved absolute path, and states the next action; without `--slug`, behavior is byte-for-byte unchanged.

## Domain Models / Ubiquitous Language

- **Base** — the `.unimatrix` directory under which all project stores live. Derived exclusively from `paths.data_dir.parent()` (C-1). There is no `--base` flag, env var, or second derivation. In-container `HOME=/data` makes the base `/data/.unimatrix`.
- **Path-hash resolver** — the CLI's legacy scheme: `data_dir = base.join(<sha256-hash16>)` (`unimatrix-engine/src/project.rs:163`). Used by every CLI subcommand today, including export/import in no-`--slug` mode. A 16-hex hash segment is itself a charset-valid slug.
- **Slug resolver** — the runtime's scheme: `base.join(slug.as_str())` via `per_slug_data_dir(base, &ProjectSlug)` (`http_provision.rs:172-174`, `projects.rs:123-125`). Used by the daemon and `project register/list/delete`. `--slug` mode routes through this.
- **Slug store** — a per-slug store dir `{base}/<slug>/` containing `unimatrix.db` and `vector/` (`PROJECT_VECTOR_DIR = "vector"`). A sibling of the path-hash dirs under the same base (pattern #4972).
- **ProjectSlug** — the validated slug type. `ProjectSlug::try_from` enforces 1..=63 bytes, first char `[a-z0-9]`, rest `[a-z0-9-]`, ASCII-only. Its existence is the proof of validation; `per_slug_data_dir` must never be called with a raw `&str`.
- **Registered project vs store dir** — `--slug` names *a store dir under the base*, not a registered `[[projects]]` entry. Export must resolve on file existence, not config registration, so a de-registered-but-not-yet-purged project can still be exported.
- **The four deploy shapes** — the coverage axis for base derivation (C-1): (1) in-container (`HOME=/data`), (2) local dev, (3) `*_with_base` test hook, (4) host bind-mount. In each, `data_dir.parent()` must be the `.unimatrix` base by construction; where it is not (host bind-mount, SR-11), the resolve must fail loud with the resolved path, never no-op.

## Functional Requirements

Each requirement is testable; the mapped AC(s) and verification appear in Acceptance Criteria.

### Slug surface (both commands)

- **FR-1** — `export` and `import` each accept an optional `--slug <name>` argument. When absent, the command resolves and behaves exactly as today (path-hash). [AC-05]
- **FR-2** — When `--slug` is present, the target store is resolved through the slug resolver: `base = paths.data_dir.parent()`, `slug_dir = per_slug_data_dir(base, &ProjectSlug)`, `db = slug_dir/"unimatrix.db"`. `per_slug_data_dir` is the only join site; a `&ProjectSlug` (never `&str`) crosses into it. [AC-01, AC-02]
- **FR-3** — Slug validation runs at the CLI edge before any filesystem or DB access, using `ProjectRegistry::validate_slug` = `ProjectSlug::try_from` (charset) + `is_reserved_slug` (reserved), kept as two separate checks. Charset-invalid (`Foo!`, `a_b`, 64+ chars) and reserved (`v1`, `health`, `observe`, `tools`) slugs are rejected. [AC-04]
- **FR-4** — Base is derived from `paths.data_dir.parent()`; the `Option` is handled with the existing fallback idiom (`main.rs:1287`, `projects.rs:181-185`). No `unwrap`/`expect`. [AC-01, C-1]

### Existence-check-before-open ordering (both commands)

- **FR-5** — Ordering in both commands is: validate slug → resolve `slug_dir` → **existence check on the `unimatrix.db` file** → open. The existence check strictly precedes `SqlxStore::open` and never relies on it. [AC-03, C-3]
- **FR-6** — When `--slug` names a slug with no store at the resolved db path, the command fails loud, creates nothing (no store, no dirs, no output file), and the error names the slug, the **fully-resolved absolute path** tried, and the next action. [AC-03]

### Export specifics

- **FR-7** — `export --slug <slug>` exports exactly the corpus of `{base}/<slug>/unimatrix.db` and none of any other store's (path-hash or sibling slug). [AC-01, AC-09]
- **FR-8** — On success, `export` prints a one-line count summary **to stderr** naming entries exported, audit rows exported, and the resolved output path (e.g. `exported N entries, M audit rows → <path>`). This applies to export in both slug and no-slug modes; it reports what already happened and is not a behavior change. [AC-06]
- **FR-9** — `export --slug` against a live daemon's slug store succeeds read-only (alongside WAL + `busy_timeout`); no locking is added. [AC-08]

### Import specifics

- **FR-10** — `import --slug <slug>` restores into `{base}/<slug>/unimatrix.db` and rebuilds the HNSW index into `{base}/<slug>/vector` — i.e. import derives `vector_dir = slug_dir/"vector"` and passes it to `reconstruct_embeddings`. [AC-02, AC-12]
- **FR-11** — `import --slug` keeps `pid_path` from the path-hash `paths` (the daemon's PID, base-scoped, correct in slug mode). [AC-13, C-4]
- **FR-12** — `import --slug` HARD-ERRORS when a live daemon PID is present. The predicate is **live-PID-only** (a present, live daemon PID) — the `[[projects]]` config half is not consulted. The error names the resolved PID path and the `stop → import → start` remedy. This reverses SR-07's warning-only PID stance for this flag; no `--force` override. [AC-13, OQ-1, C-4]
- **FR-13** — `import --slug` refuses pre-flight when the destination store's `audit_log` is non-empty, with the actionable message directing the operator to register a fresh slug and import there — never a raw SQLite UNIQUE error. The supported restore target is a freshly-registered (audit-empty) slug store. [OQ-2, C-5]
- **FR-14** — No count summary is added to import (it already prints per-table counts via `print_summary`). AC-06 is export-only. [OQ-4]

### Help text and documentation

- **FR-15** — `--slug` help text on both commands states: (a) base is derived from `--project-dir`, (b) the in-container invocation is the expected posture, (c) `--slug` means "a store dir under the base," not "a registered project." Import's `--slug` help additionally carries a one-line pointer to the README restore procedure. [AC-07, OQ-3]
- **FR-16** — The README documents the canonical restore procedure `project register <slug> → stop → import --slug <slug> → start` as the supported, load-bearing sequence. It is the canonical home; the daemon's `project register` output alone is not sufficient discovery. [AC-12, OQ-3, SR-07]

## Non-Functional Requirements

- **NFR-1 (fallthrough parity)** — Invocations without `--slug` are behaviorally unchanged: the single-project/local path-hash path is byte-for-byte identical to today for both export and import. Verified as a property, not merely by one example. [AC-05, C-1]
- **NFR-2 (fail-loud, resolved path)** — Every accept-but-inert path — missing store (SR-02), non-empty `audit_log` (SR-05), host base miss (SR-11), live daemon (SR-03) — fails loud with an actionable message naming the **fully-resolved absolute path** and the next action. No silent no-op, no auto-create, no raw SQLite error surfaced to the operator.
- **NFR-3 (four-shape coverage)** — Base derivation must resolve correctly, or fail loud with the resolved path, in each of the four deploy shapes. Host-side `--slug` outside the container resolves the host `$HOME` base and misses; it must fail loud with the resolved path (the resolved path is what distinguishes a base miss from a typo), never no-op. [C-7, SR-11]
- **NFR-4 (no unwrap)** — No `.unwrap()`/`.expect()` in non-test code, including base-`Option` handling. `cargo fmt` and `clippy -D warnings` clean; max 500 lines/file. [C-1, C-10]
- **NFR-5 (sync pre-tokio)** — Both commands remain sync pre-tokio subcommands (procedure #1192 / pattern #4577). Import keeps its multi-thread runtime (`block_in_place` in `embed_reconstruct` panics on `current_thread`, GH#554). [C-8]
- **NFR-6 (no new base mechanism)** — No `--base` flag, no env var, no second configuration scheme for the base value. Base is only `paths.data_dir.parent()`. [C-1, Non-Goal]
- **NFR-7 (blast radius)** — Signature changes to `run_export`/`run_export_with_base`/`run_import`/`run_import_with_base` touch `main.rs:556-567` and the two integration test files only; no shared runtime path modified. `ensure_data_directory` still creates/chmods the path-hash `data_dir` + `vector/` before its `db_path` is discarded in slug mode (accepted, C-6 — not to be "optimized" away). [C-6, C-9]
- **NFR-8 (structural traversal closure)** — Path traversal is closed structurally at `ProjectSlug::try_from` (`.`, `/`, `\`, `%`, whitespace, NUL, uppercase unrepresentable; ASCII-only makes the byte bound exact), not by runtime sanitization. [C-2]
- **NFR-9 (no filter change)** — The `--skip-quarantined` / `audit_log` asymmetry is not touched (audit logs must never be altered). An audit-rows-only export remains a legitimate output; the stderr count summary (FR-8) makes it self-diagnosing. [Non-Goal, SR-08]

## Acceptance Criteria (with Verification Method)

Each criterion carries the SCOPE AC-ID and the test that proves it.

- **AC-01** — `export --slug <slug>` resolves `{base}/<slug>/unimatrix.db` (base = `data_dir.parent()`) and exports exactly that store's corpus.
  **Verify:** unit/integration test asserting the resolved absolute db path equals `{base}/<slug>/unimatrix.db` and that emitted rows are the slug store's.

- **AC-02** — `import --slug <slug>` restores into `{base}/<slug>/unimatrix.db` and rebuilds the HNSW index into `{base}/<slug>/vector`.
  **Verify:** integration test asserting post-import DB rows land in the slug db and a fresh HNSW file exists under `{base}/<slug>/vector`.

- **AC-03** — `--slug` naming a slug with no store at the resolved path fails loud, creates nothing (no store, no dirs, no output file), and the error names the slug, the fully-resolved absolute path, and the next action.
  **Verify:** test invoking export and import against a nonexistent slug; assert non-zero result, error message contains the resolved absolute path, and the filesystem is unchanged (no new dirs/db/output). Covers SR-02.

- **AC-04** — Charset-invalid (`Foo!`, `a_b`, 64+ chars) or reserved (`v1`, `health`, `observe`, `tools`) slug is rejected at the CLI edge before any filesystem or DB access.
  **Verify:** parameterized test over invalid + reserved slugs asserting rejection with no filesystem side effects.

- **AC-05** — No-`--slug` invocations are behaviorally unchanged (single-project/local path byte-for-byte identical).
  **Verify:** existing export/import integration suites pass unchanged; a fallthrough assertion that the resolved path in no-slug mode is the path-hash `data_dir`. Covers NFR-1.

- **AC-06** — Export prints a one-line count summary to stderr naming entries exported, audit rows exported, and the resolved output path.
  **Verify:** test capturing stderr and asserting it contains the entry count, audit-row count, and resolved output path; assert stdout unaffected.

- **AC-07** — `--slug` help text on both commands states base derivation, in-container posture, and store-dir-not-registered-project semantics; import's help carries the README pointer.
  **Verify:** help-output snapshot/assertion test for both commands.

- **AC-08** — Export against a live daemon's slug store succeeds read-only; no locking added.
  **Verify:** integration test opening the slug store read-only while a WAL/`busy_timeout` writer context is simulated; assert export succeeds.

- **AC-09 (SEAM TEST — highest weight)** — A test seeds `{base}/<slug>/unimatrix.db` via the **`http_provision` literal-slug layout** with known entries, then `run_export_with_base(..., slug=Some("foo"), ...)` emits **exactly** those entries; with the path-hash store seeded **differently**, the export emits **none** of the hash store's contents.
  **Verify:** the two resolvers must be able to disagree — seed through the runtime layout, read through the CLI resolver. Assert (a) emitted rows == slug store's seeded rows, (b) intersection with hash-store-only rows is empty. An N=1 same-path test is insufficient (#4974, #5507). Covers SR-01, SR-09.

- **AC-10 (ROUND-TRIP — highest weight)** — Full round-trip: seed a slug store via the literal-slug layout → `export --slug` → `import --slug` into a **second, freshly-registered** slug → the restored corpus matches the source across all tables and passes hash/chain validation.
  **Verify:** integration test comparing all round-tripped tables (entries + 26 columns, entry_tags, co_access, feature_entries, graph_edges, audit_log, counters, etc.), asserting f64 bit-exact confidence, raw JSON-in-TEXT, NULL-vs-empty distinction, and a clean `chain_verify` report. Target slug is freshly registered (audit-empty) so the explicit-`event_id` INSERT cannot collide. Covers C-5 basis.

- **AC-11** — Export **without** `--slug` from a base containing a populated slug dir yields only the hash store's data — documented, not silently reinterpreted.
  **Verify:** test seeding both a slug dir and the hash store under one base; assert no-slug export emits only hash-store rows. Boundary guard for SR-04.

- **AC-12** — The `register → stop → import --slug → start` sequence is documented in the README as canonical, and the restored slug serves vector search after `start` (the rebuilt index is the one the daemon loads).
  **Verify:** README assertion (sequence present) + integration/functional test proving the outcome **from `start` onward** — vector search served post-restart, not just rows present on disk. Covers SR-07, SR-10.

- **AC-13** — `import --slug` hard-errors when a live daemon PID is present (live-PID-only predicate), naming the resolved PID path and the `stop → import → start` remedy.
  **Verify:** test with a live-PID present at the base-scoped path-hash `pid_path`; assert import refuses, message contains the resolved PID path and remedy. Covers SR-03 (clobber made structurally unreachable), OQ-1.

## User Workflows

**Backup (export):** Operator execs into the container (`HOME=/data`), runs `unimatrix --project-dir <dir> export --slug <slug> -o dump.jsonl`. CLI validates the slug, resolves `/data/.unimatrix/<slug>/unimatrix.db`, checks existence, exports, and prints `exported N entries, M audit rows → <path>` to stderr.

**Restore (import) — canonical sequence:**
1. `project register <slug>` — creates `{base}/<slug>/{unimatrix.db,vector}`, writes the `[[projects]]` stanza.
2. `stop` — daemon releases every per-slug store; the live-PID gate clears.
3. `import --slug <slug> -i dump.jsonl` — with no live daemon, safe to write DB + rebuild `{slug}/vector`.
4. `start` — daemon boots and loads the rebuilt index; the slug serves the restored corpus including vector search.

Deviating (skipping `stop`) trips the live-PID hard error (AC-13); importing into an already-used slug trips the non-empty-`audit_log` refusal (FR-13).

## Constraints

Carried from SCOPE (hard unless noted):

- **C-1 (hard)** — Base MUST be `paths.data_dir.parent()`, `Option` via existing fallback, no `unwrap`. No new base surface.
- **C-2 (hard)** — `per_slug_data_dir` is the only join site; `&ProjectSlug` (never `&str`) crosses in. Traversal closed structurally.
- **C-3 (hard)** — Existence check strictly before `SqlxStore::open` on both paths.
- **C-4 (hard, import)** — Import must not leave a live daemon holding a stale in-memory vector index over the rebuilt one; live-PID-only refusal makes the clobber structurally unreachable.
- **C-5 (hard, import)** — Restoring into a slug store with accumulated audit rows hits a UNIQUE collision even with `--force`; supported target is a freshly-registered (audit-empty) slug; anything else fails loud with an actionable message.
- **C-6** — `ensure_data_directory` still creates/chmods the path-hash `data_dir` + `vector/` before its `db_path` is discarded in slug mode. Accepted; do not optimize away.
- **C-7** — Host-side `--slug` resolves host `$HOME` base and misses; fails loud with the resolved path. One line of help text.
- **C-8** — Both commands stay sync pre-tokio subcommands; import keeps its multi-thread runtime.
- **C-9** — Signature changes touch `main.rs:556-567` and the two integration test files only.
- **C-10** — No `.unwrap()` in non-test code; `cargo fmt`/`clippy -D warnings` clean; max 500 lines/file.

## Dependencies

- `unimatrix-engine` — `per_slug_data_dir` (raise to `pub(crate)`), `ProjectRegistry::validate_slug` (raise to `pub(crate)`), `ProjectSlug::try_from`, `is_reserved_slug`, `ensure_data_directory`, `paths.data_dir`.
- `unimatrix-store` — `SqlxStore::open`, `import/inserters::insert_audit_log`, `import/mod` (`reconstruct_embeddings`, `print_summary`, `drop_all_data`), export corpus reader.
- Binary crate `main.rs` — `run_export`/`run_export_with_base`/`run_import`/`run_import_with_base` signatures, clap arg wiring.
- README — canonical restore procedure section.
- No new crates, no external services.

## NOT in Scope

- **The `--skip-quarantined` / `audit_log` filter asymmetry** — correct as designed, not touched.
- **A new base-resolution mechanism** (`--base` flag, env var, second scheme) — explicitly refused.
- **Slug-awareness for the other CLIs** (`verify`, `snapshot`, `eval`, `health`, `stop`, `client-bundle`) — out of scope; this feature establishes the `--slug` pattern they may copy.
- **Restoring over a slug store with existing audit history** — fails loud, out of scope.
- **Live-daemon import** (locking, daemon-mediated import, index invalidation) — refused outright; separate design problem.
- **Backup as disaster recovery** — DR story (volume snapshot) unchanged.
- **Version skew** (exporter/importer newer than the daemon) — pre-existing hazard; exec-into-container mitigation applies.
- **A `--force`-style override** for the live-PID or non-empty-audit refusals.
- **An import count summary** (import already prints per-table counts).
- **`#5586` capability retag** (OQ-5) — recorded for the vision session, not this one.

## Open Questions

None blocking. OQ-1..OQ-4 are RESOLVED in SCOPE and honored above. OQ-5 (`#5586` retag) is explicitly owned by the vision session, not this specification. One item for the architect to confirm during design: the exact fallback value/idiom for a `None` from `data_dir.parent()` (SCOPE points at `main.rs:1287` / `projects.rs:181-185`) — the specification requires only that it not `unwrap` and that a base miss fail loud with the resolved path (NFR-3/NFR-4).

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing (vnc-048) — surfaced personal-cloud capability cluster (#5591 per-slug isolated stores, #5565 multi-client-per-slug, #5533 one isolation seam) and per-slug dir sibling pattern #4972; confirmed slug-resolver placement and base-under-one-`.unimatrix` model. No conflicting conventions found.
- Declined: no storage — spec decisions are feature-specific interpretations of resolved scope, not generalizable patterns (read-only tier). Any interpretation that generalizes can be promoted at retro.
