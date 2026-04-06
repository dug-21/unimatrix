# Component B — services/status.rs

## Purpose

Status orchestration layer. Calls Component A (coherence functions) and writes results
into `StatusReport`. After crt-048 this component removes two call sites entirely and
updates two `compute_lambda()` call sites and one `generate_recommendations()` call site
to the new 3/5-parameter signatures. The `load_active_entries_with_tags()` allocation
and the `coherence_by_source` grouping logic are retained unchanged.

---

## Phase 5 Changes (lines ~689–819)

Phase 5 begins at ~line 689 with `// Phase 5: Coherence dimensions`. The changes below
are all within this phase block.

---

### Block 1: DELETE confidence_freshness_score() call and field assignments (lines ~695-701)

**Current code (lines 695-701):**
```rust
let (freshness_dim, stale_conf_count) = coherence::confidence_freshness_score(
    &active_entries,
    now_ts,
    coherence::DEFAULT_STALENESS_THRESHOLD_SECS,
);
report.confidence_freshness_score = freshness_dim;
report.stale_confidence_count = stale_conf_count;
```

**Post-crt-048 action:** Delete all 7 lines entirely. No replacement.

The `now_ts` variable (lines 690-694) is no longer needed by this block. Verify whether
`now_ts` is used elsewhere in Phase 5 after the deletion. If the only use was the
`confidence_freshness_score()` and `oldest_stale_age()` calls, delete the `now_ts`
declaration too. If it is used by another surviving block, retain it.

Check: scan Phase 5 for any remaining `now_ts` reference after removing blocks 1 and 2.

---

### Block 2: DELETE oldest_stale_age() call (lines ~766-770)

**Current code (lines 766-770):**
```rust
let oldest_stale = coherence::oldest_stale_age(
    &active_entries,
    now_ts,
    coherence::DEFAULT_STALENESS_THRESHOLD_SECS,
);
```

**Post-crt-048 action:** Delete all 5 lines entirely. `oldest_stale` is used only at
the `generate_recommendations()` call site (Block 4). Both are deleted.

---

### Block 3: UPDATE compute_lambda() main-path call (line ~771-777)

**Current code (lines 771-777):**
```rust
report.coherence = coherence::compute_lambda(
    report.confidence_freshness_score,
    report.graph_quality_score,
    embed_dim,
    report.contradiction_density_score,
    &coherence::DEFAULT_WEIGHTS,
);
```

**Post-crt-048 code (4 arguments — R-01 risk, verify argument order):**
```rust
report.coherence = coherence::compute_lambda(
    report.graph_quality_score,
    embed_dim,
    report.contradiction_density_score,
    &coherence::DEFAULT_WEIGHTS,
);
```

Action: Remove the `report.confidence_freshness_score,` first argument line.

Semantic verification (R-01): After the edit confirm:
- 1st arg = graph quality score (f64)
- 2nd arg = embed_dim (Option<f64>)
- 3rd arg = contradiction density score (f64)
- 4th arg = &DEFAULT_WEIGHTS

The `report.confidence_freshness_score` field will no longer exist on `StatusReport`
after Component C changes, so any accidental retention would be caught at compile time.
However, substituting a different f64 field (e.g., `report.graph_stale_ratio`) would
compile silently — verify semantics, not just arity.

---

### Block 4: UPDATE coherence_by_source loop (lines ~779-808)

**Current code (the inner loop at lines ~793-804):**
```rust
let (source_freshness, _) = coherence::confidence_freshness_score(
    &entries.iter().map(|e| (*e).clone()).collect::<Vec<_>>(),
    now_ts,
    coherence::DEFAULT_STALENESS_THRESHOLD_SECS,
);
let source_lambda = coherence::compute_lambda(
    source_freshness,
    report.graph_quality_score,
    embed_dim,
    report.contradiction_density_score,
    &coherence::DEFAULT_WEIGHTS,
);
```

**Post-crt-048 code (R-06 risk — must be identical to Block 3):**
```rust
// [DELETED] confidence_freshness_score() per-source call
let source_lambda = coherence::compute_lambda(
    report.graph_quality_score,
    embed_dim,
    report.contradiction_density_score,
    &coherence::DEFAULT_WEIGHTS,
);
```

