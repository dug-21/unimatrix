# vnc-013: Canonical Event Normalization for Multi-LLM Hook Providers — Architecture

## System Overview

Unimatrix's hook ingest boundary today carries two hardcoded Claude Code assumptions
that make the system structurally incompatible with other LLM clients:

1. Three sites in the observation pipeline hardcode `source_domain = "claude-code"`
   regardless of which client actually fired the hook.
2. `build_request()` in `hook.rs` only pattern-matches Claude Code event names.
   Gemini CLI events (`BeforeTool`, `AfterTool`, `SessionEnd`) fall through to the
   generic `RecordEvent` wildcard, bypassing structured dispatch — including the
   `build_cycle_event_or_fallthrough()` path that writes `cycle_start`/`cycle_stop`.

This feature installs a normalization layer at the single correct boundary — the hook
ingest point in `hook.rs` — so that everything below that boundary operates exclusively
on canonical event names. No downstream consumer (SQL queries, detection rules,
`context_cycle_review`, `knowledge_reuse.rs`, `DomainPackRegistry`) requires change.

The feature spans four distinct layers implemented across three crates, with six files
constituting the full blast radius. Each file maps to at least one acceptance criterion.

### Architectural Principle: Canonical = Claude Code Names

Claude Code event names are used as canonical because all existing SQL, detection
rules, `query_log.rs` SQL strings, `context_cycle_review`, and `knowledge_reuse.rs`
already operate on them. Choosing neutral names (e.g., `tool_pre_invoke`) would require
changes in every downstream comparison — blast radius equivalent to the col-023
HookType refactor (ADR-004 col-023, entry #2906) with no behavioral benefit.

### Two Event Categories — Only Category 1 Is Normalized

**Category 1 — LLM Provider Hook Events**: Lifecycle events fired by LLM client hook
systems. Provider-specific strings that differ across clients. These are normalized.

**Category 2 — Unimatrix MCP Events**: Synthetic events produced when any agent calls
`context_cycle`. Already canonical by construction; never normalized. When Gemini fires
`BeforeTool` for a `context_cycle` call, that is a Category 1 event intercepting a
Category 2 intent — normalization converts `BeforeTool` → `PreToolUse` (Category 1),
then `build_cycle_event_or_fallthrough()` produces the Category 2 `cycle_start` event.
The Category 2 output is identical regardless of provider.

---

## Component Breakdown

### Layer 1: Wire Protocol Extension (`unimatrix-engine/src/wire.rs`)

**Responsibility**: Extend `HookInput` and `ImplantEvent` to carry provider identity
as an explicit wire-protocol field.

**Changes**:
- `HookInput`: add `provider: Option<String>` with `#[serde(default)]` — populated by
  the hook binary before dispatch, never sourced from the client's JSON payload.
- `HookInput`: add `mcp_context: Option<serde_json::Value>` with `#[serde(default)]`
  — deserializes Gemini's `mcp_context` field (present in `BeforeTool`/`AfterTool`
  payloads). Also captured by the existing `extra` flatten; explicit field improves
  readability and enables typed access in `build_cycle_event_or_fallthrough()`.
- `ImplantEvent`: add `provider: Option<String>` with
  `#[serde(default, skip_serializing_if = "Option::is_none")]` — propagated from
  `HookInput.provider` into each constructed `ImplantEvent` so `listener.rs` can
  derive `source_domain` without inference.

**Backward compatibility**: All new fields use `#[serde(default)]`. Existing Claude
Code hook JSON that omits these fields deserializes to `None` without error.

### Layer 2: Normalization (`unimatrix-server/src/uds/hook.rs`)

**Responsibility**: Translate provider-specific event names to canonical names and
propagate provider identity into all constructed `ImplantEvent`s. Contains the
`normalize_event_name()` function and all Gemini-specific dispatch arms.

**Changes**:

**`normalize_event_name(event: &str, provider_hint: Option<&str>) -> (&'static str, &'static str)`**

Returns `(canonical_name, provider)`. Pure synchronous function — no I/O, no
allocations (returns `&'static str` pairs).

