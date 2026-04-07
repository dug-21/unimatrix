# Component 5: render_knowledge_reuse — `unimatrix-server/src/mcp/response/retrospective.rs`

## Purpose

Update the render function for the Knowledge Reuse section in the cycle review report.
Three changes:
1. Fix the early-return guard to `total_served == 0 && search_exposure_count == 0`
2. Replace the single "Distinct entries served" summary line with three labeled lines
3. Add an "Explicit read categories" breakdown from `explicit_read_by_category`

The function signature is unchanged. All changes are internal to the function body.

---

## Current Function State (before crt-049)

```
fn render_knowledge_reuse(reuse: &FeatureKnowledgeReuse, feature_cycle: &str) -> String:
    guard: if reuse.delivery_count == 0 { return "No knowledge entries served." }
    summary: "**Distinct entries served**: {delivery_count} | **Stored this cycle**: {total_stored}"
    bucket table: cross_feature_reuse | intra_cycle_reuse
    by_category line: "**By category (all {delivery_count} served)**: {cats}"
    top_cross_feature_entries table
    return out
```

---

## Updated Function Body

### Step 1: Update Early-Return Guard (AC-17 GATE)

```
// CURRENT (WRONG):
if reuse.delivery_count == 0 {
    out.push_str("No knowledge entries served.\n\n");
    return out;
}

// NEW (CORRECT — AC-17 GATE):
// Guard fires ONLY when nothing was served AND nothing was exposed via search.
// An injection-only cycle has total_served > 0 even with zero search exposures
// and zero explicit reads — it must NOT be suppressed.
if reuse.total_served == 0 && reuse.search_exposure_count == 0 {
    out.push_str("No knowledge entries served.\n\n");
    return out;
}
```

The old form `delivery_count == 0` (now `search_exposure_count == 0`) is the wrong
condition. A cycle with only explicit reads has `search_exposure_count == 0` but
`total_served > 0` — the old guard would suppress it. A cycle with only injections
also has `search_exposure_count == 0` but `total_served > 0` — same issue.

The three-condition form (`search_exposure_count == 0 && explicit_read_count == 0 &&
injection_count == 0`) must NOT be implemented. It is listed as superseded in
IMPLEMENTATION-BRIEF.md VARIANCE 1.

### Step 2: Replace Summary Line (FR-09, AC-14 GATE label)

```
// CURRENT:
writeln!(out, "**Distinct entries served**: {}  |  **Stored this cycle**: {}",
    reuse.delivery_count, reuse.total_stored)

// NEW — three separate labeled lines:
writeln!(out,
    "**Entries served to agents (reads + injections)**: {}  |  **Stored this cycle**: {}",
    reuse.total_served, reuse.total_stored)
out.push('\n')
writeln!(out, "- Search exposures (distinct): {}", reuse.search_exposure_count)
writeln!(out, "- Explicit reads (distinct): {}", reuse.explicit_read_count)
out.push('\n')
```

Label requirements (from AC-07 golden render assertion):
- Summary label: "Entries served to agents (reads + injections)" backed by `total_served`
- Search exposures line: "Search exposures (distinct)" backed by `search_exposure_count`
- Explicit reads line: "Explicit reads (distinct)" backed by `explicit_read_count`
- Ordering: summary first, then the two sub-lines
- The legacy string "Distinct entries served" must NOT appear in rendered output (R-13)

### Step 3: Update `by_category` Display Label

The existing `by_category` line currently reads:
```
"**By category (all {delivery_count} served)**: {cats}"
```

Update to reflect that `by_category` is sourced from search exposures, not total served:
```
"**Search exposure categories (all {search_exposure_count} exposed)**: {cats}"
```

This label change is required for semantic accuracy: `by_category` reflects search
exposure tallies (from query_log), not explicit reads or injections.

If `reuse.search_exposure_count == 0`, the `by_category` map will be empty
(since it is populated from query_log), so the existing `!reuse.by_category.is_empty()`
guard already handles this case without requiring a separate check.

### Step 4: Add Explicit Read Categories Breakdown (FR-11, AC-13 GATE)

Insert after the `by_category` section, before the `top_cross_feature_entries` table:

