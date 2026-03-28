# Pseudocode: auto-outcomes

Component: Auto-Generated Session Outcomes (P0)
Files: `crates/unimatrix-server/src/outcome_tags.rs`, `crates/unimatrix-server/src/uds_listener.rs`

---

## Purpose

When a session closes with outcome Success or Rework AND had at least one injection, automatically write a `category: outcome` entry to Unimatrix. This closes the loop between the hook signal pipeline and the queryable knowledge base.

---

## 1. outcome_tags.rs Changes

Add `"session"` to `VALID_TYPES`:

```
// Current (before col-010):
pub const VALID_TYPES: &[&str] = &[
    "feature", "bug", "spike", "gate", "phase", "result",
    "agent", "wave",
];

// After col-010:
pub const VALID_TYPES: &[&str] = &[
    "feature", "bug", "spike", "gate", "phase", "result",
    "agent", "wave",
    "session",  // col-010: auto-generated session lifecycle outcomes
];
```

No other changes to this file.

---

## 2. write_auto_outcome_entry (uds_listener.rs)

This function is called from `process_session_close` when `final_status != Abandoned && injection_count > 0`.

```
fn write_auto_outcome_entry(
    store: &Arc<Store>,
    session_id: &str,
    outcome_str: &str,       // "success" | "rework"
    injection_count: u32,
    feature_cycle: Option<&str>,
    agent_role: Option<&str>,
):
    // Build entry content (sanitized fields are already safe from SessionRegister)
    content = format!(
        "Session {} completed with outcome: {}. Injected {} entries.",
        session_id, outcome_str, injection_count
    )

    // Tags: type:session + result:pass (success) or result:rework (rework)
    result_tag = if outcome_str == "success" { "result:pass" } else { "result:rework" }
    tags = vec!["type:session", result_tag]

    entry = NewEntry {
        title: format!("Session outcome: {}", session_id),
        content: content,
        topic: format!("session/{}", session_id),
        category: "outcome".to_string(),
        tags: tags.into_iter().map(|s| s.to_string()).collect(),
        source: "hook".to_string(),
        status: Status::Active,
        created_by: "cortical-implant".to_string(),
        feature_cycle: feature_cycle.unwrap_or("").to_string(),
        trust_source: "system".to_string(),   // scores 0.7
    }

    // Write via spawn_blocking — bypasses MCP validation layer
    let store_clone = Arc::clone(store)
    spawn_blocking_fire_and_forget(move || {
        match store_clone.insert_entry(entry):
            Ok(entry_id) =>
                // Also write OUTCOME_INDEX entry for feature_cycle if present
                if !feature_cycle_str.is_empty():
                    // OUTCOME_INDEX is updated by insert_entry when feature_cycle is set
                    tracing::debug!(entry_id = %entry_id, "Auto-outcome entry written")
            Err(e) =>
                tracing::warn!(session_id = %session_id_clone, error = %e, "Auto-outcome write failed")
    })
```

Key fields:
- `embedding_dim = 0` — the engine sets this; we do NOT call ONNX. The `insert_entry` path for entries without embeddings should set `embedding_dim = 0` by default.
- `trust_source = "system"` — correctness: scores 0.7 (not the `_ => 0.3` fallback arm).

---

## 3. Entry Write Path (insert_entry with embedding_dim = 0)

The auto-outcome entry uses `store.insert_entry()` directly (bypassing the MCP request pipeline). This is the same mechanism used by existing hook-written entries.

Check if `insert_entry` in `write.rs` supports a path that writes with `embedding_dim = 0` (no embedding). If the write API requires embedding, use an alternate path that sets `embedding_dim = 0` explicitly and skips the ONNX step.

Looking at `write.rs`: the `insert_entry` function accepts a `NewEntry` and sets `embedding_dim` based on whether embedding succeeds. For auto-outcomes we skip embedding entirely. Two options:
1. Write directly via a `NewEntry` with `embedding_dim = 0` pre-set.
2. Use a `store.insert_entry_no_embed(entry)` variant if it exists.

For col-010, the implementation should look up how `col-009` writes signal entries that skip embedding. Follow the same pattern.

Constraint: SessionClose must NOT block on ONNX.

---

## 4. OUTCOME_INDEX Population

> **RETRACTED — GH #430**: The claim below is false. `store.insert()` (the raw store method) does NOT auto-populate OUTCOME_INDEX. Only `insert_outcome_index_if_applicable()` does that, and it is called separately from the MCP write path. `write_auto_outcome_entry()` used `store.insert()` directly from a fire-and-forget spawn, bypassing the index entirely. The function has been deleted as dead code with broken intent (fix: GH #430). SESSIONS already holds all session telemetry. Do not reinstate this approach.

~~When `insert_entry` is called with a non-empty `feature_cycle` and `category = "outcome"`, the existing code in `write.rs` already populates OUTCOME_INDEX. This is the col-001 integration — no additional code needed for the index write.~~

~~Verify by checking: `crates/unimatrix-store/src/write.rs` — confirm OUTCOME_INDEX is populated when `feature_cycle != ""` and `category == "outcome"`.~~

---

## Error Handling

| Error | Handling |
|-------|---------|
| `insert_entry` fails | Log warn; SessionClose proceeds normally |
| `OUTCOME_INDEX` write fails | Propagated from `insert_entry`; same handling |

Auto-outcome write failure is non-fatal. The session is already closed and its SessionRecord updated.

---

## Key Test Scenarios

1. `VALID_TYPES.contains(&"session")` → true.
2. `validate_outcome_tags(["type:session"])` → Ok(()).
3. SessionClose Success, 3 injections → auto-outcome entry written with `category=outcome`, `type:session`, `result:pass`, `trust_source=system`, `embedding_dim=0`.
4. SessionClose Rework, 1 injection → `result:rework` in tags.
5. SessionClose Abandoned, 2 injections → NO auto-outcome written.
6. SessionClose Success, 0 injections → NO auto-outcome written (trivial session).
7. Auto-outcome appears in `context_lookup(category:"outcome", tags:["type:session"])`.
8. Auto-outcome is NOT in vector search results (embedding_dim=0, not indexed).
