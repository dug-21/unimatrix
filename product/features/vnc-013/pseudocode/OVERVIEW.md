# vnc-013 Pseudocode Overview
## Canonical Event Normalization for Multi-LLM Hook Providers

---

## Components Involved

| Component | File | Why Changed |
|-----------|------|-------------|
| wire-protocol | `crates/unimatrix-engine/src/wire.rs` | New `provider` and `mcp_context` fields on `HookInput`; new `provider` field on `ImplantEvent` |
| normalization | `crates/unimatrix-server/src/uds/hook.rs` + `main.rs` | `normalize_event_name()` function; `run()` extension; Gemini dispatch arms; rework gate; `mcp_context` promotion |
| source-domain-derivation | `listener.rs` (Site A) + `background.rs` (Site B) + `services/observation.rs` (Site C) | Replace three `"claude-code"` hardcodes |
| reference-configs | `.gemini/settings.json` + `.codex/hooks.json` | New operator reference files |

---

## Data Flow

```
CLI fires hook (Gemini BeforeTool / Codex PreToolUse / Claude Code PreToolUse)
          │
          │ JSON piped to stdin
          ▼
[wire.rs]  HookInput deserialization
  - provider: Option<String>    ← #[serde(default)], populated AFTER parse by run()
  - mcp_context: Option<Value>  ← #[serde(default)], deserialized from Gemini payload
  - extra: Value (flatten)      ← catches all other fields
          │
          ▼
[hook.rs run()]
  Step 1: parse_hook_input() → HookInput (provider = None at this point)
  Step 2: two-path dispatch on provider_hint_from_cli:
            if provider.is_some() → canonical_name = map_to_canonical(raw_event)
                                    hook_input.provider = Some(hint.clone())
            else                  → (canonical_name, provider_str) = normalize_event_name(raw_event)
                                    hook_input.provider = Some(provider_str.to_string())
  Step 3: (provider already set in Step 2)
  Step 4: build_request(canonical_name, &hook_input)

[hook.rs build_request()]
  - canonical_name drives all match arms (no provider-specific strings below here)
  - "PreToolUse" arm:
      a. promote mcp_context.tool_name → extra_clone["tool_name"] if present
      b. call build_cycle_event_or_fallthrough(canonical, session_id, &input_clone)
  - "PostToolUse" arm:
      a. gate: if provider != "claude-code" → return RecordEvent (skip rework path)
      b. else: existing rework detection logic unchanged
  - All ImplantEvent constructions: include provider: Some(provider.to_string())
          │
          │ UDS frame (length-prefixed JSON)
          ▼
[listener.rs dispatch_request()]
  RecordEvent arm → extract_observation_fields(&event)
    debug_assert!(event_type != "post_tool_use_rework_candidate")  ← new guard
    source_domain = event.provider.clone().unwrap_or_else(|| "claude-code".to_string())  ← Site A
          │
          │ DB write: observations table, hook = canonical_name
          ▼
[DB read paths]
  background.rs fetch_observation_batch()
    let resolved = registry.resolve_source_domain(&event_type);    ← Site B
    source_domain = if resolved != "unknown" { resolved } else { DEFAULT_HOOK_SOURCE_DOMAIN }

  services/observation.rs parse_observation_rows()
    let resolved = registry.resolve_source_domain(&event_type);    ← Site C
    source_domain = if resolved != "unknown" { resolved } else { DEFAULT_HOOK_SOURCE_DOMAIN }
```

---

## Shared Types — New Fields

### `HookInput` (wire.rs) — two new fields

```
#[serde(default)]
pub provider: Option<String>
    Values: "claude-code" | "gemini-cli" | "codex-cli" | None
    Populated by: hook::run() after normalize_event_name(), NOT from stdin JSON
    Note: field exists in struct so run() can set it; serde(default) ensures
          existing Claude Code JSON (without this field) deserializes to None.

#[serde(default)]
pub mcp_context: Option<serde_json::Value>
    Structure when present: { "server_name": str, "tool_name": str, "url": str }
    Populated by: serde deserialization from Gemini BeforeTool/AfterTool payloads
    Also captured by extra flatten, but named field enables typed access.
```

### `ImplantEvent` (wire.rs) — one new field

```
#[serde(default, skip_serializing_if = "Option::is_none")]
pub provider: Option<String>
    Values: "claude-code" | "gemini-cli" | "codex-cli" | "unknown" | None
    Populated by: every ImplantEvent construction site in build_request() and
                  build_cycle_event_or_fallthrough()
    Used by: listener.rs Site A for source_domain derivation
    Wire behavior: None omits the field from JSON (skip_serializing_if);
                   receiver deserializes missing field as None (serde default)
```

