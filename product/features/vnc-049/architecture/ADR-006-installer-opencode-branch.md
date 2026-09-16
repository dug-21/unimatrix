## ADR-006: Non-clobbering installer OpenCode branch (additive, retrieval regression sentinel)

### Context
AC-05: `unimatrix init` (`packages/unimatrix/lib/init.js`) today writes only `.mcp.json` +
`.claude/settings.json` — there is **no** OpenCode detection or writer (the existing
`opencode.json`/`.opencode/` were hand-authored). C18's install leg must provision the plugin without
regressing C10 retrieval: `mcp.unimatrix` and the local Ollama `provider` block in `opencode.json` are
the C17 regression sentinel (#5582, SR-08) and must survive **byte-for-byte**. nan-004's
prefix-match/non-clobber merge is the *principle* to reuse, but the surface differs — OpenCode extends
via a plugin directory + a `plugin:[]` array, not per-event `hooks` matcher groups, so the nan-004
`EVENT_MATCHERS` logic does not carry over (ass-106 §D).

### Decision
Add a **strictly additive** OpenCode branch as a new module
`packages/unimatrix/lib/opencode-install.js`, called from `init.js` with one line (ADR-005 thin
wiring):

1. **Detect** OpenCode by presence of `opencode.json` or `.opencode/`.
2. **Provision** the C1 plugin: drop the shim into `.opencode/plugins/` and add its dep to
   `.opencode/package.json` (OpenCode runs `bun install` at startup), **or** append the plugin's npm
   package to the `plugin:[]` array in `opencode.json`. Idempotent (re-running does not duplicate).
3. **Preserve** `mcp.unimatrix`, the `provider` (Ollama) block, permissions, and all non-Unimatrix
   keys unchanged. Do **not** rewrite `mcp.unimatrix` — the retrieval path (C10) is proven and is the
   sentinel.
4. **Regression test** (AC-05): after provisioning, assert `context_*` retrieval still returns and the
   `mcp.unimatrix`/local-STDIO stanza is byte-for-byte identical.

Because AC-05's entry point is the real `unimatrix init` invocation, the test asserts from that path
(not an internal merge helper): run init against a fixture OpenCode repo, then confirm both the plugin
is provisioned AND retrieval is preserved.

### Consequences
Easier: no risk to C10; the merge is simpler than settings.json (file drop / array append, no matcher
groups); init.js stays small. Harder: two provisioning modes (plugin dir vs `plugin:[]`) — delivery
picks one as primary and documents the other; `bun install` of a provisioned dep is a supply-chain
surface flagged for the C17 security review (ass-106); detection assumes hand-authored markers reliably
signal OpenCode (SCOPE assumption). Cross-references ADR-004 (plugin artifact), ADR-005 (new module).
