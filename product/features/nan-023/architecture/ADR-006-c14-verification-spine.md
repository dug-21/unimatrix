## ADR-006: C14 Verification Spine — Assert Retrieval-Returns / Hook-Fires From the Wired Command's Manifest, in Local and Cloud, With a Pre-Tag Real-Server Exercise

### Context

The dominant risk (SR-09, ceremonial wiring / path-divergence) is that config/hook blocks are
**written** (seam green) but the hook never fires and/or `context_*` never returns from the wired
command — the exact class the closed design→test→gate loop cannot catch (vnc-047/#944, #4974). SCOPE
makes the return path load-bearing: a leg is not "wired" for C14 unless a `context_*` call **returns**
after wiring (Constraints), and codex hooks must be verified to **actually fire** (AC-09). Two
compounding risks: SR-02 (codex retrieval/hooks are trust-gated — green in a trusted test, inert for
an untrusted consumer) and SR-03 (cloud+local return/fire gates are release-only and, per #5267
nan-019/nan-020, fail in sequence, one tag round each). This ADR fixes *how* parity is proven so the
proof is not tautological and does not incur the multi-round tag tax.

### Decision

**1. The assertion runs against the wire manifest, never against config presence.** ADR-001's `wire()`
returns a `WireLeg[]` manifest carrying, per leg, the **exact** `command` string (hooks) and `entry`
(MCP/retrieval) written. The C14 verifier consumes that manifest and:
   - **Retrieval-returns**: spawns/connects the MCP target from `manifest[].entry` (the actual wired
     command/url) and issues a `context_*` call, asserting a response **returns** — for claude-code,
     codex-cli, and opencode (via `mcp.unimatrix`). Config-presence checks are forbidden as the
     discharge (SR-09).
   - **Hook-fires**: executes the exact `manifest[].command` (e.g.
     `node <clientPath> PreToolUse --provider codex-cli`) with a synthetic event on stdin and asserts
     the event **reaches the JS hook client** stamped `provider=codex-cli` (breadcrumb / queue /
     server record). For opencode, "firing" is plugin-level (in-process TS plugin), asserted at that
     level, not JS-hook-client-level (AC-10 per-harness feasibility).

**2. Both local and cloud, where feasible (AC-09/AC-10).** The JS-client install path + codex hook
wiring must function under both deployments (binding constraint). The verifier runs the retrieval-
return and hook-fire assertions in **both**: local (Rust binary / UDS) and cloud (JS bridge / HTTP).
Where a harness leg is genuinely infeasible in one deployment, that is stated explicitly, not skipped
silently.

**3. Trust is a surfaced precondition, never a silent no-op (SR-02).** Codex `.codex/` must be trusted
for its config/hooks to load. The verifier runs in a **trusted** fixture; and at wire time, if codex
trust cannot be established the leg reports a loud precondition warning (fail-loud / warn), never a
silent success. The parity claim is stated as conditional on a trusted `.codex/` layer.

**4. Pre-tag real-server exercise (SR-03).** Land a pre-tag, real-server exercise of the codex
hook-fire + retrieval-return (and the opencode retrieval sentinel) that runs **before** the release
chain — not a release-only gate. This surfaces layered failures during delivery instead of consuming
one tag round each (#5267). The release-only local+cloud gates remain, but they are not the *first*
signal.

**5. Golden/sentinel regressions accompany the spine (SR-06/SR-07).** Byte-for-byte golden of the
claude-code `.mcp.json`+`.claude/settings.json` common path captured **before** the per-harness
refactor and asserted unchanged; opencode `mcp.unimatrix` + Ollama `provider` byte-for-byte sentinel
plus a retrieval-still-returns regression. These guard that the parity work does not regress the
already-proven legs.

### Consequences

Easier: "wired" means demonstrably functional, per harness, in both deployments — ceremonial wiring
cannot pass (SR-09); failures surface pre-tag, cutting the release-round tax (SR-03); the manifest
gives the verifier one unambiguous thing to execute. Harder: the test harness must spawn real MCP
targets and feed real hook events in both local and cloud, and must establish codex trust in the
fixture (the tester/risk-strategist must confirm this is achievable, ARCHITECTURE open question #2) —
richer than a config-diff test; the trust precondition makes the codex parity claim conditional, which
must be stated honestly rather than hidden; per-provider `source_domain` remains claude-code-forced
(#5737), so the firing assertion is scoped to `provider` stamping, not domain. Cross-refs ADR-001
(manifest), ADR-003 (the command fired), ADR-005 (the wired command path).
