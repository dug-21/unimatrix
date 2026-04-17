## ADR-001: Claude Code Event Names as Canonical Unimatrix Event Names

### Context

vnc-013 introduces a normalization layer that must translate provider-specific hook
event names (Gemini CLI `BeforeTool`, Codex CLI `PreToolUse`) into a single canonical
form. Two strategies exist:

**Option A — Neutral names**: Introduce new canonical strings (`tool_pre_invoke`,
`session_start`, etc.) that are provider-agnostic. Requires updating every downstream
string comparison: `query_log.rs` SQL, `context_cycle_review`, `knowledge_reuse.rs`,
`extract_observation_fields()` match arms, all 21 detection rules, test fixtures. This
is equivalent in blast radius to the col-023 HookType enum replacement (ADR-004
col-023, entry #2906), which touched ~25 files across 4 crates in a single PR.

**Option B — Claude Code names as canonical**: Use the existing Claude Code event
name strings as the Unimatrix canonical form. Gemini and Codex names map onto them.
The blast radius of the normalization layer is isolated to `hook.rs` (ingest boundary)
and the three `source_domain` hardcode sites. Nothing below the ingest boundary changes.
No DB migration. No test fixture updates. Confirmation from ASS-051 (Option A) is that
this choice is already validated by research.

The col-023 ADR-004 (entry #2906) documented the lesson: blast-radius-wide refactors
accumulate rework when any site is missed. vnc-013 must not repeat this by choosing
neutral names without commensurate justification.

### Decision

Use Claude Code event names as canonical Unimatrix event names. The normalization
table is:

| Provider Event | Canonical Name | Provider |
|----------------|----------------|----------|
| `BeforeTool` (Gemini) | `PreToolUse` | `gemini-cli` |
| `AfterTool` (Gemini) | `PostToolUse` | `gemini-cli` |
| `SessionEnd` (Gemini) | `Stop` | `gemini-cli` |
| `SessionStart` (Gemini) | `SessionStart` | `gemini-cli` |
| `PreToolUse` (Codex) | `PreToolUse` | `codex-cli` (via `--provider`) |
| `PostToolUse` (Codex) | `PostToolUse` | `codex-cli` (via `--provider`) |
| `SessionStart` (Codex) | `SessionStart` | `codex-cli` (via `--provider`) |
| `Stop` (Codex) | `Stop` | `codex-cli` (via `--provider`) |
| All Claude Code names | Themselves | `claude-code` |

The `hook_type` module string constants (`PRETOOLUSE = "PreToolUse"`, etc.) in
`unimatrix-core/src/observation.rs` remain unchanged — they are already canonical.
The `builtin_claude_code_pack()` `event_types` list in `domain/mod.rs` requires no
change. `query_log.rs` SQL strings require no change. No DB migration required.

### Consequences

Easier: downstream code requires no changes; normalization is a pure ingest concern;
new providers can be added by extending `normalize_event_name()` only.

Harder: the canonical names reflect Claude Code's naming convention, not a
provider-neutral taxonomy. If a future provider uses `PreToolUse` with fundamentally
different semantics, the canonical name would be misleading. This is acceptable because
provider identity is tracked separately via the `provider` field on `ImplantEvent`.
