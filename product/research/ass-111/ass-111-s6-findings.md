# FINDINGS: S6 — Harness depth vs ubiquity

**Spike**: ass-111 S6 (campaign ass-111, GH #1050)
**Date**: 2026-11-26
**Approach**: evaluation + proof-of-concept (attestation)
**Confidence**: directional overall; **validated** (working PoC against real harness binaries) for the attestation claims marked VALIDATED below.

PoC environment: OpenCode **v2.4.1** (installed binary, `--standalone`, isolated HOME/XDG) and Claude Code **2.1.332** (isolated `CLAUDE_CONFIG_DIR`), each driven by a scripted adversarial mock LLM (OpenAI-compatible and Anthropic-Messages mock servers on 127.0.0.1) against a mock stdio MCP server that logs every `tools/call` and verifies an HMAC attestation. Source reading: OpenCode `anomalyco/opencode` at tag `v2.4.1`. Scratch (temporary, not kept): `/var/tmp/claude-1000/-workspaces-unimatrix/19789040-5002-4a22-aa04-29e1732b9913/scratchpad/{poc,poc-cc,src/oc}`. No Codex binary is installed here, so Codex claims come from docs only.

---

## Headline

H3's premise is false at current harness versions. **A non-forgeable per-call agent-type attestation is not unique to OpenCode.** It works on **Claude Code 2.1.332** too, through a `PreToolUse` hook (harness-supplied `agent_type` plus `updatedInput`), and the PoC measured the same in-band forgery resistance on both harnesses. So deep OpenCode integration does not buy identity that Claude Code cannot provide. Neither harness provides the two properties S1 most needs:
- **Definition provenance**: an LLM-authored agent file is attested as that agent type (VALIDATED on both).
- **Credential confinement**: in OpenCode, the shell tool can read the signing secret (VALIDATED).

Recommendation: **reject H3 as framed. Keep ubiquity.** Specify one harness-neutral *adapter attestation contract* with identity tiers. Implement it first on Claude Code and OpenCode, and leave Codex at the tenant/session tier.

---

## Attestation verdict for S1

Terms:
- **Attest** means the identity on the call comes from the harness runtime, not from LLM-written text. The Unimatrix adapter (an OpenCode plugin or a Claude Code hook) signs it, and the server verifies it.
- **Forge** means getting a call accepted at the server with an agent identity that the harness did not assign to the calling session.

### PROVEN (validated by PoC on real binaries)

| # | Claim | OpenCode 2.4.1 | Claude Code 2.1.332 |
|---|---|---|---|
| P1 | The harness supplies the calling agent's identity per tool call, out of band from the LLM | `tool.hook("execute.before")` event carries `agent` (session's agent). Measured: primary `build`, child `writer` | `PreToolUse` stdin carries `agent_type` (`writer`) and `agent_id` (`ab445ffa068fe68c4`) for subagent calls; both absent for the main thread |
| P2 | The adapter can rewrite the outbound MCP call args with an attestation | `execute.before` `input` is mutable, and the mutated object is what executes (`tool.ts` `run` → `executeTool(…event.input…)`). Measured on the server | `hookSpecificOutput.updatedInput` applied to `mcp__mockmx__context_store`. Measured on the server |
| P3 | LLM forgery through tool args fails | LLM sent `agent_id:"uni-architect"` plus a fake `_attest{agent:"uni-architect",sig:"forged"}`. The adapter stripped it and re-signed as `build`. Server: `ATTESTED build`. Child LLM claimed `uni-scrum-master`; server: `ATTESTED writer` | Same probes. Server: `ATTESTED main` and `ATTESTED writer` |
| P4 | Invalid subagent types are rejected by the harness | `Unknown agent: ghost-agent`; `Agent build cannot run as a subagent`; a permission rule `subagent:{secret-writer:deny}` gives `Subagent denied: secret-writer` | `Agent type 'ghost-agent' not found` |
| P5 | The attestation is not echoed into the LLM's history, so the LLM cannot copy and replay a signature | The next model request shows the LLM's original args only | Same |
| P6 | The attestation also covers Code Mode (OpenCode 2.4 default for MCP tools: the LLM writes JS calling `tools.unimx.context_store`) | The inner call fires `execute.before` with `codemode:true, agent:"build"`. Server: `ATTESTED build` | n/a |
| P7 | Child sessions share the parent's MCP connection. Per-call session identity rides harness-set `_meta` | One MCP process, `_meta["ai.opencode/sessionID"]` distinct per session (answers ass-106's open question: shared connection, so transport-level `clientInfo` cannot carry per-subagent identity) | `_meta["claudecode/toolUseId"]` per call |
| P8 | The adapter can read the resolved agent *definition* in process and bind a definition hash | `ctx.agent.list()` returns `Agent.Info` (incl. `system`, `permissions`). The hash differed between agents and was carried in the attestation | Not available in process (hook gets the name only) |

