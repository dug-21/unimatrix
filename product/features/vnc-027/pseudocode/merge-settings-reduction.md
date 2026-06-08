# Component: merge-settings-reduction (`lib/merge-settings.js`)

ADR-004 §1,§2,§5. FR-28, FR-29, AC-08. Risks R-11 (s1), R-12. Merge step 4.
Existing source: merge-settings.js (read in full).

## Purpose

Two install-level changes: narrow the PreToolUse matcher from `"*"` to the cycle
tools so the hook process no longer spawns for ordinary tool calls; and make
SubagentStop opt-in via a durable settings key (default off). All other events and
matchers are unchanged. This install surface also drives legacy Rust-hook re-inits
(shared surface — intended, ADR-004 §5).

## Constants

```
SUBAGENT_STOP_KEY_PATH = ["unimatrix", "hooks", "subagent_stop"]   // settings.local.json
PRETOOLUSE_CYCLE_MATCHER = "context_cycle|mcp__unimatrix__context_cycle"   // ADR-004 §1
```

## Modified: `EVENT_MATCHERS.PreToolUse` (line 42-52)

```
EVENT_MATCHERS = {
  SessionStart: "", Stop: "", UserPromptSubmit: "",
  PreToolUse: "context_cycle|mcp__unimatrix__context_cycle",   // CHANGED: was "*"
  PostToolUse: "*", PostToolUseFailure: "*",
  PreCompact: "", SubagentStart: "*", SubagentStop: "*",
}
```

This is the real noise win (ADR-004 §1): one fewer hook-process spawn per non-cycle
tool invocation. PreToolUse STAYS in `HOOK_EVENTS`. Claude Code regex-matcher
semantics become load-bearing for cycle interception (R-11) — the client-side
exact-equality sentinel (build-request-sentinel.md) is the defense-in-depth backstop.

## Modified: SubagentStop opt-in (ADR-004 §2, FR-28)

`HOOK_EVENTS` keeps SubagentStop in the table, but the event is registered into
settings.json ONLY when the opt-in key is set true. Resolve the key from the project
root's `settings.local.json` and filter the event list.

```
// NEW helper — read the opt-in key, fail-open (non-boolean / missing / unreadable → false)
FUNCTION subagentStopEnabled(projectRoot):
  filePath = join(projectRoot, ".claude", "settings.local.json")
  TRY: parsed = JSON.parse(readFileSync(filePath, "utf8"))
  CATCH: RETURN false
  v = parsed?.unimatrix?.hooks?.subagent_stop
  RETURN v === true            // non-boolean values treated as unset (AC-08 matrix)
```

`mergeSettings` (line 125) must learn which events to register. The matcher map stays
static; the event LIST is filtered:

```
// inside mergeSettings, after normalizeCommandSource(commandSource):
events = source.events
IF events includes "SubagentStop" AND NOT subagentStopEnabled(projectRoot):
    events = events WITHOUT "SubagentStop"     // omit from the install set
// ... existing per-event merge loop iterates `events` ...
```

### Wiring `projectRoot` into mergeSettings

`mergeSettings(filePath, commandSource, options)` derives the settings dir from
`filePath` already (`path.dirname(filePath)` is `{root}/.claude`). Resolve the opt-in
file as a sibling of `filePath`:

```
optInFile = join(dirname(filePath), "settings.local.json")   // sibling of settings.json
```

Use `optInFile` in `subagentStopEnabled` instead of recomputing the project root —
keeps the function pure to its inputs and avoids a second root walk. (Adjust the
helper signature to take the file path directly.)

### Removal on opt-out (idempotency, ADR-004 §5)

When SubagentStop is NOT in the resolved `events`, the merge loop simply does not add
it. To make opt-out actually remove a previously-registered entry, the existing
`isUnimatrixHook` dedup logic should prune a Unimatrix SubagentStop entry that is
present in settings.json but absent from the resolved event set. Minimal approach:
after the merge loop, for any HOOK_EVENTS event NOT in `events`, strip Unimatrix-owned
entries from that event's matcher groups (reuse `isUnimatrixHook`). This keeps the
on/off matrix bidirectional and idempotent (AC-08).

## NOT changed (ADR-004 §3, R-11 s1)

- `HOOK_EVENTS` membership (PreToolUse and SubagentStop both stay in the table).
- All other matchers: SessionStart/Stop/UserPromptSubmit/PreCompact `""`,
  PostToolUse/PostToolUseFailure/SubagentStart `"*"`.
- `UNIMATRIX_PATTERNS`, `buildHookClientCommand`, `normalizeCommandSource`,
  `isUnimatrixHook` ownership logic, the malformed-JSON throw, dry-run handling.
- Legacy local-binary back-compat path (string commandSource) — the reduced set
  applies to Rust-hook re-inits too (ADR-004 §5, intended).

## Server-side independence note (R-12, ADR-004 §2)

No server lifecycle depends on SubagentStop (listener.rs:2919 all-None fallthrough).
Default-off installs leak no sessions/buffers. This file does not change server code;
the no-SubagentStop lifecycle test (parity/integration) pins the contract.

## Data flow

`init`/merge invocation → `mergeSettings(filePath, commandSource, options)` →
resolve event set (filter SubagentStop by opt-in) → per-event matcher-group merge →
write settings.json. PreToolUse entries now carry the cycle matcher.

## Error handling

`subagentStopEnabled` fail-open: unreadable/missing/malformed settings.local.json or
non-boolean value → false (default off). The existing malformed-settings.json throw
(for the MAIN settings.json being merged) is retained — that is a loud init error,
not a hook-runtime path.

## Key test scenarios (hints for tester)

1. Output snapshot: PreToolUse matcher is exactly
   `context_cycle|mcp__unimatrix__context_cycle`; all other events' matchers
   unchanged — R-11 s1, AC-08.
2. Opt-in matrix: key absent → SubagentStop NOT in generated settings; key `true` →
   registered with matcher `*`; non-boolean value → treated as unset (omitted) —
   AC-08, R-12 s2 (security: type confusion).
3. Opt-out removal: a previously-registered Unimatrix SubagentStop entry is stripped
   when the key is unset on re-init (idempotent) — AC-08.
4. Shared surface: both node-client and legacy local-binary command shapes get the
   reduced set — ADR-004 §5.
5. Sync-injection + PostToolUse/PostToolUseFailure + lifecycle entries untouched —
   AC-08.
