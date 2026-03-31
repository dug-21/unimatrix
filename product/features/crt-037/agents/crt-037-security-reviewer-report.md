# Security Review: crt-037-security-reviewer

## Risk Level: low

## Summary

crt-037 adds `RelationType::Informs` as a new graph edge type and implements automated
detection via NLI scoring in the background inference tick. The change is a pure additive
feature extension (no schema migration, no new external input surface) operating entirely
on internal data. All new configuration fields are operator-controlled TOML values validated
at startup. No injection, path traversal, deserialization, or access control concerns were
found. The one structural concern — Phase 8's cap-break iterating a mixed `merged_pairs`
vec — was examined in detail and is correctly structured. No blocking findings.

---

## Findings

### Finding 1: Phase 8 cap-break iterates mixed merged_pairs (non-blocking)

- **Severity**: low
- **Location**: `nli_detection_tick.rs:594-623`
- **Description**: Phase 8's for-loop iterates `merged_pairs`, which contains both
  `SupportsContradict` and `Informs` variants. The cap guard at line 595
  (`if edges_written >= max_graph_inference_per_tick { break; }`) fires on every element
  including `Informs` variants. However, because Phase 6 appends Supports pairs first
  and Informs pairs second (building `pair_origins` in order), `Informs` variants appear
  at the tail of `merged_pairs`. The break would only fire if Supports writing exhausted
  the budget — at which point the break occurs before reaching any `Informs` variants.
  Phase 8b then handles Informs via its own independent loop with no cap. The cap intent
  is preserved. This is not incorrect behaviour, but the loop structure is not obviously
  safe from a future maintainability standpoint: if Informs variants were ever interleaved
  with SupportsContradict variants in merged_pairs, a spurious break could skip Informs
  variants that should have been processed by Phase 8b.
- **Recommendation**: Consider adding a comment at the Phase 8 break site noting the
  ordering invariant: "Informs variants appear after SupportsContradict variants in
  merged_pairs (Phase 6 fetch order). This break only fires within the Supports block."
  This would make the ordering dependency explicit and prevent future ordering changes
  from silently breaking Phase 8b coverage.
- **Blocking**: no

### Finding 2: f32::EPSILON comparison for config merge (known pattern, non-blocking)

- **Severity**: low
- **Location**: `config.rs:merge_configs` (Informs fields block)
- **Description**: The config merge for `nli_informs_cosine_floor` and
  `nli_informs_ppr_weight` uses `(project.value - default.value).abs() > f32::EPSILON`
  as the "was this field set?" heuristic. This is the same pattern used for all other
  f32 fields in `merge_configs`. For a field like `nli_informs_cosine_floor` with a
  default of 0.45, `f32::EPSILON` is approximately `1.2e-7`, meaning the project config
  must differ from the default by more than ~0.0000001 to be treated as an override.
  In practice this is fine for configuration purposes. The issue is not new to crt-037
  and is consistent with the existing merge pattern.
- **Recommendation**: No action required unless the project plans to support
  sub-epsilon configuration differences, which is not a use case for these fields.
- **Blocking**: no

### Finding 3: debug_assert for weight finitude (non-blocking, elided in release)

- **Severity**: low
- **Location**: `nli_detection_tick.rs:642`
- **Description**: `debug_assert!(weight.is_finite(), ...)` guards the Informs edge
  weight before write. `debug_assert!` compiles to nothing in release builds. The
  comment references C-13/NF-08 and config validation ensures `nli_informs_ppr_weight`
  is in [0.0, 1.0]. The HNSW similarity values are cosines in (-1.0, 1.0). The product
  `cosine * ppr_weight` is always finite for finite inputs. The risk of NaN/Inf is
  effectively zero given the input constraints, so `debug_assert!` rather than a runtime
  check is appropriate here. However, this means the NF-08 protection is not enforced
  in production if an upstream bug produces a non-finite similarity value.
- **Recommendation**: Consider a `if !weight.is_finite() { continue; }` guard before
  `write_nli_edge` as a belt-and-suspenders production safeguard, consistent with how
  other edge weight paths handle potential non-finite values. Not blocking for this PR.
- **Blocking**: no

---

## OWASP Concerns Evaluated