### PROVEN LIMITS (validated breaks of "non-forgeable")

| # | Break | Evidence |
|---|---|---|
| L1 | **LLM-authored agent definitions are attested as genuine.** | OpenCode: the LLM used the `write` tool (no shell) to create `.opencode/agents/uni-architect.md`. OpenCode hot-reloaded it in the same session. `subagent` spawn succeeded and the server saw `ATTESTED uni-architect`. Claude Code: in `acceptEdits` mode the write to `.claude/agents/` was refused (protected path). In `bypassPermissions` the write succeeded, the same session said "not found", and the **next session** spawned it and the server saw `ATTESTED uni-architect`. This repo's `opencode.json` sets `"edit": "allow"`. |
| L2 | **Signing secrets reachable from the LLM's shell are forgeable.** | The OpenCode `shell` tool printed the `POC_KEY` prefix from the inherited env (`04f54733a5b10147`, matching). It also listed `opencode.db`, the credential and session store. OpenCode has no OS sandbox for shell (no bwrap/landlock/seatbelt in source). Same uid means the LLM can read any at-rest key and call the server directly. `kernel.yama.ptrace_scope=0` here, so in-memory secrets are reachable too. |
| L3 | A primary LLM can choose any subagent type its permissions allow, so it can borrow that type's privileges. | P4 shows only that unknown, primary-mode and explicitly denied types are blocked. Per-agent-type policy is therefore only as strong as the spawn-permission rules (`permission.subagent` in OpenCode; agent availability in Claude Code). |

### SPECULATIVE / UNVALIDATED (S1 must not depend on these without further work)

- **Confinement closing L2.**
  - Claude Code documents a Bash sandbox with `filesystem.denyRead`, a `network.allowedDomains` proxy and managed-settings enforcement (`failIfUnavailable`, `allowUnsandboxedCommands:false`). Hooks run **outside** the sandbox, so a hook can hold a key the LLM's Bash cannot read. Docs only, not tested. The sandbox is off by default.
  - Codex documents an OS sandbox on by default. Not tested.
  - OpenCode: none in the harness. It would need an external container or network policy, or v2 remote "environments" (the tool-execution environment separated from the server process). Speculative.
- **Definition pinning closing L1.** The server pins `agent_type → allowed definition hash`, or definitions come only from a location the LLM cannot write (managed/global config, or Unimatrix-distributed definitions; ties to S5/H6). OpenCode can hash in process (P8). Claude Code's hook would have to hash the file itself (racy; plugin and user-scope agents are ambiguous). Not built.
- **Codex per-agent attestation.** Per the docs, `PreToolUse` covers MCP calls but its input lists **no** `agent_type`/`agent_id`. Only `SubagentStart`/`SubagentStop` carry them. Whether `updatedInput` applies to MCP tools is ambiguous ("for Bash and apply_patch it must include `command`; other shapes are reported as errors"). Treat Codex as session-level only. Not tested.
- **Remote HTTPS transport.** The PoC used stdio MCP. The attestation rides MCP `tools/call` arguments, which are transport-agnostic, but OpenCode/Claude Code → remote streamable-HTTP was not run.
- **Escalation via OpenCode's local API from Code Mode.** `opencode:///api/session/current/agent` (switchAgent) is reachable from Code Mode `fetch` with no permission assert in the source. In the 2.4.1 release it returned "opencode:// URLs are off", and `OPENCODE_FLAGS=opencode_api` did not enable it (dev builds only). Closed today. It is a watch item if upstream ships it on.
- **Unimatrix-side consumption.** The `external_identity` seam (`server.rs:526`, always `None`) and the vnc-050 `cap_ceiling` intersection exist. Neither was exercised. Unimatrix params structs do not set `deny_unknown_fields`, so an args-borne `_attest` would be silently ignored today. The server must extract it from the raw arguments or from `_meta`.

