# C7 — RetrospectiveReport.tags field + SUMMARY v6 cascade

> Files: `crates/unimatrix-observe/src/types.rs` (`RetrospectiveReport`, add `#[serde(default)] pub
> tags: Vec<String>` after `goal`, ~:436); `crates/unimatrix-store/src/cycle_review_index.rs`
> (`SUMMARY_SCHEMA_VERSION` :54, pinned test :712).
> Risks: **R-02 (Critical)**, R-14 (Low). ACs: AC-05a, AC-05b, AC-05c, AC-08.
> **Cascade #2 of two (fidelity STAMP — NO DB migration). Do NOT lump with schema v31
> (cycle_tags-migration.md).** This is the codebase's recurring gate miss (#4153/#4373).

## Reuse
Serde round-trip pattern: `types.rs` `test_retrospective_report_roundtrip_with_new_fields` (:1014),
missing-field-default guard (:1181), and the negative "must gain NO field of these types" guards
(:1965-2005). Pinned-constant pattern: `cycle_review_index.rs` `test_summary_schema_version_is_5`
(:712). Store `summary_json` round-trip: `CycleReviewRecord` store-and-fetch asserting
`fetched.schema_version` (:841).

## AC-05a — constant bump
- `test_summary_schema_version_is_6` — replace/update the pinned `test_summary_schema_version_is_5`;
  `assert_eq!(SUMMARY_SCHEMA_VERSION, 6u32, "…vnc-047 added RetrospectiveReport.tags…")` with a
  rationale message referencing vnc-047 (AC-05c is this same pinned test).

## AC-05b — serde round-trip (all three schema paths)
- `test_retrospective_report_tags_roundtrip` — build a `RetrospectiveReport` with populated
  `tags=["arm:A","workflow:v1.3"]`; `to_string` → `from_str` → assert `tags` intact; assert the JSON
  string contains `"tags"` (present) — parity with the existing new-field round-trip.
- `test_report_tags_survives_summary_json_store_fetch` — store a `CycleReviewRecord` whose
  `summary_json` carries populated `tags`, fetch, assert `tags` present and `schema_version == 6`
  (the actual DB column path).

## AC-05b backward-read (MANDATORY — R-02 scenario 4)
- `test_v5_blob_deserializes_tags_default_empty` — deserialize a v5-era `summary_json` blob that has
  NO `tags` key → `RetrospectiveReport.tags == vec![]`, no error. This `#[serde(default)]`
  backward-read is what makes no-back-fill (AC-08/NFR-8) non-fatal. Assert it explicitly, not by
  inference.

## AC-05c — pinned test green (see AC-05a; same test, message references vnc-047).

## AC-08 / R-14 — no back-fill (doc + optional confirm)
- `test_v5_cached_review_shows_no_tags` (OPTIONAL confirming) — a review cached at v5 surfaces no
  `tags` without a recompute. Documented expectation, not a defect; historical `## Tags` renders
  empty by design.

## Negative guard
- `test_tags_field_is_not_transient` — assert `tags` DOES serialize into JSON (it is a real surfaced
  field), reusing the `assert!(json.contains("tags"))` guard pattern — contrast the crate's
  "must-NOT-serialize" transient-field guards.

## Compiler-enforced fan-out
`tags` is a required field (not `Option`) → every `RetrospectiveReport { … }` construction site must
be mechanically filled (compiler-enforced). Note in coverage that the workspace compiles clean after
the field add (proves all construction sites updated).
