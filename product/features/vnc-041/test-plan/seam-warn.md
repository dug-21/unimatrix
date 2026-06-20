# C5 — Locked-Key Seam WARN Test Plan (`resolve_slug_config` file-present arm)

> File: `http_provision.rs` (`resolve_slug_config`). ADR-005 (#5239). Risks: **R-04 (Critical)**,
> R-06, R-07, R-08. ACs: **AC-04** (one WARN per ignored key, once per boot, names key+slug, value
> ignored, WARN-only). Tests extend `http_provision/slug_config_tests.rs` (the `TempBase` harness;
> `#[tracing_test::traced_test]` + `logs_contain` for WARN capture, per
> `slug_config_classification_tests.rs::test_sha256_pins_global_wins_under_per_slug_pairing`).

## What this component is

A WARN pass added to `resolve_slug_config`'s **file-present arm only** (no-file arm UNCHANGED — byte-for-
byte fallthrough preserved):
1. parse the file text ONCE into a raw `toml::Value` table (alongside, not replacing,
   `load_single_config`) to enumerate PRESENT keys at `section.key` granularity;
2. for each present key, if `is_per_slug_overlayable(key) == false` ⇒ emit ONE
   `tracing::warn!(slug, key, "...managed globally; value ignored")`;
3. once per boot per (slug, key);
4. WARN-ONLY — return type / merge / validate / errors UNCHANGED. The raw parse must NEVER turn a
   parseable file into a new `ServerError`.

## Unit / integration tests

### R-04 / AC-04 — WARN fires for locked keys, silent for overlayable keys
- `test_resolve_warns_when_per_slug_sets_global_locked_key`
  Arrange: (b) sets a `GlobalLocked` key (e.g. `[embedding]`/`*_sha256`, or a transport field, or
  `permissive`). Act: `resolve_slug_config`. Assert (`logs_contain`): a WARN naming the key AND the slug.
- `test_resolve_no_warn_when_per_slug_sets_overlayable_key`
  Arrange: (b) sets only `PerSlugOverlayable` keys (e.g. `inference.nli_top_k`,
  `server.instructions`). Act: resolve. Assert: NO WARN emitted (only the overlay applies). (R-04
  scenario 2.)
