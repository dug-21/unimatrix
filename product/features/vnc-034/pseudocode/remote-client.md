# RemoteClient — `init --remote <bundle>` (pure JS)

> `packages/unimatrix/lib/init.js` (extend `initRemote`) + `packages/unimatrix/lib/hook-client/` (bundle decoder + cert pin). Realizes FR-B1..B9, R-05 (bundle parser), R-06 (1:1 transport), R-02 (cert pin / parity). Pure JS, zero native binary, zero added runtime deps (Node stdlib only).

## Purpose

`init --remote <bundle> [--slug <s>]` ingests the C1 bundle, pins the server cert by the C2 fingerprint, appends the (optional) slug to form the endpoint, persists client config, copies skills, and enforces the < 250 KB size gate. It mirrors the bundle decode contract from bundle-codec.md EXACTLY (guard ordering is locked) and uses the parity corpus from the Rust oracle (fingerprint-computer.md) for the pin-compute test — never a hand-written golden (SR-02).

## Context: existing surface (extend, don't replace)

The current `initRemote(options)` (init.js:287–369) takes `{remote, token}` and writes `unimatrix.remote = {url, token}` into `.claude/settings.local.json` (0600), merges hooks, and Ping-validates via `transport.pingForInit`. vnc-034 ADD: a bundle path that derives `{url, token}` (+ pinned `fp`) FROM a single `unimatrix-bundle:` blob, plus cert pinning threaded into the HTTPS request options. Keep the existing `{remote, token}` path working (backward-compat).

## New module: `lib/hook-client/bundle.js` (decoder — mirrors bundle-codec.md)

```
const BUNDLE_SCHEME = "unimatrix-bundle:";
const MAX_RAW_LEN   = 4096;                      // bytes, RAW string, BEFORE decode/parse
const TOKEN_RE      = /^[0-9a-f]{64}$/;
const FP_RE         = /^sha256:[0-9a-f]{64}$/;

function decodeBundle(raw):
    // GUARD 1 — LENGTH CAP FIRST on the RAW pasted string (Buffer.byteLength), BEFORE decode/parse.
    if Buffer.byteLength(raw, "utf8") > MAX_RAW_LEN:
        throw BundleError("bundle too long (> 4 KB) — refusing to decode")   // rejects even if not valid base64url (AC-W1-C10)

    // GUARD 2 — scheme prefix.
    if not raw.startsWith(BUNDLE_SCHEME):
        throw BundleError("not a unimatrix bundle (missing 'unimatrix-bundle:' prefix)")
    body = raw.slice(BUNDLE_SCHEME.length)

    // GUARD 3 — base64url decode (no pad). Node: Buffer.from(body, "base64url").
    let json
    try: bytes = Buffer.from(body, "base64url"); jsonStr = bytes.toString("utf8")
    catch: throw BundleError("bundle payload is not valid base64url")
    // Node base64url is lenient; re-encode round-trip check to reject smuggled chars:
    if Buffer.from(jsonStr, "utf8").toString("base64url") !== body.replace(/=+$/, ""):  // tolerate no-pad
        // (optional strictness; primary guard is the schema below)

    // GUARD 4 — JSON parse.
    try: obj = JSON.parse(jsonStr)
    catch: throw BundleError("bundle payload is not valid JSON")

    // GUARD 5 — STRICT SCHEMA (LOAD-BEARING): EXACTLY {v, base_url, token, fp}; missing/extra/typed.
    keys = Object.keys(obj)
    if keys.length !== 4 or not setEquals(keys, ["v","base_url","token","fp"]):
        throw BundleError("bundle has unexpected fields (expected exactly v, base_url, token, fp)")
    if obj.v !== 1:                              throw BundleError("unsupported bundle version: " + obj.v)
    if typeof base_url !== "string" or not base_url.startsWith("https://"):
                                                 throw BundleError("base_url must be an https URL")
    if typeof token !== "string" or not TOKEN_RE.test(token):
                                                 throw BundleError("token must be 64 lowercase hex chars")
    if typeof fp !== "string" or not FP_RE.test(fp):
                                                 throw BundleError("fp must be sha256:<64 hex>")
    return { v:1, base_url, token, fp }
```
Error messages NEVER include the token (NFR-06). Guard order is locked: GUARD 1 before 3/4; GUARD 5 is load-bearing (AC-W1-C9/C10).

