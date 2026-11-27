# Test Plan — C3 writer API (`lib/safe-write.js`) — new `test/safe-write.test.js`

Risks: R-01, R-02, R-03, R-13, R-22, R-24. Supports AC-3, AC-7, AC-10; does not discharge them.

## Results
- Absent file, mutate returns string → `{action:"created"}`, mode `newMode` (default 0600).
- mutate returns `null` → `unchanged`, no temp, no rename, mtime equal.
- mutate returns equal bytes → `unchanged` (FR-26).
- `dryRun:true` → returns planned action, no file/dir/temp/chmod; absent parent dir stays absent.

## Symlink cases S1–S12 (FR-23)
- Absolute, relative, 2-hop link → still link, `readlink` unchanged, target holds bytes.
- Dangling → `SafeWriteError{code:"dangling-symlink", file:link}`.
- Link to dir / dest is dir → `not-regular`.
- `containWithin:R`: target inside R → written; outside → `outside-project` naming link and target; prefix sibling `/p` vs `/p-x` → outside; R itself a symlink → realpath resolved first, inside; parent-dir link `.claude -> /out` → outside; escape-and-return chain → decided by final realpath (inside → written); injected win32/darwin case-different target → inside.
- `fault("before-rename")` retargets link outside R → `outside-project`, outside file bytes unchanged.
- Without `containWithin`, link outside home followed (AC-3 user/global).

## CAS / verify (AC-7 support)
- `fault("before-rename")` mutates once → attempt 2 succeeds, both changes present.
- mutates every attempt → `conflict` after 3 attempts.
- `fault("after-rename")` strips content so mutate(readBack) !== null → `verify-failed`.
- Non-idempotent mutate (appends each call) → `verify-failed` (R-02 guard).

## Temp (AC-10, R-22)
- Spy `fs.openSync`: flags `"wx"`; basename matches `/^\.[^/]+\.tmp-[0-9a-f]{16}$/`; temp in real target's dir.
- Pre-created colliding temp → regenerated; 5 collisions → `io`.
- Failure injected at open/write/fsync/rename/CAS/verify and throwing mutate → no `.*.tmp-*` in dir.

## Modes (R-01)
- `enforceMode:0o600` existing 0644 → 0600, `updated`; already 0600 + equal bytes → `unchanged`, zero chmod calls.
- No `enforceMode`, existing 0664 changed → stays 0664.
- chmod EPERM injected on enforced file → `io` naming file.

## Platform (R-24)
- Injected win32: rename retry ×10 on EPERM/EBUSY/EACCES; dir fsync skipped; POSIX: dir open throwing → write still succeeds.

## Static
- Source contains no `async`, `await`, `.then(`, `setTimeout` (NFR-03); ≤300 code lines.