```
// Explicit read categories breakdown (crt-049)
if !reuse.explicit_read_by_category.is_empty():
    // Sort by count descending, then by category name ascending for determinism
    let mut explicit_cats: Vec<(&String, &u64)> =
        reuse.explicit_read_by_category.iter().collect()
    explicit_cats.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)))

    let cat_parts: Vec<String> = explicit_cats.iter()
        .map(|(cat, count)| format!("{}x{}", cat, count))
        .collect()

    writeln!(out,
        "**Explicit read categories**: {}",
        cat_parts.join(", "))
    out.push('\n')
```

---

## Complete Updated Function Skeleton

```
fn render_knowledge_reuse(reuse: &FeatureKnowledgeReuse, feature_cycle: &str) -> String:
    let mut out = String::new()
    out.push_str("## Knowledge Reuse\n\n")

    // [GATE: AC-17] Guard: fires only when nothing served AND no search exposures
    if reuse.total_served == 0 && reuse.search_exposure_count == 0:
        out.push_str("No knowledge entries served.\n\n")
        return out

    // Summary: entries served (reads + injections)
    writeln!(out,
        "**Entries served to agents (reads + injections)**: {}  |  **Stored this cycle**: {}",
        reuse.total_served, reuse.total_stored)
    out.push('\n')

    // Sub-metrics
    writeln!(out, "- Search exposures (distinct): {}", reuse.search_exposure_count)
    writeln!(out, "- Explicit reads (distinct): {}", reuse.explicit_read_count)
    out.push('\n')

    // Bucket table (unchanged)
    out.push_str("| Bucket | Count |\n")
    out.push_str("|--------|-------|\n")
    writeln!(out, "| Cross-feature (prior cycles) | {} |", reuse.cross_feature_reuse)
    writeln!(out, "| Intra-cycle ({}) | {} |",
        escape_md_cell(feature_cycle), reuse.intra_cycle_reuse)
    out.push('\n')

    // Search exposure categories (formerly "by_category (all N served)")
    if !reuse.by_category.is_empty():
        let mut cats = reuse.by_category.iter().collect::<Vec<_>>()
        cats.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)))
        let cat_parts = cats.iter().map(|(cat, count)| format!("{}x{}", cat, count)).collect::<Vec<_>>()
        writeln!(out,
            "**Search exposure categories (all {} exposed)**: {}",
            reuse.search_exposure_count,
            cat_parts.join(", "))
        out.push('\n')

    // Explicit read categories (new — crt-049)
    if !reuse.explicit_read_by_category.is_empty():
        let mut explicit_cats = reuse.explicit_read_by_category.iter().collect::<Vec<_>>()
        explicit_cats.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)))
        let cat_parts = explicit_cats.iter()
            .map(|(cat, count)| format!("{}x{}", cat, count)).collect::<Vec<_>>()
        writeln!(out, "**Explicit read categories**: {}", cat_parts.join(", "))
        out.push('\n')

    // Top cross-feature entries table (unchanged)
    if !reuse.top_cross_feature_entries.is_empty():
        out.push_str("**Top cross-feature entries**:\n\n")
        out.push_str("| Entry | Type | Served | Source |\n")
        out.push_str("|-------|------|--------|--------|\n")
        for entry in &reuse.top_cross_feature_entries:
            writeln!(out, "| `#{}` {} | {} | {}x | {} |",
                entry.id, escape_md_cell(&entry.title),
                escape_md_cell(&entry.category), entry.serve_count,
                escape_md_cell(&entry.feature_cycle))
        out.push('\n')

    return out
```

---

## Test Fixtures in retrospective.rs — Required Updates

All test fixtures in `retrospective.rs` that construct `FeatureKnowledgeReuse` using the
Rust field name `delivery_count` must be updated to `search_exposure_count` (compile-time
change). Fixtures that set `total_served: 0` may stay as-is since `total_served` is
semantically different now but the zero value remains valid for most test fixtures.

---

## Error Handling

This function has no error return (unchanged). It uses `writeln!` to a `String`, which
is infallible. The `let _ =` pattern on `writeln!` calls is kept for consistency with
existing code in the file.

---

## Key Test Scenarios

All tests in the `#[cfg(test)]` module in `retrospective.rs`. Extend the existing module.