## Cert pin: `lib/hook-client/cert-pin.js` (C2 / R-02)

A custom `checkServerIdentity` that pins `sha256(cert.raw)` against the bundle `fp`. CA-chain validation is bypassed (self-signed, no CA path — ADR-002). Threaded into the HTTPS request options in transport-http.js.

```
const crypto = require("crypto");

function computeFingerprint(derBuffer):                 // mirrors Rust fingerprint_leaf_der
    hex = crypto.createHash("sha256").update(derBuffer).digest("hex")   // lowercase hex
    return "sha256:" + hex

function makeCheckServerIdentity(pinnedFp):
    return function(host, cert):
        // cert.raw is the leaf DER Node presents (same bytes rustls served).
        if not cert or not cert.raw:
            return new Error("no certificate presented")
        presented = computeFingerprint(cert.raw)
        if presented !== pinnedFp:
            // CLEAN, DIAGNOSABLE mismatch (FR-A11 pairing / AC-CT-ROT): name expected vs presented,
            // point at re-bundle. This is what makes cert rotation a legible 3-step fix.
            return new Error(
              "pinned certificate fingerprint mismatch — the server cert was likely rotated.\n" +
              "  expected (pinned): " + pinnedFp + "\n" +
              "  presented (server): " + presented + "\n" +
              "  Fix: re-run `unimatrix client-bundle` on the server and re-run `init --remote <new-bundle>`.")
        return undefined        // undefined == accept (Node convention)
```

Thread into the request (transport-http.js `mod.request({...})`, line ~108) for TLS requests:
```
if isTls and config.pinnedFp:
    options.rejectUnauthorized = true                       // keep TLS errors meaningful
    options.checkServerIdentity = makeCheckServerIdentity(config.pinnedFp)
    options.ca = undefined                                  // no CA trust path; pin is the trust model
```
`config.pinnedFp` is read from `unimatrix.remote.fingerprint` in settings.local.json (added below).

## Function: initRemote (extended — bundle path)

```
async function initRemote(options):
    dryRun = options.dryRun || false

    // NEW Step 0a: bundle path vs legacy {remote, token} path.
    let remote, token, pinnedFp = null, slug = options.slug || null
    if options.bundle:
        b = decodeBundle(options.bundle)            // throws on any guard failure -> bin -> stderr + exit 1
        // Step 0b: append slug -> endpoint (ADR-005 grammar).
        endpointBase = b.base_url.replace(/\/+$/, "")
        if slug:
            assertSlugAllowlist(slug)               // ^[a-z0-9][a-z0-9-]{0,62}$  (client-side parse edge)
            remote = endpointBase + "/v1/" + slug   // -> .../v1/<slug>/tools/... at request time
        else:
            remote = endpointBase + "/v1"           // default alias -> .../v1/tools/...
        token   = b.token
        pinnedFp = b.fp
    else:
        // Legacy path (backward-compat): {remote, token} provided directly. No pin.
        remote = options.remote; token = options.token
        if not remote or not token: throw Error("--remote and --token are both required")
        validate remote is http(s) URL (existing checks)

    // Step 1: project root (existing detectProjectRoot / projectDir override).
    // Step 2: resolve installed client path (existing require.resolve hook-client/index.js).

    // Step 3: write settings.local.json unimatrix.remote {url, token, fingerprint?} (0600).
    writeRemoteSettingsLocal(projectRoot, remote, token, pinnedFp, dryRun)   // EXTENDED: add fingerprint

    // Step 3b: gitignore warning (existing).
    // Step 4: merge hooks (existing mergeSettings, full event set).
    // Step 5: copy skills (FR-B7) — remote mode DOES copy skills (copySkills); do NOT append CLAUDE.md
    //         knowledge block; print the /unimatrix-init pointer only (init.js printSummary already does).
    actions.push(...copySkills(projectRoot, dryRun))

    // Step 6: Ping validation over the PINNED TLS connection (FR-B2). The ONE loud checkpoint.
    if not dryRun:
        res = await transport.pingForInit(remote, token, /*timeouts*/ undefined, pinnedFp)
        // pingForInit threads pinnedFp into the request so a mismatch surfaces HERE, diagnosably.
        if not res.ok: throw Error("Remote validation failed: " + res.message + " ...")

    // Step 7: size gate (FR-B8 / NFR-01) — asserted as a Wave-1 acceptance TEST, not a runtime branch.
    printSummary(actions, dryRun)
```

