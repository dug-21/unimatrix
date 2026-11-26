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
