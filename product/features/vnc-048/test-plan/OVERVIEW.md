# vnc-048 Test Strategy — Per-Slug Backup/Restore (`--slug` on export/import)

Root risk class: **two resolvers over one base** (#5507). The whole strategy proves the funnel
`resolve_slug_store` resolves the store the *runtime writes to*, driven from the operator's real
CLI entry points (`run_export_with_base` / `run_import_with_base` / the full CLI sequence) — never
from a proxy one layer beneath (a `PathBuf` return, a disk-state stat). A test that asserts an
outcome beneath the entry point is **necessary but not sufficient** (Risk Strategy, behavioral lens).

Sources: SPECIFICATION.md (FR-1..16, AC-01..13, C-1..10), ARCHITECTURE.md (ADR-001..006),
RISK-TEST-STRATEGY.md (R-01..14, SR-01..11), ACCEPTANCE-MAP.md, IMPLEMENTATION-BRIEF.md.

## Gate Non-Negotiables (TOP weight — feature is unproven for personal-cloud without both)

1. **AC-09 / R-01 S1 — the disagreement seam** (`export.md`). Using the `*_with_base` hook so
   `data_dir.parent() == X`: seed `X/<slug>/unimatrix.db` via the **runtime `http_provision`
   literal-slug layout** with entry set **A**; seed the path-hash store `X/<hash>/unimatrix.db`
   **differently** with a **disjoint, NON-EMPTY** set **B**. Call `run_export_with_base(project_dir,
   base=X, slug=Some("foo"))`. Assert `emitted == A` **and** `emitted ∩ B == ∅`.
   **An N=1 same-path test (seed and read through the same layout, or B empty/aliased) is CEREMONIAL
   (#4974) and DOES NOT satisfy AC-09 — the test file MUST state this and the seed path (runtime
   layout) MUST be different code from the read path (CLI resolver).** Paired divergence guard
   (R-01 S2): same seeding, **no** `--slug` → assert export emits **B**, proving the fixture does
   not accidentally alias the two stores onto one path.

2. **AC-12 / R-03 S2 — served vector search from `start`** (`import.md`). Run the full
   `register → stop → import --slug → start` sequence; after `start`, issue a **served vector
   search** against the restored slug and assert it returns the restored corpus's semantic hits.
   **Proven from `start` onward, not from disk state.** A disk-state stat that `{slug}/vector`
   holds a fresh HNSW file (AC-02) is necessary but does NOT discharge SR-10.

Absent either, no count of same-path or disk-state tests substitutes.

## Test Levels

| Level | Vehicle | Scope |
|-------|---------|-------|
| Unit | `#[cfg(test)]` in `projects.rs` | `resolve_slug_store` funnel order, base-derivation fallback, validation edge, existence gate, `SlugStorePaths` shape (derivation-unit assertions for deploy shapes 2 & 3) |
| Integration (Rust) | `crates/unimatrix-server/tests/export_integration.rs`, `import_integration.rs` | Everything driven through `run_export_with_base` / `run_import_with_base` — the operator entry points. This is the **primary** integration surface for this feature. |
| CLI-sequence / functional | new test (import side) driving the compiled binary through `register → stop → import → start` + served vector query | AC-12 / R-03 S2 outcome-from-`start` |
| Help / doc | snapshot assertion (`main-dispatch.md`) + README content check (`readme.md`) | AC-07, AC-12/FR-16 |

Unit tests here are **funnel-shape** proofs (ordering, no-`unwrap`, structural rejection); they never
stand in for a gate AC. Per the "prove the assembled path" principle, every `done_when`-backing test
(AC-01/02/03/09/10/12/13) drives a real `run_*` entry point or the assembled CLI sequence, not a
hand-constructed `SlugStorePaths` re-asserting its own literal.

## Risk → Test → AC mapping

| Risk | Priority | AC(s) | Test plan location | Vehicle |
|------|----------|-------|--------------------|---------|
| R-01 resolver disagreement | Critical | AC-09, AC-01 | export.md | integration seam + divergence guard |
| R-02 `open` before existence gate | Critical | AC-03 | export.md, import.md, resolve_slug_store.md | integration FS-unchanged + unit ordering |
| R-03 live-daemon vector clobber | Critical | AC-12, AC-13, AC-02 | import.md | CLI-sequence served-query + refusal + no-write-to-vector |
| R-04 round-trip lossless A→B | High | AC-10 | import.md | integration all-tables diff + chain_verify |
| R-05 base-derivation per shape | High | AC-01 | resolve_slug_store.md | unit derivation axis (shapes 1–3) |
| R-06 host bind-mount silent no-op | High | AC-03 | resolve_slug_store.md, export.md | fail-loud naming host path |
| R-07 non-empty-audit refusal | High | AC-10/FR-13 | import.md | integration pre-flight refusal |
| R-08 validation bypass / traversal | High | AC-04 | resolve_slug_store.md, main-dispatch.md | parameterized reject, zero FS side effects |
| R-09 no-`--slug` fallthrough parity | High | AC-05 | export.md, import.md | existing suites unchanged + resolved-path identity property |
| R-10 silent sparse export | Med | AC-06 | export.md | stderr count summary capture |
| R-11 live-PID gate correctness | Med | AC-13 | import.md | live-only predicate (stale/stanza do not block) |
| R-12 restore-sequence discoverability | Med | AC-07, AC-12 | readme.md, main-dispatch.md | README + help content |
| R-13 stray/hash-dir boundary | Low→Med | AC-11 | export.md | no-slug emits only hash store |
| R-14 partial-failure side effects | Med | AC-03 | export.md, import.md | FS-unchanged on every fail-loud path |

## The `_with_base` wrinkle (load-bearing for AC-09/AC-10)

`ensure_data_directory(_, Some(X))` sets `unimatrix_base = X` **verbatim** (not `X/.unimatrix`), so
`data_dir.parent() == X` and `resolve_slug_store` joins `X/<slug>` — the *same* path the
`http_provision` literal-slug layout writes. Every seam/round-trip integration test relies on this to
seed via the runtime layout at `X/<slug>` and read via the CLI resolver. Existing helper
`setup_project()` in both test files already returns `(project_dir, base_dir, db_path)` with the
GH#640 `data_dir.starts_with(base_dir)` guard — extend it, do not fork a new scaffold (test infra is
cumulative).

