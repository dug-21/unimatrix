# Test Plan — README canonical restore procedure

Change: new README section documenting the canonical restore sequence (FR-16, OQ-3, SR-07).
Verification type: `file-check` (content asserted present). Risk: R-12 (restore-sequence
discoverability — omission → operator skips `stop`, hits R-03).

## AC-12 (README half) / FR-16

- `test_readme_documents_canonical_restore_sequence` — assert the README contains the canonical,
  ordered sequence `project register <slug> → stop → import --slug <slug> → start` as the supported,
  load-bearing procedure. A `file-check` (substring/ordered-content assertion) in the test suite or a
  doc-lint check.
- The section must state **why `stop` is load-bearing** (live-PID hard-error / vector-clobber
  avoidance) so the operator understands the ordering is not optional.
- The daemon's `project register` output alone is not the canonical home — the README is (SR-07).

## Cross-reference

- Import's `--slug` help text carries the one-line pointer to this README section (asserted in
  main-dispatch.md, `test_cli_import_slug_help_carries_readme_pointer`) — keep the pointer target and
  the section heading consistent so the pointer does not dangle.

## Note

This is documentation, not runtime behavior — the served-outcome half of AC-12 (vector search from
`start`) is proven in import.md (`test_restore_sequence_serves_vector_search_from_start`). This plan
covers only the "documented as canonical" half.
</content>