| Concern | Finding |
|---------|---------|
| Injection (SQL) | The new SQL in `query_existing_informs_pairs` uses literal string constants only — `'Informs'` and `0` are hardcoded values, not user inputs. No parameterized user data. No injection surface. |
| Injection (command/path) | No shell commands or file path operations introduced. |
| Broken access control | No new access control paths. The inference tick runs as an internal background service; `Informs` edge writing follows the same EDGE_SOURCE_NLI path as existing Supports edges. No new trust boundary crossed. |
| Security misconfiguration | Config validation at startup (`validate()`) correctly enforces bounds on both new f32 fields. Empty `informs_category_pairs` disables the feature without error — a legitimate safe-default. |
| Deserialization risks | `format_nli_metadata_informs` uses `serde_json::json!` with typed f32 fields. No deserialization of untrusted input. The metadata JSON is written to the DB and read back for observability only. |
| Input validation gaps | Category pair strings flow from operator TOML through config to Phase 4b as runtime values. They are used only for `HashSet<&str>` membership checks — no SQL interpolation, no format string, no file path. Validated: length checks are not present on category strings, but long strings only affect in-memory map sizes, not security surface. |
| Vulnerable dependencies | No new dependencies added (Cargo.toml/Cargo.lock unchanged). |
| Hardcoded secrets | None present. The four default category pair strings ("lesson-learned", "decision", "pattern", "convention") are domain vocabulary, not credentials. |

---

## Blast Radius Assessment

Worst case if the fix has a subtle bug:

- **Spurious Informs edges written** — if one composite guard predicate were omitted,
  semantically unrelated entries could receive Informs edges. The effect is PPR mass
  inflation for entries that didn't merit it. Downstream symptom: irrelevant lessons
  surface when decisions are queried. Detectable via AC-13–AC-17 tests and post-hoc
  PPR score audit. Not a data loss or security event.

- **Supports regression** — if the NliCandidatePair type routing broke and
  SupportsContradict pairs were evaluated by Phase 8b logic, they would fail the
  `neutral > 0.5` guard (high-entailment pairs have low neutral). Net result: fewer
  Supports edges written. Existing tests (R-16/R-04 coverage) would detect this.

- **PPR score shift** — once any Informs edges exist in GRAPH_EDGES, PPR traversal
  includes them. A large-scale spurious Informs edge batch could distort ranking for
  all entries with Informs relationships. The Phase 5 cap (`remaining_capacity` bounded
  by `max_graph_inference_per_tick`) limits per-tick damage.

- **Maximum safe failure**: complete loss of Informs functionality. If the HNSW floor
  or neutral threshold was misconfigured, zero Informs edges would be written. The
  existing knowledge graph is unchanged. The feature simply doesn't activate.

**No data corruption or data loss path exists in this change.** All failure modes are
additive (extra edges) or null (no edges).

---

## Regression Risk

The primary regression surface is the `merged_pairs` refactor of Phase 8. The
existing `write_inferred_edges_with_cap` call was replaced by an inline loop.

**Verified mitigations:**

1. The Phase 8 inline loop preserves the same cap logic: `edges_written >= max_graph_inference_per_tick` → break. The semantics are equivalent.
2. Phase 8 only pattern-matches on `NliCandidatePair::SupportsContradict` — Informs variants are silently ignored in Phase 8 (they appear in Phase 8b).
3. The length-mismatch guard (line 544-552) is preserved: `raw_scores.len() != scored_input.len()` → return early.
4. All existing `graph_tests.rs` and `graph_ppr_tests.rs` variants still pass (their test content is unchanged; only new tests are appended).
5. The `status.rs` change is whitespace-only (trailing space removal in test fixture).

**Regression risk: low.** The Supports write path is structurally equivalent to before, and the tagged-union routing ensures compile-time enforcement of variant separation.

---

## Dependency Safety

No new crate dependencies were introduced. `Cargo.toml` and `Cargo.lock` are unchanged across all crates in this PR.

---

## PR Comments

- Posted 1 comment on PR #467 (findings summary + one structural note).
- Blocking findings: no.

---

## Knowledge Stewardship

- Stored: nothing novel to store -- the security anti-patterns examined here (domain vocab
  in config vs. detection, tagged union for routing safety, cap ordering invariant) are
  all well-documented in this PR's architecture docs and are feature-specific. No
  generalizable cross-feature anti-pattern was identified that warrants a lesson entry.
