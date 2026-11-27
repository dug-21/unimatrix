# C3 — Writer API (`lib/safe-write.js`, new)

## Purpose
Public writer every installer module imports (ADR-003 #5962). Adds mutate, unchanged short-circuit, dry-run, modes, containment, fixpoint verify and the 3-attempt loop on top of C2.

## Exports (ARCHITECTURE §6)
Return type: `{action, file, target, before}` (`before` additive, Q1).

`updateFileSync`, `writeFileSync`, `SafeWriteError` (re-export from trust.js), `tempPath`, `renameWithRetry` (re-exports).

## updateFileSync

```
function updateFileSync(file, mutate, opts = {}) -> {action, file, target}
  o = {dryRun:false, newMode:0o600, enforceMode:undefined, containWithin:undefined,
       verify:true, attempts:3, retry:undefined, platform:process.platform, fault:noop, ...opts}
  lastCode = "conflict"
  for attempt in 1..o.attempts:
    r = core.resolveTarget(file)                      // throws dangling-symlink / not-regular
    core.checkContained(file, r.target, o.containWithin, o.platform)   // also in dry-run (D-026)
    st = core.readState(r.target)
    raw = st.bytes === null ? null : st.bytes.toString("utf8")
    o.fault("after-read")
    next = mutate(raw)                                // may throw (parse abort) -> propagate unchanged
    wantMode = o.enforceMode ?? (st.mode ?? o.newMode)
    if next === null or (raw !== null and next === raw):
       if raw === null: return {action:"unchanged", file, target:r.target}   // mutate declined to create
       if o.enforceMode !== undefined and st.mode !== o.enforceMode and o.platform !== "win32":
          if !o.dryRun: fs.chmodSync(r.target, o.enforceMode)
          return {action:"updated", file, target:r.target}
       return {action:"unchanged", file, target:r.target}                     // no temp, no rename, mtime kept (FR-26)
    action = raw === null ? "created" : "updated"
    if o.dryRun: return {action, file, target:r.target}                     // FR-60/62
    try
      core.hardenedWriteSync(file, next, {expect:{target:r.target, hash:st.hash}, mode:wantMode,
                             platform:o.platform, retry:o.retry, fault:o.fault, containWithin:o.containWithin})
    catch e: if e.code === "conflict": lastCode="conflict"; continue; else throw e
    o.fault("after-rename")
    if o.verify and !verifyOk(file, r, next, mutate): lastCode="verify-failed"; continue
    return {action, file, target:r.target}
  throw SafeWriteError(lastCode, file, `${file}: ${lastCode === "conflict" ? "changed concurrently" : "could not verify write"} after ${o.attempts} attempts; re-run 'npx unimatrix init'`)

function verifyOk(file, r, written, mutate) -> boolean       // FR-29
  now = core.resolveTarget(file)            // throws -> treat as false? No: propagate SafeWriteError
  if now.target !== r.target: return false
  if r.isLink and !fs.lstatSync(file).isSymbolicLink(): return false
  back = fs.readFileSync(now.target, "utf8")
  if sha256(back) !== sha256(written): return false
  return mutate(back) === null or mutate(back) === back      // fixpoint
```

Leg detail (skip reasons, key, commands, removedUserScope) must come from the raw the decision was made on, not from the verify re-invocation. Rule adopted: **`updateFileSync` additionally returns `before: string|null`** — the raw passed to the mutate on the deciding call of the successful attempt. Harness modules split their logic into a pure `compute(raw) -> {text: string|null, detail}`; the mutate is `raw => compute(raw).text`; after return the caller does `compute(res.before).detail`. Additive field on the §6 return type — flagged as Q1. In every return statement above, add `before: raw`.

## writeFileSync

```
function writeFileSync(file, data, opts={}) -> {action, file, target}
  // whole-file replace; content-equality short-circuit; byte-equal verify
  bytes = Buffer.isBuffer(data) ? data : Buffer.from(data, "utf8")
  mutate = raw => (raw !== null and Buffer.from(raw,"utf8").equals(bytes)) ? null : bytes.toString(binary-safe)
  // binary-safe: implement via a Buffer-aware internal path; updateFileSync internals operate on Buffers,
  // the utf8 string view is only what is passed to text mutates. writeFileSync uses an internal
  // _updateBuffer(file, bufMutate, opts) sharing the same loop.
  if opts.expectHash: CAS against that hash instead of the freshly read one (no CAS otherwise = still uses read hash)
  return _updateBuffer(file, mutate, {...opts, verify: true /* byte-equal only */})
```
Implementation note: make the loop operate on Buffers internally (`_updateBuffer`) and have `updateFileSync` wrap a text mutate (`buf => mutate(buf===null?null:buf.toString("utf8"))`). Bin files (`trust.js`, `shim-entry.js`) are Buffers.

## Error handling
- `SafeWriteError` from core propagates.
- Errors thrown by `mutate` propagate unchanged (e.g. `ClaudeJsonError`, "Malformed ..." Error) so existing exit-1 messages stay.
- Exhausted attempts → `SafeWriteError{code:"conflict"|"verify-failed"}`.

## Key tests (S1–S12, FR-24..FR-30)
- S1 mode kept 0644; S2/S3 created 0600 (umask 022 and 077); S4 absolute/relative/chained link: target updated, every link `readlink` unchanged; S5 dangling → `dangling-symlink`, nothing created; S6 loop; S7 dir; S8 symlinked parent dir; S9 unwritable real dir → no temp; S10 retarget between read and rename via `fault("after-read")` → retry then success / conflict after 3.
- `fault("after-read")` mutates file each time → `conflict` after 3, temp count 0.
- Mutate returning different bytes on readback → `verify-failed`.
- Unchanged: mtime equal; enforced mode drift → chmod + `updated`, second run `unchanged`.
- dryRun: no file/dir/temp created; containment still throws `outside-project`.
- `containWithin` fixtures from FR-22a (in-root link, out-of-root link, symlinked root, 2-hop leave/re-enter).
