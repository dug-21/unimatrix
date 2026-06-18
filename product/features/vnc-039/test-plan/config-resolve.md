# Test Plan — C5 hook-client `resolve()` repoint + `okHttp` `pinnedFp`

> Scope **B** · `[no-cloud]` · MODIFIED `lib/hook-client/config.js` (file-mode `resolve()` → out-of-tree store; `okHttp` gains `pinnedFp`).
> **The current-break fix.** Risks: **R-06 (Critical), R-13, R-15**, contributes R-07/R-08. ACs: **AC-08c, AC-08d**.
> Cumulative: **extend `test/hook-client/config.test.js`** (reuse `makeProject`, `computeProjectHash`, `project-hash-goldens.json`); add a `writeRemoteStore(projectHash, cred)` helper mirroring `writeLocalSettings`; reuse the LIVE `https.createServer` + `computeFingerprint` recipe from `cert-pin-tls.test.js` for AC-08d. Override `os.homedir()` to a temp root.

## Behavior under test (ARCHITECTURE §4.4, ADR-004)
File-mode `resolve()` is repointed from `<root>/.claude/settings.local.json` to `~/.unimatrix/<projectHash>/remote.json`. It reads the canonical schema (`observe_url`+`fingerprint`, **not** `url`), returns `okHttp(observe_url, token, timeouts, "file", …)` **plus `pinnedFp: fingerprint`**. Precedence preserved exactly: env-pair (unpinned) → store file → UDS fall-through. **The break being fixed:** today the read keys on the never-written `url` and never reads `fingerprint`, so file-mode remote observe silently falls through to UDS and would run unpinned.

> **Binding rule (R-06, lesson #4970):** asserting `config.pinnedFp` is populated is **necessary but NOT sufficient**. AC-08d MUST prove the observe POST actually transits a pinned HTTPS connection to a LOCAL `https.createServer`. A `pinnedFp`-is-set shape check would have passed vnc-034's dead-pin false-green.

---

## AC-08c — canonical-read regression: `observe_url` + populated `pinnedFp` (R-06)
- `test_resolve_fileMode_returnsObserveUrlNotUrl` — seed `~/.unimatrix/<projectHash>/remote.json` with the canonical schema; assert `resolve()` returns `observe_url` (not `url`) as the post target.
- `test_resolve_fileMode_populatesPinnedFpFromFingerprint` — **the load-bearing regression assertion**: assert the resolved config's `pinnedFp` equals the store's `fingerprint` (the field that was never read before).
- `test_resolve_fileMode_readsTokenAndTimeouts` — token resolved; `timeouts` applied when present, `DEFAULT_TIMEOUTS` when absent.
- `test_resolve_oldUrlKeyNeverRead` — a store with NO `url` key still resolves (the hook client never reads `url`); a stray `url` key is ignored.
- `test_resolve_precedencePreserved_envPairWinsUnpinned` — `UNIMATRIX_REMOTE_URL`/`_TOKEN` env pair still wins outright and stays unpinned (legacy override unchanged).

## AC-08d — file-mode remote observe ACTUALLY runs over pinned HTTPS (R-06 — the break-fix proof)
**LIVE wire test against a LOCAL pinned `https.createServer` (self-signed, known leaf fp via `computeFingerprint`; capturing `observedAuth[]`). This is the formal test for the file-mode remote-observe error boundary (ARCHITECTURE §9 cross-refs AC-08d).**
- `test_observe_fileMode_goodPin_postsToObserveUrlOverPinnedHttps` — stand up a local pinned `https.createServer`; compute its `fp`; seed the store with that `observe_url`/`fingerprint`/token; drive a hook event through file-mode `resolve()` + the observe POST; assert the request **lands on the HTTPS server** (not UDS), targets `observe_url`, and the connection was pinned (it connects on good pin — it could NOT before; it fell through to UDS).
- `test_observe_fileMode_wrongPin_failOpenExit0_noTokenOnWire` — seed a WRONG `fingerprint`; assert the observe POST fails connect-class → **fail-open exit 0** (unchanged observe posture, distinct from the bridge's fail-loud) AND the capturing server received **no `Authorization`/no token** (token never on the wire).
- `test_observe_fileMode_noUdsFallthrough_withValidRemoteCred` — with a valid file-mode remote credential present, assert observe does **NOT** silently fall through to UDS (the current break) — it targets `observe_url` over HTTPS.
- **Gate rule:** field-presence of `pinnedFp` is necessary but not sufficient; this section proves the request transits pinned HTTPS (R-06 coverage requirement). Faithful-port of the old break (UDS-fallthrough / unpinned) is a gate failure.

## Both-consumers-one-schema (R-07, AC-08c — integration with C1/C2)
- `test_resolve_and_bridge_readSameStoreFile` — seed ONE `remote.json`; assert the hook client (C5) reads `observe_url`/token/`fingerprint`/`timeouts` AND (cross-ref mcp-bridge plan) the bridge reads `mcp_url`/token/`fingerprint` from the **same** file — no per-consumer dialect. (C5 side asserts the hook fields here.)
- `test_resolve_keyedByProjectHash_roundTrip` — write store for project P via `computeProjectHash(projectRoot)`; assert `resolve()` reads it back keyed by the SAME derivation (one-key, R-07); a different project's hash → ENOENT → UDS fall-through, not a crash.

## R-13 — store read-error posture for the hook client
| Input | Expected (hook posture) | Test |
|-------|-------------------------|------|
| ENOENT | UDS fall-through (exit 0) | `test_resolve_storeEnoent_udsFallthrough` |
| Malformed JSON | terminal `malformed` (not UDS) | `test_resolve_storeMalformed_terminalMalformed` |
| Unknown `schema_version` | terminal diagnosable (not silent skip) | `test_resolve_unknownSchemaVersion_terminal` |
| Incomplete (missing `observe_url`) | defined fall-through, not unpinned silent run | `test_resolve_incompleteEntry_definedPosture` |

> Note the posture asymmetry vs the bridge: the **hook client** UDS-falls-through on ENOENT (fail-open) but terminal-`malformed` on parse error; the **bridge** fails loud non-zero on both ENOENT and malformed (mcp-bridge plan). Same `credstore.read` throw, different consumer mapping — asserted on each side.

## R-15 — legacy `fingerprint: null` stays unpinned (Low, AC-08c)
- `test_resolve_nullFingerprint_resolvesUnpinned` — seed a legacy entry with `fingerprint:null`; assert `resolve()` returns the config with `pinnedFp` **unset** → observe posts unpinned (preserves today's legacy behavior); **no crash on null**.
- `test_resolve_presentFingerprint_pinned` — a bundle entry (`fingerprint` present) DOES populate `pinnedFp` (pin-or-not is driven by the value, not by pin-or-fail-on-null).

## `okHttp` shape (AC-08c)
- `test_okHttp_gainsPinnedFpField` — assert `okHttp(...)` resolved config now carries a `pinnedFp` field sourced from `fingerprint`, threaded through to `transport-http.post` via `config.pinnedFp` (the minimal change that makes the observe path pinned — ARCHITECTURE §4.4).

## Scope B independence (R-08, AC-11)
- The whole file runs with **no cloud reachable and `mcp-bridge.js` never required**. The AC-08d `https.createServer` is a LOCAL pinned server (not the cloud) — preserves `[no-cloud]`. B-without-A: hook-client resolution + observe pinning tested without invoking the bridge.
