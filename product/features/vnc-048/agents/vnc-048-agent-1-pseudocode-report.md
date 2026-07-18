# Agent Report — vnc-048-agent-1-pseudocode

Stage 3a pseudocode for per-slug backup/restore (`--slug` on `export`/`import`).

## Deliverables

Under `product/features/vnc-048/pseudocode/`:

- `OVERVIEW.md` — component map, build waves, shared types (`SlugStorePaths`), the
  `resolve_slug_store` contract, four-shape base derivation, fail-loud inventory, invariants.
- `resolve_slug_store.md` — Component 1 (funnel + two `pub(crate)` visibility raises).
- `export.md` — Component 2 (export slug branch + stderr count summary).
- `import.md` — Component 3 (import slug branch + live-PID gate + non-empty-audit gate + vector redirect).
- `main-dispatch.md` — Component 4 (clap `--slug` wiring).
- `readme.md` — Component 5 (canonical restore-procedure doc content).

## Components covered

resolve_slug_store (projects.rs), export.rs, import/mod.rs, main.rs, README. `infra/pidfile.rs`
reused unchanged (no pseudocode).

## Invariant fidelity

One base derivation (`data_dir.parent()`, no unwrap, Component 1 only); one join site
(`per_slug_data_dir`, `&ProjectSlug`); one validation edge (`validate_slug`); existence gate
strictly before `SqlxStore::open` (C-3); live-PID-only import refusal (ADR-003); non-empty-audit
pre-flight before any write (ADR-005); vector rebuild into `slug_dir/vector`, PID stays
`paths.pid_path` (ADR-004); every accept-but-inert path names the fully-resolved absolute path.
Export/import branches call the funnel; neither re-derives, re-joins, or re-validates.

## Grounding

Verified signatures against source: `run_export`/`run_export_with_base`/`run_export_inner`
(export.rs:32-68), `run_import`/`_with_base`/`_inner`/`run_import_async` (import/mod.rs:54-128),
`check_preflight` (import/mod.rs:250-276), `per_slug_data_dir` (projects.rs:123),
`ProjectRegistry::validate_slug` (projects.rs:206), clap `Export`/`Import` + dispatch
(main.rs:332-365, 556-581).

## Open questions (flagged, non-blocking)

1. **`PROJECT_VECTOR_DIR` constant scope in `projects.rs`** — funnel must reuse the canonical
   "vector" constant (byte-identical to `http_provision`), never a second literal. Implementer
   confirms the constant is importable or defines one matching value.
2. **Export count plumbing** — carrying `(entries, audit_rows)` out for the AC-06 summary:
   either widen `block_export_sync` to `Result<T>` or emit the summary inside the async block
   after COMMIT (recommended — no signature ripple). `export_audit_log` must return its written
   row count (currently `()`).
3. **AC-06 "resolved output path" wording** — summary names the output destination (`→ <path>`
   or `→ stdout`) per FR-8; confirm tester does not also expect the source db path echoed.
4. **Audit-gate message path** — `check_preflight` receives path-hash `&paths`; the non-empty-
   audit message must name the resolved *slug* db path. Recommend moving the audit gate into
   `run_import_async` (where `db_target` is in scope) or passing `db_target` in.

None block Stage 3b; all are local implementation choices with a recommended resolution.

## Knowledge Stewardship

- Queried: `context_search` "export import CLI slug resolution patterns per_slug_data_dir"
  (pattern) — surfaced #4617, #682, #5170, #5643, #4972 (per-slug sibling-dir pattern); no
  conflicting convention. `context_search` "vnc-048 architectural decisions resolve_slug_store"
  (decision) — surfaced the feature's own ADRs #5693 (ADR-001), #5694 (ADR-002), #5696 (ADR-004);
  all consistent with the ADR files read. Applied #4972 (siblings under one base) and the
  two-resolver trap (#5507) throughout the funnel + AC-09 seam design.
- Declined: no storage. Read-only tier; pseudocode is a feature-specific translation of resolved
  architecture, not a generalizable cross-feature pattern. The four-deploy-shape-as-coverage-axis
  and single-funnel-resolver patterns already exist (#4950, #5507) and can be promoted at retro
  if the sibling-CLI slug-awareness work recurs.
- Deviations from established patterns: none. Design reuses the existing triad seams; no new
  scheme, flag, or resolver introduced.
