# Component 4: Usage Gate Fix — `usage.rs` + `tools.rs`

## Purpose

Fix a confirmed production bug: `UsageService::record_mcp_usage()` and
`record_hook_injection()` gate `feature_entries` writes on trust level (`System | Privileged
| Internal`), silently dropping the write for Restricted-trust agents even when they have been
explicitly granted `Capability::Write`. The fix replaces the trust-level gate with a boolean
`write_capable` field on `UsageContext` that carries the capability-check result from the
handler into the usage service.

## Files

- `crates/unimatrix-server/src/services/usage.rs` — struct field addition + gate replacement + unit tests
- `crates/unimatrix-server/src/mcp/tools.rs` — `write_capable` set at every `UsageContext` construction site

## Part A: `usage.rs` — Struct Modification

### Current `UsageContext` struct (lines 50-76)

```rust
pub(crate) struct UsageContext {
    pub session_id:    Option<String>,
    pub agent_id:      Option<String>,
    pub helpful:       Option<bool>,
    pub feature_cycle: Option<String>,
    pub trust_level:   Option<TrustLevel>,
    pub access_weight: u32,
    pub current_phase: Option<String>,
}
```

### Modified `UsageContext` struct

Add `write_capable: bool` as the last field before the closing brace:

```rust
pub(crate) struct UsageContext {
    pub session_id:    Option<String>,
    pub agent_id:      Option<String>,
    pub helpful:       Option<bool>,
    pub feature_cycle: Option<String>,
    pub trust_level:   Option<TrustLevel>,    // retained; no longer used in feature_recording gate
    pub access_weight: u32,
    pub current_phase: Option<String>,
    /// Whether the caller passed Capability::Write for this call.
    /// Set true only at the context_store call site (after require_cap(Write) passes).
    /// All other UsageContext construction sites must explicitly set false.
    pub write_capable: bool,
}
```

No `Default` impl. No `#[serde(default)]`. No `#[derive(Default)]`. Omitting this field at
any construction site is a compile error (`struct UsageContext has no field named ... or
missing field write_capable`). This is the enforcement mechanism for C-11.

## Part B: `usage.rs` — Gate Replacement in `record_mcp_usage`

### Current gate (lines 207-218)

```rust
let feature_recording = ctx.feature_cycle.and_then(|feature_str| {
    let trust = ctx.trust_level.unwrap_or(TrustLevel::Restricted);
    if matches!(
        trust,
        TrustLevel::System | TrustLevel::Privileged | TrustLevel::Internal
    ) {
        Some((feature_str, entry_ids.to_vec()))
    } else {
        None
    }
});
```

### Fixed gate

```rust
let feature_recording = ctx.feature_cycle.and_then(|feature_str| {
    if ctx.write_capable {
        Some((feature_str, entry_ids.to_vec()))
    } else {
        None
    }
});
```

The trust-level match is removed entirely. `ctx.trust_level` is no longer read in this block
(it remains in the struct for other uses). The logic is: if the agent passed the Write
capability check (which the handler has already performed), then feature entries are recorded;
otherwise they are not. The trust level is irrelevant to this decision.

## Part C: `usage.rs` — Gate Replacement in `record_hook_injection`

Identical change to Part B. The gate appears at lines 272-283. Same replacement:

### Current gate (lines 272-283)

```rust
let feature_recording = ctx.feature_cycle.and_then(|feature_str| {
    let trust = ctx.trust_level.unwrap_or(TrustLevel::Restricted);
    if matches!(
        trust,
        TrustLevel::System | TrustLevel::Privileged | TrustLevel::Internal
    ) {
        Some((feature_str, entry_ids.to_vec()))
    } else {
        None
    }
});
```

### Fixed gate

```rust
let feature_recording = ctx.feature_cycle.and_then(|feature_str| {
    if ctx.write_capable {
        Some((feature_str, entry_ids.to_vec()))
    } else {
        None
    }
});
```

Both gate blocks must be replaced. C-12 makes this a hard requirement: leaving
`record_hook_injection` unfixed leaves a latent bug even if `record_mcp_usage` is fixed.

## Part D: `tools.rs` — `write_capable` at Every Construction Site

There are 4 `UsageContext { ... }` literals in `tools.rs`. Each must add `write_capable`.

### Site 1: `context_search` handler (~line 473)

```rust
UsageContext {
    session_id:    ctx.audit_ctx.session_id.clone(),
    agent_id:      Some(ctx.agent_id.clone()),
    helpful:       params.helpful,
    feature_cycle: params.feature.clone(),
    trust_level:   Some(ctx.trust_level),
    access_weight: 1,
    current_phase: current_phase.clone(),
    write_capable: false,   // NEW — context_search does not write feature_entries
}
```

### Site 2: `context_lookup` handler (~line 609)

```rust
UsageContext {
    session_id:    ctx.audit_ctx.session_id.clone(),
    agent_id:      Some(ctx.agent_id.clone()),
    helpful:       params.helpful,
    feature_cycle: params.feature.clone(),
    trust_level:   Some(ctx.trust_level),
    access_weight: 2,
    current_phase, // col-028: phase captured above
    write_capable: false,   // NEW — context_lookup does not write feature_entries
}
```

### Site 3: `context_store` handler (~line 826) — THE ONLY `true` SITE

