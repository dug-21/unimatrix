# Agent Report: vnc-013-agent-1-pseudocode

**Agent ID**: vnc-013-agent-1-pseudocode
**Feature**: vnc-013 — Canonical Event Normalization for Multi-LLM Hook Providers
**Stage**: Session 2 Stage 3a (pseudocode)
**Status**: Complete

---

## Files Produced

- `product/features/vnc-013/pseudocode/OVERVIEW.md`
- `product/features/vnc-013/pseudocode/wire-protocol.md`
- `product/features/vnc-013/pseudocode/normalization.md`
- `product/features/vnc-013/pseudocode/source-domain-derivation.md`
- `product/features/vnc-013/pseudocode/reference-configs.md`

---

## Components Covered

1. **wire-protocol** (`wire.rs`): Two new fields on `HookInput`, one new field on `ImplantEvent`. Pure struct extension. Serde ordering constraint documented (named field before flatten).

2. **normalization** (`hook.rs` + `main.rs`): `normalize_event_name()` function with `(&'static str, &'static str)` return type resolved via static sentinel `"__unknown__"`. No defense-in-depth arms in `build_request()`. Provider-hint path factored out to caller. `mcp_context` promotion adapter. Rework gate. Provider propagation to all `ImplantEvent` constructions. `--provider` CLI argument.

3. **source-domain-derivation** (`listener.rs` Site A + `background.rs` Site B + `services/observation.rs` Site C): All three `"claude-code"` hardcodes replaced. `debug_assert!` guard in `extract_observation_fields()`. `DEFAULT_HOOK_SOURCE_DOMAIN` constant defined in `observation.rs`. `_registry` prefix removal documented.

4. **reference-configs** (`.gemini/settings.json` + `.codex/hooks.json`): JSON structures, matcher regex, `--provider codex-cli` mandatory flag, caveat text requirements.

---

## Open Questions / Gaps Flagged

### OQ-1: Site A ambiguity in listener.rs

The architecture says "Site A: listener.rs:1894 — live write path. Has ImplantEvent.provider directly." However, reading the source at that offset, the code is in `content_based_attribution_fallback()`, which is a DB READ path (builds `ObservationRecord` from SQL), not the live write path. The live write path (`dispatch_request()` → `extract_observation_fields()`) uses `ObservationRow` struct.

The implementer must locate:
a) Where `ObservationRow.source_domain` is set in the live write path (possibly inside `extract_observation_fields()` or `insert_observation()` — the struct definition needs to be read to confirm)
b) Whether `content_based_attribution_fallback()` at line 1894 also has a `source_domain` hardcode

Both locations require replacement. Pseudocode covers both cases. The implementer must grep for `source_domain` in `listener.rs` to find all hardcode sites.

### OQ-2: `HookInput: Clone` requirement

The `mcp_context` promotion adapter needs to clone `HookInput` when `mcp_context.tool_name` is present. The current `HookInput` struct does NOT derive `Clone` (reading the source, no `#[derive(Clone)]` is visible). The implementer must either:
- Add `#[derive(Clone)]` to `HookInput` (straightforward)
- Or restructure the promotion to avoid cloning (pass fields individually to `build_cycle_event_or_fallthrough`)

Recommended: add `#[derive(Clone)]` to `HookInput`. It is a simple data struct.

### OQ-3: Gemini tool name format in matcher regex

The `.gemini/settings.json` matcher `mcp_unimatrix_.*` is specified in architecture.
However, Gemini CLI may use `mcp__unimatrix__.*` (double underscores, same as Claude Code)
or `mcp_unimatrix_.*` (single underscore). The implementer must verify the exact format
from ASS-049 FINDINGS-HOOKS.md before writing the config. The regex must match
all 12 Unimatrix tool names in the format Gemini actually uses.

### OQ-4: normalize_event_name hint path return value

The function signature returns `(&'static str, &'static str)` but when `provider_hint`
is `Some`, the provider string is dynamic. The pseudocode resolves this by having the
caller (run()) use the hint directly and only call normalize_event_name for the
inference case. The function still handles `Some` defensively (returning a static
sentinel). The implementer must follow the caller-pattern described in normalization.md
to keep the return type honest.

### OQ-5: content_based_attribution_fallback registry availability

`content_based_attribution_fallback()` in `listener.rs` does not currently take a
registry parameter. If applying Approach A there (which the pseudocode recommends),
the implementer must either pass the registry to this function or use
`DEFAULT_HOOK_SOURCE_DOMAIN` directly as a shortcut (since this is a secondary
attribution path and correctness degradation is acceptable).

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — 13 results. Entries #4305 (ADR-001 canonical event strategy), #4298 (hook-normalization-boundary pattern), #4306 (ADR-002 provider field) directly applicable. Entry #4298 confirms mcp_context promotion as highest-risk integration point.
- Queried: `mcp__unimatrix__context_search` (pattern, hook normalization provider wire protocol) — entries #4298, #763, #319 returned.
- Queried: `mcp__unimatrix__context_search` (decision, vnc-013) — entries #4306, #4310, #4308 returned. All vnc-013 ADRs confirmed in Unimatrix.
- Deviations from established patterns: none. The hook-normalization-at-boundary pattern (entry #4298) is followed exactly. Approach A (entry #4308) is implemented as specified. The rework-gate design follows ADR-005 (entry #4309). The `debug_assert` canary placement follows R-02 and AC-16 in RISK-TEST-STRATEGY.
