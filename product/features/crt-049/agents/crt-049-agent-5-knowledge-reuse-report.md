# Agent Report: crt-049-agent-5-knowledge-reuse

**Agent ID**: crt-049-agent-5-knowledge-reuse
**Component**: Components 2 and 3 — `knowledge_reuse.rs`
**File Modified**: `crates/unimatrix-server/src/mcp/knowledge_reuse.rs`

---

## Work Completed

### Component 2: `extract_explicit_read_ids`

Added `pub(crate) fn extract_explicit_read_ids(attributed: &[ObservationRecord]) -> HashSet<u64>`.

Implementation follows the five-condition predicate from the pseudocode:
1. `event_type == "PreToolUse"` — string comparison
2. `normalize_tool_name(tool.as_deref().unwrap_or(""))` in `{"context_get", "context_lookup"}`
3. Two-branch input parse: `Some(Value::Object(_))` used as-is; `Some(Value::String(s))` parsed via `serde_json::from_str(s).ok()`
4. Object has `id` field (any parseable form)
5. `obj["id"].as_u64().or_else(|| obj["id"].as_str().and_then(|s| s.parse().ok()))` returns `Some(n)`

### Component 3: `compute_knowledge_reuse` extension

Extended signature with `explicit_read_ids: &HashSet<u64>` and `explicit_read_meta: &HashMap<u64, EntryMeta>`.

New computations added (Steps 8-10):
- `explicit_read_count = explicit_read_ids.len() as u64`
- `explicit_read_by_category`: tally from `explicit_read_meta`
- `total_served = explicit_read_ids.union(&all_injection_ids).count() as u64`

Early-return guards updated:
- Step 3 guard: now includes `|| !explicit_read_ids.is_empty()`
- Step 5 guard: changed to `all_entry_ids.is_empty() && explicit_read_ids.is_empty()` — prevents suppression of explicit-read-only cycles (AC-09)

Final return struct updated: `explicit_read_count` and `explicit_read_by_category` now use computed values instead of stubs.

### Existing test updates

All 34 existing `compute_knowledge_reuse` call sites in the test module updated to pass `&HashSet::new(), &HashMap::new()` as the two new trailing args.

Two tests with now-superseded assertions updated:
- `test_total_served_distinct_ids`: updated to assert `total_served == 0` (no injections, no explicit reads) and `search_exposure_count == 2`
- `test_total_served_equals_delivery_count` → renamed `test_total_served_injection_only_no_explicit_reads`: updated to assert `total_served == 1` (one injected entry) vs `search_exposure_count == 4`

---

## Test Results

### New tests added: 25

**`extract_explicit_read_ids` tests (Component 2):**
- `test_extract_explicit_read_ids_context_get_included` (AC-12a)
- `test_extract_explicit_read_ids_filter_lookup_excluded` (AC-12b)
- `test_extract_explicit_read_ids_null_id_excluded` (AC-12b variant)
- `test_extract_explicit_read_ids_single_id_lookup_included` (AC-12c)
- `test_extract_explicit_read_ids_prefixed_context_get_matched` (AC-12d GATE)
- `test_extract_explicit_read_ids_prefixed_context_lookup_matched` (AC-12d variant)
- `test_extract_explicit_read_ids_empty_slice_returns_empty` (AC-12e)
- `test_extract_explicit_read_ids_string_form_id_handled` (AC-16 GATE)
- `test_extract_explicit_read_ids_non_pretooluse_excluded` (AC-04)
- `test_extract_explicit_read_ids_search_tool_excluded` (AC-03)
- `test_extract_explicit_read_ids_deduplication` (E-02)
- `test_extract_explicit_read_ids_none_tool_skipped` (I-04)
- `test_extract_explicit_read_ids_hook_path_string_input`

**`compute_knowledge_reuse` tests (Component 3):**
- `test_compute_knowledge_reuse_explicit_read_by_category_populated` (AC-13 GATE)
- `test_compute_knowledge_reuse_explicit_read_by_category_empty_when_no_reads` (AC-13 variant)
- `test_compute_knowledge_reuse_total_served_union_of_reads_and_injections` (AC-14 GATE)
- `test_compute_knowledge_reuse_total_served_excludes_search_exposures` (AC-15 GATE)
- `test_compute_knowledge_reuse_total_served_deduplication_overlap` (AC-15)
- `test_compute_knowledge_reuse_total_served_disjoint_sets`
- `test_compute_knowledge_reuse_no_early_return_for_explicit_read_only_cycle` (AC-09)
- `test_compute_knowledge_reuse_injection_only_cycle_has_nonzero_total_served` (AC-17 partial)
- `test_total_served_injection_only_no_explicit_reads` (updated from old test)

**Final test run**: `cargo test -p unimatrix-server --lib mcp::knowledge_reuse` — **65 passed, 0 failed**

**Full workspace**: `cargo test --workspace` — **all suites passed, 0 failures**

---

## Issues Encountered

One non-obvious bug: the second early-return guard at Step 5 (`if all_entry_ids.is_empty()`) fires when query_log and injection_log are both empty — even when `explicit_read_ids` is non-empty. This caused `test_compute_knowledge_reuse_explicit_read_by_category_populated` and `test_compute_knowledge_reuse_no_early_return_for_explicit_read_only_cycle` to fail on first run. Fix: changed condition to `all_entry_ids.is_empty() && explicit_read_ids.is_empty()`. Not mentioned in pseudocode but required for correctness.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — found ADR #4218 (extract_explicit_read_ids helper), ADR #4216 (total_served redefinition), pattern #921 (col-020b compute/IO separation). All directly applicable.
- Stored: entry #4221 "Two-branch Value parse required for ObservationRecord.input from hook listener" via `/uni-store-pattern` — the two-branch parse requirement is invisible in source code, compiles without the fix, and produces silent zero-count results in production.