Provider hint takes precedence. When present, the event name is mapped to its
canonical form and the hint value is used as provider:

```
provider_hint = Some("codex-cli"), event = "PreToolUse"  → ("PreToolUse", "codex-cli")
provider_hint = Some("claude-code"), event = "SessionStart" → ("SessionStart", "claude-code")
```

When absent, inference runs:
- Gemini-unique names (`BeforeTool`, `AfterTool`, `SessionEnd`) are unambiguous:
  - `"BeforeTool"` → `("PreToolUse", "gemini-cli")`
  - `"AfterTool"` → `("PostToolUse", "gemini-cli")`
  - `"SessionEnd"` → `("Stop", "gemini-cli")`
- All Claude Code names map to themselves with `"claude-code"` (backward-compatible):
  - `"PreToolUse"`, `"PostToolUse"`, `"SessionStart"`, `"Stop"`, `"TaskCompleted"`,
    `"Ping"`, `"UserPromptSubmit"`, `"PreCompact"`, `"PostToolUseFailure"`,
    `"SubagentStart"`, `"SubagentStop"` → `(same, "claude-code")`
- Unknown names → `(event, "unknown")` — passthrough for graceful degradation.

**`run()` changes**:

1. Accept an additional `--provider <name>` CLI argument. The `Hook` command variant
   in `main.rs` gains `provider: Option<String>` and passes it to `run()`.
2. Call `normalize_event_name(event, provider.as_deref())` immediately after
   `parse_hook_input()`, before `build_request()`.
3. Store normalized name and provider into `HookInput.provider` before passing to
   `build_request()`.
4. Thread `provider` into every `ImplantEvent` constructed in `build_request()` and
   its callees.

**`build_request()` Gemini arms**:

After normalization, `"BeforeTool"` arrives as `"PreToolUse"` — the existing
`"PreToolUse"` arm handles it. However, Gemini's `BeforeTool` payload places the
MCP tool name in `mcp_context.tool_name` (bare: `"context_cycle"`) rather than
`input.extra["tool_name"]` (prefixed: `"mcp__unimatrix__context_cycle"` for Claude
Code). An adapter step in the `"PreToolUse"` arm must promote this before calling
`build_cycle_event_or_fallthrough()`.

Similarly, `"AfterTool"` arrives as `"PostToolUse"`. The rework detection path MUST
be gated by `provider != "claude-code"` — Gemini's `AfterTool` covers MCP tool calls
only (filtered by the `mcp_unimatrix_.*` matcher in `.gemini/settings.json`) and must
never enter `is_rework_eligible_tool()`.

**Gemini `mcp_context.tool_name` promotion** (SR-08 mitigation):

In the `"PreToolUse"` arm, before calling `build_cycle_event_or_fallthrough()`:

```rust
// If mcp_context is present (Gemini payload), promote tool_name to top-level
// so build_cycle_event_or_fallthrough() can find it at input.extra["tool_name"].
if let Some(mcp_ctx) = input.mcp_context.as_ref()
    .and_then(|v| v.as_object())
{
    if let Some(bare_name) = mcp_ctx.get("tool_name").and_then(|v| v.as_str()) {
        // build_cycle_event_or_fallthrough reads extra["tool_name"].
        // bare name "context_cycle" matches: tool_name == "context_cycle" → allowed.
        // No synthetic prefix needed — matching logic handles bare names already.
        input_clone.extra["tool_name"] = serde_json::Value::String(bare_name.to_string());
    }
}
```

`build_cycle_event_or_fallthrough()` already contains:
```rust
if tool_name != "context_cycle" && !tool_name.contains("unimatrix") {
    return generic_record_event(...);
}
```
The bare name `"context_cycle"` satisfies `tool_name == "context_cycle"`, so the
security check passes without any prefix. No change to `build_cycle_event_or_fallthrough()`
is needed — only the promotion adapter before calling it.

### Layer 3: `source_domain` Derivation — Three Sites

**Responsibility**: Replace the three `"claude-code"` hardcodes with dynamic
derivation. Each site uses a different mechanism appropriate to its access pattern.

