# C7 — Report field `RetrospectiveReport.tags` (+ SUMMARY v6 cascade)

**Files:** `crates/unimatrix-observe/src/types.rs` (RetrospectiveReport, after `goal` :436);
`crates/unimatrix-store/src/cycle_review_index.rs` (`SUMMARY_SCHEMA_VERSION` :54, pinned test :709-716)
**ADR:** ADR-004. **Risks:** R-02, R-10. **AC:** AC-05a/b/c, AC-08. **This is version cascade #2 — keep SEPARATE from C1's v31.**

## Purpose

Add a required `tags: Vec<String>` field to `RetrospectiveReport` so tags ride `summary_json` into
`context_cycle_review` JSON automatically, and advance `SUMMARY_SCHEMA_VERSION` 5->6 (a round-trip
FIDELITY STAMP — NO DB migration). `#[serde(default)]` makes v5 blobs (no `tags` key) backward-readable
as an empty vec (this is what makes no-back-fill non-fatal, AC-08).

## Path A — the field (`unimatrix-observe/src/types.rs`, after `goal` :436)

```
# In struct RetrospectiveReport, beside `pub goal`:

    /// Opaque run-identity labels for this cycle, read from cycle_tags at review time (vnc-047).
    /// REQUIRED field (not Option) so the compiler flags every construction site.
    /// `#[serde(default)]` ONLY (NO skip_serializing_if) -> always serialized; a v5 blob with
    /// no `tags` key deserializes to an empty vec (backward-read, AC-08).
    #[serde(default)]
    pub tags: Vec<String>,
```

- **Required `Vec<String>`, NOT `Option`** — the compiler then forces every `RetrospectiveReport {...}`
  construction site to set `tags` (or `..Default::default()`), which is the enforcement mechanism for
  "all construction/round-trip paths updated" (AC-05b).
- **`#[serde(default)]` with NO `skip_serializing_if`** (contrast `goal`, which has
  `skip_serializing_if = "Option::is_none"`): tags ALWAYS serialize, even as `[]`, so JSON output
  deterministically includes a `tags` key (AC-05d). Deserializing a v5 blob without the key -> `[]`.

## Path B — SUMMARY version bump (`cycle_review_index.rs:54`)

```
PRECHECK (SR-02/R-10, record in coverage report): assert SUMMARY_SCHEMA_VERSION reads 5 at HEAD;
         if != 5 -> a parallel feature claimed 6; STOP and flag renumber.

CHANGE  pub const SUMMARY_SCHEMA_VERSION: u32 = 5   ->   6
```

## Path C — pinned test (`cycle_review_index.rs:709-716`)

```
UPDATE the pinned assertion CRS-V5-U-01:
    assert_eq!(SUMMARY_SCHEMA_VERSION, 6, "SUMMARY_SCHEMA_VERSION must be 6 (bumped in vnc-047: \
        adds RetrospectiveReport.tags run-identity labels)");
    # rename marker to CRS-V6-U-01, update the message to reference vnc-047 (parity crt-055).
```

## Path D — backward-read test (MANDATORY, R-02.4 / AC-08)

```
NEW test: take a v5-shaped summary_json blob (no `tags` key) -> deserialize into RetrospectiveReport
          -> assert `report.tags == []` and no error (proves #[serde(default)] backward-read).
```

## Construction sites (compiler-enforced)

The required field breaks compilation at every `RetrospectiveReport` literal until each sets `tags`.
Non-review construction sites (e.g. test builders, aggregators that don't have cycle context) should
set `tags: Vec::new()`. The ONE site that populates real tags is C8 (review handler).

## Error handling

None at the type level. Serde round-trip is total: populated -> JSON `["..."]`; absent-in-v5 -> `[]`.

## Key test scenarios (hints)

1. Constant advanced 5->6 (AC-05a).
2. Populated `tags` serialize -> `summary_json` -> deserialize with tags intact (AC-05b).
3. Pinned test updated and green (AC-05c).
4. v5 blob (no `tags` key) deserializes to `[]` via `#[serde(default)]`, no error (AC-08, R-02.4).
5. Cascade separation: this bump is proven WITHOUT touching C1's v31 assertions (SR-01).
