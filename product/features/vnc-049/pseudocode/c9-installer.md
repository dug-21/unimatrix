# C9 — Installer OpenCode branch

**Location:** `packages/unimatrix/lib/init.js` (one delegating call) + **new module**
`packages/unimatrix/lib/opencode-install.js`.
**ADRs:** ADR-006 (non-clobbering additive branch), ADR-005 (new-module-thin-wiring).
**Risks:** R-08 (retrieval regression), R-09 (idempotence), R-15 (untrusted file input).
**Wave:** 4 (provisions the C1 plugin artifact; needs C1 to exist). Independent of Rust waves.
**Dep:** C17 (#5582) — reuse nan-004 (#1201) prefix-match non-clobber *principle* (surface differs).

## Purpose

Make `unimatrix init` detect OpenCode and provision the C1 plugin without regressing C10 retrieval.
The delta is strictly additive: `mcp.unimatrix`, the local STDIO command, the Ollama `provider` block,
permissions, and all non-Unimatrix keys are preserved byte-for-byte (AC-05/SR-08).

## init.js edit (thin wiring, ADR-005)

```
// after existing .mcp.json / .claude/settings.json writers:
if (detectsOpenCode(projectDir)) {
    await require("./opencode-install").provisionOpenCode(projectDir);   // one call
}
```

## New module `opencode-install.js`

```
function detectsOpenCode(dir):
    return exists(join(dir,"opencode.json")) || exists(join(dir,".opencode"))

async function provisionOpenCode(dir):
    provisionPlugin(dir)          // primary mode (delivery picks one, documents the other)
    // preserve everything else — do NOT touch mcp.unimatrix or provider block
```

### Provisioning modes (ADR-006 §2 — delivery picks primary, documents alternate)
- **Mode A (plugin dir + package dep):** drop the C1 shim into `.opencode/plugins/`; add its dep to
  `.opencode/package.json` (OpenCode runs `bun install` at startup).
- **Mode B (`plugin:[]` append):** append the plugin's npm package name to the `plugin:[]` array in
  `opencode.json`.

### Idempotent, non-clobbering merge (NFR-06, R-09)
```
function appendPluginEntry(config, entry):
    config.plugin = config.plugin ?? []
    if !config.plugin.includes(entry):        // prefix-match / dedupe (nan-004 principle)
        config.plugin.push(entry)
    // else: no-op — re-run produces no duplicate

function preserveWrite(path, mutatedConfig):
    // write back preserving key order + all non-Unimatrix keys; never rewrite mcp.unimatrix
    // or the provider block. Serialize only the additive change.
```
- **Byte-for-byte preservation (R-08):** `mcp.unimatrix`, local STDIO command, Ollama `provider` block,
  permissions, non-Unimatrix keys — untouched. Do NOT re-serialize the whole file in a way that reorders
  or reformats preserved stanzas; mutate minimally.
- **Idempotence:** re-running init adds no duplicate plugin/package entry and drifts no preserved key.

## Data flow

IN: user's `opencode.json` / `.opencode/` (untrusted, R-15) + the C1 plugin artifact. OUT: plugin
provisioned; retrieval config preserved. Entry point is the real `unimatrix init` (AC-05 asserts from
this path, not an internal merge helper).

## Error handling

- No OpenCode detected → no-op (branch skipped).
- Malformed `opencode.json` → fail safe: warn + skip provisioning, do NOT corrupt the file or clobber
  retrieval.
- **R-15:** do not follow injected paths from user config; do not execute untrusted content; validate
  the target is within the project dir before writing.

## Key test scenarios (hints for tester)

1. **Provision + preserve** (R-08.1-3/AC-05): init on a fixture OpenCode repo provisions the plugin
   (`.opencode/plugins/` + dep and/or `plugin:[]` append) AND `mcp.unimatrix` + STDIO command + provider
   block + non-Unimatrix keys are byte-for-byte identical.
2. **Retrieval regression** (NFR-03/AC-05c): after provisioning, `context_*` retrieval still returns.
3. **Idempotence** (R-09/NFR-06): run init twice → no duplicate plugin/package entry, no key drift.
4. **Pre-existing partial plugin array** (edge): appending is a no-op if entry already present.
5. **Malformed config** (R-15): fail safe, no clobber.
6. Entry-point assertion drives real `unimatrix init`, not the merge helper (AC-05 verification method).
</content>