**Site A: `listener.rs:1894`** — live write path. Has `ImplantEvent.provider` directly:
```rust
source_domain: event.provider.clone().unwrap_or_else(|| "claude-code".to_string()),
```
This is the only site that correctly labels Gemini events as `"gemini-cli"`.

**Site B: `background.rs:1330`** — DB read path (`fetch_observation_batch()`). Has
event_type from DB but no provider. Apply registry-with-fallback (Approach A, SR-03):
```rust
let source_domain = {
    let resolved = registry.resolve_source_domain(&event_type);
    if resolved != "unknown" { resolved } else { DEFAULT_HOOK_SOURCE_DOMAIN.to_string() }
};
```
Where `DEFAULT_HOOK_SOURCE_DOMAIN: &str = "claude-code"`. This preserves existing
behavior for `"Stop"`, `"SessionStart"`, `"cycle_start"`, `"cycle_stop"` etc. which
are not in the builtin claude-code pack's `event_types` list.

**Site C: `services/observation.rs:585`** — DB read path (`parse_observation_rows()`).
The `_registry: &DomainPackRegistry` parameter already exists but is unused (prefixed
with `_`). Remove the underscore prefix and apply the same registry-with-fallback:
```rust
let source_domain = {
    let resolved = registry.resolve_source_domain(&event_type);
    if resolved != "unknown" { resolved } else { DEFAULT_HOOK_SOURCE_DOMAIN.to_string() }
};
```

**Known limitation** (OQ-4, accepted): After normalization, Gemini `"BeforeTool"`
records are stored as `"PreToolUse"` in the DB. DB read paths cannot distinguish
Claude Code from Gemini origins without a `source_domain` column — that would require
a schema migration, which is explicitly out of scope. Only Site A (write path) labels
live events correctly with `"gemini-cli"`. Sites B and C return `"claude-code"` for
stored canonical event types, which is the correct backward-compatible behavior.

### Layer 4: Reference Configurations

**Responsibility**: Provide `.gemini/settings.json` and `.codex/hooks.json` reference
files so operators can connect Gemini CLI and Codex CLI to Unimatrix without guessing
the hook registration format.

**`.gemini/settings.json`**: Registers four hook events using the `mcp_unimatrix_.*`
matcher regex that covers all 12 Unimatrix tools. The `BeforeTool` hook fires on
every Unimatrix tool call; only `context_cycle` calls are intercepted by
`build_cycle_event_or_fallthrough()` — all others fall through to generic
`RecordEvent` with canonical `"PreToolUse"`.

**`.codex/hooks.json`**: Registers four hook events identical in structure to
`.claude/settings.json`. Each invocation passes `--provider codex-cli` — this flag
is **mandatory** because Codex uses identical event names to Claude Code; without it,
events default to `"claude-code"` and Codex records are mislabeled. Config carries
a caveat that live Codex MCP hook support is blocked by Codex bug #16732.

---

## Component Interactions

```
Gemini CLI / Codex CLI / Claude Code
          │
          │ fires hook (BeforeTool / PreToolUse / etc.)
          ▼
[Layer 1: wire.rs]
  HookInput { provider: None, mcp_context: Option<Value>, ... }
          │
          ▼
[Layer 2: hook.rs run()]
  1. parse_hook_input() → HookInput
  2. normalize_event_name(raw_event, --provider flag)
     → (canonical_name: &str, provider: &str)
  3. hook_input.provider = Some(provider.to_string())
  4. (Gemini PreToolUse) promote mcp_context.tool_name → extra["tool_name"]
  5. build_request(canonical_name, &hook_input)
     → ImplantEvent { event_type: canonical, provider: Some("gemini-cli"), ... }
          │
          │ UDS frame (length-prefixed JSON)
          ▼
[listener.rs dispatch_request()]
  RecordEvent arm → write ObservationRecord
    source_domain = event.provider.unwrap_or("claude-code")   ← Site A
          │
          │ DB write: observations table, hook = canonical_name
          ▼
[DB read paths]
  background.rs fetch_observation_batch()
    source_domain = registry_with_fallback(event_type)         ← Site B
  services/observation.rs parse_observation_rows()
    source_domain = registry_with_fallback(event_type)         ← Site C
```

