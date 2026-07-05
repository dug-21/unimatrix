# nxs-014 Acceptance Criteria Map

> AC-01..AC-07 are the SCOPE.md acceptance criteria (all carried). AC-08..AC-12 are spec
> risk-hardening ACs traced to SCOPE-RISK-ASSESSMENT (A-1, SR-01, SR-05, D-4, NFR-02).
> Verification detail derives from SPECIFICATION §Acceptance Criteria and RISK-TEST-STRATEGY.

| AC-ID | Description | Verification Method | Verification Detail | Status |
|-------|-------------|--------------------|--------------------|--------|
| AC-01 | After a correction, the new entry's persisted `previous_hash` equals the superseded entry's `content_hash`. | test (DB read-back) | Drive `context_correct`; fresh `SELECT previous_hash FROM entries WHERE id=<new_id>` (not the in-memory record); assert `== original.content_hash`. Fails on a struct-only half-fix. (R-01, SR-06, C-04) | PENDING |
| AC-02 | After a correction, persisted `version == superseded.version + 1`; across N-step chain versions are `1..N` monotonic. | test (DB read-back) | Read `version` back from DB after correction and after an N=3 chain; assert increment and monotonicity from persisted rows. | PENDING |
| AC-03 | Chain-verify walks a supersedes chain, asserting `successor.previous_hash == predecessor.content_hash` for every non-empty hop; empty `previous_hash` = unverifiable-legacy, not a break. | test | Build multi-hop corrected chain; run `verify_entries`; assert clean and each non-empty hop exercised; genesis/legacy skipped via `skipped_legacy`. (R-03, SR-02) | PENDING |
| AC-04 | Tamper fails loud, naming the entry: mutating a superseded entry's content (without perfectly rewriting both its `content_hash` and successor's `previous_hash`) makes verify fail and name the offending id. | test | Mutate content directly in DB; run verify; assert non-clean AND error names entry id. Satisfied by content-hash recompute AND chain-link check together. (R-01+R-03) | PENDING |
| AC-05 | Mixed legacy(empty)+new(chained) corpus verifies clean; export→import round-trips `previous_hash`/`version` unchanged and passes import chain-verify; paired negative mutation fails loud. | test | Construct mixed corpus; verify clean; export→import; assert values byte-identical + import verify clean; then mutate and assert fail. (R-03, R-07, SR-07) | PENDING |
| AC-06 | README integrity wording corrected: no unqualified "tamper-evident" claim vs a DB-write adversary; states tamper-**recorded** / correction-history integrity; ships same PR. | grep + manual | Inspect README diff in PR: assert no unqualified "tamper-evident"/"tamper evident" on the correction chain remains; corrected text present; shipped integrity not under-sold. (R-11, C-08) | PENDING |
| AC-07 | Threat model documented durably (README integrity section and/or ADR-002) so downstream agents cannot re-upgrade the claim. | file-check + manual | Confirm threat-model boundary (accidental-corruption + API-surface tamper detectable; DB-write adversary out of tier) present in README and ADR-002. (R-11) | PENDING |
| AC-08 | Correction with an empty `original.content_hash` is rejected with an error naming `original_id`, rather than persisting an empty/malformed `previous_hash`. | test | Attempt correction whose `original` has empty `content_hash`; assert correction fails before any row persists, error names `original_id`. (FR-04, A-1, R-08) | PENDING |
| AC-09 | CLI verify contract: clean corpus → exit 0 + summary; tampered corpus → non-zero exit + output naming the offending entry id. | shell + test | Run `unimatrix verify` against (a) clean corpus: assert exit 0 + checked summary; (b) tampered corpus: assert non-zero exit + id named (not just a count). Assert read-only open + `ensure_data_directory` resolution. (FR-08, R-10, SR-05) | PENDING |
| AC-10 | Hash format frozen (tripwire): `hash.rs` known-value vectors byte-identical and pass; `compute_content_hash` signature unchanged. | test + grep | Assert `e3b0c442…` genesis + `"Test: Content"` vectors unchanged and passing; grep signature `pub fn compute_content_hash(title: &str, content: &str) -> String` unchanged. Any digest-input diff fails. (NFR-01, SR-01, R-06) | PENDING |
| AC-11 | No MCP tool added for chain-verify; verify core signature free of transport/CLI/MCP types. | grep + manual | Assert no new MCP tool registered in server tool surface; `verify_entries(&[EntryRecord]) -> ChainReport` has no CLI/MCP types. (FR-09, D-4) | PENDING |
| AC-12 | No schema migration: schema version unchanged (still 30), no new migration step. | grep + file-check | Assert `migration.rs` schema version still 30; no new migration step under `migration.rs`. (NFR-02) | PENDING |

## Loader coverage (architect Open Q1 — R-02, Critical) — gating guard

Not a standalone AC but a blocking correctness guard that AC-03/04/05/09 depend on: both verify
callers (`query_all_entries` for CLI; the import in-flight-transaction load) MUST return **Deprecated**
entries — predecessors are Deprecated. A verify test on each path MUST include a Deprecated predecessor
and assert it is counted as `checked` (proving it was loaded, not absent-and-ignored). A direct guard
test on `query_all_entries()` must assert it returns `status = Deprecated` rows.

## Coverage confirmation

All seven SCOPE.md acceptance criteria (AC-01..AC-07) are present. AC-08..AC-12 add verification for
constraints/assumptions the risk assessment flagged. Every AC has a concrete verification method.
