# docstring drive-by — `{alpha}-{digits}` correction

**Source**: `crates/unimatrix-observe/src/attribution.rs` +
`packages/unimatrix/lib/hook-client/topic-signal.js`. **ADR**: ADR-004 §C9 /
FR-25. **Comment-only — NO behavior change** to the extractor.

## Purpose

Correct misleading docstrings that claim the feature-id filter requires an
`{alpha}-{digits}` shape. The actual filter (`is_valid_feature_id` /
`isValidFeatureId`) has NO digit requirement — it is: non-empty, byte-length ≤ 128,
contains a hyphen, no leading/trailing hyphen, only `[A-Za-z0-9\-_.]`. The
misleading docstrings were the OQ2 resolution: uni-zero provenance is ordinary
extraction; the filter is structural, not `{alpha}-{digits}`.

## attribution.rs corrections (comments only)

The CODE (`is_valid_feature_id` :15-23, the extractor chain) is UNCHANGED. Only
these doc comments are corrected:

- **:43** — `extract_feature_id_pattern` doc: currently
  "Accepts any feature ID matching the `alpha-digits` pattern (e.g., "col-002",
  "eng-001")." → reword to describe the ACTUAL structural filter (hyphen required,
  `[A-Za-z0-9\-_.]`, no digit requirement; e.g. accepts `col-002` AND `foo-bar`).
- **:76** — the `extract_topic_signal` doc step "2. Feature ID pattern:
  word-boundary `{alpha}-{digits}` tokens" → "word-boundary feature-id tokens
  (structural filter: hyphen required, `[A-Za-z0-9\-_.]`, no digit requirement)".

(Also scan :7-13 `is_valid_feature_id` doc — it already says "only safe characters
(ASCII alphanumeric, hyphen, underscore, dot)" which is correct; leave unless it
repeats the digit claim.)

## topic-signal.js correction (comment only)

- **:11** (and the priority-chain header :8-12) — the JS port docstring
  "2. extractFeatureIdPattern — word-boundary `{alpha}-{digits}` tokens" → match
  the corrected Rust wording: "word-boundary feature-id tokens (hyphen required,
  `[A-Za-z0-9\-_.]`, no digit requirement)". The CODE (`isValidFeatureId` :22-32)
  is UNCHANGED and already implements the correct structural filter.

Keep the JS and Rust docstrings WORD-ALIGNED (topic-signal.js is a documented
parity port of attribution.rs).

## Data Flow / Error Handling

None — comment-only. No runtime path touched. The extractor's permissive filter
behavior is explicitly OUT of scope (only docstrings corrected; tightening the
filter is a non-goal).

## Key Test Scenarios

- Existing `is_valid_feature_id` / `isValidFeatureId` tests pass UNCHANGED (no
  behavior change) — e.g. attribution.rs:227-238 positive/negative cases still
  hold (`col-002` accepts, `col` rejects, etc.).
- Diff review: the change set is comments only; no token of executable code differs.

## Open Questions / Gaps

None. Pure documentation correction with a fixed target wording.