### AC-07 — Golden output assertion (full section)

```
Test: test_render_knowledge_reuse_golden_output
    reuse = FeatureKnowledgeReuse {
        search_exposure_count: 10,
        explicit_read_count: 3,
        explicit_read_by_category: {"decision": 2, "pattern": 1},
        by_category: {"decision": 7, "convention": 3},
        total_served: 5,       // |reads u injections| = 5
        total_stored: 200,
        cross_session_count: 0,
        category_gaps: vec![],
        cross_feature_reuse: 3,
        intra_cycle_reuse: 2,
        top_cross_feature_entries: vec![],
    }

    output = render_knowledge_reuse(&reuse, "test-cycle")

    // Required assertions (ordered):
    assert output.contains("Entries served to agents (reads + injections)")
    assert output.contains("| 5 |") or output.contains(": 5 ") at the served line
    assert output.contains("Search exposures (distinct): 10")
    assert output.contains("Explicit reads (distinct): 3")
    assert output.contains("Explicit read categories")
    assert output.contains("decision") in the explicit categories section
    assert output.contains("pattern") in the explicit categories section
    assert output.contains("Search exposure categories")

    // Legacy label must NOT appear:
    assert !output.contains("Distinct entries served")
```

### AC-17 GATE — Injection-only cycle not suppressed

```
Test: test_render_injection_only_cycle_not_suppressed
    reuse = FeatureKnowledgeReuse {
        search_exposure_count: 0,
        explicit_read_count: 0,
        explicit_read_by_category: {},
        by_category: {},
        total_served: 4,      // 4 injected entries
        total_stored: 50,
        cross_session_count: 0,
        category_gaps: vec![],
        cross_feature_reuse: 4,
        intra_cycle_reuse: 0,
        top_cross_feature_entries: vec![],
    }

    output = render_knowledge_reuse(&reuse, "test-cycle")

    // Must NOT be the short-circuit output:
    assert !output.contains("No knowledge entries served")
    assert output.contains("Entries served to agents (reads + injections)")
    assert output.contains("4")  // total_served = 4 appears in summary
    assert output.contains("Search exposures (distinct): 0")
    assert output.contains("Explicit reads (distinct): 0")
```

### AC-09 — Explicit-read-only cycle not suppressed

```
Test: test_render_explicit_read_only_cycle_not_suppressed
    reuse = FeatureKnowledgeReuse {
        search_exposure_count: 0,
        explicit_read_count: 2,
        explicit_read_by_category: {"decision": 2},
        total_served: 2,      // 2 explicit reads, no injections
        total_stored: 50,
        ...zero/empty for remaining fields
    }

    output = render_knowledge_reuse(&reuse, "test-cycle")

    assert !output.contains("No knowledge entries served")
    assert output.contains("Entries served to agents (reads + injections)")
    assert output.contains("Explicit reads (distinct): 2")
    assert output.contains("Search exposures (distinct): 0")
```

### True zero cycle — early return fires

```
Test: test_render_zero_cycle_short_circuits
    reuse = FeatureKnowledgeReuse {
        search_exposure_count: 0,
        explicit_read_count: 0,
        total_served: 0,
        ...all zero/empty
    }

    output = render_knowledge_reuse(&reuse, "test-cycle")

    assert output.contains("No knowledge entries served")
```

### AC-07 additional: label ordering

```
// The three metric lines must appear in order:
// 1. "Entries served to agents (reads + injections)" (backed by total_served)
// 2. "Search exposures (distinct)" (backed by search_exposure_count)
// 3. "Explicit reads (distinct)" (backed by explicit_read_count)
let pos_served = output.find("Entries served to agents")
let pos_search = output.find("Search exposures (distinct)")
let pos_explicit = output.find("Explicit reads (distinct)")
assert pos_served < pos_search
assert pos_search < pos_explicit
```

---

## Integration Surface

| Name | Signature | Notes |
|------|-----------|-------|
| `render_knowledge_reuse` | `fn(&FeatureKnowledgeReuse, &str) -> String` | Signature unchanged; body modified |
| Caller: `render_retrospective_report` | One call site (line ~128 in retrospective.rs) | No change at call site |
