## ADR-004: One canonical credential-store schema; reconcile the current unpinned/UDS-fallback break instead of porting it (SR-07)

### Context

The relocation (ADR-003) rewrites the exact load/store pair that today carries a **pre-existing schema mismatch** (SR-07, entry #5107):

- **Writer** (`writeRemoteSettingsLocal`, `init.js:230-240`) emits `unimatrix.remote.{mcp_url, observe_url, token, fingerprint}`.
- **Reader** (hook client `resolve()`, `config.js:289-306`) reads `unimatrix.remote.{url, token, timeouts}` and **never reads `fingerprint`**.

**This is a CURRENT break, not a latent one.** Today, in shipped code, the file-mode remote observe path reads `unimatrix.remote.url` — a key the writer **never writes** (it writes `mcp_url`/`observe_url`) — and **never reads `fingerprint`** at all. The concrete present-tense consequences: (a) the reader's guard requires `remote.url`, which is absent, so the file-mode branch **silently fails its guard and falls back to local UDS** right now — file-mode remote observe effectively does not run; (b) `config.pinnedFp` is **never populated** on the file path, so were the guard ever satisfied, the bearer would be POSTed **unpinned over the wire today**. Both effects are active in the current code, not hypothetical future regressions. SR-07 (High/High) is explicit: land ONE coherent schema, the hook client must newly read `observe_url`+`fingerprint` (not `url`), and fix this present break rather than faithfully port it.

### Decision

Define **one canonical `remote.json` schema** that both consumers read (no per-consumer dialect):

```json
{
  "schema_version": 1,
  "mcp_url": "https://host/v1/<slug>",
  "observe_url": "https://host/v1/<slug>/observe",
  "token": "<64 hex>",
  "fingerprint": "sha256:<64 hex>",   // or null on the legacy/unpinned path
  "timeouts": { "connect_ms": 750, "sync_ms": 2000, "fnf_ms": 3000 }  // optional
}
```

- `schema_version` future-proofs the store: an unknown version is a **terminal, diagnosable** read failure (the hook client's `malformed` terminal; the bridge exits non-zero) — never a silent skip.
- **`observe_url` replaces `url`.** The hook client's file-mode branch reads `observe_url` as its post target — this is the schema fix; the broken `url` key is gone.
- **`fingerprint` is read by both consumers.** The hook client newly threads it into the resolved config as `pinnedFp` (via a new field on `okHttp`, `config.js:203-216`), so `transport-http.post`'s existing pinned-flush actually engages — this fixes the current break: file-mode remote observe both resolves (it no longer falls through to UDS on the missing `url` key) and POSTs genuinely pinned on the bundle path. The bridge reads `fingerprint` for its own pin (ADR-001). `fingerprint: null` (legacy path) leaves the hook client unpinned, preserving today's unpinned-legacy behavior without regression.

**Post-fix validation is behavioral, not field-presence.** It is NOT sufficient to assert that `config.pinnedFp` is populated. Validation MUST confirm that **file-mode remote observe actually runs over pinned HTTPS** end to end: with a `remote.json` carrying a `fingerprint`, the observe POST (a) resolves to the http file path (does **not** fall through to UDS), (b) targets `observe_url`, and (c) flushes the bearer only after the leaf fingerprint matches — a good-pin POST round-trips, a wrong-pin POST is rejected with the token never reaching the wire. Asserting `pinnedFp` is set is a necessary precondition but does not prove the path actually pins; the test must exercise the wire behavior. (This is the SR-02 lesson from vnc-034's dead-pin false-green — a shape assertion passed three gates while the pin was dead.)
- `timeouts` optional; absent → `DEFAULT_TIMEOUTS` via the existing `mergeTimeouts` (`config.js:135`).
- Field ownership: `mcp_url` → bridge only; `observe_url`+`timeouts` → hook client only; `token`+`fingerprint` → both.

**Reader repointing (preserves precedence exactly):** the hook client `resolve()` order stays (1) env pair `UNIMATRIX_REMOTE_URL`/`_TOKEN` → http (unchanged, wins outright, remains unpinned by design); (2) **store file** `~/.unimatrix/<projectHash>/remote.json` (was `<root>/.claude/settings.local.json`) → read canonical schema → `okHttp(observe_url, token, timeouts, "file", …)` **with `pinnedFp: fingerprint`**; ENOENT/incomplete → UDS fall-through; non-ENOENT parse / unknown schema_version → terminal `malformed`; (3) UDS fall-through (unchanged). The env-pair override and UDS fall-through semantics are preserved.

**`.mcp.json` write contract (token-free):** the remote `.mcp.json` entry written by `init` is `{command:"node", args:[<bridge module path>, <projectHash>], env:{}}` — **no token, no `mcp_url`, no `fp`** (AC-09). The bridge loads everything from the store at spawn time keyed by the `projectHash` argument. The write is idempotent, merge-preserving (preserves co-resident MCP servers), `--dry-run`-aware, and throws on malformed `.mcp.json` — mirroring `writeMcpJson` (`init.js:59-104`, SR-09).

A single `credstore` module (`lib/hook-client/credstore.js`) owns `write(projectHash, cred, {dryRun})`, `read(projectHash)`, `pathFor(projectHash)` — the sole owner of path, schema, and 0600 enforcement, so neither consumer hand-rolls store access.

### Consequences

**Easier:** the current unpinned/UDS-fallback break (file-mode remote observe silently not running today) is fixed in the same change that touches it — and the fix is verified behaviorally, not by field presence (the observe path resolves to http and becomes actually pinned on bundle credentials); both consumers share one schema and one accessor module, so a future field add is one place; `.mcp.json` is committable with zero secret exposure (no token on the command line or in any tracked file — AC-09).

**Harder:** the hook client gains a `pinnedFp` plumb on the file path (small, additive); legacy credentials carry `fingerprint: null` and must be tolerated (the schema permits null, the pin stays off — a deliberate carve, not an oversight); the store accessor becomes a load-bearing shared module both consumers depend on (single point — but that is the point, vs. two drifting readers).

Related: ADR-003 (this feature, where the schema lives + the key), ADR-001 (this feature, bridge reads `mcp_url`/`fingerprint`), ADR-005 (this feature, B lands first), entry #5107 (reconcile-don't-port; the documented mismatch), `transport-http.js:117` (the `config.pinnedFp` consumer this fix finally feeds).