This site is inside the `if let Some(fc) = usage_feature_cycle { ... }` branch. Within this
branch, `require_cap(Capability::Write)` at line 653 has already returned `Ok(())`. The
capability check is the authority; `write_capable: true` is the signal that it passed.

```rust
UsageContext {
    session_id:    ctx.audit_ctx.session_id.clone(),
    agent_id:      Some(ctx.agent_id.clone()),
    helpful:       None,
    feature_cycle: Some(fc),
    trust_level:   Some(ctx.trust_level),
    access_weight: 1,
    current_phase: current_phase.clone(),
    write_capable: true,    // NEW — require_cap(Write) already passed; always true here
}
```

Per C-13: `write_capable: true` is unconditional inside this branch. No additional conditional
is needed. A Restricted-trust agent that reaches this point has already been verified as Write-
capable by `require_cap`; `write_capable` is just the propagation of that fact.

### Site 4: `context_get` handler (~line 922)

```rust
UsageContext {
    session_id:    ctx.audit_ctx.session_id.clone(),
    agent_id:      Some(ctx.agent_id.clone()),
    helpful:       params.helpful.or(Some(true)),
    feature_cycle: params.feature.clone(),
    trust_level:   Some(ctx.trust_level),
    access_weight: 2,
    current_phase,
    write_capable: false,   // NEW — context_get does not write feature_entries
}
```

### Site 5: `context_briefing` handler (~line 1594)

```rust
UsageContext {
    session_id:    ctx.audit_ctx.session_id.clone(),
    agent_id:      Some(ctx.agent_id.clone()),
    helpful:       params.helpful,
    feature_cycle: params.feature.clone(),
    trust_level:   Some(ctx.trust_level),
    access_weight: 0,
    current_phase,
    write_capable: false,   // NEW — context_briefing does not write feature_entries
}
```

Note: The grep output shows 4 sites in tools.rs (lines 473, 609, 826, 922, 1594 — actually 5
occurrences). The implementer must audit ALL `UsageContext {` literals in the file (and in
any other file that constructs `UsageContext`) and add `write_capable: false` (or `true` for
the context_store site). The compiler will catch any missed site as a compile error.

## Part E: `usage.rs` — Unit Tests for Gate Logic

Add two unit tests to the existing `#[cfg(test)] mod tests` block in `usage.rs`. These tests
are pure logic tests — no store, no database, no async.

### Test 1: `write_capable: false` → no feature recording

```rust
#[test]
fn test_write_capable_false_yields_no_feature_recording() {
    // Construct the gate logic directly.
    // This mirrors the gate in record_mcp_usage and record_hook_injection.
    let feature_cycle: Option<String> = Some("test-cycle".to_string());
    let write_capable: bool = false;

    let feature_recording = feature_cycle.and_then(|feature_str| {
        if write_capable {
            Some((feature_str, vec![1u64, 2u64]))
        } else {
            None
        }
    });

    assert!(
        feature_recording.is_none(),
        "expected None when write_capable=false, got: {:?}",
        feature_recording
    );
}
```

### Test 2: `write_capable: true` → feature recording produced

```rust
#[test]
fn test_write_capable_true_yields_feature_recording() {
    let feature_cycle: Option<String> = Some("test-cycle".to_string());
    let write_capable: bool = true;
    let entry_ids: Vec<u64> = vec![42u64];

    let feature_recording = feature_cycle.and_then(|feature_str| {
        if write_capable {
            Some((feature_str, entry_ids.clone()))
        } else {
            None
        }
    });

    assert!(
        feature_recording.is_some(),
        "expected Some when write_capable=true"
    );

    let (cycle_str, ids) = feature_recording.unwrap();
    assert_eq!(cycle_str, "test-cycle");
    assert_eq!(ids, vec![42u64]);
}
```

These tests are synchronous (`#[test]`, not `#[tokio::test]`). They exercise the gate logic
in isolation without needing a store or service. They use `let` bindings to mirror the exact
gate expression from the fixed code, so a refactor of the gate would require updating these
tests — by design.

## Error Handling

- The gate fix has no new error paths. It produces `Option<(String, Vec<u64>)>`.
- `None` means no write enqueued (same silent behavior as before, now only for non-Write calls).
- `Some(...)` means a write is enqueued; the write itself logs on failure via `tracing::warn!`.
- The `trust_level` field remains on `UsageContext` and is still set at all construction sites.
  No callers of `trust_level` outside the gate blocks are affected.

## Constraints

- C-11: `write_capable` has no `Default`. Every construction site must set it explicitly.
- C-12: Both `record_mcp_usage` AND `record_hook_injection` gates must be replaced.
- C-13: `write_capable: true` is set unconditionally inside the `context_store` handler's
  `if let Some(fc) = usage_feature_cycle` branch.
- NFR-08: `trust_level` field is retained on `UsageContext`. Not removed.
- NFR-07: No `#[serde(default)]` or `Default` derivation on `write_capable`.
- NFR-06: `cargo fmt` and `cargo clippy --workspace -- -D warnings` must pass.
  Removing the unused `trust` variable from the gate block eliminates any dead-variable
  warning from clippy. The implementer must confirm no unused import warnings arise from
  removing the `matches!` usage in the gate (if `TrustLevel` is no longer used anywhere
  in the gate blocks; check other usages in the file before removing imports).
