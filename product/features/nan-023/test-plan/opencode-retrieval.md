# Test Plan — opencode Retrieval Writer (`writeOpencodeMcp`, `lib/opencode-install.js`)

> Component: new `writeOpencodeMcp(dir, {binaryPath?, url?}, dryRun) -> WireLeg` (ADR-001)
> Runner: `node --test` · Test file: extend `test/opencode-install.test.js` (cumulative — reuse `sentinelConfig()`, `makeTempProject`, `writeOpencodeJson`)
> Primary risk: **R-08** (vnc-049 sentinel regression). Secondary: R-11 (malformed), R-14 (dry-run/containment).
> ACs: AC-05 (mcp.unimatrix ensured, sentinel byte-for-byte, retrieval returns), AC-04 (idempotence), AC-11 (intent — new-entry gate).
> Constraint C-05: `mcp.unimatrix` + Ollama `provider` block survive byte-for-byte.

The existing `sentinelConfig()` (mcp.unimatrix + Ollama `provider` + `permission`) is the canonical
regression fixture. The writer is **additive only** and preserves indent via `detectIndent`. The live
`context_*` return leg (AC-05/AC-10) is a **Stage 3c integration concern** exercised by the C14
verifier — see `c14-verifier.md`; this component plan covers the byte-preservation + WireLeg contract.

## vnc-049 sentinel preservation (R-08 — first-class, C-05)

- `test_writeOpencodeMcp_preserves_ollama_provider_block` — run against `sentinelConfig()` fixture that already has `mcp.unimatrix` → byte-diff the `provider.ollama` region = empty; foreign `permission` keys unchanged (SR-06, #5743).
- `test_writeOpencodeMcp_preserves_existing_mcp_unimatrix` — existing `mcp.unimatrix` present → not clobbered; if value already correct, `action:"unchanged"`.
- `test_writeOpencodeMcp_preserves_foreign_keys_and_indent` — foreign top-level keys + a 4-space-indented file → foreign keys byte-identical; new/updated content uses `detectIndent` output (indent preserved).

## Fresh additive write (AC-05)

- `test_writeOpencodeMcp_fresh_creates_mcp_unimatrix` — opencode fixture with **no** `mcp.unimatrix` → additive write creates it; foreign keys preserved; WireLeg `{harness:"opencode", surface:"retrieval", action:"created", path, entry}` returned with the exact entry.
- `test_writeOpencodeMcp_entry_shape_local_vs_cloud` — `binaryPath` (local) vs `url` (cloud) → `entry` carries the correct transport shape for each.

## Idempotence (AC-04, R-04-class)

- `test_writeOpencodeMcp_run_twice_byte_identical` — run twice → byte-diff of `opencode.json` between runs = empty; second run `action:"unchanged"`.

## Malformed / fail-safe (R-11, AC-15)

- `test_writeOpencodeMcp_malformed_json_skips_and_preserves` — invalid JSON `opencode.json` → `action:"skipped-malformed"`, input byte-preserved, **no throw**, no partial write; distinct from `skipped-undetected` (uses existing `readJsonSafe.malformed`).

## Intent gate (AC-11, R-12) — writer-visible portion

- `test_writeOpencodeMcp_new_entry_without_intent_skipped` — new `mcp.unimatrix` where none exists and no opt-in → `action:"skipped-intent"` with a `reason`/help line, no write. (The `--harness` opt-in arm is driven end-to-end in `cli-routing.md`.)
- `test_writeOpencodeMcp_additive_into_existing_no_optin_needed` — merging into a surface the user already maintains proceeds without opt-in.

## Containment (AC-12)

- `test_writeOpencodeMcp_within_project_guard` — target escaping resolved root → `skipped`, never followed (`isWithinProject`).

## Coverage note

Byte-for-byte sentinel diff (R-08) + WireLeg contract asserted here; **retrieval-still-returns** (the load-bearing AC-05/AC-10 opencode arm) is executed from the manifest by the C14 verifier, never discharged by config presence.
