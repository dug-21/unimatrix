# Test Plan — `[transcript_signals]` config + `validate()`

**Component**: `[transcript_signals]` config table (sibling to `[retention]`), `Vec<{ class_name: String, pattern: String, enabled: bool }>`, `#[serde(default)]`. `validate()` enforces `MAX_SIGNAL_CLASSES`, rejects invalid regex + duplicate `class_name` loudly at load. `const MAX_SIGNAL_CLASSES: usize = 16` (pinned, MUST equal crt-055's). v1 default: `error` (0), `refusal` (1).
**Pseudocode**: `pseudocode/transcript-signals-config.md` · **Layer**: unit.
**Anchor ACs**: **AC-10** (default set), AC-10a (calibration — manual), **AC-11** (cap + invalid-regex loud), AC-15 (no residue). **Risks**: R-10, R-12, R-05.

## Default catalog (AC-10, R-05) — unit

`crates/unimatrix-server/src/infra/config.rs` tests (extend).

1. `test_default_config_error_refusal_only` — `#[serde(default)]` parsing of an empty/absent `[transcript_signals]` yields EXACTLY two v1 classes: `error` at index 0, `refusal` at index 1. No third class. (FR-C2/C4, AC-10.)
2. `test_default_catalog_no_sdlc_literals` — the default patterns are domain-neutral behavioral signatures (model refusal phrasings, provider hard/overload errors) — assert NO SDLC literal patterns. (FR-C2, AC-10.)
3. `test_default_catalog_no_reread_or_compaction_class` — assert NO `reread` class and NO `compaction` class (removed-scope residue). (R-12, AC-10/AC-15.)
4. `test_class_index_mapping_stable` — class-to-index follows config order and is stable: `class_counts[0]=error`, `class_counts[1]=refusal` for the v1 default — the mapping crt-055 reads (`class_counts[0]→error_count`, `[1]→refusal_count`). (FR-C4, R-05.)

## Bound + loud rejection (AC-11, R-10) — unit

5. `test_max_signal_classes_is_16` — `MAX_SIGNAL_CLASSES == 16` exactly (compile-time/const assertion). MUST equal crt-055's constant (it crosses the boundary via `class_counts: [u32; MAX_SIGNAL_CLASSES]`). (NFR-6, AC-11.)
6. `test_config_over_cap_rejected` — a config with > `MAX_SIGNAL_CLASSES` enabled classes fails `validate()` with a clear error at load — NO silent truncation to 16. (FR-C3, R-10, AC-11.)
7. `test_config_invalid_regex_rejected` — an unparseable `pattern` fails `validate()` loudly at load — NO runtime fallback, NO silent drop. (FR-C3, R-10, AC-11; dsn-001/#4591 precedent.)
8. `test_config_duplicate_class_name_rejected` — duplicate `class_name` fails `validate()` loudly. (FR-C3.)

### Negative-mutation (AC-11)
- A `validate()` that silently truncated an over-cap set to 16 (instead of erroring) must fail `test_config_over_cap_rejected`. Silent fallback = a quiet believable-zero on `class_counts` — the exact failure this AC guards.

## Calibration (AC-10a) — manual / delivery-time

9. `calibration_check` (manual, recorded in the delivery artifact) — a sample of real transcript deltas exercises each default `error`/`refusal` pattern; precision/false-positive observations recorded; the pattern set finalized BEFORE merge; doc review asserts counts are surfaced as **directional, not precise** (content-opacity means false-positive rate can never be audited post-ship). (Coordination Item 3, FR-C2a, AC-10a.) Not an automated test — a delivery gate item.

## No-residue (AC-15, R-12) — grep, shared with activity-snapshot.md

10. `test_diff_no_cycle_review_index_no_summary_schema_version` — grep: crt-054's diff touches NEITHER `cycle_review_index` NOR `SUMMARY_SCHEMA_VERSION`. (AC-15, R-12.)
11. `test_no_token_symbol` — grep: crt-054 introduces no `token_*` symbol (`token_bytes_per_unit` included). Bytes-only. (AC-15, NFR-2, R-12.)

## Notes
- The scanner's SCAN behavior (one pass, multi-class) is `transcript-activity.md`; this file owns the catalog/validate boundary.
- `MAX_SIGNAL_CLASSES == 16` is also a producer-contract conformance point (R-05) — single source = crt-055.
