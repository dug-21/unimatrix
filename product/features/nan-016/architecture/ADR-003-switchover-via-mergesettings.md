## ADR-003: Switchover Repoints Through Shipped `mergeSettings` (Both Promote and Rollback)

### Context
The switchover must repoint this repo's hooks from the Rust binary to the installed TS
client, and roll back. A minimal approach would string-swap commands in
`.claude/settings.json`. But `lib/merge-settings.js` (shipped, frozen by C-8) already
encodes the correct, idempotent, ownership-aware merge — including the vnc-027 narrowing of
the `PreToolUse` matcher from `"*"` to `context_cycle|mcp__unimatrix__context_cycle`
(`PRETOOLUSE_CYCLE_MATCHER`). A string swap would preserve the stale `"*"` matcher and
diverge from shipped semantics; the eventual soak would not exercise what `init` produces.

### Decision
`scripts/dogfood-switchover.sh` repoints by requiring the **installed** client's
`lib/merge-settings.js` and calling `mergeSettings`:

- **promote:** `mergeSettings(settingsPath, { events: HOOK_EVENTS, commandForEvent: e =>
  buildHookClientCommand(join(clientDir,"lib/hook-client/index.js"), e) }, { dryRun })` —
  the same call shape `initRemote` uses (`lib/init.js` step 4). Emits
  `node <clientDir>/lib/hook-client/index.js <EVENT>`, recognized by the `isUnimatrixHook`
  node-client arm, so it **updates the existing Rust commands in place** (no duplicates) and
  applies shipped matcher semantics — deliberately narrowing the live `PreToolUse "*"`
  matcher (the SR-05 delta, asserted by the harness, documented in the runbook).
- **rollback:** `mergeSettings(settingsPath, "<repo>/target/release/unimatrix", {dryRun})`
  — passing a **string** triggers `normalizeCommandSource`'s legacy arm, emitting
  `LD_LIBRARY_PATH=<binDir> <binary> hook <EVENT>` over `HOOK_EVENTS`, byte-identical to the
  pre-F5 local form. Rollback is the same mechanism in reverse — no bespoke revert logic to
  drift.

Requiring `mergeSettings` from the **installed** copy (not the in-repo one) additionally
validates that the frozen tree's merge logic is runnable. (OQ-1c resolved.)

### Consequences
Easier: idempotent, ownership-scoped re-points (foreign hooks preserved, duplicates removed)
for free; promote and rollback share one battle-tested code path; the soak exercises shipped
`init`-equivalent settings, including the intended matcher narrowing.
Harder: the matcher narrowing is a real behavioral delta the operator must be told about
(runbook AC-04, SR-05); `SubagentStop` is opt-in, so the registered event set on a fresh
scratch root is 8, not 9 — the harness asserts the actual set rather than assuming.

Related: ADR-001/ADR-002 (provide the installed `mergeSettings` and the client path),
ADR-005 (how this is verified by effect). Frozen contract per C-8.
