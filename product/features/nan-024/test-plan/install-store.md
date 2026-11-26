# Test Plan: install-store (`lib/install-store.js`)

Risks: R-24, R-09, R-08, R-30, R-06. File: `test/install-store.test.js` (modes tests skip on win32).

- Copy to `~/.unimatrix/pkg/<version>/`; `install.json` `{version, packageRoot, node}` written `.tmp` then renamed; abs paths; node re-recorded on init/upgrade.
- Modes (POSIX): `~/.unimatrix` 0700, `bin` 0700, `bin/unimatrix` 0700, `install.json` 0600; init under umask 002 yields these.
- Upgrade: pointer flips to new version; prune keeps current + previous only; a version held by an in-flight reader (open handle / Windows) still loads.
- Interrupted copy (kill copier mid-way via fault injection hook) -> pointer stays on previous version, no partial `install.json`.
- Two concurrent inits (spawn 2 processes) -> one consistent `install.json`, valid JSON.
- Windows: rename over open `install.json` retried on EPERM/EBUSY (inject by holding handle) -> succeeds or exits 1 with message.
- `<hash>/root` marker: one line, project root; legacy dir without `root` untouched.
- Windows `trust.json` created with `{size, mtimeMs, ino}` after full ACL check; `trust.json` itself covered (R-30).
- Shim path containing `'` (Unix) / `%` or `"` (Windows) -> init exits 1, nothing written.

## Rework (D-078, D-082)
- D-078 (Linux only, skip elsewhere): after init, `pkg/<version>/node_modules/<platform binary pkg>/` exists with the binary present and executable; `install.json.packageRoot` points inside `pkg/<version>`. Assembled test: init with fixture package whose platform binary lives in an "npx cache" dir; delete the cache dir (simulate `npx cache clean`); `resolveRuntime` still resolves the binary from `pkg/<version>/node_modules` (`binarySource:"install"`) and a literal hook command still stores an event. Resolver must not read the original package location (assert by removing it before the run).
- Upgrade: new version copies its own binary package; prune of the previous version removes its binary too; interrupted copy keeps previous binary usable.
- Platform binary package absent in source (macOS/Windows or cloud-only Linux): copy step skipped, init succeeds (cloud); Linux local with the binary package missing -> exit 1 naming the package (no silent partial copy).
- D-082: init exits 1 when the node path cannot be recorded (stub `process.execPath` unresolvable/not executable via injectable resolver): non-zero exit, stderr names the cause, `install.json` NOT swapped, prior pointer intact; no shim written pointing at nothing.
- Size measurement (Stage 3c, not an assertion): script `test/measure-pkg-size.js` prints `du -sb pkg/<version>` and the platform-binary subtree size on Linux; value goes into RISK-COVERAGE-REPORT (see OVERVIEW). Soft guard only: fail if pkg copy > a limit recorded in the report after first measurement (limit set by Stage 3c, not guessed here).

## Rework 3 (D-090)
- `trust.js` copy: init/upgrade writes `~/.unimatrix/bin/trust.js` via tmp+rename (same atomicity assertions as `install.json`): interrupted write (fault injection) leaves previous `trust.js` intact and the pointer/`install.json` unswapped-or-consistent; bytes equal package `lib/trust.js` after success; mode asserted (POSIX); trust.js is in the trust set.
- Shim path unchanged across upgrade: init v1, record shim command string, upgrade to v2 (trust.js refreshed, `pkg/` flips) -> `buildShimCommand` output and settings hook command byte-equal; no settings write occurs (merge reports no change).
