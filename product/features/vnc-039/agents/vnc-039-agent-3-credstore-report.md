# Agent Report — vnc-039-agent-3-credstore (C1 credstore.js)

## Component
C1 `lib/hook-client/credstore.js` (NEW, Scope B) — sole owner of the out-of-tree per-projectHash credential store `~/.unimatrix/<projectHash>/remote.json` (mode 0600).

## Files created
- `packages/unimatrix/lib/hook-client/credstore.js`
- `packages/unimatrix/test/credstore.test.js`

## Results
- Tests: 33 pass / 0 fail (`node --test test/credstore.test.js`). Config suite regression check: 49 pass / 0 fail.
- Size gate: PASS — stripped 85263/100000 (headroom 14737 B), raw 151186/160000 (headroom 8814 B).
- Zero-deps: PASS — package.json/package-lock.json unchanged; credstore requires only fs/os/path.

## Contract notes
- `pathFor` mirrors config.js:socketPathFor exactly (same home derivation, null-on-no-home). projectHash not sanitized (fixed-grammar derived value).
- `read` returns null on ENOENT/no-homedir; THROWS token-free on malformed JSON, non-object root, and unknown/missing schema_version (R-13). No field-completeness validation beyond schema_version (consumer-owned, preserves ENOENT-vs-incomplete).
- `write` idempotent merge at 0600 with chmod re-assert (writeRemoteSettingsLocal pattern); preserves unknown future fields; recovers over malformed existing file; throws loud on no-homedir / fs failure (R-12). fingerprint===undefined normalized to null. timeouts persisted only when a plain object.
- ACs backed by tests: AC-08 (0600, out-of-tree, structural no-network import check), AC-08b (per-projectHash idempotent, no-clobber across two hash dirs), AC-08c (one-key round-trip + single-schema-serves-both-consumers keystone). NFR-06: no token/fingerprint in any thrown message or action string.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search (pattern + decision) and context_get #5117 (ADR-003), #5118 (ADR-004) — found canonical schema, projectHash keying, reconcile-don't-port posture; applied directly.
- Stored: entry #5121 "credstore.read no-homedir returns null (ENOENT parity), but write throws loud — asymmetric posture by design" via /uni-store-pattern.

## Issues / blockers
- Minor doc inconsistency: test-plan/credstore.md names the test file `test/hook-client/credstore.test.js` while the spawn prompt specified `test/credstore.test.js`. Followed the spawn prompt (`test/credstore.test.js`). No functional impact; flag for the Delivery Leader if a relocation is desired.
- Did NOT commit (Delivery Leader commits the wave). Did NOT touch integration tests.