- `test_resolve_warn_names_key_and_slug` (AC-04 content)
  Assert the WARN message/fields contain BOTH the locked key name and the slug — and **never the operator's
  set VALUE** (C-11 content-free logging, #4749). Bounded identifiers only.

### R-04 / R-06 / AC-04 — the WARN FLIP TEST (proven, not restated) — load-bearing centerpiece
- `test_resolve_warn_behavior_flips_when_disposition_flips`
  The WARN half of the AC-03/AC-04 flip proof. Approach: drive the WARN decision through
  `is_per_slug_overlayable(key)` so flipping a key's disposition flips whether the WARN fires for a (b)
  that sets that key. Since the production registry is static, the test mechanism is delivery's choice:
  - If the WARN pass calls `is_per_slug_overlayable(key)` directly (it must — SR-02/SR-07), a focused test
    can assert: for a key classified `PerSlugOverlayable`, setting it ⇒ no WARN; for a key classified
    `GlobalLocked`, setting it ⇒ WARN. Pairing those two against keys with KNOWN opposite dispositions in
    the live registry already demonstrates the binding (e.g. `inference.nli_top_k` overlayable ⇒ no warn;
    `inference.embedding_model_sha256` locked ⇒ warn).
  - The conceptual "one flip moves both behaviors" is shared with C2's renderer flip
    (per-slug-seed-renderer.md). ONE flip of an `OverlayDisposition` must move BOTH the rendered
    annotation (C2) AND the WARN (C5) — plan a shared flip harness if a test-injectable classification
    slice is exposed; otherwise prove each surface binds to `is_per_slug_overlayable` independently.
- `test_resolve_no_hand_enumerated_locked_list_in_warn_pass` (structural, 3c)
  Confirm (grep/read) the WARN pass has NO hardcoded locked-key list — it consults
  `is_per_slug_overlayable` only. (R-04 coverage requirement; SR-02/SR-07.)

### R-04 — unknown / typo'd key also warns (conservative default — explicitly asserted)
- `test_resolve_warns_for_unknown_key`
  Arrange: (b) sets a non-registry / typo'd key (e.g. `[inference]\nnli_topk = 5` or a bogus section).
  Act: resolve. Assert: a WARN fires (conservative `is_per_slug_overlayable == false` default, ADR-005) —
  AND document that this is intended (a typo'd key is also silently ineffective). (R-04 scenario 4, Edge
  Cases.) Confirm it does NOT error.

### R-07 — WARN-only: resolution output identical with/without WARN (no new error path)
- `test_resolve_output_identical_with_and_without_warn_path` (the FR-12 equivalence proof)
  Arrange: (b) sets a `GlobalLocked` key (e.g. a per-slug `*_sha256` diverging from global). Act:
  resolve. Assert: the merged `Cow<UnimatrixConfig>` value is byte/value-identical to the resolved value
  the no-WARN code path would produce — the locked value remains IGNORED (merged value == global, not the
  per-slug value). Only logs differ. (R-07 scenarios 1 + 2.)
- `test_resolve_warn_pass_does_not_add_error_on_uninspectable_file`
  Arrange: a per-slug file that `load_single_config` rejects (malformed TOML — reuse
  `test_resolve_invalid_class_fails_loud_naming_slug__malformed_toml`'s input). Act: resolve. Assert:
  the SOLE error is the existing loud, slug-named `ServerError::Config` from `load_single_config` — the
  WARN pass adds NO new error variant and does not convert a parseable file into an error (R-07 scenario
  3; the raw pass degrades to no-warn on a file it cannot inspect).
- `test_resolve_no_file_arm_unchanged_no_warn` (R-07 scenario 4 — vnc-040 AC-02 sentinel preserved)
  Arrange: slug dir with NO (b). Act: resolve. Assert: `Cow::Borrowed(&global)` (the existing
  `test_resolve_no_file_returns_cow_borrowed_global_no_merge` invariant still holds), NO WARN, byte-for-
  byte fallthrough. The WARN pass touches ONLY the file-present arm.

### R-08 / AC-04 — WARN granularity: once per boot per (slug, key)
- `test_resolve_repeated_calls_same_slug_key_warns_once`
  Act: call `resolve_slug_config` repeatedly for the same slug+locked-key within one boot. Assert:
  exactly ONE WARN for that (slug, key) (count occurrences of the message). NOTE per ADR-005/OQ-C: the
  resolver runs once per slug per boot (main.rs:1089), so confirm whether dedup is even reachable and
  where the once-per-boot state lives — if the resolver is genuinely called once per slug per boot, this
  test documents that single call emits one WARN; if a dedup map exists, it asserts the dedup. (R-08
  scenario 1, AC-04 "once per boot not per request".)
- `test_resolve_two_slugs_same_locked_key_warn_per_slug`
  Arrange: two different slugs, each setting the same locked key. Act: resolve each. Assert: a DISTINCT
  WARN per slug (named with each slug) — no cross-slug suppression. (R-08 scenario 2.)
- `test_resolve_warn_state_resets_across_boots` (if a dedup map exists)
  Assert any dedup state is NOT persisted — a fresh boot/fresh resolver re-emits the WARN. If dedup is
  scoped to a fresh `resolve` call per boot (no persisted map), document that the contract holds by
  construction. (R-08 scenario 3.)

### Integration risk — duplicate WARN with existing `*_sha256` merge warn (acceptable)
- `test_resolve_sha256_divergence_warns_present_and_acceptable`
  Arrange: (b) sets `*_sha256` diverging from a global pin that is `Some`. Act: resolve. Assert: the new
  C5 WARN AND the existing `merge_configs` "global hash pin takes precedence" WARN may BOTH log — both
  present, neither errors, resolution unchanged. (RISK-TEST-STRATEGY Integration Risks; mild duplicate
  signal is acceptable, not a defect.)

## Edge cases (RISK-TEST-STRATEGY Edge Cases)
- Empty (b) (zero keys) ⇒ no WARN, resolves to global (overlaps
  `test_resolve_empty_file_merges_to_global_equivalent`).
- (b) sets only overlayable keys ⇒ no WARN, overlay applies.
- (b) sets a key present in the template AND classified locked ⇒ WARN, value ignored.
- Malformed (hostile) TOML ⇒ existing error path only; WARN pass cannot crash the resolver (Security
  Risk: operator-authored file parsed twice — assert no panic via the WARN pass).

## Coverage requirement (RISK-TEST-STRATEGY R-04, R-07, R-08)

WARN binds to `is_per_slug_overlayable` at runtime (flip moves it; no hand-list); resolution output
identical with/without the WARN; no new error variant reachable from the WARN pass; the no-file arm is
unmodified; at most one WARN per (slug, key) per boot; per-slug and per-boot isolation; key+slug named,
never the value.
