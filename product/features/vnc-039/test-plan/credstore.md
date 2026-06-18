# Test Plan — C1 `credstore.js` (out-of-tree credential store)

> Scope **B** · `[no-cloud]` · NEW file `lib/hook-client/credstore.js` · sole owner of `~/.unimatrix/<projectHash>/remote.json`.
> Risks: **R-07, R-12, R-13**, contributes R-08/R-09. ACs: **AC-08b, AC-08c** (write/read half), supports AC-08/08d.
> New test file: `test/hook-client/credstore.test.js`. Cumulative: reuse `makeProject()` + `computeProjectHash` from the `config` suite; temp-`HOME` override pattern from `init-remote.test.js`.

## Unit under test (signatures — ARCHITECTURE §8)
- `credstore.pathFor(projectHash) → string | null` (null on no-homedir)
- `credstore.read(projectHash) → {schema_version, mcp_url, observe_url, token, fingerprint, timeouts?} | null` (null on ENOENT; **throws** on malformed/unknown version)
- `credstore.write(projectHash, cred, {dryRun}) → string[]` (actions; mode 0600; idempotent merge)

## Test harness
- Override `os.homedir()` to a per-test temp dir (or set `HOME`/`USERPROFILE`) so the store lands under a temp root — never the real home. Restore in `afterEach`.
- Seed via `credstore.write` for round-trip tests; seed via raw `fs.writeFileSync` for malformed/version tests.

## Path derivation (R-07)
- `test_pathFor_validHash_returnsHomeUnimatrixHashRemoteJson` — assert `pathFor(h)` === `<home>/.unimatrix/<h>/remote.json`.
- `test_pathFor_noHomedir_returnsNull` — stub homedir to null/empty → returns `null` (same posture as `socketPathFor`).
- `test_pathFor_colocatedWithSocket` — assert the dir equals the dir `socketPathFor` uses (one per-project root; ADR-003 colocation).

## Write — mode 0600, idempotent merge, dry-run (R-12, AC-08b)
- `test_write_creates0600File` — `write(h, cred)` → `fs.statSync(...).mode & 0o777 === 0o600`.
- `test_write_persistsCanonicalSchema` — written JSON has `schema_version:1`, `mcp_url`, `observe_url`, `token`, `fingerprint`, and `timeouts` only when provided.
- `test_write_idempotent_sameCredNoDuplicateNoGrowth` — write twice with same cred → single coherent file, identical content, mode re-asserted 0600.
- `test_write_update_overwritesEntryForSameHash` — write cred A then cred B for the same hash → file holds B (re-attach update).
- `test_write_dryRun_noFileWritten` — `write(h, cred, {dryRun:true})` → returns intended actions, **no file on disk**, no dir created.
- `test_write_returnsActionStrings` — actions array describes the write (for `printSummary`); assert **token absent** from every action string (R-09).
- `test_write_reassertsModeOnExistingFile` — pre-create the file at 0644, write → mode forced back to 0600 (the `writeRemoteSettingsLocal` chmod pattern).

## Per-project separation (AC-08b, R-07)
- `test_write_twoProjects_twoDistinctHashDirs` — write for projectHash A and B → two distinct `~/.unimatrix/<hash>/remote.json` files.
- `test_write_reinitProjectA_doesNotTouchProjectB` — re-write A → A updated, B byte-unchanged (directory separation, not in-file keying).

## Read — happy path + one-key round-trip (R-07, AC-08c)
- `test_read_afterWrite_roundTripsCanonicalSchema` — `write` then `read` for the same hash → all fields equal (the **one-derivation round-trip**: write-key and read-key are both `computeProjectHash`).
- `test_read_returnsTimeoutsWhenPresent_absentWhenOmitted` — `timeouts` round-trips; omitted → field absent (consumer applies `DEFAULT_TIMEOUTS`).
- `test_read_wrongHash_returnsNull` — read with a different `projectHash` → `null` (ENOENT), not a throw (defined fall-through).

## Read-error posture matrix (R-13)
| Input | Expected | Test |
|-------|----------|------|
| ENOENT (no file) | `null` | `test_read_enoent_returnsNull` |
| Malformed JSON | **throws** (diagnosable, token-free) | `test_read_malformedJson_throws` |
| Unknown `schema_version` (e.g. 999) | **throws** terminal diagnosable | `test_read_unknownSchemaVersion_throws` |
| Missing `schema_version` | **throws** (not a silent default) | `test_read_missingSchemaVersion_throws` |

- `test_read_throwMessage_tokenFree` — seed a malformed file containing a token-shaped string in a bad position; assert the thrown message does **not** contain the token (R-09).
- Note: credstore.read throws on malformed/unknown for BOTH consumers; the *consumer* maps that throw to its posture (bridge: non-zero exit; hook client: terminal `malformed`) — those mappings are tested in the mcp-bridge and config-resolve plans, not here.

## Both-consumers-one-schema keystone (R-07, AC-08c — integration)
- `test_read_oneFile_servesBridgeFieldsAndHookFields` — write ONE `remote.json`; assert the object exposes **both** the bridge's fields (`mcp_url`, `token`, `fingerprint`) and the hook client's fields (`observe_url`, `token`, `fingerprint`, `timeouts`) — proving no per-consumer dialect. (The live wiring of each consumer to this file is asserted in the mcp-bridge / config-resolve plans; this asserts the single-schema contract at the store.)

## Scope B independence (R-08, AC-11)
- The whole file runs with **no cloud reachable and `mcp-bridge.js` never required**. Assert (lint/structural) that `credstore.test.js` imports neither `mcp-bridge.js` nor any network module beyond what the store itself uses (`fs`/`path`/`os`).

## Edge cases
- No-homedir → `pathFor` null → `write`/`read` surface the terminal posture (not a silent skip).
- Concurrent re-write of the same hash leaves a single coherent 0600 file (no partial token-bearing temp left behind — R-12; if a tmp-then-rename is used, assert no `.tmp` lingers on success and none in-tree ever).
