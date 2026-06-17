# Component 3 — Client Attach (JS)

**File:** `packages/unimatrix/lib/init.js`
**ADR:** ADR-001 (#5080) · **AC:** AC-05, AC-07 · **Risk:** R-01

## Purpose

Read finished URLs from the validated `v:2` bundle and store them VERBATIM into client config. Delete every client-side path-composition site (C-1 slug append `:305`, C-2 default append `:307`). The init-time Ping uses the bundle's `observe_url` directly. After this component the set of client path-composition sites in `init.js` is EMPTY (NFR-01 invariant).

## Modified Functions

### `resolveRemoteTarget(options)` (MODIFY — delete C-1/C-2; return both URLs)

```
// BEFORE (the bug source): decode -> endpointBase = base_url; if slug then base+"/v1/"+slug else base+"/v1"
// AFTER (dumb-client): decode -> two finished URLs, composed by NOTHING client-side.

function resolveRemoteTarget(options):
    if options.bundle:
        b = decodeBundle(options.bundle)            // throws BundleError on any guard (trust boundary)
        // ADR-001: NO endpointBase, NO "/v1" append, NO slug branch, NO assertSlugAllowlist.
        // The --slug flag is RETIRED for the bundle path.
        return { mcpUrl: b.mcp_url, observeUrl: b.observe_url, token: b.token, pinnedFp: b.fp }

    // Legacy {remote, token} path (backward-compat) — unchanged in spirit, but note:
    // legacy supplies a single endpoint; map it to BOTH fields so downstream is uniform.
    remote = options.remote; token = options.token
    if not remote or not token: throw Error("--remote and --token are both required")
    validate remote is http(s) URL (unchanged)
    // Legacy has no server-composed observe URL; preserve prior behavior by deriving observe
    // ONLY on the legacy branch (the bundle branch composes nothing). Flag: legacy is not the
    // #766 surface; keep it working but do not extend it.
    return { mcpUrl: remote, observeUrl: legacyObserveFrom(remote), token, pinnedFp: null }
```

> DESIGN NOTE (OQ-A): the bundle path is the dumb-client path — zero composition. The legacy `{remote,token}` path predates bundles and has no `observe_url`; it keeps a single local derivation so existing legacy users are not broken, but it is explicitly NOT the closed-set surface the invariant test guards (the test asserts the BUNDLE path composes nothing). If the human wants legacy retired entirely, that is a one-line deletion — flag, do not assume.

### `writeRemoteSettingsLocal(...)` (MODIFY — store both URLs verbatim)

```
// settings.local.json subtree was: unimatrix.remote = { url, token, fingerprint? }
// becomes: unimatrix.remote = { mcp_url, observe_url, token, fingerprint? }

function writeRemoteSettingsLocal(projectRoot, mcpUrl, observeUrl, token, pinnedFp, dryRun):
    existing = read-or-init settings.local.json (mode 0600), merge-preserving
    existing.unimatrix.remote = Object.assign({}, existing.unimatrix.remote, {
        mcp_url: mcpUrl,                 // stored VERBATIM (no normalization, no trailing-slash edit)
        observe_url: observeUrl,         // stored VERBATIM
        token: token,
    }, pinnedFp ? { fingerprint: pinnedFp } : {})
    write atomically at mode 0600 (unchanged mechanics)
```

### `runRemoteInit(options)` / init flow (MODIFY — Ping the bundle's observe_url)

```
target = resolveRemoteTarget(options)
mcpUrl = target.mcpUrl; observeUrl = target.observeUrl; token = target.token; pinnedFp = target.pinnedFp

writeRemoteSettingsLocal(projectRoot, mcpUrl, observeUrl, token, pinnedFp, dryRun)
merge hooks (transport reads observe_url from settings) — unchanged structure
copy skills — unchanged

// Init-time validation Ping (AC-07): post to the bundle's OBSERVE URL verbatim.
res = await transport.pingForInit(observeUrl, token, undefined, pinnedFp)   // was: pingForInit(remote, ...)
if not res.ok: present res.message (actionable; token-free)
```

## Data Flow

- IN: `options.bundle` (untrusted paste) → `decodeBundle` → `{mcp_url, observe_url, token, fp}`.
- OUT: `settings.local.json` `unimatrix.remote.{mcp_url, observe_url, token, fingerprint?}` (verbatim).
- The init Ping target is `observe_url` (server-composed `/v1/{slug}/observe`), which is the #766 fix (was `/v1/observe` 404).

## Error Handling

- Bundle guard failures → `BundleError` surfaced cleanly by init's catch; token never in message.
- Ping failure → actionable message (host unreachable / token rejected / cert pin mismatch), unchanged classifier.

## Key Test Scenarios (hints)

1. Closed-set invariant (R-01 sc.1): grep/AST assertion that `init.js` contains NO `+ "/v1"`, no `"/v1/" + slug`, no slug branch in the bundle path — the composition set is empty.
2. Verbatim store (R-01 sc.2): given a decoded bundle, `settings.local.json.unimatrix.remote.mcp_url` and `.observe_url` are byte-equal to the bundle fields.
3. AC-07 repro: drive `init --bundle <v:2>`; assert the init Ping posts to the bundle's `observe_url` and the real `/v1/{slug}/observe` returns 200 (was 404).
4. `--slug` flag is gone on the bundle path: passing it is ignored or rejected (confirm chosen behavior); the bundle URL already encodes the slug.
5. Legacy `{remote, token}` still attaches (non-regression).
