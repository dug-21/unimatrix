# Test Plan — chain-verify-core (`unimatrix-store/src/chain_verify.rs`, new)

> Pure, I/O-free integrity oracle: `verify_entries(&[EntryRecord]) -> ChainReport`.
> Tests are unit tests in the module's `#[cfg(test)]` block over hand-built `Vec<EntryRecord>`
> — no DB needed (the core takes a slice). Covers R-03, R-09, R-12, and the R-02 core half.

## Test fixtures (in-test builders)

Build `EntryRecord` values directly (struct is crate-visible). Helper suggested:
`fn chained(id, supersedes: Option<u64>, prev_hash: &str, version, title, content, status) -> EntryRecord`
that sets `content_hash = compute_content_hash(title, content)` so a "clean" fixture is genuinely
consistent. A predecessor is built with `status = Deprecated`, successor with `status = Active`.

## Unit test expectations

### Clean walk + legacy skip (R-03, AC-03)
- `test_verify_clean_two_hop_chain_is_clean` — genesis (prev="", v1) + successor
  (prev=genesis.content_hash, v2, `supersedes = Some(genesis.id)`). Assert `is_clean()`,
  `report.violations` empty, `report.checked >= 2`, `report.skipped_legacy == 1` (genesis).
- `test_verify_mixed_legacy_and_chained_is_clean` — corpus with N legacy entries (prev="") AND a
  chained pair. Assert `is_clean()` AND `report.skipped_legacy == <count of empty-prev entries>`
  (every empty-prev entry counted as skipped, NOT as checked-and-passed — proves the skip is real,
  guards #5180 green-on-skip).
- `test_verify_genesis_supersedes_none_empty_prev_skipped_not_dangling` — entry `supersedes==None`,
  `previous_hash==""`. Assert legacy-skip (`skipped_legacy += 1`), NOT `DanglingPreviousHash`.

### Deprecated predecessor present in the checked set (R-02 core half, Critical)
- `test_verify_deprecated_predecessor_counted_as_checked` — predecessor `status = Deprecated`
  (`superseded_by = Some(succ.id)`), successor `Active` with populated `previous_hash`. Pass the
  whole slice. Assert `is_clean()` AND the successor hop was exercised (predecessor present in the
  built map → link check ran). Assert `report.checked` counts the Deprecated predecessor (e.g.
  `checked` includes both). This proves the core does NOT filter by status — the filtering risk lives
  in the caller/loader (see import-validation.md and verify-cli.md loader guards).

### Each ViolationKind produced by a dedicated scenario (R-09, AC-04)
- `test_verify_content_hash_mismatch_named` — mutate a fixture's `content` field WITHOUT recomputing
  `content_hash` (stored hash now stale). Assert one violation, `kind == ContentHashMismatch { computed, stored }`,
  `violation.entry_id == <that id>`, and computed != stored surfaced.
- `test_verify_chain_link_mismatch_named` — successor `previous_hash` set to a wrong (non-empty)
  value while `supersedes` points at a real predecessor. Assert `ChainLinkMismatch { predecessor_id,
  expected: predecessor.content_hash, found }` on the successor's id.
- `test_verify_missing_predecessor` — successor `supersedes = Some(999)` (999 not in slice),
  non-empty `previous_hash`. Assert `MissingPredecessor { predecessor_id: 999 }` on the successor.
- `test_verify_dangling_previous_hash` — non-empty `previous_hash` but `supersedes == None`.
  Assert `DanglingPreviousHash` (fail loud), NOT a legacy-skip. Boundary against the genesis case.

### Fail-loud posture (R-12, NFR-06)
- `test_is_clean_false_whenever_violations_nonempty` — for each `ViolationKind` variant, a report
  carrying exactly one violation returns `is_clean() == false`. Property-style over the variants.
- `test_report_names_every_offending_id` — multi-violation corpus (2+ distinct broken entries);
  assert every offending `entry_id` appears in `describe()`/`Display` output (not a bare count).

### Edge cases (RISK-TEST-STRATEGY §Edge Cases)
- `test_verify_empty_corpus_is_clean` — `verify_entries(&[])` → `is_clean()`, `checked == 0`,
  `skipped_legacy == 0`, no panic.
- `test_verify_single_legacy_entry_is_clean` — one genesis entry, no panic, counted as skipped.
- `test_verify_long_chain_versions_monotonic_single_pass` — N-hop (e.g. N=10) all-consistent chain
  verifies clean; assert O(n) single pass (no quadratic re-walk — construct, verify, assert clean;
  performance is structural, asserted by construction not timing).
- `test_verify_mid_chain_deprecated_predecessor_with_own_predecessor` — an entry that is BOTH a
  predecessor (Deprecated, `superseded_by` set) AND a successor (its own `previous_hash` populated,
  `supersedes` set). Both hops checked, corpus clean.
- `test_verify_legacy_predecessor_new_successor` — successor's `previous_hash` populated pointing at
  a legacy predecessor whose own `previous_hash` is empty. Successor→predecessor hop is checkable and
  passes; the predecessor's own genesis link is legacy-skipped. Both hold in one corpus.

## Frozen-hash dependency (AC-10, R-06)
This core recomputes via `compute_content_hash(title, content)`. Its correctness is coupled to the
FROZEN vectors in `hash.rs` (`e3b0c442…` both-empty, `"Test: Content"`). Those existing tests
(`test_content_hash_known_value`, `test_content_hash_both_empty`) are the tripwire — run unchanged;
any digest change breaks both them and every clean fixture here. Do NOT add `previous_hash` to the
digest (strong mode / Non-Goal 1 / C-01).

## Assertions summary (concrete)
- `verify_entries(&clean_slice).is_clean() == true`
- `verify_entries(&mutated_slice).violations[0].kind == ContentHashMismatch { .. }`
- `verify_entries(&mutated_slice).violations[0].entry_id == mutated_id`
- `report.skipped_legacy == count_of_empty_previous_hash`
- `report.checked` includes Deprecated predecessors
- Signature stays `pub fn verify_entries(entries: &[EntryRecord]) -> ChainReport` — no CLI/MCP types (C-07, AC-11).
