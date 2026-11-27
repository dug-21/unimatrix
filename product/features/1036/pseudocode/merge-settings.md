# C8 — Claude hooks (`lib/merge-settings.js`)

## Purpose
`mergeSettings` and `removeOwnedHooks` compute inside `updateFileSync` (FR-20). Hook logic (Steps 2–3c, 3b) unchanged. File stays ≤ 478 lines (net-neutral refactor).

## Removed
`atomicWriteText` import from claude-json; direct `fs.existsSync/readFileSync` of the target; `crossScopeFiles` option (callers moved to C4 steps R2–R4; drop it if no caller remains — grep tests/lib first; if a test uses it, keep it implemented via `removeOwnedHooks`).

## Refactor

```
function computeMerged(raw, filePath, source) -> {text, detail:{content, commands, actions}}
  // Body = today's Step 1..3b with `rawExisting = raw` instead of reading the file.
  // subagentStopEnabled(optInFile) currently reads the file separately: replace with
  // subagentStopEnabledFromContent(content) evaluated on the parsed `raw` (same key lookup),
  // so the decision is made on the same bytes as the CAS (no second read).
  serialized = JSON.stringify(content, null, raw ? detectIndent(raw) : 2) + "\n"
  return {text: serialized === raw ? null : serialized, detail:{content, commands, actions}}
  // parse failure -> throw Error("Malformed " + filePath + ...)  (unchanged text)

function mergeSettings(filePath, commandSource, options) -> {actions, content, commands, action}
  source = normalizeCommandSource(commandSource); dryRun = !!options?.dryRun
  res = updateFileSync(filePath, raw => computeMerged(raw, filePath, source).text,
                       {dryRun, newMode:0o600, containWithin: options?.containWithin})
  d = computeMerged(res.before, filePath, source).detail
  return {actions: dryRun ? d.actions.map(a=>"[dry-run] "+a) : d.actions, content:d.content, commands:d.commands, action: res.action}
```

`subagentStopEnabledFromContent`: extract the key-reading part of `subagentStopEnabled` into a pure helper; `subagentStopEnabled(file)` (exported) becomes `read file → parse → helper` for other callers.

```
/** W1 step body (called by C4). */
function wireProjectHooks({root, platform, home, dryRun}) -> WireLeg[]
  file = join(root, ".claude", "settings.local.json")
  r = mergeSettings(file, {events:HOOK_EVENTS, commandForEvent: e => buildShimCommand(e, undefined, platform, home)},
                    {dryRun, containWithin: root})
  return r.commands.map(({command}) => ({harness:"claude-code", surface:"hooks", action:r.action, path:file, command}))
```
`r.action` from the writer replaces wire.js's deep-equal before/after compare (old code compared parsed JSON; new compares bytes — a whitespace-only difference now yields `updated` with a rewrite. Acceptable: FR-16 untouched-file rule is about files with no owned change; a reformat-only diff can only arise from hand edits). Flagged Q6.

```
function computeRemoveOwned(raw, file) -> {text, detail:{changed, actions}}
  // today's removeOwnedHooks body on `raw`; text null when no owned entry; throws "Malformed ..." on parse error

function removeOwnedHooks(file, opts) -> {actions, changed}      // signature kept
  res = updateFileSync(file, raw => computeRemoveOwned(raw,file).text, {dryRun:!!opts?.dryRun, containWithin: opts?.containWithin})
  d = computeRemoveOwned(res.before, file).detail
  return {actions:d.actions, changed:d.changed}

function removeOwnedHooksLeg(file, {dryRun, containWithin, reason}) -> WireLeg[]    // R2/R3/R4 step body
  r = removeOwnedHooks(file, {dryRun, containWithin})
  return r.changed ? [{harness:"claude-code", surface:"hooks", action:"removed", path:file, reason}] : []
```
R2 passes `containWithin: root`; R3/R4 (user/global) pass none.

Absent file: mutate returns null for `raw === null` → `unchanged`, nothing created.

## Error handling
Malformed JSON → Error propagates (exit 1, existing text). `SafeWriteError` propagates.

## Key tests
- Existing merge-settings.test.js suite green unchanged (behaviour).
- Second run: `action:"unchanged"`, mtime equal.
- SubagentStop opt-in read from same bytes (fault("after-read") flips opt-in → retry recomputes).
- Symlinked `settings.local.json` → outside root: `outside-project`; inside: written through.
- `removeOwnedHooks` foreign hooks deep-equal + order kept; indent reused.