### writeRemoteSettingsLocal (extended)

```
existing.unimatrix.remote = Object.assign({}, existing.unimatrix.remote, {
    url: remote,
    token: token,
    ...(pinnedFp ? { fingerprint: pinnedFp } : {}),
})
// still mode 0600; token-bearing file; gitignore warning unchanged.
```

## 1-client:1-project at the transport (R-06 / FR-B3/B5)

The client bakes EXACTLY ONE endpoint (`base_url + /v1[/<slug>]`) into `unimatrix.remote.url`. There is NO field, flag, or config in which a second project can be named — cross-project fan-out is UNREPRESENTABLE, not runtime-rejected (AC-W1-C5). Multi-LLM = N separate client instances each running `init --remote` against the SAME bundle+slug (N:1) — no per-LLM code path (FR-B6/AC-W1-C7). Windows is HTTPS-remote only; no local-mode code path is reachable (FR-B5).

## Attach ≠ register (FR-B4 / C5)

`init` creates NO store. If the slug is unregistered, the SERVER returns `UnknownProject` at connect; the client surfaces it as a clean error at the Step-6 Ping. The client never auto-creates a project.

## Size gate (FR-B8 / NFR-01 / R-12)

The shipped remote install (client JS + skills, no binary, no model) must be < 250 KB. Enforced as a hard acceptance test measuring the install footprint (AC-W1-C3). New code (bundle.js + cert-pin.js) is tiny and dependency-free to stay under budget.

## Parity test hook (R-02 / SR-02)

A JS test reads the committed corpus from the Rust oracle (fingerprint-computer.md): for each `GOLDEN\t<der-hex>\t<fp>`, assert `computeFingerprint(Buffer.from(derHex,"hex")) === fp`. The JS golden is NEVER hand-written.

## Error handling

| Condition | Result |
|-----------|--------|
| Any bundle guard fails (too long / bad scheme / bad base64 / bad json / schema) | `BundleError` thrown -> bin catches -> stderr + exit 1; token never in message |
| Bad `--slug` (allowlist) | thrown error at the client parse edge |
| Cert fingerprint mismatch at connect | diagnosable error naming expected vs presented + re-bundle pointer (FR-A11/AC-CT-ROT) |
| Unregistered slug | server `UnknownProject` surfaced at Ping; no store created |
| Ping auth/connect failure | loud failure; config written, fix-and-rerun message (existing behavior) |

`init` is the one loud checkpoint (throws), opposite the hook client's exit-0 posture.

## Key test scenarios (hints for tester)

- Decode round-trip vs Rust encoder fixture: `decodeBundle(serverBlob)` yields identical `{v,base_url,token,fp}` (R-05 sc.3).
- Guard ordering: over-cap raw string that is invalid base64url rejects on **length** before decode (AC-W1-C10).
- Strict schema: missing/extra/wrong-type field, `v:2`, non-https base_url, non-hex token, malformed fp -> reject, no crash (AC-W1-C9, R-05 sc.1/2).
- Cert pin: matching cert connects; changed/wrong cert rejected with diagnosable mismatch message (AC-W1-C2, R-02 sc.3, AC-CT-ROT).
- Parity: JS `computeFingerprint` == Rust oracle golden for the same DER (AC-CT-C2, R-02 sc.1).
- Slug append: `--slug myproj` -> url `.../v1/myproj`; no slug -> `.../v1`; bad slug rejected (C5).
- 1:1 unrepresentable: no client field names a second project (AC-W1-C5, R-06).
- Skills copied; CLAUDE.md block NOT appended; `/unimatrix-init` pointer printed (AC-W1-C6).
- Install footprint < 250 KB (AC-W1-C3, hard gate).
- Token absent from settings.json, hook commands, and all stdout/stderr (NFR-06).
```
