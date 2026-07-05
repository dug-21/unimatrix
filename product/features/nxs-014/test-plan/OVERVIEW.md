# nxs-014 Test Strategy Overview — Cross-Version Hash Chain (Weak Mode) + Chain-Verify Core

> Rooted in RISK-TEST-STRATEGY.md (R-01..R-12) and ACCEPTANCE-MAP.md (AC-01..AC-12).
> Every test traces to a risk. Component test plans map 1:1 to the pseudocode components
> in IMPLEMENTATION-BRIEF.md §Component Map. The non-negotiable design rule: tests must be
> written so a half-fix FAILS — no in-memory-only assertions, no green-on-skip.

## Test Levels

| Level | Where | Covers |
|-------|-------|--------|
| Unit — pure core | `unimatrix-store/src/chain_verify.rs` `#[cfg(test)]` | `verify_entries` over hand-built `Vec<EntryRecord>`; every `ViolationKind`; legacy-skip; is_clean (R-03, R-08, R-09, R-12) |
| Unit/integration — write path | `unimatrix-store/src/write_ext.rs` `#[cfg(test)]` (`#[tokio::test]`) | correction populates both sites; **DB read-back** assertions; N-hop chain; empty-predecessor reject (R-01, R-08) |
| Unit — loader guard | `unimatrix-store/src/read.rs` (or write_ext test mod) | `query_all_entries()` returns `Deprecated` rows (R-02) |
| Unit — frozen hash | `unimatrix-store/src/hash.rs` `#[cfg(test)]` (existing) | known-value vectors unchanged (R-06, AC-10) |
| Integration — import | `unimatrix-server` import tests | `validate_hashes` refactor: AND-halves, ROLLBACK atomicity, Deprecated predecessor on txn conn, existing tests unchanged (R-04, R-05, R-07) |
| CLI integration | `unimatrix-server` Rust integration test (`assert_cmd`-style, mirrors existing Export/Import CLI tests) | exit-code branches, id-naming, read-only open (R-10, AC-09) |
| MCP integration (regression) | `product/test/infra-001` suites | correction chain still works end-to-end through `context_correct`; no false-alarm (regression guard only) |
| Diff / grep | README + `hash.rs` + `main.rs` server tool surface | claim wording, frozen signature, no MCP tool, schema v30 (R-06, R-11, AC-06/07/10/11/12) |

## Risk-to-Test Mapping

| Risk | Priority | Primary component test plan | Signature test |
|------|----------|-----------------------------|----------------|
| R-01 two-site half-fix | Critical | correction-write-path | DB read-back after `correct_entry`; fails on struct-only fix |
| R-02 Deprecated predecessor invisible | Critical | chain-verify-core + import-validation | Deprecated predecessor counted as `checked` on BOTH callers; loader-returns-Deprecated guard |
| R-03 legacy tolerance regression | High | chain-verify-core | mixed corpus clean + `skipped_legacy` count; mutation fails loud naming id |
| R-04 validate_hashes refactor | High | import-validation | existing import tests unchanged; AND-halves via import |
| R-05 import atomicity | High | import-validation | tampered import ROLLBACK; post-failure row count == pre |
| R-06 frozen-hash scope-creep | Medium | (hash.rs existing) + readme-integrity | AC-10 known-value tripwire; signature grep |
| R-07 round-trip lossy | Medium | import-validation | multi-hop + legacy export→import byte-identical + re-verify clean |
| R-08 empty predecessor masked | Medium | correction-write-path | correction rejects empty `original.content_hash`, names `original_id` |
| R-09 dangling/missing mis-class | Medium | chain-verify-core | dedicated scenario per `ViolationKind` variant |
| R-10 CLI contract | Medium | verify-cli | exit 0 clean / non-zero tampered + id named; read-only open |
| R-11 README re-drift | Medium | readme-integrity | no unqualified "tamper-evident"; tamper-recorded present; durable |
| R-12 fail-silent | Low | chain-verify-core | is_clean false on any violation (property over variants) |

## Cross-Component Test Dependencies

- **Loader coverage (R-02) is a shared precondition** for AC-03/04/05/09. The pure core trusts its
  input slice; the bug hides in the caller. Test on BOTH callers (`query_all_entries` for CLI;
  the import in-flight-transaction load), each with a `Deprecated` predecessor asserted as `checked`.
- **Single-oracle AND (R-04):** import-validation tests must prove import runs the *same*
  `verify_entries` as the CLI — a corpus with good content-hash but broken link fails import, and
  a corpus with good link but mutated content fails import. No divergent second legacy path remains.
- **Write path ↔ read path (R-01):** correction-write-path assertions MUST cross the persistence
  boundary (fresh `SELECT`), never assert on the returned in-memory `EntryRecord`.
- Frozen-hash vectors (`hash.rs`, AC-10) are a tripwire for every component — any digest change
  cascades into chain-verify and content-hash-recompute correctness.

## Integration Harness Plan (infra-001)

