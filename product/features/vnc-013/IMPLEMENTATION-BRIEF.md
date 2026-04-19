# vnc-013 Implementation Brief
## Canonical Event Normalization for Multi-LLM Hook Providers

---

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/vnc-013/SCOPE.md |
| Architecture | product/features/vnc-013/architecture/ARCHITECTURE.md |
| Specification | product/features/vnc-013/specification/SPECIFICATION.md |
| Risk Strategy | product/features/vnc-013/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-013/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| wire-protocol | pseudocode/wire-protocol.md | test-plan/wire-protocol.md |
| normalization | pseudocode/normalization.md | test-plan/normalization.md |
| source-domain-derivation | pseudocode/source-domain-derivation.md | test-plan/source-domain-derivation.md |
| reference-configs | pseudocode/reference-configs.md | test-plan/reference-configs.md |

### Cross-Cutting Artifacts

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | product/features/vnc-013/pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | product/features/vnc-013/test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Goal

Install a normalization layer at the hook ingest boundary (`hook.rs build_request()`) so that Gemini CLI, Codex CLI, and Claude Code can all participate in Unimatrix without downstream changes. All provider-specific event names are translated to canonical Claude Code names at ingest; provider identity is threaded as an explicit `provider` field through the wire protocol; and the three hardcoded `source_domain = "claude-code"` sites are replaced with dynamic derivation. Nothing below the normalization boundary branches on provider-specific strings.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|------------|--------|----------|
| Canonical event name strategy | Claude Code event names are canonical; Gemini/Codex names map onto them; zero downstream changes | ADR-001 (Unimatrix #4305) | architecture/ADR-001-canonical-event-name-strategy.md |
| Provider identity mechanism | Explicit `provider: Option<String>` field on `HookInput` and `ImplantEvent`; populated before normalization; not inferred at the listener | ADR-002 (Unimatrix #4306) | architecture/ADR-002-provider-field-on-wire-protocol.md |
| Gemini `mcp_context` field access | Named field `mcp_context: Option<serde_json::Value>` on `HookInput` (not stringly-typed `extra` access); coexists with `extra` flatten | ADR-003 (Unimatrix #4307) | architecture/ADR-003-gemini-mcp-context-named-field.md |
| DB read path `source_domain` derivation | Approach A: `registry.resolve_source_domain(event_type)` with `"claude-code"` fallback when result is `"unknown"`; constant `DEFAULT_HOOK_SOURCE_DOMAIN = "claude-code"` replaces literals | ADR-004 (Unimatrix #4308) | architecture/ADR-004-db-read-path-source-domain-approach-a.md |
| Rework detection gate | `provider != "claude-code"` gate in `"PostToolUse"` arm; tool-name guard is a secondary filter for Claude Code only; accidental exclusion via tool names is not a contract | ADR-005 (Unimatrix #4309) | architecture/ADR-005-rework-detection-provider-gate.md |
| Codex `--provider` flag | Mandatory in `.codex/hooks.json` reference config; config carries caveat that live MCP hook support is blocked by Codex bug #16732; omission degrades to `"claude-code"` label (silent, not a crash) | ADR-006 (Unimatrix #4310) | architecture/ADR-006-codex-provider-flag-mandatory.md |

---

## Files to Create / Modify

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-engine/src/wire.rs` | Modify | Add `provider: Option<String>` and `mcp_context: Option<serde_json::Value>` to `HookInput`; add `provider: Option<String>` to `ImplantEvent`; all fields `#[serde(default)]` |
| `crates/unimatrix-server/src/uds/hook.rs` | Modify | Add `normalize_event_name()` function; extend `run()` with `provider: Option<String>` parameter; add Gemini dispatch arms; gate rework path on `provider`; add `mcp_context.tool_name` promotion adapter |
| `crates/unimatrix-server/src/uds/listener.rs` | Modify | Replace Site A `source_domain` hardcode (line 1894) with `event.provider.clone().unwrap_or_else(|| "claude-code".to_string())`; add `debug_assert!` guard in `extract_observation_fields()` for `"post_tool_use_rework_candidate"` |
| `crates/unimatrix-server/src/background.rs` | Modify | Replace Site B `source_domain` hardcode (line 1330) in `fetch_observation_batch()` with Approach A registry-with-fallback pattern |
| `crates/unimatrix-server/src/services/observation.rs` | Modify | Replace Site C `source_domain` hardcode (line 585) in `parse_observation_rows()`; remove `_` prefix from `_registry` parameter; update test comment in `test_parse_rows_unknown_event_type_passthrough` |
| `crates/unimatrix-server/src/main.rs` | Modify | Add `--provider` optional argument to `Hook` command variant; pass to `hook::run()` |
| `.gemini/settings.json` | Create | Reference Gemini CLI hook configuration; matcher `mcp_unimatrix_.*`; four events: `BeforeTool`, `AfterTool`, `SessionStart`, `SessionEnd` |
| `.codex/hooks.json` | Create | Reference Codex CLI hook configuration; identical schema to `.claude/settings.json`; `--provider codex-cli` on every invocation; caveat about Codex bug #16732 |

---

## Data Structures

### HookInput (wire.rs) — new fields

```rust
#[serde(default)]
pub provider: Option<String>,        // "claude-code" | "gemini-cli" | "codex-cli"

#[serde(default)]
pub mcp_context: Option<serde_json::Value>,  // Gemini BeforeTool/AfterTool structured field
                                              // { "server_name", "tool_name", "url" }
```

### ImplantEvent (wire.rs) — new field

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub provider: Option<String>,        // propagated from HookInput.provider
```

### Event Mapping Table (Category 1 only — normalization target)

| Gemini CLI Event | Claude Code Event | Codex CLI Event | Canonical Name |
|---|---|---|---|
| `BeforeTool` | `PreToolUse` | `PreToolUse` | `PreToolUse` |
| `AfterTool` | `PostToolUse` | `PostToolUse` | `PostToolUse` |
| `SessionStart` | `SessionStart` | `SessionStart` | `SessionStart` |
| `SessionEnd` | `Stop` | `Stop` | `Stop` |
| (none) | `SubagentStart` | (none) | `SubagentStart` |
| (none) | `SubagentStop` | (none) | `SubagentStop` |
| (none) | `PreCompact` | (none) | `PreCompact` |
| (none) | `UserPromptSubmit` | (none) | `UserPromptSubmit` |
| (none) | `PostToolUseFailure` | (none) | `PostToolUseFailure` |
| (none) | `TaskCompleted` | (none) | `TaskCompleted` |
| (none) | `Ping` | (none) | `Ping` |

Category 2 events (`cycle_start`, `cycle_stop`, `cycle_phase_end`) are already canonical. They are NOT normalized.

---

## Function Signatures

### `normalize_event_name` (hook.rs)

```rust
/// Translate a provider-specific event name to its canonical Unimatrix name
/// and infer (or confirm) the originating provider.
///
/// Returns (&'static str canonical_name, &'static str provider).
///
/// When provider_hint is Some(p), p is used as provider and the event is
/// mapped to its canonical form. When None, inference runs:
///   - Gemini-unique names (BeforeTool, AfterTool, SessionEnd) → gemini-cli
///   - All other names (shared or Claude Code-only) → claude-code (backward compat)
///   - Unknown names → (event, "unknown") passthrough — note: this arm cannot
///     return &'static str for the event name, so unknown events return the
///     tuple with a static "unknown" provider and a fallback to a known branch;
///     implementation must handle the lifetime carefully (see NFR-01).
pub fn normalize_event_name(
    event: &str,
    provider_hint: Option<&str>,
) -> (&'static str, &'static str)
```

Note: The return type is `(&'static str, &'static str)` per ARCHITECTURE.md Layer 2. SPECIFICATION.md FR-01.1 mistakenly omits the `'static` lifetime — use the architecture version. Unknown event names cannot be returned as `&'static str`; the implementation must handle this (e.g., fall through to a known canonical or use a different return type for the unknown arm). Implementer must resolve during pseudocode.

### `hook::run` (hook.rs) — extended signature

```rust
pub fn run(
    event: String,
    provider: Option<String>,
    project_dir: Option<PathBuf>,
) -> Result<(), HookError>
```

### `DEFAULT_HOOK_SOURCE_DOMAIN` constant

```rust
/// Fallback source_domain for DB read paths when the registry returns "unknown".
/// Preserves hook-path invariant: all stored observations that lack explicit
/// source_domain are attributed to the default provider (Approach A, ADR-004).
pub const DEFAULT_HOOK_SOURCE_DOMAIN: &str = "claude-code";
```

Constant placement: co-locate with the primary consumer (`services/observation.rs`) and re-export for `background.rs`. See ARCHITECTURE.md OQ-A — placement decision deferred to implementer.

### Approach A pattern (Sites B and C)

```rust
let source_domain = {
    let resolved = registry.resolve_source_domain(&event_type);
    if resolved != "unknown" { resolved } else { DEFAULT_HOOK_SOURCE_DOMAIN.to_string() }
};
```

### mcp_context promotion adapter (hook.rs, PreToolUse arm)

```rust
// Gemini BeforeTool places tool_name in mcp_context.tool_name (bare name).
// Claude Code places it in extra["tool_name"] (prefixed). Promote before calling
// build_cycle_event_or_fallthrough() which reads extra["tool_name"].
if let Some(bare_name) = input.mcp_context
    .as_ref()
    .and_then(|v| v.get("tool_name"))
    .and_then(|v| v.as_str())
{
    extra_clone["tool_name"] = serde_json::Value::String(bare_name.to_string());
}
// Pass extra_clone (not original input.extra) to build_cycle_event_or_fallthrough().
```

CRITICAL: The clone must be passed to `build_cycle_event_or_fallthrough()`, not the original input. Passing the original makes the promotion a no-op (R-01 failure mode).

### rework detection gate (hook.rs, PostToolUse arm)

```rust
"PostToolUse" => {
    // Rework detection is Claude Code-specific (ADR-005).
    if provider.as_deref() != Some("claude-code") {
        return HookRequest::RecordEvent { event: ImplantEvent {
            event_type: canonical_event.to_string(),
            provider: Some(provider.to_string()),
            // ... other fields
        }};
    }
    // Existing rework detection logic (Claude Code only) follows ...
}
```

### debug_assert guard (listener.rs, extract_observation_fields)

```rust
// Guard: "post_tool_use_rework_candidate" must never reach the hook column.
// Normalization in build_request() should have converted it to "PostToolUse"
// before this point. This assert is compiled out in release builds; the
// "PostToolUse" | "post_tool_use_rework_candidate" arm below is the real enforcement.
debug_assert!(
    event_type != "post_tool_use_rework_candidate",
    "rework candidate string escaped normalization boundary"
);
```

---

## Constraints

1. **No DB schema changes.** `observations.hook TEXT` is sufficient. `source_domain` is not persisted; derived at runtime. No schema migration, no version bump.
2. **No async / tokio in hook.rs.** `normalize_event_name()` must be pure synchronous with no I/O.
3. **Rework detection is Claude Code-only.** Gate using `provider != "claude-code"`. Tool-name guard is a secondary filter — not the primary gate.
4. **`build_cycle_event_or_fallthrough()` is the single implementation.** No duplication. Gemini adapter normalizes payload then calls it.
5. **Hook exit is always 0.** Normalization failures (unknown provider, malformed `mcp_context`) log via `eprintln!` and degrade to `generic_record_event`. `run()` never returns error from normalization failures.
6. **Gemini matcher regex must be valid Gemini CLI v0.31+ syntax.** Pattern `mcp_unimatrix_.*` confirmed for this version.
7. **`HookInput` backward compatibility.** `provider` and `mcp_context` fields use `#[serde(default)]`. Existing Claude Code hook JSON deserializes to `None` without error.
8. **Codex is built, not live-tested.** Reference config ships. Code paths handle Codex events with `--provider codex-cli`. Live end-to-end testing blocked by Codex bug #16732.
9. **Approach A is the required DB-read-path contract.** Approach B (accepting `"unknown"`) is rejected — it changes existing behavior for non-listed event types.
10. **`PostToolUseFailure` path is untouched.** AC-16's guard targets rework candidate string only. Per ADR-003 col-027 (entry #3475), `PostToolUseFailure` is intentionally not normalized to `PostToolUse`.
11. **`DomainPackRegistry` requires no changes.** The builtin claude-code pack's 4-event `event_types` list is not modified. Gemini names never reach the registry.

---

## Dependencies

### Crates (No New Dependencies)

| Crate | Usage |
|-------|-------|
| `serde_json` | `mcp_context: Option<serde_json::Value>`; `tool_name` extraction via `get()` / `as_str()` |
| `serde` | `#[serde(default)]` and `#[serde(skip_serializing_if)]` on new fields |
| `clap` (existing) | `--provider` optional `Option<String>` argument on `Hook` command variant |

### Internal Components

| Component | Change |
|-----------|--------|
| `unimatrix-engine::wire::HookInput` | Modified — two new fields |
| `unimatrix-engine::wire::ImplantEvent` | Modified — one new field |
| `unimatrix-server::uds::hook::build_request()` | Modified — normalization + Gemini arms + rework gate + mcp_context promotion |
| `unimatrix-server::uds::hook::build_cycle_event_or_fallthrough()` | Unchanged — called after normalization |
| `unimatrix-server::uds::listener::extract_observation_fields()` | Modified — debug_assert guard |
| `unimatrix-server::uds::listener` (line 1894) | Modified — Site A source_domain |
| `unimatrix-server::background` (line 1330) | Modified — Site B source_domain |
| `unimatrix-server::services::observation` (line 585) | Modified — Site C source_domain; `_registry` prefix removed |
| `unimatrix-server::main::Hook` command | Modified — `--provider` argument |
| `unimatrix-core::observation::hook_type` | Unchanged — constants are already canonical |
| `DomainPackRegistry::resolve_source_domain()` | Unchanged — used by Approach A, no changes |

### External

| Dependency | Notes |
|------------|-------|
| Gemini CLI v0.31+ | Reference config target. `mcp_unimatrix_.*` matcher confirmed. |
| Codex CLI | Reference config target. Live hooks blocked by upstream bug #16732. Schema confirmed identical to Claude Code (ASS-049). |

---

## NOT In Scope

- Live Codex CLI end-to-end testing (blocked by Codex bug #16732)
- `source_domain` as a persisted column (no schema migration)
- Continue, Cursor, or Zed hook provider support
- SubagentStart/SubagentStop equivalents for Gemini or Codex
- Gemini `BeforeModel` hook (separate architecture)
- Changes to `context_cycle_review` logic (canonical names already provider-neutral)
- Changes to `DomainPackRegistry` (insulated by normalization)
- MCP schema fixes (Gemini `$defs`, union types, reserved names) — separate feature
- Tool description rewrites (`context_briefing`, `context_cycle`) — separate delivery task
- `max_tokens` enforcement on MCP path — separate delivery task
- Server-side session attribution via `clientInfo.name` + `Mcp-Session-Id` (ASS-049 recommendation)
- `PostToolUseFailure` normalization to `PostToolUse` (intentionally excluded per ADR-003 col-027, entry #3475)
- Any change to `DomainPackRegistry` builtin pack's `event_types` list

---

## Alignment Status

Vision alignment: **PASS.** The feature directly closes a Critical Gap identified in the product vision ("HookType enum tied to Claude Code events", partially addressed by col-023). vnc-013 extends that fix to the three remaining `source_domain` hardcodes and the `build_request()` dispatch boundary. No variances requiring approval.

**Four WARN items (informational — no approval required):**

1. **`debug_assert` canary not mandated in any AC.** RISK-TEST-STRATEGY R-02 references a `debug_assert!(event.provider.is_some(), "ImplantEvent.provider must be set at normalization boundary")` canary in `listener.rs`. SPECIFICATION.md does not include this in any FR. Implementer should add it as a defensive measure regardless — it confirms all `ImplantEvent` construction sites thread `provider` correctly and is compiled out in release builds.

2. **Lifetime annotation: use `&'static str` per ARCHITECTURE.md.** SPECIFICATION.md FR-01.1 specifies `-> (&str, &str)` but ARCHITECTURE.md Layer 2 specifies `-> (&'static str, &'static str)`. The architecture version is correct — the function is a pure static mapping with no borrowed returns. Follow the architecture. (Note: the unknown event passthrough arm cannot return `&'static str` for a dynamic event string; implementer must resolve, e.g., by returning a known canonical for unknown events or changing the return type.)

3. **`mcp_context.tool_name` equality check: recommend stricter match.** The security risk section in RISK-TEST-STRATEGY notes that `build_cycle_event_or_fallthrough()`'s `contains("context_cycle")` guard is permissive — a tool name like `"malicious_contains_context_cycle"` would pass it. There is no AC enforcing a stricter check. Recommend using `tool_name == "context_cycle"` equality check instead of `contains`, since the bare name format is confirmed and the equality check covers all valid cases. No AC enforces this, but the implementer should prefer it.

4. **`extract_observation_fields()` wildcard arm for Gemini `PostToolUse` — documentation gap only.** ARCHITECTURE.md OQ-D notes the wildcard arm in `extract_observation_fields()` handles `"SubagentStop"` and other events. Gemini `AfterTool` normalized to `"PostToolUse"` hits the explicit `"PostToolUse" | "post_tool_use_rework_candidate"` arm, not the wildcard. Architecture analysis confirms the behavior is already correct — this is a documentation gap, not a behavioral issue.

---

## Known Limitation

DB read paths (`background.rs:1330`, `services/observation.rs:585`) cannot distinguish Claude Code from Gemini records after normalization. A Gemini `BeforeTool` event is stored as canonical `"PreToolUse"` — `resolve_source_domain("PreToolUse")` returns `"claude-code"`. This is the accepted known limitation from SCOPE.md OQ-4. Only the write path (`listener.rs:1894`) correctly labels live Gemini events as `"gemini-cli"`. The DB read paths affect `context_cycle_review` and the metrics tick for retrospective attribution only — behavioral correctness of cycle interception and observation storage is unaffected.

---

## Gate Prerequisite

AC-14 unit tests (R-01 scenarios 1–3: `mcp_context.tool_name` promotion, non-cycle fallthrough, missing key graceful degradation) must be green before any other Gemini `BeforeTool` AC is attempted. This is the highest-risk integration point. A failure here causes silent fallthrough to generic `RecordEvent` with no error signal.

---

## Blast Radius Coverage Table

| File | Crate | Change Required | Validating ACs |
|------|-------|----------------|----------------|
| `unimatrix-engine/src/wire.rs` | unimatrix-engine | `provider` on `HookInput` + `ImplantEvent`; `mcp_context` on `HookInput` | AC-05, AC-14 |
| `unimatrix-server/src/uds/hook.rs` | unimatrix-server | `normalize_event_name()`; `run()` provider flag; Gemini arms; rework gate; mcp_context promotion | AC-01–AC-05, AC-11, AC-12, AC-15, AC-17, AC-18 |
| `unimatrix-server/src/uds/listener.rs` | unimatrix-server | Site A source_domain (line 1894); debug_assert guard | AC-06, AC-07(a), AC-16 |
| `unimatrix-server/src/background.rs` | unimatrix-server | Site B source_domain (line 1330) | AC-07(b) |
| `unimatrix-server/src/services/observation.rs` | unimatrix-server | Site C source_domain (line 585); `_registry` prefix removed; test comment update | AC-07(c), AC-08 |
| `unimatrix-server/src/main.rs` | unimatrix-server | `--provider` on `Hook` command variant | AC-15, AC-17, AC-18 |
| `.gemini/settings.json` | (config) | New reference configuration | AC-10 |
| `.codex/hooks.json` | (config) | New reference configuration | AC-19 |
