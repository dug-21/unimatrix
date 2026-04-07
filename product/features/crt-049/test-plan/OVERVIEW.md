# Test Plan Overview: crt-049 — Knowledge Reuse Metric: Explicit Read Signal

## Test Strategy

crt-049 spans four crates (`unimatrix-observe`, `unimatrix-server`, `unimatrix-store`, and
`unimatrix-core` read-only) across six functional boundaries. The test approach is:

1. **Unit tests** — All pure-function logic is unit-tested in-module with synthetic
   `ObservationRecord` slices and constructed `FeatureKnowledgeReuse` values. No store
   fixture required for extraction or computation tests.
2. **In-module integration** — `compute_knowledge_reuse_for_sessions` has one existing
   test (`test_compute_knowledge_reuse_for_sessions_no_block_on_panic`) that requires a
   real `SqlxStore`. This test must be updated for the new signature. A new AC-05
   integration assertion is added alongside it.
3. **Golden output assertions** — `render_knowledge_reuse` is tested via full-string
   render output assertions (pattern #3426). Label text, ordering, and absence of legacy
   strings are all explicitly checked.
4. **Forced-value constant assertion** — `CRS-V24-U-01` in `cycle_review_index.rs` is an
   existing forced-value assertion test that must be updated from `2` to `3`.
5. **Integration harness (infra-001)** — `lifecycle` and `tools` suites exercise the
   `context_cycle_review` tool end-to-end through the MCP protocol. Smoke gate is
   mandatory. No new integration suite is required, but the `lifecycle` suite must be
   verified to pass after the schema version bump.

Test infrastructure is cumulative. No isolated test scaffolding is created. All new tests
are added to the existing `#[cfg(test)] mod tests` block in the relevant source file.

---

## Risk-to-Test Mapping

| Risk | Priority | Covering AC | Test Location | Test Form |
|------|----------|-------------|---------------|-----------|
| R-01: Triple-alias serde chain silent zero | Critical | AC-02 [GATE] | `types.rs` mod tests | 5 serde round-trip unit tests |
| R-02: normalize_tool_name omission | High | AC-06 [GATE], AC-12(d) | `knowledge_reuse.rs` mod tests | Unit test with `mcp__unimatrix__` prefix |
| R-03: total_served semantics change | High | AC-14 [GATE], AC-15 [GATE], AC-17 [GATE] | `knowledge_reuse.rs` + `retrospective.rs` mod tests | Unit tests for set union + render guard |
| R-04: explicit_read_by_category cap | Medium | — | `tools.rs` mod tests | Boundary unit test at 500/501 |
| R-05: Early-return guard retains old condition | Medium | AC-09, AC-17 [GATE] | `knowledge_reuse.rs` + `retrospective.rs` mod tests | Unit test with zero search exposures, non-zero reads |
| R-06: render_knowledge_reuse label regression | Medium | AC-07 | `retrospective.rs` mod tests | Golden-output assertion |
| R-07: attributed slice not threaded through | Medium | AC-05 | `tools.rs` mod tests | Store-backed integration test |
| R-08: SUMMARY_SCHEMA_VERSION not bumped | High | AC-08 | `cycle_review_index.rs` mod tests | Forced-value assertion (CRS-V24-U-01) |
| R-09: explicit_read_by_category contract break | High | AC-13 [GATE] | `knowledge_reuse.rs` mod tests | Unit test with known category distribution |
| R-10: Filter-based lookup included | Medium | AC-04, AC-12(b) | `knowledge_reuse.rs` mod tests | Unit test with no-id input |
| R-11: N+1 query pattern | Medium | structural | Code review + single-call assertion in AC-05 | One `batch_entry_meta_lookup` call |
| R-12: total_served deduplication not applied | Medium | AC-14 [GATE], AC-15 [GATE] | `knowledge_reuse.rs` mod tests | Set union deduplication unit test |
| R-13: Fixture updates incomplete | Medium | AC-10, AC-13 | `retrospective.rs` + `knowledge_reuse.rs` | Compilation + golden-output key check |

---

## Gate Items

All seven gate ACs are blocking. Delivery merge is rejected if any is failing:

| Gate AC | Description | Test | Component File |
|---------|-------------|------|----------------|
| AC-02 | Triple-alias serde chain round-trips | 5 unit tests in types.rs | feature-knowledge-reuse.md |
| AC-06 | normalize_tool_name called (prefixed name matched) | AC-12(d) unit test | extract-explicit-read-ids.md |
| AC-13 | explicit_read_by_category field contract | unit test with category map | compute-knowledge-reuse.md |
| AC-14 | total_served excludes search exposures | unit test, set union | compute-knowledge-reuse.md |
| AC-15 | total_served deduplication unit test | explicit overlap scenario | compute-knowledge-reuse.md |
| AC-16 | String-form ID extraction | unit test both forms | extract-explicit-read-ids.md |
| AC-17 | Injection-only cycle render guard | unit test + guard inspection | render-knowledge-reuse.md |

---

## Cross-Component Test Dependencies

- `feature-knowledge-reuse.md` tests must compile with the renamed field (`search_exposure_count`)
  before all other test files that reference `FeatureKnowledgeReuse` can compile.
- `extract-explicit-read-ids.md` unit tests have no dependency on the store — they are
  runnable immediately after the function is added to `knowledge_reuse.rs`.
- `compute-knowledge-reuse.md` tests depend on the extended `compute_knowledge_reuse`
  signature. All callers of this function in the test module must pass the two new params.
- `compute-knowledge-reuse-for-sessions.md` tests depend on both `extract_explicit_read_ids`
  and the extended `compute_knowledge_reuse` — validate these unit tests pass first.
- `render-knowledge-reuse.md` tests depend on `FeatureKnowledgeReuse` having the new
  fields (`explicit_read_count`, `explicit_read_by_category`). Run after types.rs compiles.
- `schema-version-bump.md` tests are independent — can be verified at any time.

---

## Integration Harness Plan (infra-001)

### Mandatory Smoke Gate

```bash
cd product/test/infra-001
python -m pytest suites/ -v -m smoke --timeout=60
```

Must pass before any other integration suite. This gate is non-negotiable for Stage 3c.

### Suite Selection

crt-049 touches store tool logic (schema version) and a lifecycle flow (`context_cycle_review`
step 13-14). Applicable suites:

| Suite | Reason | Priority |
|-------|--------|----------|
| `smoke` | Mandatory minimum gate | Non-negotiable |
| `lifecycle` | `context_cycle_review` is a multi-step lifecycle flow; schema version bump affects stale-record detection path | Required |
| `tools` | `context_cycle_review` is a tool; `context_get`/`context_lookup` tool observations are the extraction source | Required |

Suites NOT required: `security`, `confidence`, `contradiction`, `volume`, `edge_cases`,
`protocol` — crt-049 does not introduce new security boundaries, confidence scoring logic,
contradiction detection, volume-scale changes, or protocol changes.

### Existing Suite Coverage

The `lifecycle` suite already exercises `context_cycle_review` end-to-end. After the schema
version bump from 2 to 3, the stale-record advisory path will trigger for any stored review
with `schema_version = 2`. Existing lifecycle tests that verify re-review behavior must
still pass.

The `tools` suite exercises `context_get` and `context_lookup` as tool calls; these tests
confirm the observation recording path that feeds `extract_explicit_read_ids`. No changes
to these tools in crt-049, so existing coverage is sufficient.

### New Integration Tests Needed

**One new integration test** is required in `suites/test_lifecycle.py` or as a standalone
test in the tools.rs test module (Store-backed, preferred per architecture):

```
test_cycle_review_explicit_read_count_populated
```

Scenario:
1. Store an entry.
2. Record a `context_get` observation with a valid entry ID as a PreToolUse event in the
   attributed observations for a cycle.
3. Call `compute_knowledge_reuse_for_sessions` with the attributed slice.
4. Assert `explicit_read_count > 0` in the returned `FeatureKnowledgeReuse`.

This covers AC-05 and R-07 (attributed slice threading). It is the only behavior visible
only through the end-to-end pipeline that cannot be validated by `extract_explicit_read_ids`
unit tests alone.

**No new infra-001 integration tests needed** — the AC-05 scenario is adequately covered
by the in-module Store-backed test in `tools.rs`. Full MCP-level `context_cycle_review`
integration is out of scope for this feature's test additions (existing lifecycle suite
covers the frame; the unit-level test confirms the specific extraction signal).

### Running

```bash
cd product/test/infra-001

# Smoke (mandatory)
python -m pytest suites/ -v -m smoke --timeout=60

# Lifecycle (required for crt-049)
python -m pytest suites/test_lifecycle.py -v --timeout=60

# Tools (required for crt-049)
python -m pytest suites/test_tools.py -v --timeout=60
```

### Failure Triage

Any lifecycle suite failures after the schema version bump must be triaged:
- If caused by stale-record advisory path change → fix the code (new behavior expected)
- If pre-existing → do NOT fix; file GH Issue, mark `xfail`
- If test assertion uses hardcoded version `2` → fix the test assertion