The feature's observable effects are **DB-level** (`previous_hash`/`version` columns) and
**CLI-level** (`unimatrix verify`). Neither is exposed through the MCP JSON-RPC surface:
`context_correct`'s response does not surface `previous_hash`/`version`, and verify is CLI-only
with **no MCP tool** (FR-09, AC-11). Therefore the harness role here is **regression guard**, not
primary coverage.

**Suites to run (Stage 3c):**

| Suite | Why | Gate |
|-------|-----|------|
| `smoke` (`-m smoke`) | Mandatory minimum gate — server still boots, tools discover, one critical path each | MANDATORY |
| `lifecycle` | `test_correction_chain_integrity`, `test_multi_step_correction_chain`, `test_deprecate_then_correct_errors`, `test_data_persistence_across_restart` — confirm correction still works end-to-end and the new non-empty `previous_hash` write introduces no regression or persistence break | Required (touches store/retrieval + schema-adjacent write) |
| `tools` | `test_correct_creates_chain`, `test_correct_preserves_metadata`, `test_correct_all_formats`, `test_correct_with_edges_attaches_to_new_entry` — the `context_correct` tool contract is unchanged for callers | Required (server tool logic) |
| `protocol` | Handshake/JSON-RPC unaffected; baseline | Via smoke |

Suite selection rationale (from suite-selection table): feature touches server tool logic
(`context_correct`) → `tools`, `protocol`; store/retrieval + schema-adjacent write → `lifecycle`.
Confidence/contradiction/security/volume suites are **not** in this feature's blast radius — do not
run them as gates (run only if a smoke failure implicates them).

**Gaps — new integration tests needed:** NONE in infra-001. Rationale:
- AC-01/AC-02 (persisted `previous_hash`/`version`) are **not MCP-visible** → Rust DB read-back unit
  tests are the only valid surface (integration harness cannot see the column). Planning them in
  infra-001 would be a false-green (the value isn't in any MCP response).
- AC-09 (CLI verify contract) is a **separate CLI invocation**, not the server's MCP loop. It is
  covered by a Rust integration test in `unimatrix-server` mirroring the existing `Export`/`Import`
  CLI subcommand tests (direct-DB, no running server) — see verify-cli.md. Adding a Python subprocess
  test to infra-001 would duplicate that and require harness infra changes (out of scope; file a GH
  Issue instead per USAGE-PROTOCOL if a stable machine surface is later wanted — see ARCHITECTURE
  Open Q2 on `--json`).
- Import chain-verify (AC-04/05) runs on the CLI import path, also covered by Rust integration tests
  in `unimatrix-server` (import test module), not the MCP harness.

**If a smoke or lifecycle/tools test newly fails:** triage per USAGE-PROTOCOL.md — feature-caused
(fix + document), pre-existing/unrelated (GH Issue + `xfail`, do NOT fix in this PR), or bad
assertion (fix test + document). Never delete or comment out an integration test.

## Acceptance Criteria → Test Plan Index

| AC | Verification | Component plan |
|----|--------------|----------------|
| AC-01 | DB read-back: persisted `previous_hash == original.content_hash` | correction-write-path |
| AC-02 | DB read-back: `version == superseded.version + 1`; N-hop `1..N` | correction-write-path |
| AC-03 | `verify_entries` walks non-empty hops; legacy skipped | chain-verify-core |
| AC-04 | mutate content → non-clean, names id (recompute AND link) | chain-verify-core + import-validation |
| AC-05 | mixed corpus clean; export→import byte-identical + re-verify; mutation fails | import-validation |
| AC-06 | README no unqualified "tamper-evident"; tamper-recorded present | readme-integrity |
| AC-07 | threat model durable in README and/or ADR-002 | readme-integrity |
| AC-08 | correction rejects empty `original.content_hash`, names `original_id` | correction-write-path |
| AC-09 | CLI: exit 0 clean + summary; non-zero tampered + id; read-only open | verify-cli |
| AC-10 | `hash.rs` vectors byte-identical; signature unchanged | chain-verify-core (references hash.rs) + readme-integrity grep |
| AC-11 | no new MCP tool; core signature free of transport types | verify-cli (grep) |
| AC-12 | schema version still 30; no new migration step | verify-cli (grep/file-check) |

## Global Test Conventions (from existing code)

- Store tests: `let store = open_test_store(&dir).await;` (tempdir), insert via
  `TestEntry::new(topic, category).build()`, close with `store.close().await.unwrap()`.
  See existing `write_ext.rs` `#[cfg(test)]` module. Extend it; do not create isolated scaffolding.
- Async store tests use `#[tokio::test]`.
- DB read-back uses `sqlx::query("SELECT ... FROM entries WHERE id = ?1").bind(id as i64)`
  against `store.write_pool` / `read_pool()` and `entry_from_row` — the existing test pattern.
- Naming: `test_{concept}_{scenario}_{expected}`.
- CLI tests mirror the existing `Export`/`Import` subcommand integration tests (direct-DB, temp
  project dir). Reuse their harness, do not build a new one.
