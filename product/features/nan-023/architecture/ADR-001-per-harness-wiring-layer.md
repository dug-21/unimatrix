## ADR-001: Per-Harness Wiring Layer — One Non-Clobbering Writer per (Harness × Surface), Returning a Structured Wire Manifest

### Context

`unimatrix init` wires each harness's config in a different native file and format (Unimatrix
#5762): claude-code `.mcp.json`+`.claude/settings.json` (JSON, wired), opencode `opencode.json`
`mcp.unimatrix` + `.opencode/plugins/` (JSON; retrieval **not** wired), codex-cli `.codex/config.toml`
(TOML) + `.codex/hooks.json` (JSON; **not** wired at all). Today the logic is split across `init.js`
(claude MCP/skills) and `opencode-install.js` (opencode plugin), with no shared shape and no wire-only
path. nan-023 must add opencode retrieval and full codex wiring, provide a `wire` verb, and — the
load-bearing constraint (SR-09/#5762) — prove parity by asserting a `context_*` call **returns** and a
hook **fires** *from the exact command that was wired*, not from a config-presence check.

`mergeSettings` (prefix-match ownership, additive merge, dedup, dry-run) and `opencode-install.js`
(detect → parse-safe → additive → indent-preserving write → warn-and-skip → containment guard) are
the reusable **principle**; the **surface** differs per harness, so their bodies do not carry over
directly (vnc-049 ADR-006 established this).

### Decision

Introduce a per-harness **wiring layer**: a thin orchestrator `lib/wire.js` plus one non-clobbering
writer per `(harness × surface)`. Each writer follows the shared principle and **returns a structured
`WireLeg`** rather than only log lines.

1. **Orchestrator** `wire(projectRoot, {harness?, clientPath, binaryPath?, mcp?, dryRun})`:
   detect harnesses by project-local markers → apply the intent gate (ADR-005) → dispatch the
   writers for detected/selected harnesses → aggregate every `WireLeg` into a **manifest**. Returns
   `{ actions: string[], manifest: WireLeg[] }`.
2. **Writers** (reuse where they exist, add where they do not):
   - claude-code MCP → reuse `writeMcpJson`; claude-code hooks → reuse `mergeSettings`.
   - opencode plugin → reuse `maybeProvisionOpenCode`; opencode retrieval → **new** `writeOpencodeMcp`
     (additive `mcp.unimatrix`, preserving the Ollama `provider` sentinel + all foreign keys
     byte-for-byte — vnc-049, SR-06).
   - codex → **new** `lib/codex-install.js` (`writeCodexMcpToml` ADR-002 + `writeCodexHooks` ADR-003).
3. **`WireLeg`** = `{ harness, surface, action, path, command?, entry?, reason? }` where `action ∈
   {created, updated, unchanged, skipped-undetected, skipped-malformed, skipped-intent}`. For hooks,
   `command` is the **exact** string written; for MCP/retrieval, `entry` is the exact entry.
4. **The manifest is the verifier's and the dry-run printer's single source of truth** — the C14
   verifier (ADR-006) executes `manifest[].command` / connects via `manifest[].entry`, so the
   assertion runs against what was written, not a reconstruction. `--dry-run` prints from the same
   manifest (AC-14). A skipped leg is a visibly-reported `WireLeg`, never a silent pass (SR-08).
5. **Fail-safe posture**: every wire-layer writer warns-and-skips on malformed input and never throws
   (AC-15), mirroring `opencode-install.js`. Each is containment-guarded by `isWithinProject` (AC-12).
   The pre-existing claude `init` throw-on-malformed checkpoints are unchanged (backward compat, SR-07).

### Consequences

Easier: parity assertions become non-tautological (SR-09) — one manifest drives verify, dry-run, and
summary; a new harness = one writer + one dispatch line; claude-code writers are reused untouched so
the byte-for-byte golden (SR-07) stays trivial; `init.js` stops accreting harness logic (modularity).
Harder: writers must return structured legs, not just log — a uniform `WireLeg` contract all writers
honor; two error postures now coexist in the codebase (init's loud claude checkpoints vs the wire
layer's warn-and-skip) — the boundary is documented so a swallowed warning is never mistaken for
success. Cross-refs ADR-002 (codex TOML), ADR-003 (codex hooks), ADR-005 (verb/intent), ADR-006
(verifier consumes the manifest).
