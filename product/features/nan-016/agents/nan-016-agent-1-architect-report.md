# nan-016 Architect Report

## Artifacts
- product/features/nan-016/architecture/ARCHITECTURE.md
- product/features/nan-016/architecture/ADR-001-copy-install-npm-pack.md (#4924)
- product/features/nan-016/architecture/ADR-002-fixed-install-dir.md (#4925)
- product/features/nan-016/architecture/ADR-003-switchover-via-mergesettings.md (#4926)
- product/features/nan-016/architecture/ADR-004-no-daemon-lifecycle.md (#4927)
- product/features/nan-016/architecture/ADR-005-effect-verification-scratch-harness.md (#4928)

## Key Decisions
1. Copy-install via `npm pack` + extract, clean-replace, never `npm link` (ADR-001). Tarball honors `files` array, excludes the platform binary (client never needs it); postinstall copied-but-not-run.
2. Fixed install dir `~/.unimatrix/dogfood-client/` (ADR-002) — npm global prefix is node-version-pinned and breaks on container rebuild.
3. Switchover repoints through the shipped `mergeSettings` for both promote (node-client commandSource) and rollback (legacy string arm → Rust binary) (ADR-003). Inherits idempotency, ownership scoping, and the intended PreToolUse matcher narrowing.
4. No daemon lifecycle management; rely on client fail-open (ADR-004, C-7).
5. Verify by effect against a scratch project root + re-fired hook; never touch live settings (ADR-005). Non-vacuous AC-02/AC-03 proofs.

## Risk Coverage
SR-01/02/03 → ADR-001 (complete frozen tree, clean-replace, byte-stable). SR-04 → ADR-005 (real `mergeSettings` + re-fired hook, not string-diff). SR-05 → ADR-003 (matcher delta asserted + documented). SR-06 → ADR-005 (scratch-only, tmpdir guard, live read-only). SR-07 → ADR-005 AC-03 (code-freeze + non-symlink, not state separation; #4923). SR-08 → ADR-004 + ADR-005 (re-fired daemon-absent exit-0). SR-09 → C-8 freeze, no `lib/` edits, AC-06 gate untouched.

## Open Questions
1. AC-03 isolation proof should edit a throwaway copy of the tree (or re-pack to a second scratch install), never the working tree. Pin in pseudocode.
2. Harness should run `dogfood-install.sh --target <tmp>` in a `before` hook (test-scoped temp, not the real dogfood dir).
3. `scripts/` shell-wrapping-Node vs a single Node CLI — team preference; architecture unaffected.
4. Deferred live-flip + F6 soak-clock-start follow-up issue (#682 checklist or new issue) — for the human; nan-016 does not create it.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- returned prior init/merge ADRs (#1199, #1200/#1201 ADR-003/004); plus targeted context_get on #4923 (project-root-hash shared-state pattern) and #1200 (init-is-JS). Applied #4923 as the load-bearing fact for shared-state and code-freeze framing.
- Stored: #4924 ADR-001, #4925 ADR-002, #4926 ADR-003, #4927 ADR-004, #4928 ADR-005 via context_store (decision/nan-016). No prior ADR superseded (init/merge-settings ADRs remain valid, frozen by C-8). No typed edges asserted — intra-feature Prerequisite spine deferred to retro per the default-no-edge / HIGH-bar convention.
