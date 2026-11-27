# Test Plan — C12 install store (`lib/install-store.js`) — extend `test/install-store.test.js`

Risks: R-01, R-06, R-08, R-21, R-22. AC-3, AC-5, AC-10, S-3.

## Migration (FR-33)
- Existing retry/mode/cleanup tests re-pointed to the shared writer, green before private writers deleted.

## Legs (FR-26a)
- First install: each store file leg `created`; package-copy leg `created`.
- Second install: every store leg `unchanged`, package-copy `unchanged` (`isCompleteCopy`), no timestamp churn in `install.json`.
- Dry-run reports identical actions.

## Modes (D-025)
- Pre-chmod `install.json` 0664, `trust.json` 0644, `<hash>/root` 0644, `bin/*` 0775 → after `installStore` 0600/0600/0600/0700; mode-only repair reports `updated`.
- Already correct → `unchanged`, zero chmod (spy).
- New under umask 000/002/022 → `bin/*` 0700, others 0600. Win32 injected → no POSIX chmod path.

## Symlinks
- Symlinked `trust.json` (live/dangling) → exit 1 exact FR-20a text; link+target unchanged.
- Symlinked `install.json`/`bin/*` → written through, then trust-check refusal rewrapped with §3.7 message, exit 1.

## Package copy (R-21)
- Fault between staging and dir rename → no partial `pkg/<ver>/`; `.tmp-` prefix pruned next run; concurrent complete dest kept; `install.json` swapped only after rename.
- Rename failure per FR-20 file → no temp, exit 1 naming file.