## Deploy-shape coverage axis (four shapes, not one representative — C-1/NFR-3, R-05/R-06)

| Shape | Vehicle | Assertion |
|-------|---------|-----------|
| `_with_base` hook (`data_dir.parent()==X`) | integration (AC-09 vehicle) | slug resolves at `X/<slug>`; correct-resolve |
| In-container (`HOME=/data`→`/data/.unimatrix`) | unit derivation assertion | `data_dir.parent()==/data/.unimatrix` (no container in CI) |
| Local dev (`None`→`~/.unimatrix`) | unit derivation assertion | parent == base |
| Host bind-mount (host `$HOME` base ≠ container base) | integration fail-loud | `--slug` fails loud naming the **host** resolved absolute path; never no-ops, never resolves container store |

Also: `data_dir.parent()==None` (base at FS root) exercises the fallback idiom with **no `unwrap`**
(NFR-4) — unit test in resolve_slug_store.md.

## Integration Harness Plan (infra-001 + Rust integration + link smoke)

**This feature adds NO MCP-visible behavior.** `--slug` is an operator CLI (`export`/`import`)
concern; no server tool, no JSON-RPC surface, no `[[projects]]`/HTTP path is modified (C-9). Per the
"When NOT to plan integration tests" rule (pure behavior with no MCP-visible effect), **no new
infra-001 pytest suite tests are added.**

What runs in Stage 3c, and why:

1. **infra-001 smoke (`pytest suites/ -v -m smoke --timeout=60`) — MANDATORY minimum gate, as a
   NON-REGRESSION guard.** C-9 promises no shared runtime/MCP path changed; a green smoke run proves
   the signature changes and `pub(crate)` visibility raises produced no collateral on the server's
   tool surface. No suite selection beyond smoke is warranted by the suite-selection table because
   the feature touches no server tool logic, confidence, contradiction, or security-scanning path.
   (If the build/link changes perturb the binary, smoke is where it surfaces.)
2. **Rust workspace tests (hardened convention) — primary functional coverage.** The AC-01..13
   evidence lives in `export_integration.rs` / `import_integration.rs` (+ `projects.rs` units).
   Run via the canonical hardened form (`setsid -w timeout ... cargo test --workspace`).
3. **Full-workspace LINK smoke (#878 guard) — MANDATORY for this Rust change.**
   `bash product/test/infra-002/check-workspace-link-smoke.sh`. New integration tests add test
   binaries to link; this guards the link-OOM regression. Do NOT `--lib`-salvage.
4. **CLI-sequence functional test (AC-12)** drives the compiled binary (`cargo build --release`
   first) through `register → stop → import --slug → start` and a served vector query. This is the
   one place a real daemon boot is exercised; keep it in the import integration file or a dedicated
   `slug_restore_sequence` test module.

Existing infra-001 suites that would flag *collateral* regressions if the visibility raises or
signature changes leaked: `protocol`, `tools` (tool discovery / count — note the tool-count
assertion is currently 15 per #942). None are expected to change; any change is a triage signal, not
an edit target (failure triage: pre-existing/unrelated → xfail + GH Issue, never fix in this PR).

## Cross-component test dependencies

- `resolve_slug_store` (foundation, Wave A) must land before export/import integration tests can
  drive slug mode. Unit tests for the funnel gate the wave.
- The AC-09 seam (export) and AC-10 round-trip (import) share the **runtime literal-slug seeding
  helper** — define it once (seed `X/<slug>/unimatrix.db` via `per_slug_data_dir(base,&slug)` +
  `SqlxStore::open` + row inserts, mirroring the `http_provision` layout) and reuse across both test
  files. Guard against interface drift with pseudocode: the helper's signature is fixed by
  `per_slug_data_dir(base:&Path, slug:&ProjectSlug)->PathBuf` (col-030 lesson — parallel
  pseudocode/test-plan divergence).
- `main-dispatch` help snapshot depends on the final help strings authored in `readme.md`/FR-15.

## WARN-1 reconciliation (must-carry in R-09 tests)

AC-05 byte-for-byte identity is scoped to **exported file + stdout + exit code — stderr EXCLUDED**
(FR-8 adds the stderr count summary on both modes). R-09 test plan carries a one-line check:
**confirm no existing export/import integration test asserts on empty/absent stderr**; if one does,
update it to permit the summary line (not a regression). Grep target: `stderr` assertions in
`export_integration.rs` / `import_integration.rs`.

## Conventions

- Arrange/Act/Assert; naming `test_{fn}_{scenario}_{expected}`.
- Integration tests use `run_export_with_base` / `run_import_with_base` (base pinned to a TempDir);
  never `~/.unimatrix` (GH#640 leak guard already enforced by `setup_project`).
- Import tests keep the multi-thread runtime (`#[tokio::test(flavor = "multi_thread")]` / the
  existing `open_store` `block_in_place` idiom) — `block_in_place` in `embed_reconstruct` panics on
  `current_thread` (GH#554, C-8).
- No test deletes or comments out an existing integration test.
</content>
