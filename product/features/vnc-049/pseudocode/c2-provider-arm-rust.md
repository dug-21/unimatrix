# C2 — Provider normalization arm (Rust)

**Location:** `unimatrix-server/src/uds/hook.rs` (804 code-lines, over cap — minimal wiring only) +
**new module** `unimatrix-server/src/uds/hook/opencode.rs`.
**ADRs:** ADR-004 (provider arm), ADR-005 (new-module-thin-wiring). **Risks:** R-03 (split-brain), R-15.
**Wave:** 2. **col-022:** lands together with C3 (normalize.js) + C8 (parity corpus).

## Purpose

Recognize `"opencode"` as a known provider and canonicalize OpenCode-shaped event names to the 7
canonical names, stamping `provider="opencode"` and carrying `model_id`. The heavy mapping table lives
in the new `hook/opencode.rs`; `hook.rs` gains only a const entry + delegating arm (ADR-005).

## hook.rs edits (minimal wiring)

### 1. `KNOWN_PROVIDERS` (`:158`)
```
const KNOWN_PROVIDERS: &[&str] = &["claude-code", "gemini-cli", "codex-cli", "opencode"];
```
This alone makes the hint path (`--provider opencode`) accepted instead of warned-and-dropped (`:159-168`).

### 2. `hook::run()` model_id plumbing (`:133`, ADR-002)
The subcommand gains a `model: Option<String>` param (from `--model` CLI arg, C1):
```
pub fn run(event, provider, model, project_dir) -> Result<()>:
    ...
    hook_input = parse_hook_input(stdin)
    # existing provider normalization (hint vs inference path) unchanged
    # NEW: set model_id from the CLI arg, validated (C4 charset)
    hook_input.model_id = model.filter(|m| opencode::is_valid_model_id(m))
    ... construct ImplantEvent, copying hook_input.model_id → event.model_id (every site)
```
- The **hint path** (provider supplied) is what OpenCode uses: `--provider opencode` present, so
  `map_to_canonical(event)` handles the name. OpenCode already emits canonical names via C1, so
  `map_to_canonical` needs an opencode-aware delegating arm only for any opencode-unique aliases.

### 3. Delegating arm in `map_to_canonical` / `normalize_event_name` (`:71-126`)
OpenCode's C1 shim emits **already-canonical** names (SessionStart, UserPromptSubmit, PreToolUse,
PostToolUse, SubagentStart, PreCompact, Stop) — so the existing canonical arms already match. The new
module exists to (a) own any opencode-specific alias table and (b) keep the provider-inference honest:
```
# in normalize_event_name (inference path, --provider absent) — opencode never hits this because
# C1 always passes --provider opencode (hint path). Add a delegating guard for safety/parity only:
if opencode::is_opencode_event_alias(event):
    return (opencode::canonicalize(event), "opencode")
```
Keep the delta in hook.rs to the const + one delegating call (ADR-005 over-cap rule).

## New module `uds/hook/opencode.rs` (ships at/under cap)

```
//! OpenCode event-name canonicalization + carrier validation (vnc-049 C2, ADR-004/005).

/// Map an opencode event name to canonical. C1 emits canonical names already, so this is
/// identity for the 7 canonical names + any opencode alias. Returns "__unknown__" sentinel
/// for unrecognized (caller preserves raw string, matching hook.rs convention).
pub fn canonicalize(event: &str) -> &'static str:
    match event:
        "SessionStart"|"UserPromptSubmit"|"PreToolUse"|"PostToolUse"
        |"SubagentStart"|"PreCompact"|"Stop" => <same literal>
        # (no opencode-unique aliases known today; table exists for future + parity symmetry)
        _ => "__unknown__"

/// True if `event` is an opencode-specific alias needing remap (currently none).
pub fn is_opencode_event_alias(event: &str) -> bool

/// model_id carrier charset (C4): "<providerID>/<modelID>", allows '/'.
pub fn is_valid_model_id(s: &str) -> bool:
    1..=128 chars, each in [a-z0-9._/-], non-empty
```
- **Rationale for the module even though names are canonical (ADR-005 + #5737):** onboarding a harness
  is three coupled touchpoints; the module is the Rust anchor the parity corpus (C8) and JS mirror (C3)
  assert against, and the home for `is_valid_model_id` + future opencode aliases. It keeps hook.rs
  additions to wiring lines and gives C8 a single-source contract (#5302).

## Data flow / transformations

IN: CLI `hook <EVENT> --provider opencode --model <id>` + stdin HookInput JSON (from C1).
OUT: `HookInput { provider:Some("opencode"), model_id:Some(...) }` → `ImplantEvent` (C4) → C5.

## Error handling

- Unknown `--provider` still dropped-to-None+warn (existing gate) — now `opencode` is allowlisted so it
  is NOT dropped.
- Unknown event name → `"__unknown__"` sentinel, caller substitutes raw string (existing convention).
- Invalid model_id → dropped to None + warn (fail-open, R-15). Never blocks the session (NFR-02).
- No `.unwrap()`, no panics in non-test code.

## Scheduled decomposition (ADR-005)

Record `hook.rs` (over-cap) in the delivery-phase decomposition issue with a test plan; do NOT carve
mid-delivery. New `hook/opencode.rs` ships under cap.

## Key test scenarios (hints for tester)

1. **`"opencode"` ∈ KNOWN_PROVIDERS** (R-03.3, AC-02a) — unit assertion.
2. **provider on stored record** = `"opencode"` (R-03.4, AC-02c) — via assembled path.
3. `canonicalize` returns each of the 7 canonical names unchanged; unknown → `"__unknown__"`.
4. `is_valid_model_id` accepts `"ollama/qwen3-coder"`, rejects illegal/oversized (R-15).
5. **Parity** deferred to C8: Rust `canonicalize` output == JS `normalize.js` opencode arm for all 7 events.
</content>
