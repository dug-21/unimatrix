## ADR-005: `unimatrix wire` Verb, `--harness` Routing, and the #960 Intent Model

### Context

SCOPE decided a distinct `unimatrix wire` verb with `--harness <name>` (Goal 3, Decided §1), a wire-
only path that touches zero definition files and no DB (AC-03), and an intent model reconciled with
#960: detection triggers wiring, but writing a **new** MCP/retrieval entry into a user-owned config
(`opencode.json`, `.codex/config.toml`) requires explicit `--harness`/opt-in; merging into a surface
the user already has stays additive (AC-11, AC-16, SR-11). `bin/unimatrix.js` currently routes only
`init`, `mcp-bridge`, and binary passthrough. This ADR fixes the command surface, argument routing,
and the intent gate as one coupled mechanism (`--harness` is both the routing selector and the
new-entry opt-in signal).

### Decision

**1. `wire` verb in `bin/unimatrix.js`.** Route `args[0] === "wire"` to a `wire` entry that resolves
the project root and the client/binary paths, then calls `wire(projectRoot, {harness, clientPath,
binaryPath, mcp, dryRun})` (ADR-001). The wire path **skips** definition copy (`installSkills`) and all
DB/validate steps (AC-03) — it runs the wiring layer and nothing else. `init` continues to run
`installSkills` **and** `wire()` (wiring stays part of init).

**2. `--harness <name>` routing.** `name ∈ {claude-code, opencode, codex-cli}`. When supplied, wire
targets only that harness (and only if its markers are present — undetected named harness → a reported
`skipped-undetected` leg, not an error). When omitted, wire processes every **detected** harness under
the intent gate below. An unrecognized `--harness` value surfaces help, not a fall-through write (AC-16).

**3. Intent gate (#960).** For each harness surface, classify the write:
   - **Additive-into-existing** (the user-owned surface already contains the entry, or the entry is
     merged into a config the user already maintains) → proceeds without extra opt-in.
   - **New entry into a user-owned config** — specifically creating `mcp.unimatrix` in `opencode.json`
     or `[mcp_servers.unimatrix]` in `.codex/config.toml` where none exists — is an intent-bearing
     action and requires **explicit** `--harness <that harness>` (or an equivalent opt-in). Without
     it, the leg is `skipped-intent` with a help line naming the exact command to run (AC-11, SR-11).
   - claude-code `.mcp.json` is the historically-wired common path and is **not** gated (backward
     compat, SR-07): `init`/`wire` continue to ensure it as today.

**4. Ambiguity surfaces help.** Any ambiguous invocation (unknown harness, conflicting flags) prints
help/usage and writes nothing, rather than silently choosing a surface (AC-16).

**5. Uniform flags.** `--dry-run` (AC-14) and the containment guard (AC-12) apply to `wire` exactly as
to `init`. `--force` is **not** accepted by `wire` (it is definitions-only, ADR-004) — passing it to
`wire` is a no-op/usage note, guaranteeing `--force` never re-asserts wiring (AC-02, SR-04).

### Consequences

Easier: a wire-only refresh (MCP+hooks+retrieval) no longer forces a skill overwrite (the original
pain); `--harness` gives explicit-intent targeting per #960 in a single obvious place; `init` and
`wire` share one wiring layer, so behavior cannot drift between them. Harder: the intent classifier
must distinguish "new entry" from "additive-into-existing" per surface, and get it right for
opencode/codex user-owned configs — a wrong classification either nags on a benign merge or silently
installs an unintended surface (both asserted: AC-11 both directions); help text must state the
skills-only definition boundary (SR-05) and the exact opt-in command; `bin` argument parsing grows a
verb and a flag (small, stays in `bin/unimatrix.js`). Cross-refs ADR-001 (wire layer + manifest),
ADR-004 (`--force` never reaches wire), ADR-006 (verifier drives the wired command).
