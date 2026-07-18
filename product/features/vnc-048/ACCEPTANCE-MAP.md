# vnc-048 Acceptance Criteria Map

Every AC-01..AC-13 from SCOPE.md, mapped to its source requirement, verification method, and the risk (R-XX / SR-XX) it retires. **AC-09 (seam divergence) and AC-10 (round-trip) carry top weight** — the feature is unproven for the personal-cloud destination without both (Risk Strategy gate non-negotiables).

| AC-ID | Description | Source (FR / Constraint) | Verification Method | Verification Detail | Retires | Weight | Status |
|-------|-------------|--------------------------|---------------------|---------------------|---------|--------|--------|
| AC-01 | `export --slug` resolves `{base}/<slug>/unimatrix.db` (base = `data_dir.parent()`) and exports exactly that store's corpus | FR-2, FR-7, C-1 | test | Assert resolved absolute db path == `{base}/<slug>/unimatrix.db`; emitted rows are the slug store's | SR-01, R-01 | High | PENDING |
| AC-02 | `import --slug` restores into `{base}/<slug>/unimatrix.db` and rebuilds HNSW into `{base}/<slug>/vector` | FR-10, ADR-004 | test | Integration: post-import DB rows land in slug db; fresh HNSW file exists under `{base}/<slug>/vector`; nothing written to hash `vector/` | SR-10, R-03 | High | PENDING |
| AC-03 | `--slug` naming a slug with no store fails loud, creates nothing (no store/dirs/output), error names slug + fully-resolved absolute path + next action | FR-5, FR-6, C-3 | test | export + import vs nonexistent slug: non-zero exit; error contains resolved absolute path; stat parent before/after → FS unchanged (no db, `vector/`, `-wal`/`-shm`, output) | SR-02, R-02, R-14 | High | PENDING |
| AC-04 | Charset-invalid (`Foo!`, `a_b`, 64+ chars) or reserved (`v1`, `health`, `observe`, `tools`) slug rejected at CLI edge before any FS/DB access | FR-3, C-2, NFR-8 | test | Parameterized reject set (invalid + reserved + traversal `../x`, `%2e`, `/abs`, NUL); assert rejection with zero FS side effects | SR-01, R-08 | High | PENDING |
| AC-05 | No-`--slug` invocations behaviorally unchanged (byte-for-byte identical — **exported file + stdout + exit code**; stderr excluded per WARN-1) | FR-1, NFR-1, C-1 | test | Existing export/import suites pass unchanged; fallthrough assertion: no-slug resolved path == path-hash `data_dir`; confirm no existing test asserts stderr emptiness | R-09 | High | PENDING |
| AC-06 | Export prints one-line count summary to stderr naming entries, audit rows, resolved output path | FR-8, ADR-006 | test | Capture stderr: contains entry count, audit-row count, resolved output path; stdout unaffected; 0-entry+audit-rows case reads `exported 0 entries, M audit rows` | SR-08, R-10 | Med | PENDING |
| AC-07 | `--slug` help text states base derivation, in-container posture, store-dir-not-registered-project semantics; import help carries README pointer | FR-15, OQ-3 | test | Help-output snapshot/assertion for both commands | SR-04, SR-07, R-12 | Med | PENDING |
| AC-08 | Export against a live daemon's slug store succeeds read-only; no locking added | FR-9 | test | Integration: open slug store read-only under WAL + `busy_timeout` writer context; assert export succeeds (#2621 analogue) | R-03 (read side) | Med | PENDING |
| **AC-09** | **SEAM TEST** — seed `{base}/<slug>/unimatrix.db` via `http_provision` literal-slug layout (set A); seed path-hash store differently (set B); `run_export_with_base(slug=Some("foo"))` emits **exactly A** and **none of B** | FR-7, ADR-001, `_with_base` axis | test | Seed via runtime layout, read via CLI resolver (must be different code). Assert emitted == A **and** ∩(emitted, B) == ∅; B non-empty + disjoint. N=1 same-path test is ceremonial (#4974) — DOES NOT satisfy | **SR-01, SR-09, R-01** | **TOP** | PENDING |
| **AC-10** | **ROUND-TRIP** — seed slug store via literal-slug layout → `export --slug` → `import --slug` into a **second, freshly-registered** slug → corpus matches across all tables + passes hash/chain validation | FR-13, ADR-005, C-5 | test | Diff all tables (entries + 26 cols, entry_tags, co_access, feature_entries, graph_edges, audit_log, counters). f64 bit-exact confidence; raw JSON-in-TEXT; NULL-vs-empty preserved; clean `chain_verify`. Target audit-empty so explicit-`event_id` INSERT cannot collide. Crosses two different slugs (A→B, not A→A) | **SR-05, SR-08, R-04, R-07** | **TOP** | PENDING |
| AC-11 | Export **without** `--slug` from a base containing a populated slug dir yields only the hash store's data — documented, not silently reinterpreted | ADR-002, C-1 | test | Seed both a populated slug dir and the hash store under one base; assert no-slug export emits only hash-store rows | SR-04, R-13 | Med | PENDING |
| AC-12 | `register → stop → import --slug → start` documented in README as canonical; restored slug serves vector search after `start` (rebuilt index is the one loaded) | FR-16, ADR-004, OQ-3 | test + file-check | README assertion (sequence present) + integration: run full sequence, then served vector query returns restored corpus's semantic hits — proven from `start`, not disk state | SR-07, SR-10, R-03 (S2), R-12 | High | PENDING |
| AC-13 | `import --slug` hard-errors when a live daemon PID is present (live-PID-only predicate), naming resolved PID path + `stop → import → start` remedy | FR-11, FR-12, ADR-003, C-4 | test | Live PID at base-scoped path-hash `pid_path` → import refuses; message contains resolved PID path + remedy; assert no write to `{slug}/vector`. Predicate is live-PID-only: `[[projects]]` stanza w/o live daemon does NOT block; stale/dead PID does NOT block (`is_process_alive`/`is_unimatrix_process`) | SR-03, R-03 (S1), R-11 | High | PENDING |

## Verification Type Legend

- `test` — cargo test / specific test function (unit or integration)
- `file-check` — content asserted present in a file (README sequence)

## Gate Non-Negotiables

Per RISK-TEST-STRATEGY.md, the feature is **unproven for the personal-cloud shape** — regardless of how many same-path / disk-state tests pass — absent both:

1. **AC-09** disagreement seam: hash-store set B non-empty and disjoint from A, seeded via runtime literal-slug layout, read via CLI resolver.
2. **AC-12 / AC-13 (R-03 S2)** served vector search after the full `register → stop → import --slug → start` sequence — the outcome proven from `start` onward, not from disk state.

## Coverage Notes

- Every accept-but-inert path (AC-03, AC-04, AC-06, AC-13, and non-empty-audit under AC-10) is a **fail-loud** requirement naming the fully-resolved absolute path — no silent no-op, no auto-create, no raw SQLite error surfaced.
- Four deploy shapes (in-container / local dev / `_with_base` hook / host bind-mount) are a coverage axis for base derivation, not one representative — each resolves correctly or fails loud with the resolved path (C-1, NFR-3; AC-01 / AC-03 / AC-09).
