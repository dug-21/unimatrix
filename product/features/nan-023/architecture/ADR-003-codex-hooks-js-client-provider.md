## ADR-003: Codex Hooks Target the JS Hook Client via a New `--provider` Argv Hint; Emitted Event Set

### Context

SCOPE mandates the package write Codex hooks in a Claude-like, event-driven shape whose command
targets the **JS hook client** (`packages/unimatrix/lib/hook-client/`), **not** the `unimatrix`
binary, with `--provider codex-cli` on every command (AC-07/AC-08, SR-10). The existing hand-authored
`.codex/hooks.json` reference (vnc-013 ADR-006) targets the **binary** (`unimatrix hook <EVENT>
--provider codex-cli`) — the binary's Rust `main.rs` parses `--provider`.

The blocking discovery: the **JS hook client does not parse `--provider`.** `index.js:343-351` reads
only `process.argv[2]` as the raw event and then *infers* the provider via
`normalize.normalizeEventName(rawEvent)` (comment: "no --provider, F3"). Codex shares event names with
Claude (`PreToolUse`, `PostToolUse`, `SessionStart`, `Stop`), so inference returns `"claude-code"` —
every codex event routed through the JS client would be **silently mislabeled** `claude-code`
(exactly SR-10; the same defect vnc-013 ADR-006 prevents for the binary). Config presence would be
green while attribution is wrong — the ceremonial-wiring trap (SR-09).

`normalize.js` already has the machinery: `KNOWN_PROVIDERS` includes `"codex-cli"`, and a `--provider`
**hint** path already exists (used for `opencode`). What is missing is argv parsing in `index.js` to
carry the hint into that path. Two open SCOPE questions are resolved here: the hook target/attribution
mechanism, and the exact event set.

### Decision

**1. Teach the JS hook client a `--provider <name>` argv hint.**
Add `parseHookArgs(argv) -> {event, providerHint}` to `index.js`: `event = argv[2]`; scan remaining
args for `--provider <name>` (and `--provider=<name>`). When a hint is present, use the
`normalize.js` **hint path** (as opencode does) to stamp `provider = <hint>` instead of the inference
path; when absent, behavior is byte-identical to today (claude-code inference — backward compat, SR-07).
An unknown hint (not in `KNOWN_PROVIDERS`) is ignored and falls back to inference (fail-open, never
throws — the client's exit-0 contract). This is the **only** JS-client behavior change.

**2. The codex hook writer emits JS-hook-client commands.**
`writeCodexHooks(dir, {clientPath, dryRun})` writes `.codex/hooks.json` (JSON, same matcher-group
shape as `.claude/settings.json`). Each command is
`buildHookClientCommand(clientPath, event, "codex-cli")` →
`node <clientPath> <EVENT> --provider codex-cli`. `buildHookClientCommand` gains an optional 3rd
`providerHint` arg that appends `" --provider <hint>"`. The existing `UNIMATRIX_PATTERNS` ownership
regex (pattern 5) already matches this command form, so re-runs are idempotent and non-clobbering
(AC-04); foreign hook entries are preserved via `isUnimatrixHook` scoping. `--provider codex-cli` is
**mandatory on every command** — a missing flag is a fail-loud defect (SR-10, vnc-013 ADR-006), not a
silent degrade.

**3. Emitted event set (resolves the open question).** Mirror the Claude managed events the JS client
+ server actually process AND that Codex fires, matching the vetted reference config:

| Event | Matcher | Rationale |
|---|---|---|
| `SessionStart` | (none) | session lifecycle |
| `UserPromptSubmit` | (none) | prompt capture |
| `PreToolUse` | `^context_cycle$\|^mcp__unimatrix__context_cycle$` | cycle interception only (narrowed, vnc-027) |
| `PostToolUse` | `*` | rework/observation |
| `PreCompact` | (none) | compaction boundary |
| `SubagentStart` | `*` | subagent lifecycle |
| `Stop` | (none) | session end |

Excluded: `PostToolUseFailure` (Claude-specific arm) and `SubagentStop` (opt-in even for Claude) — not
emitted for codex. #16732 is fixed upstream, so this set fires (not wired-but-inactive).

**4. Attribution boundary (in scope vs not).** `--provider codex-cli` stamps
`ImplantEvent.provider="codex-cli"`. Deeper `source_domain` resolution is **out of scope** (SCOPE
non-goal) — the hook ingress still forces `DEFAULT_HOOK_SOURCE_DOMAIN="claude-code"` (Unimatrix
#5737). AC-09 firing is therefore "the wired codex hook event reaches the JS hook client stamped
`provider=codex-cli`", not "source_domain=codex-cli". This is stated so the firing assertion is not
over-claimed.

### Consequences

Easier: codex hooks route through the same audited JS client as claude (one hook runtime, not two);
attribution is correct by construction and asserted against the written command (AC-08); the reference
config's binary form is superseded by the package-written JS-client form. Harder: `index.js` is under
the hook-client **byte size gate** (110 KB stripped / 200 KB raw; current raw 189,946 B — ~10 KB
headroom) — the `--provider` parse must be lean, trim comment prose, and **never** raise the gate
(Unimatrix #5372, lesson #4780); the split-brain rule (#5737) means any change to provider-hint
handling must keep `index.js`/`normalize.js` in step with the Rust oracle, guarded by the parity
corpus (#4751); the `source_domain="claude-code"` forcing remains a truthful limitation to document so
firing is not mis-sold as full per-provider domain attribution. Cross-refs ADR-001 (leg returns the
exact `command`), ADR-006 (verifier fires that command), vnc-013 ADR-006 (`--provider` mandate).