### What S1 can safely conclude

1. **Tier T2, "adapter-attested agent type", is feasible on Claude Code and OpenCode now** (P1–P7). It defeats the realistic in-band threats: hallucinated or prompt-injected `agent_id`, forged attestation fields, unknown, primary or denied subagent types, and replay of a seen signature.
2. **T2 is not "non-forgeable" against an LLM with unsandboxed shell, or with write access to agent-definition directories** (L1, L2). The stronger tier T3 ("confined + definition-pinned") is a design target. It has a plausible path on Claude Code (documented sandbox plus managed settings) and no in-harness path on OpenCode.
3. **PL-4 enforcement shape.**
   - The server verifies signature, freshness, single-use call id, args hash, tenant/project binding and (T3) definition hash.
   - An unattested or invalid call is downgraded to the self-asserted tier (T0/T1). It is never auto-enrolled into a privileged identity.
   - Privileged operations (for example writing `decision`) require T2+ with an allowed `agent_type`. Everything else keeps today's bearer/scope behaviour.
   - The client adapter stays fail-open (observation never blocks the harness). Enforcement is fail-closed on the server.
4. **Keys (Principle 8).** Use a per-machine asymmetric device key, generated at install and stored 0600 outside the workspace. Register only its public key with the tenant (OAuth device enrolment). Never use a shared HMAC secret in env. (The PoC's HMAC-in-env setup is exactly what L2 broke.)

Proposed tiers (for S1 to adopt or amend):

| Tier | Identity source | Harnesses (today) |
|---|---|---|
| T0 | self-asserted `agent_id` (audit only) | all, incl. claude.ai connector / public chat |
| T1 | token-bound tenant/project/scope (vnc-050 ceiling) | all MCP clients |
| T2 | T1 + adapter-signed harness `agent_type` + session + call binding | **Claude Code (validated), OpenCode (validated)** |
| T3 | T2 + definition hash pinned + LLM tool surface confined (no key read, no direct egress to Unimatrix) | Claude Code (sandbox+managed; unvalidated); Codex (sandbox; no per-call agent); OpenCode only inside an external container (unvalidated) |

---

## Findings

### Q: What does OpenCode-first deep integration concretely enable: non-forgeable agent identity, per-agent-type capability policy, coordination across project scopes? Which parts are proven vs speculative?

**Answer**: OpenCode v2.4.1 enables the following.

1. **Per-call harness-attested agent type.** Proven (P1–P7). It is not unique to OpenCode: Claude Code reaches the same level.
2. **Per-agent-type policy keyed on that identity.** Server-side enforcement is feasible but unbuilt. It is only meaningful with spawn-permission rules (L3) and definition provenance (L1).
   - OpenCode-only depth: in-process definition hashing (P8).
   - OpenCode-only depth: a mutable per-request `session.hook("context")` exposing `agent`, `system`, `messages` and `tools`. From source, this allows per-agent tool filtering and proactive injection into **subagent** requests, which closes ass-106's v1 SubagentStart-injection gap. Not run.
3. **Coordination across project scopes.** Speculative, and not OpenCode-specific. The v2 background service hosts sessions for many directories, and each plugin context carries `ctx.location`. The adapter could therefore stamp a harness-derived project claim per call. Claude Code hooks carry harness-set `cwd`, which gives the same capability. Anything beyond the project claim is coordination, which belongs to the harness: Unimatrix stays a knowledge engine.

**Evidence**: PoC runs 2–6 (OpenCode) and 1–5 (Claude Code), tabulated in the verdict above. Source:
- `packages/plugin/src/promise/tool.ts` (`ToolHooks.execute.before.agent`, mutable `input`);
- `packages/core/src/tool.ts:161-237` (event input used for execution);
- `packages/core/src/tool/mcp.ts` (MCP tools registered in the same registry, so hooks fire);
- `packages/core/src/mcp/client.ts:884` (`_meta["ai.opencode/sessionID"]`);
- `packages/core/src/tool/plugin/subagent.ts:127-138` (validation + permission);
- `packages/plugin/src/promise/session.ts:207-222` (session hooks).

The diff `v2.0.0..v2.4.1` of the tool-hook types is additive only; `agent` has been present since 2.0.0.

**Recommendation**: Treat OpenCode as a **first-class T2 adapter and the vehicle for local models**, not an exclusive one. Use its depth advantages only where they are additive and degrade cleanly on other harnesses: definition hashing, the context hook, Code Mode coverage.

### Q: What do Claude Code and Codex lose or keep in V2 (retrieval-only? full hooks?) if OpenCode is primary?

**Answer**: Under the recommended harness-neutral contract, nothing is lost. Claude Code is at T2 parity today and has the best documented path to T3. If H3 were adopted literally (identity-gated features only on OpenCode), Claude Code would drop to T1, a regression on the reference harness. The human's delivery workflow runs on Claude Code: `.claude/` protocols and agents are canonical, and `.opencode/agents/*.md` are thin redirects to them. Codex keeps retrieval and hooks, and stays at T1 until it exposes per-call agent identity.

**V2 capability matrix per harness** (V = validated here; S = source-read; D = docs only; — = absent):

| Capability | Claude Code 2.1.332 | OpenCode 2.4.1 | Codex (current docs) |
|---|---|---|---|
| Retrieval (`context_*` over MCP, stdio bridge or remote) | V (C14 proven) | V (C10/C18) | V (C14 proven) |
| Observation events | 8 hook events (nan-024) | plugin hooks; #998 not shipped | hooks incl. Pre/PostToolUse, SubagentStart/Stop (D) |
| Proactive injection | SessionStart / SubagentStart / UserPromptSubmit `additionalContext` (D) | `session.hook("context")` mutable system/messages per request, incl. subagents (S) | SubagentStart stdout → developer context (D) |
| Per-call harness agent identity | **V** (`agent_type`, `agent_id` in PreToolUse) | **V** (`execute.before.agent`) | — on PreToolUse (D); SubagentStart only |
| Adapter rewrites MCP call | **V** (`updatedInput`) | **V** (mutable `input`) | unclear for MCP (D) |
| Subagent type validated by harness | **V** | **V** (+ permission-gated, + primary-mode block) | not checked |
| Agent definition provenance in adapter | name only | **V** (`Agent.Info` hash) | name only (D) |
| LLM can author a new attested agent type | V: next session; `.claude/` protected unless bypass | **V: same session**, plain `edit:allow` | not checked |
| Credential confinement from LLM shell | sandbox denyRead + egress allowlist, managed-enforceable, hooks outside sandbox (D; off by default) | **— (V: shell read the key)** | OS sandbox on by default (D) |
| Harness-set per-call `_meta` | `claudecode/toolUseId` (V) | `ai.opencode/sessionID` (V) | not checked |
| Multi-model / local models | Anthropic (+ gateways) | any (ai-sdk providers, Ollama) | OpenAI (+ OSS providers) |
| Source | closed | MIT | Apache-2.0 |
| Achievable tier today | **T2** (T3 path) | **T2** (T3 only in external container) | **T1** |

**Recommendation**: Ship V2 with all three harnesses at T1 and with Claude Code and OpenCode adapters at T2. Gate privileged per-agent-type policy on T2+, so Codex and claude.ai connectors keep full retrieval and observation but cannot exercise agent-type-gated writes. Make T3 a Claude Code-first follow-on (managed sandbox profile plus definition pinning).

### Q: What is OpenCode's maturity, adoption trajectory, and extension stability risk as a strategic dependency?

**Answer**: OpenCode has high adoption and very high velocity, but its extension surface was hard-broken 2.5 months ago and v2 is still early in uptake. The risk is **acceptable for a T2 adapter behind a harness-neutral contract**. It is **not acceptable as the sole identity root** (H3).

**Evidence** (collected 2026-11-26):

**Adoption and velocity**
- Repo `anomalyco/opencode` (formerly `sst/opencode`): MIT, 229,377 stars, 31,240 forks, 4,188 open issues, created 2025-04-30.
- 5,513 commits from 141 distinct authors in the last 90 days.
- The contributors API paginates to 448+.

**Cadence**
- v2.0.0 shipped 2026-09-11, and there are 52 v2.x tags in ~11 weeks: v2.1.0 10-20, v2.2.0 10-27, v2.3.0 11-13, v2.4.0 11-23, v2.4.2 11-25.
- The v1 line is still released in parallel (1.19.6 on 2026-11-17; npm `opencode-ai` `latest` is still 1.19.6).

**Breakage**
- V2 has three intentional breaking changes: the plugin API, the server API and terminal config. "V1 plugin implementations do not run in V2" (opencode.ai/v2/migrate-v1).
- The plugin package was renamed (`@opencode-ai/plugin` → `@opencode/plugin`).
- Unimatrix already paid this cost. The vnc-049 plugin targeted v1 1.18.31 (2026-09-15, four days after v2.0.0), and #998 now exists to rebuild it for v2.
- The dogfood v2 plugin in `.opencode/plugins/unimatrix.js` already mismatches the 2.4.1 types (see Out-of-Scope).

**Within-v2 stability**
- The tool-hook types are additive only from 2.0.0 to 2.4.1 (diff above).
- Several surfaces are explicitly experimental: `experimental.idle`, `experimental.model.resolve`, `experimental.ws.*`, and Code Mode `opencode://` gated to dev builds.

**v2 uptake (npm weekly, 11-19..11-25)**
- `@opencode/plugin` (v2): 629,860.
- `@opencode-ai/plugin` (v1): 32,515,929. Both are inflated by OpenCode's startup `bun install`, but the ratio puts v2 at about 2% of v1.
- For reference (npm only; native installers excluded): `@openai/codex` 27,967,476 and `@anthropic-ai/claude-code` 17,297,952.

**Mitigants**
- MIT licence: forkable, and upstream FRs are possible (for example a plugin-settable `_meta`).
- Managed config (system dir / MDM) exists for enterprise policy (`core/src/config/managed.ts`).

**Recommendation**:
- Pin the OpenCode adapter to a tested v2 minor range and run it in CI against a real binary (the #998 fire-test pattern). Re-validate P1–P7 on every minor.
- Keep the attestation payload format Unimatrix-owned, so a harness break changes only the adapter, never the server contract.
- Do not let any V2 goal's `done_when` depend on OpenCode alone.

---

## Unanswered Questions

- **Codex per-call agent identity and `updatedInput` on MCP tools.** There is no Codex binary in this environment and the docs are ambiguous. Needs a live Codex run.
- **T3 confinement efficacy.** Not tested here: whether Claude Code's sandbox (`denyRead` on the key path plus an egress allowlist excluding the Unimatrix host) actually keeps a shell-capable LLM from reading the adapter key or calling the server, while the hook still works. Needs a sandbox PoC. Requires bubblewrap, which this devcontainer may lack.
- **Remote HTTPS MCP path.** Attestation was proven over stdio only. It should be transport-agnostic, but OpenCode/Claude Code → remote streamable-HTTP was not run.
- **OpenCode alternative channel.** Not tested: wrapping MCP tool `execute` via `tool.transform().update()` to send the attestation as an HTTP header or `_meta` instead of an args field. Types allow it.
- **Hook-latency cost.** A per-call Node spawn in Claude Code `PreToolUse` was not measured. It is relevant to injection-path latency budgets.
- **Claude Code auto mode.** Docs say a resent call after an auto-mode denial drops `updatedInput`. The effect (an unattested call, which the server would downgrade) was not exercised.
- **Cross-project coordination semantics.** The SCOPE does not define what "coordination" Unimatrix would provide beyond a harness-derived per-call project claim. This needs S1/S3 framing.

---

## Out-of-Scope Discoveries

- **Dogfood v2 plugin field drift (#998).** `.opencode/plugins/unimatrix.js` has these mismatches:
  - It matches `^context_cycle$|^mcp__unimatrix__context_cycle$`, but OpenCode v2 names MCP tools `<server>_<tool>` (validated: `unimx_context_store`). The cycle `PreToolUse` intercept therefore never fires.
  - It reads `event.callID` and `event.output`, which are `id` and `result` in 2.4.1.
  - It reads `fork`'s `event.parentID`, which does not exist: the `fork` event has only `sessionID`.
- **OpenCode 2.4 defaults MCP tools to Code Mode.** The LLM calls `tools.<server>.<tool>` from JS. Tool descriptions and any tool-name heuristics in Unimatrix should account for this.
- **Harness-set `_meta` gives a trustworthy session/call correlator.** OpenCode sends `ai.opencode/sessionID` and Claude Code sends `claudecode/toolUseId`. This could replace the LLM-supplied `session_id` for observation attribution (SLN1, #5528).
- **Agent-definition provenance links to H6/S5.** If Unimatrix distributes harness definitions, it can also be the pinning authority for the definition hashes that T3 needs. Worth S5's attention.
- **Plaintext service secret.** `~/.config/opencode/service.json` holds the OpenCode v2 background-service password in plaintext, readable by any same-uid process (value not recorded). This is a further instance of L2.
- **`ptrace_scope=0` in this devcontainer.** Same-uid processes can read harness memory, so "keep the key only in memory" is not a mitigation here.
- **Repo `opencode.json` sets `"edit": "allow"`.** Combined with hot-reload of `.opencode/agents/`, the LLM can mint agent types (L1). This is harmless in V1 because identity is not enforced, but it must change before any T2 enforcement.

---

## Escalations for human

1. **H3 disposition.** The evidence says reject "forego ubiquity for OpenCode depth": Claude Code matches OpenCode on T2. Do you accept the harness-neutral adapter-attestation contract? Or do you want OpenCode-first for reasons outside identity, such as local models?
2. **Required V2 threat model.** Is T2 (in-band forgery resistance) enough for V2's per-agent-type policy? Or is T3 (resistance against a shell-capable LLM) required? T3 implies mandating a managed sandbox profile for Claude Code and an external container for OpenCode.
3. **Definition provenance authority.** Should Unimatrix pin agent-definition hashes (and thus own or distribute definitions, touching H6), or should it rely on harness permission rules that the LLM can bypass via shell?

---

## Recommendations Summary

- **Q1 (what OpenCode depth enables)**:
  - Per-call harness-attested agent type is VALIDATED on OpenCode 2.4.1, but it is not OpenCode-exclusive.
  - OpenCode-only extras are in-process definition hashing (validated) and a per-request context hook for subagent injection and tool filtering (source).
  - Cross-project coordination is speculative and harness-neutral (a per-call project claim from `location`/`cwd`).
- **Q2 (Claude Code / Codex in V2)**:
  - Claude Code is at T2 parity (VALIDATED via `PreToolUse` `agent_type` + `updatedInput`) and has the only documented T3 confinement path.
  - Codex is T1 (no per-call agent identity in `PreToolUse`).
  - Gate agent-type-privileged writes on T2+. All harnesses keep retrieval and observation.
- **Q3 (OpenCode dependency risk)**:
  - Adoption is high and velocity very high, but the v1→v2 plugin break 2.5 months ago already cost Unimatrix a rebuild (#998), and v2 is still ~2% of v1 plugin installs.
  - Use OpenCode as a pinned, CI-verified adapter behind a Unimatrix-owned attestation format, never as the sole identity root.
- **H3**: Reject as framed. Keep harness ubiquity, and specify one adapter attestation contract (T0–T3) with Claude Code and OpenCode as the first T2 adapters.
- **S1 handoff**:
  - Adopt T2 as feasible-now.
  - Treat L1 (LLM-authored definitions) and L2 (secret reachable from shell) as proven limits.
  - Use asymmetric per-machine device keys (public key only server-side, Principle 8).
  - Enforce fail-closed on the server and fail-open in the adapter.
  - Activate `external_identity` and extract the attestation from raw args or `_meta`, because `deny_unknown_fields` is not set.

---

Unimatrix entries consulted: #5734, #5834, #5835, #5894, #5745, #5705, #5812 (→ #5819). Briefing also listed #5743, #5763, #5768 (not opened).