### Rework Detection Gate

The rework candidate path (`is_rework_eligible_tool`, `is_bash_failure`,
`extract_file_path`) is gated by `provider == "claude-code"` in the `"PostToolUse"`
arm. Gemini's `AfterTool` (canonicalized to `"PostToolUse"`) is an MCP tool call
(restricted by the `.gemini/settings.json` matcher) and must never enter rework
tracking. The provider field is the only reliable discriminator — tool names from
Gemini MCP calls are Unimatrix server tool names, not the Claude Code tool names
(`Bash`, `Edit`, `Write`, `MultiEdit`) that `is_rework_eligible_tool()` checks against.
Using provider is consistent with the feature's design intent and documents the
contract explicitly (see ADR-004).

### `post_tool_use_rework_candidate` Guard (AC-16, SR-04)

`extract_observation_fields()` in `listener.rs` already normalizes
`"post_tool_use_rework_candidate"` to `"PostToolUse"` in its match arm:
```rust
"PostToolUse" | "post_tool_use_rework_candidate" => { ... }
```
A `debug_assert!` is added before this match to verify the string has not escaped
past the normalization boundary into the `hook` column directly. This enforces the
contract without runtime cost in production. Scoped to the rework candidate string
only — `PostToolUseFailure` path is untouched (ADR-003 col-027, entry #3475).

---

## Technology Decisions

See ADR files in this directory for full rationale. Summary:

| Decision | Choice | ADR |
|----------|--------|-----|
| Canonical event name strategy | Claude Code names as canonical | ADR-001 |
| Provider identity mechanism | Explicit `provider` field on wire protocol | ADR-002 |
| Gemini mcp_context field | Named field on `HookInput` (not stringly-typed extra access) | ADR-003 |
| DB read path source_domain approach | Registry-with-`"claude-code"`-fallback (Approach A) | ADR-004 |
| Rework detection gate | `provider != "claude-code"` (not tool-name guard) | ADR-005 |
| Codex --provider flag | Mandatory in reference config, documented fallback if absent | ADR-006 |

---

## Integration Points

### Existing Components — No Changes Required

| Component | Why Unchanged |
|-----------|---------------|
| `DomainPackRegistry.resolve_source_domain()` | Operates on canonical names; Gemini names never reach it |
| `builtin_claude_code_pack()` event_types list | Canonical names are Claude Code names; list is correct |
| `query_log.rs` SQL (`AND o.hook = 'PreToolUse'`) | Gemini `BeforeTool` stored as `"PreToolUse"` — queries work |
| `context_cycle_review` tool handler | Operates on canonical `"cycle_start"`, `"cycle_stop"` — no change |
| `knowledge_reuse.rs` (`record.event_type != "PreToolUse"`) | DB values are canonical after normalization |
| `hook_type` string constants module | Constants are Claude Code names; already canonical |
| `extract_observation_fields()` match arms | Gemini canonical names map to existing arms |
| Detection rules (21 rules) | All operate on canonical names + source_domain guards |

### New CLI Interface

The `Hook` command variant in `main.rs` gains a `--provider` argument:

```rust
Hook {
    event: String,
    #[arg(long)]
    provider: Option<String>,
}
```

`hook::run(event, provider, project_dir)` signature extends to carry provider. The
provider value is validated as one of `"claude-code"`, `"gemini-cli"`, `"codex-cli"`,
or `None` (defaults to inference). Unknown values are logged and treated as `None`.

---

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|----------------|--------|
| `HookInput.provider` | `Option<String>` with `#[serde(default)]` | `unimatrix-engine/src/wire.rs` |
| `HookInput.mcp_context` | `Option<serde_json::Value>` with `#[serde(default)]` | `unimatrix-engine/src/wire.rs` |
| `ImplantEvent.provider` | `Option<String>` with `#[serde(default, skip_serializing_if = "Option::is_none")]` | `unimatrix-engine/src/wire.rs` |
| `normalize_event_name(event: &str, provider_hint: Option<&str>) -> (&'static str, &'static str)` | Pure fn, no I/O | `unimatrix-server/src/uds/hook.rs` |
| `hook::run(event: String, provider: Option<String>, project_dir: Option<PathBuf>)` | Extended signature | `unimatrix-server/src/uds/hook.rs` |
| `DEFAULT_HOOK_SOURCE_DOMAIN: &str` | `"claude-code"` | `unimatrix-server/src/uds/hook.rs` or `unimatrix-server/src/services/observation.rs` |
| `parse_observation_rows(rows, registry)` | `_registry` prefix removed; registry is now used | `unimatrix-server/src/services/observation.rs` |
| `.gemini/settings.json` | Gemini hook registration; matcher `mcp_unimatrix_.*` | repo root `.gemini/` |
| `.codex/hooks.json` | Codex hook registration; `--provider codex-cli` on each event | repo root `.codex/` |

---

## Blast Radius — Full Coverage Table

| File | Crate | Change | AC Coverage |
|------|-------|--------|-------------|
| `unimatrix-engine/src/wire.rs` | unimatrix-engine | Add `provider` to `HookInput` and `ImplantEvent`; add `mcp_context` to `HookInput` | AC-05, AC-14 |
| `unimatrix-server/src/uds/hook.rs` | unimatrix-server | `normalize_event_name()`, `run()` provider flag, Gemini arms, rework gate, mcp_context promotion | AC-01 through AC-05, AC-11, AC-12, AC-15, AC-17, AC-18 |
| `unimatrix-server/src/uds/listener.rs` | unimatrix-server | Site A source_domain (line 1894), rework candidate debug_assert | AC-06, AC-07a, AC-16 |
| `unimatrix-server/src/background.rs` | unimatrix-server | Site B source_domain (line 1330) | AC-07b |
| `unimatrix-server/src/services/observation.rs` | unimatrix-server | Site C source_domain (line 585), remove `_registry` prefix, update tests | AC-07c, AC-08 |
| `unimatrix-server/src/main.rs` | unimatrix-server | Add `--provider` to `Hook` command variant | AC-15, AC-17, AC-18 |
| `.gemini/settings.json` | (config) | New reference configuration | AC-10 |
| `.codex/hooks.json` | (config) | New reference configuration | AC-19 |

---

## Open Questions for the Spec Writer

**OQ-A: `DEFAULT_HOOK_SOURCE_DOMAIN` constant placement**
The constant should live where both `background.rs` and `services/observation.rs` can
import it without a circular dependency. Options: (1) define in `observation.rs` and
re-export, (2) define in a new `hook_constants` module in `unimatrix-server/src/`,
(3) define in `unimatrix-engine` alongside other wire constants. Recommend option 1
(co-located with the primary consumer, exported). Spec writer decides.

**OQ-B: `extract_event_topic_signal()` Gemini payload — RESOLVED (SPECIFICATION.md FR-04.7)**
ASS-049 FINDINGS-HOOKS.md confirmed `tool_input` is at the top level of Gemini's
`BeforeTool` payload — same position as Claude Code. No promotion needed for `tool_input`.
Only `tool_name` (inside `mcp_context`) requires promotion. `extract_event_topic_signal()`
requires no changes for Gemini events.

**OQ-C: Gemini `AfterTool` response field name**
`response_size` and `response_snippet` in the `"PostToolUse"` arm read from
`input.extra["tool_response"]`. Gemini may use a different field name. The scope
specifies graceful degradation (null response fields are acceptable). Spec writer
should attempt to confirm from Gemini CLI source or a live capture. If confirmed,
use it; if not, document degraded mode and null-test the fields explicitly.

**OQ-D: `extract_observation_fields()` Gemini `SubagentStop` arm behavior**
The wildcard arm handles `"SubagentStop"` and unknown events. After normalization,
Gemini produces no `"SubagentStop"` equivalent — this arm is unaffected. However,
spec writer should confirm that the wildcard arm's behavior is correct for Gemini
`"PostToolUse"` records (canonicalized `"AfterTool"`) passing through the
`"PostToolUse" | "post_tool_use_rework_candidate"` arm rather than the wildcard.