### `DEFAULT_HOOK_SOURCE_DOMAIN` constant (observation.rs, re-exported)

```
pub const DEFAULT_HOOK_SOURCE_DOMAIN: &str = "claude-code"
    Used by: Sites B and C as fallback when registry.resolve_source_domain()
             returns "unknown" for event types not in the builtin claude-code pack.
    Placement: defined in services/observation.rs, pub(crate) re-export for background.rs.
    Rationale: OQ-A resolved as option 1 (co-located with primary consumer).
```

---

## Event Mapping (Category 1 only — normalization target)

| Input event | Canonical output | Provider (inferred) |
|-------------|-----------------|---------------------|
| `BeforeTool` | `PreToolUse` | `gemini-cli` |
| `AfterTool` | `PostToolUse` | `gemini-cli` |
| `SessionEnd` | `Stop` | `gemini-cli` |
| `SessionStart` | `SessionStart` | `claude-code` (fallback; use `--provider gemini-cli` in config) |
| `PreToolUse` | `PreToolUse` | `claude-code` (fallback) or via `--provider` |
| `PostToolUse` | `PostToolUse` | `claude-code` (fallback) or via `--provider` |
| `Stop` | `Stop` | `claude-code` (fallback) or via `--provider` |
| All other Claude Code names | same | `claude-code` |
| Unknown name | unchanged | `unknown` |

Category 2 events (`cycle_start`, `cycle_stop`, `cycle_phase_end`) are NOT normalized —
they are already canonical and produced only by `build_cycle_event_or_fallthrough()`.

---

## Unknown Event Name Resolution (lifetime constraint)

`normalize_event_name` returns `(&'static str, &'static str)`. Unknown event names are
dynamic `&str` values — they cannot be returned as `&'static str`.

Resolution: for unknown events, return `("__unknown__", "unknown")`. The
`build_request()` wildcard arm receives `"__unknown__"` as the canonical name and routes
to `generic_record_event()`, which uses `event.to_string()` — i.e. `"__unknown__"` is
stored in the DB hook column.

This is a safe degradation: `"__unknown__"` is an unrecognized event name that DB read
paths handle via the wildcard arm of `extract_observation_fields()`. The alternative
(changing the return type to `Cow<'static, str>`) would introduce allocations on the
unknown path while the common (non-unknown) path stays zero-alloc. Given that unknown
events are a degradation path, the sentinel string approach is simpler and correct.

The caller in `run()` should check: if `canonical_name == "__unknown__"`, use the
original raw event string for storage (to preserve the original unrecognized name).
This means `run()` reconstructs: if canonical is `"__unknown__"`, pass the raw event
to `build_request()` unchanged.

Alternatively: the implementation may change the return type to
`(Cow<'static, str>, &'static str)` if the implementer prefers zero sentinel strings.
Either approach is acceptable. Document the choice in the implementation.

---

## Sequencing Constraints (Wave Dependency Order)

Wave 1 (must land first — no other component depends on it being absent):
  - wire-protocol: `HookInput.provider`, `HookInput.mcp_context`, `ImplantEvent.provider`
  - All downstream components read these fields; nothing breaks if they are absent
    in the existing codebase, but they must exist before hook.rs can set them.

Wave 2 (depends on Wave 1):
  - normalization: `normalize_event_name()`, `run()` extension, Gemini arms, rework gate,
    `mcp_context` promotion, `DEFAULT_HOOK_SOURCE_DOMAIN`
  - source-domain-derivation: Sites A, B, C (can be parallel with normalization,
    but Site A reads `event.provider` which requires Wave 1 `ImplantEvent.provider`)

Wave 3 (no code dependency — config files):
  - reference-configs: `.gemini/settings.json`, `.codex/hooks.json`

The reference-configs component has no compile-time dependency on any other wave.
It can be written at any time. Recommended order: write after normalization is complete
so the command flags in the config match the actual CLI.

---

## Components NOT Changed

- `DomainPackRegistry` and `builtin_claude_code_pack()` — insulated by normalization
- `build_cycle_event_or_fallthrough()` — called after normalization, unchanged
- `extract_event_topic_signal()` — works on canonical names; `tool_input` is at
  top-level in both Claude Code and Gemini payloads (OQ-B confirmed)
- `context_cycle_review` and all detection rules — operate on canonical names
- `extract_observation_fields()` match arms — canonical names already handled;
  only the `debug_assert` guard is added (not a behavioral change)
- `hook_type` string constants — already canonical
