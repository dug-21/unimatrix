## ADR-004: PreToolUse — Narrowed Matcher + No-Send Sentinel; SubagentStop Opt-In Settings Key (Default Off)

### Context

ass-069 Q3: standalone PreToolUse observation duplicates the PostToolUse signal
(every tool call fires both — two hook-process spawns per tool); SubagentStop is
currently mandatory. But PreToolUse cannot simply be dropped: it carries the
`context_cycle` interception (`cycle_start`/`phase-end`/`stop` events with the
F-02 exact-tool-name security gate, build-request-tools.js:314-398). SR-06: the
reduction deliberately diverges from the Rust-hook event set, so the parity bar
must be split. SR-09: "optional" semantics border F5 installer territory.

### Decision

1. **Two-level PreToolUse reduction**:
   - **Install level** (`merge-settings.js`): `EVENT_MATCHERS.PreToolUse` narrows
     from `"*"` to `"context_cycle|mcp__unimatrix__context_cycle"` — the hook
     process no longer spawns for ordinary tool calls at all (the real noise win:
     one fewer process per tool invocation). PreToolUse stays in `HOOK_EVENTS`.
   - **Client level** (`build-request-tools.js`): `buildCycleEventOrFallthrough`
     returns a `null` no-send sentinel instead of `genericRecordEvent` on every
     non-cycle path — non-cycle tool name, missing `tool_input`, and failed
     `validateCycleParams` (existing stderr lines retained). index.js: a `null`
     request → return immediately (exit 0, no network, no stdout). This keeps the
     F-02 defense in depth: a regex-substring match like
     `evil_context_cycle_bypass` spawns the hook (matcher is a regex) but sends
     nothing (exact-equality gate). Only PreToolUse gets the sentinel; all other
     events' fallthrough observation is untouched.
2. **SubagentStop opt-in (OQ3, uni-zero recommendation adopted)**: removed from
   the default install set. `merge-settings.js` includes it only when
   `{root}/.claude/settings.local.json` contains
   `unimatrix.hooks.subagent_stop: true` (a durable, user-visible settings key —
   snake_case matching `unimatrix.remote.*`). Client-side handling of a
   SubagentStop event that does arrive (pre-existing installs, opted-in installs)
   is unchanged — "optional" is install-set membership, not client logic. F5 owns
   any installer UX around the key (SR-09 boundary). **Server-side independence
   (R-12)**: no server lifecycle (session close, buffer finalization) depends on
   SubagentStop — the observation builder treats it as an all-None fallthrough
   (`crates/unimatrix-server/src/uds/listener.rs:2919`,
   `"SubagentStop" | _ => (None, None, None, None)`), so default-off installs
   leak no sessions or buffers; the no-SubagentStop lifecycle test pins this.
3. **Unchanged**: sync-injection events (SessionStart, UserPromptSubmit,
   SubagentStart, PreCompact), discrete signals (PostToolUse, PostToolUseFailure),
   and lifecycle events (Stop) — `HOOK_EVENTS` and matchers identical except the
   two changes above.
4. **Parity bar split (SR-06, binding)**: transport/framing parity is full;
   event-set parity is explicitly NOT a goal. The parity corpus excludes retired
   PreToolUse observation frames and treats SubagentStop as opt-in; cycle-event
   frames remain fully parity-tested (the Rust hook emits identical cycle frames).
5. **Shared install surface caveat**: `merge-settings.js` also drives legacy Rust
   hook installs (back-compat path). The reduced set applies to any re-init of
   either client — intended: ass-069 Q3 is a product decision about the hook set,
   not a TS-client implementation detail. The Rust hook binary is untouched.

### Consequences

Easier: per-tool-call hook spawn overhead drops to zero for non-cycle tools
(matcher-level, not just client-level); event volume falls without losing any
signal PostToolUse already carries; default installs get the minimal-necessary
set by construction.

Harder: deliberate event-set divergence from the Rust oracle must be encoded in
the corpus exclusion list (ambiguity here was the vnc-026 rework driver — the
exclusions are enumerated in ARCHITECTURE.md's parity bar); Claude Code matcher
regex semantics become load-bearing for cycle interception (if a host stops
supporting regex matchers, interception silently stops — the client-side gate
still prevents wrong sends, and PostToolUse still records the tool call);
pre-existing installs keep firing PreToolUse `*` until re-init (client sentinel
makes those spawns no-ops — graceful, but the noise win awaits re-init).

Cross-references: ass-069 Q3, F-02 security gate, SR-06/SR-09, ADR-002 (sentinel
flows through index.js before transport selection matters).