Action: Delete the entire `confidence_freshness_score()` call block (5 lines including
the let binding). Update `compute_lambda()` call to remove `source_freshness` as the
first argument. The call must be semantically identical to the main-path call in Block 3.

Post-deletion note: The per-source Lambda is now computed from global `graph_quality_score`
and `contradiction_density_score` plus global `embed_dim`. This is unchanged from the
pre-crt-048 behavior for graph and contradiction (those were already global values, not
per-source). The only per-source value that was computed was `source_freshness`, which is
now deleted. This is intentional per FR-12 and AC-13 — the architecture explicitly notes
that only the call signature update is required for `coherence_by_source`.

The outer loop structure (HashMap grouping, sort, assignment to `report.coherence_by_source`)
is not changed.

---

### Block 5: UPDATE generate_recommendations() call (lines ~811-818)

**Current code (lines 811-818):**
```rust
report.maintenance_recommendations = coherence::generate_recommendations(
    report.coherence,
    coherence::DEFAULT_LAMBDA_THRESHOLD,
    report.stale_confidence_count,
    oldest_stale,
    report.graph_stale_ratio,
    report.embedding_inconsistencies.len(),
    report.total_quarantined,
);
```

**Post-crt-048 code (5 arguments — stale_confidence_count and oldest_stale removed):**
```rust
report.maintenance_recommendations = coherence::generate_recommendations(
    report.coherence,
    coherence::DEFAULT_LAMBDA_THRESHOLD,
    report.graph_stale_ratio,
    report.embedding_inconsistencies.len(),
    report.total_quarantined,
);
```

Action: Remove the `report.stale_confidence_count,` and `oldest_stale,` argument lines.
`oldest_stale` is already deleted in Block 2. `report.stale_confidence_count` is a field
that will no longer exist on `StatusReport` after Component C changes — compile error if
not also removed here.

---

## now_ts Variable Audit

The `now_ts` variable is declared at lines 690-694:
```rust
let now_ts = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap_or_default()
    .as_secs();
```

After removing blocks 1 and 2, audit whether `now_ts` appears anywhere else in Phase 5.
Grep for `now_ts` in the function body. If its only remaining uses were in the deleted
blocks, delete the declaration too to avoid a `dead_code` warning (FR-18).

If any other Phase 5 block references `now_ts` — for example, future coherence dimensions
or a timestamp comparison — retain the declaration.

---

## active_entries Retention (FR-11)

The `load_active_entries_with_tags()` call that populates `active_entries` must be
retained. Post-crt-048 it serves the `coherence_by_source` grouping loop (Block 4).
Do not remove this call or the allocation. The only thing removed is the freshness scan
over its contents (Blocks 1 and 2).

---

## run_maintenance() — NOT MODIFIED

`run_maintenance()` at ~line 1242 uses `coherence::DEFAULT_STALENESS_THRESHOLD_SECS` for
confidence refresh targeting. This function is outside Phase 5 and is not touched by
crt-048. The constant must remain defined in Component A (ADR-002).

---

## Error Handling

No new error handling introduced. The changes are deletions of call sites; the surviving
`compute_lambda()` and `generate_recommendations()` calls do not return `Result`. Existing
error handling for `load_active_entries_with_tags()`, graph metrics, and other Phase 5
queries is unchanged.

---

## Key Test Scenarios (integration-level)

1. Phase 5 completes without referencing `confidence_freshness_score` or `oldest_stale_age`
   (R-03 grep check: zero matches in `services/status.rs` post-delivery).
2. Both `compute_lambda()` call sites pass exactly 4 arguments in identical order (R-01,
   R-06 — grep for `compute_lambda(` in `services/status.rs`, confirm exactly 2 matches,
   each with 4 arguments).
3. `generate_recommendations()` call passes exactly 5 arguments (grep confirms).
4. `coherence_by_source` still populates: the per-source loop runs and assigns non-empty
   results when active entries exist with distinct trust sources.
5. Lambda value in `report.coherence` is in [0.0, 1.0] for a live knowledge base call.
