# FINDINGS: OpenCode as a Unimatrix observation harness — parity assessment + plugin-model opportunity

**Spike**: ass-106
**Date**: 2026-09-15
**Approach**: investigation (code + ecosystem)
**Confidence**: directional (feeds a uni-zero decision on opening a design/delivery cycle for C18 #5730)

**Headline**: opencode has **no command-hook mechanism**. The `.codex/hooks.json` model (spawn a process, JSON on stdin, exit code) **does not generalize** to opencode. opencode's only extension surface is an **in-process TypeScript plugin API**. This changes the ingestion path (a thin TS plugin shim, not a hooks-file mirror) and simultaneously unlocks something Claude's and Codex's stdin contract cannot: a **harness-validated, out-of-band agent identity** — the missing precondition for Principle-3 capability enforcement.

---

## A. OpenCode extension-surface map

**Mechanism: in-process TypeScript plugin API only. No command hooks.**
- opencode does NOT spawn an external process with JSON on stdin / exit code. There is no stdin-contract analog to `.claude/settings.json` hooks or `.codex/hooks.json`. (Source: opencode.ai/docs/plugins, `sst/opencode` `packages/plugin/src/index.ts`.)
- A plugin is a JS/TS module loaded into the opencode (Bun) runtime. Signature:
  `export type Plugin = (input: PluginInput, options?) => Promise<Hooks>`.
  `PluginInput` provides `client` (opencode SDK client), `project`, `directory`, `worktree`, `serverUrl`, and `$` (Bun shell — the escape hatch to shell out to an external binary).
- The plugin returns a `Hooks` object: keys are hook names, values are `(input, output) => Promise<void>` callbacks.

**Two event surfaces:**
1. **Named typed hooks** — `chat.message`, `chat.params`, `chat.headers`, `tool.execute.before`, `tool.execute.after`, `experimental.session.compacting`, `permission.ask`, `command.execute.before`, plus experimental transforms. Several are **mutable** (can rewrite prompt/args/params → injection surface).
2. **Generic event bus** — one `event: (input:{event}) => Promise<void>` hook receives every bus event: `session.created`, `session.updated`, `session.idle`, `session.compacted`, `session.deleted`, `message.updated`, `permission.asked/replied`, `file.edited`, `command.executed`, `tool.execute.before/after`, etc. Bus events are **observe-only** (no output mutation).

**Where wiring is declared:**
- Local plugin files auto-loaded from `.opencode/plugins/` (project) and `~/.config/opencode/plugins/` (global). Deps in `.opencode/package.json`; opencode runs `bun install` at startup.
- OR npm packages listed in the `plugin: [...]` array in `opencode.json`.
- Load order: global config → project config → global plugin dir → project plugin dir; hooks run in sequence.
- MCP servers (the retrieval path, already working) are declared in `opencode.json` under `mcp.<id>` with `type: "local"` (stdio) or `"remote"` (HTTP) — unrelated to the plugin surface.

**What is wired in this repo today** (`opencode.json` + `.opencode/`, both untracked, hand-authored — NOT by `unimatrix init`):
- `opencode.json`: `mcp.unimatrix { type:"local", command:[<npm binary>], env:{LD_LIBRARY_PATH} }` (retrieval, C10 — the same Rust binary as `.mcp.json`), a local Ollama `provider`, agent shims, permissions. **No `hooks`/`plugin` wiring — retrieval yes, behavioral signal no.**
- `.opencode/`: `package.json` with the single dep `@opencode-ai/plugin@1.18.31` (the plugin API is installed but no plugin code exists), and 14 thin `agent/*.md` role redirects. No plugin `.ts`, no observation code.

---

## B. Parity table — the 7 canonical events (vnc-013 canon)

Claude stdin contract (`HookInput`, `crates/unimatrix-engine/src/wire.rs`): flat JSON `{ hook_event_name, session_id, cwd, transcript_path, prompt, provider, mcp_context, extra{tool_name, tool_input, tool_response, agent_type, exit_code} }`, all `#[serde(default)]`.

| Canonical event | opencode equivalent | Kind | Fires OOB? | Payload-shape delta vs Claude `HookInput` |
|---|---|---|---|---|
| **SessionStart** | `session.created` (+`session.updated`) | bus | Yes (observe-only) | Session object, not flat fields. `session_id`→`sessionID`; no `cwd`/`transcript_path` on the event (use `PluginInput.directory`/`worktree` + SDK `client`). `agent_role`/`feature` must be derived (session `agent` field + client). Maps to `SessionRegister`. |
| **UserPromptSubmit** | `chat.message` | typed, **mutable** | Yes | input `{sessionID, agent?, model?, messageID?, variant?}`, output `{message, parts}`. `prompt` is inside `message`/`parts`, not a flat field. Mutable output = **retrieval injection works** (the `ContextSearch` inject path is reachable). |
| **PreToolUse** | `tool.execute.before` | typed, **mutable** | Yes | input `{tool, sessionID, callID}`, output `{args}` (mutable). `tool_name`→`tool`; `tool_input`→`output.args` (note: on OUTPUT for `before`). **No inline agent field** — needs `sessionID→agent` correlation. cycle-intercept (`context_cycle`) still works by matching `tool`. |
| **PostToolUse** | `tool.execute.after` | typed, **mutable** | Yes | input `{tool, sessionID, callID, args}`, output `{title, output, metadata}`. `tool_response`/`had_failure`/`exit_code` must be mapped from `output`/`metadata` — shim must synthesize Claude's failure semantics. Rework detection reachable. |
| **SubagentStart** | `session.created` with `parentID`≠null + `agent` | bus (derived) | **Partial — derived only** | No dedicated pre-spawn hook (upstream FR #20387 open). Reachable only by correlating a child `session.created` (bus, **observe-only**). `agent_type`→validated session `agent` (richer, trusted). **Cannot inject retrieval into the subagent** — the `ContextSearch(source:"SubagentStart")` inject leg is unreachable. |
| **PreCompact** | `experimental.session.compacting` | typed, **mutable**, EXPERIMENTAL | Yes (unstable API) | input `{sessionID}`, output `{context[], prompt?}`. Injection reachable (mutable output ⇒ `CompactPayload` inject works). Transcript-block extraction shifts from a `transcript_path` file to SDK queries. API marked experimental — stability risk. |
| **Stop** | `session.idle` | bus (derived) | **Partial — derived only** | Closest signal; no dedicated response-end hook. `SessionClose` `outcome`/`duration_secs` must be computed plugin-side. Semantics undocumented (may fire on every idle transition → SessionClose over-count risk). |

**Named parity gap (C18 `done_when` — legs opencode cannot reach OOB):**
1. **SubagentStart retrieval-injection** — the primary casualty. opencode surfaces subagent spawns only as an observe-only bus event (`session.created`+`parentID`); it cannot inject Unimatrix retrieval into the new subagent's context the way Claude's `SubagentStart` hook stdout does. Observation of the spawn is derivable; injection is not.
2. **No stdin command contract at all** — structural, not per-event: opencode cannot consume the Claude `HookInput` schema directly. Every reachable event must pass through a plugin shim that constructs the Claude-shaped frame. A `--provider opencode` path is necessary but not sufficient; there is no process for it to receive stdin from unless a plugin spawns it.
3. **Stop / SessionStart are bus-derived, degraded** — reachable as observation, but `duration`/`outcome`/`agent_role` and the transcript must be computed/queried plugin-side (no `transcript_path` file), and `session.idle`→Stop mapping is semantically loose.
4. **PreCompact is experimental** — reachable including injection, but on an API opencode flags unstable.

**Direct answer to "can `--provider opencode` consume it directly": no.** A normalization/shim branch is required. The shim lives in the TS plugin (shape mapping), and a `--provider opencode` arm is added to the existing normalizer for name canonicalization + provider stamping.

---

## C. Recommended ingestion path (feasibility — flagged, not designed)

| Candidate | Feasibility | `source_domain` correctness |
|---|---|---|
| **(a) command-hook mirror** like `.codex/hooks.json` | **Impossible.** opencode has no command-hook mechanism (no stdin/exit-code contract, no `hooks` block that spawns a process). The Codex precedent does not generalize. | N/A |
| **(b) TS plugin shim → `unimatrix hook`/UDS** | **Feasible — RECOMMENDED.** A plugin in `.opencode/plugins/` maps each typed hook / bus event to a Claude-shaped `HookInput` JSON and (simplest) shells out via `$` to `unimatrix hook <EVENT> --provider opencode`, reusing the **entire existing Rust pipeline unchanged** (`hook.rs` UDS transport, `wire.rs`, listener, queue, fail-open). Optimization later: open the per-project UDS socket in-process (port the `packages/unimatrix/lib/hook-client` logic into the plugin) to avoid per-event process spawn. | Correct **iff** a `resolve_source_domain` / domain-pack entry maps opencode events to an `opencode` domain. `--provider opencode` only sets `ImplantEvent.provider`; `source_domain` is resolved server-side and defaults to `"claude-code"` (`DomainPackRegistry::resolve_source_domain`, `services/observation.rs`). Flag: a domain-pack mapping is a named requirement, not automatic. |
| **(c) `--provider opencode` normalization branch (vnc-013)** | **Required, but a component of (b), not an alternative.** Add `opencode` to `KNOWN_PROVIDERS` + a `map_to_canonical`/`normalize_event_name` arm in `hook.rs` (and `normalize.js` for the JS client). It normalizes names and stamps provider; it produces no events on its own. | Sets `ImplantEvent.provider="opencode"`; still needs the (b) `resolve_source_domain` mapping above for correct `source_domain`. |

**Recommended path: (b) + (c).** A TS plugin shim (b) feeds `unimatrix hook --provider opencode`; the vnc-013 provider branch (c) plus a `resolve_source_domain` opencode mapping give correct attribution. The plugin is the normalization boundary that the stdin contract cannot be. (a) is architecturally ruled out — this is the key finding that distinguishes opencode from Codex.

---

## D. C17 installation-change assessment

**Today:** `unimatrix init` (`packages/unimatrix/lib/init.js`) writes only `.mcp.json` + `.claude/settings.json`. There is **no opencode detection, no opencode writer, no `.codex` writer** — the existing `opencode.json`/`.opencode/` were hand-authored. This is the C17 gap C18 depends on (#5730 open item 2).

**Regression sentinel (C17 #5582 hard constraint):** the existing `mcp.unimatrix` retrieval entry in `opencode.json` (and local STDIO byte-for-byte) **must not regress**. The installer delta must be additive.

**Installer delta to scope (net-new; extends C17/nan-004, does not reuse settings.json merge verbatim):**
1. **Detect opencode** — presence of `opencode.json` or `.opencode/`.
2. **Provision the plugin** — drop the shim plugin into `.opencode/plugins/` and add its dep to `.opencode/package.json` (opencode `bun install`s at startup), OR append the plugin's npm package to the `plugin: [...]` array in `opencode.json`.
3. **Merge, non-clobbering** — the nan-004 `merge-settings.js` prefix-match/**preserve-and-prune** *principle* is the template, but the **surface is different**: opencode extends via a plugin directory + `plugin` array, not a per-event `hooks` block with matcher groups. So the merge is simpler (idempotent file drop / array append) — the per-event `EVENT_MATCHERS`/matcher-group logic does not carry over. Preserve `mcp.unimatrix`, the `provider` (local Ollama) block, permissions, and all non-Unimatrix keys.
4. **Keep MCP/STDIO retrieval untouched** — do not rewrite `mcp.unimatrix`; the retrieval path (C10) is already proven and is the regression sentinel.

Scope estimate: a genuinely new installer branch (opencode detect + plugin provision + a small JSON merge for `plugin`), plus the shim plugin artifact itself. This is real C17-extension work, not a config-line addition.

---

## E. Doing better — Unimatrix-improvement opportunities (ranked, led by trusted identity)

### E1 (LEAD) — Harness-attested agent identity → real capability enforcement

**Ground truth (Unimatrix side).**
- Per-call `agent_id` on every MCP tool params struct (`crates/unimatrix-server/src/mcp/tools.rs`) is **self-declared, AUDIT-ONLY, NEVER an authorization input** (tools.rs comment, SD-9 / convention #1301). It is really an agent *type* the LLM types into the call.
- vnc-014 two-field model (#4361): `agent_id` (spoofable) vs `agent_attribution`/`client_type` (transport-attested from the MCP `initialize` `clientInfo.name`). But `client_type` identifies the **harness** (would be `"opencode"`), set **once per MCP connection** — it cannot vary per-subagent per-call.
- alc-003 (#2155): capability resolution already moved off per-call `agent_id` to a **session-level** `UNIMATRIX_SESSION_AGENT` env var at startup — process-wide, cannot distinguish subagents within a session.
- The consumption seam **already exists but is dormant**: `build_context_with_external_identity(external_identity: Option<&ResolvedIdentity>)` (`server.rs`) is always `None` today — the W2-3 bearer-auth activation point. This is exactly the seam this leg targets.
- PL-4 (#5705, L2): fail-closed enforcement is **hollow** — `resolve_or_enroll` auto-enrolls unknown ids (fail-**open**), no trust-binding. The enforcement-gate rewrite + transport trust-binding + per-agent PKI are OUT of scope now, **demand-pulled** — "build only on real-consumer evidence."

**What opencode adds (the trust chain).**
- opencode validates the subagent type through `agent.get()` (rejects unknown types: `Unknown agent type: ...`) and **stamps the validated `agent` name onto the child session** with `parentID` (`packages/opencode/src/tool/task.ts`). The LLM's `subagent_type` string is validated, not trusted raw.
- The validated `agent` reaches plugins **out-of-band**: directly on `chat.message`/`chat.params`/`chat.headers` inputs, and on tool hooks via `sessionID→agent` correlation (`client.session.get`). The plugin — harness-trusted, LLM not in the loop — reads it.
- **This is strictly better than Claude Code**, which exposes no out-of-band per-subagent identity bound to a tool call at all (Claude's `SubagentStart` stdin `agent_type` is the closest, and it is not bound to each subsequent `context_*` call).

**Feasibility (directional).**
- Trust-chain legs **1–3 are solid**: `agent.get()` validation (harness) → `session.agent` stamp (harness, LLM out of loop) → plugin reads it (trusted, in-process). Unimatrix's **consumption leg is already stubbed** (`external_identity` seam) and needs activation + a PL-4 fail-closed flip.
- The **open leg is delivery**: getting the trusted agent identity onto the **specific outbound MCP `context_*` call** on a channel the server treats as non-forgeable. Two candidate channels, both needing work:
  - **(i) Per-call MCP request mutation.** No documented opencode hook mutates outbound MCP request headers/metadata per-call (`chat.headers` mutates the *LLM provider* request, not the MCP-server request; FR #21240 is also provider-call, not MCP). If a plugin injected the agent into the tool **args**, it lands on the same channel the server treats as spoofable — trustworthy at source, indistinguishable at the server. Needs an opencode surface (or upstream FR) to stamp a distinct, plugin-only request field, consumed via `external_identity`.
  - **(ii) MCP-proxy shim** (reuse the `mcp-bridge.js` remote pattern): point `mcp.unimatrix` at a local proxy the plugin runs; the proxy obtains the trusted `agent` (via the SDK/plugin) and stamps it into each forwarded request as a transport-attested field. Feasible, net-new component, lands identity on a server-trusted channel without an opencode change.
- **What Unimatrix would need to consume it** (vs current self-reported `agent_id`): activate `external_identity` in `build_context_with_external_identity` to accept a transport-attested agent identity; route `require_cap` off that trusted identity (validated against, or replacing, the session env var); flip PL-4 from `resolve_or_enroll` fail-open to fail-closed under an enforce flag. Target enforcement shape to validate against (illustrative, not a design): `uni-scrum-master` → read-only (Search/Read, no Write); a delivery agent → may write `lesson-learned` but not `decision`/ADR — a per-agent-**type** policy keyed on the validated `session.agent` opencode supplies.
- **Integrity / SLN1 touch:** SLN1 (#5528) attributes implicit labels to **sessions, not agent_ids**, and rate-limits per-agent influence — a **non-forgeable** agent identity lets that attribution/rate-limiting key on a trustworthy identity, strengthening poison-resistance. The `asserted` trust_source (low-trust, self-asserted content) could have its floor raised for writes a harness-attested identity authorizes.
- **L2 seam (ass-100/101):** PL-4 is the L2 policy/auth seam; ass-100/101 (edge identity + root-of-trust) are its trust-binding leg. opencode's validated `session.agent` is a concrete **root-of-trust candidate and a real demand-pull consumer** — exactly the evidence PL-4 says to build on.

**Verdict:** highest strategic value (unlocks Principle 3 / PL-4), source and consumption legs are ready-or-better; the delivery channel is the one unresolved feasibility question — resolvable by an MCP-proxy shim (ii) without waiting on opencode. Warrants its own design decision inside a C18 cycle, not blind delivery.

### E2 — Sharper per-model provider / `source_domain` attribution incl. local models (C14)
opencode exposes `model:{providerID, modelID}` in-process on `chat.message`, and full `model`/`provider` on `chat.params`/`chat.headers`. Today provider attribution is coarse (`--provider opencode`). With in-process model info, each event can carry the **actual backend model** (e.g. `ollama`/`qwen3-coder`) → per-model `source_domain`/provider attribution, enabling **local-model observation** and multi-LLM parity comparison (C14) — directly serving C18's local-model raison d'être. Feasible now. Maps to the vnc-013 provider field + `source_domain`.

### E3 — Richer in-process behavioral / transcript signal
The stdin contract loses structure: Claude passes a `transcript_path` file the hook re-extracts. opencode's plugin holds the SDK `client` and typed events — structured message parts, tool metadata, mutated args/results, and dedicated `permission.asked/replied`, `file.edited`, `command.executed` events. Improves `unimatrix-observe`: e.g. the `PermissionRetriesRule`/permission-friction metric could read `permission.replied` directly instead of inferring from a Pre/Post differential. Feasible now; net-new plugin-side extraction. Maps to `unimatrix-observe`.

### E4 — Tighter session / cycle binding from typed structured events
Today cycle events are recovered by pattern-matching `context_cycle` tool calls inside PreToolUse (`hook.rs` intercept). opencode's stable `sessionID`/`parentID` give direct subagent→parent linkage and typed events, enabling cleaner session/cycle correlation without stdin marshalling or tool-name matching. Quality/robustness improvement. Maps to cycle binding / `unimatrix-observe`.

---

## Unanswered Questions

- **Do opencode child (subagent) sessions get a separate MCP client connection, or share the parent's?** Determines whether transport-attested `client_type` could carry per-subagent identity without a proxy. Likely shared, but **undocumented** — the `Session` schema was not exhaustively verified. Reason: needs an opencode MCP-client-per-session source dive or a PoC. (Directly gates E1 delivery channel (i).)
- **Does opencode expose any plugin surface to mutate outbound MCP tool-call requests (headers/metadata) per-call?** Not documented; FR #21240 covers LLM provider calls, not MCP. Reason: no documented surface; needs source dive or upstream FR. (Gates E1 channel (i); channel (ii) proxy is the fallback.)
- **Stability of `experimental.session.compacting` (PreCompact).** Marked experimental upstream. Reason: API may change; PreCompact parity rests on an unstable surface.
- **Does `session.idle` fire exactly once per Stop with derivable `duration`/`outcome`, or on every idle transition?** Undocumented semantics; risk of `SessionClose` over-counting. Reason: needs a PoC measurement in a C18 spike/cycle.

---

## Out-of-Scope Discoveries

- **opencode `chat.message`/`chat.params`/`chat.headers` are mutable** — Unimatrix could inject retrieval/policy at inference time far more richly than Claude's stdout injection. Potential future "active governance" capability; beyond C18 observation scope. Flag.
- **`permission.ask` hook** could make Unimatrix a policy decision point for tool permissions (allow/deny/ask) — a capability-enforcement surface distinct from knowledge writes. Relevant to the Principle-3/enforcement roadmap; not C18. Flag.
- **opencode `bun install`s `.opencode/package.json` deps at startup** (`@opencode-ai/plugin@1.18.31` already pinned) — a supply-chain/installer consideration for a C17 security review; not designed here.
- **`.claude/hooks/*.sh` scripts in this repo are now inert `exit 0` stubs** (JSONL path removed per col-012) — confirms all observation flows through `unimatrix hook`/UDS, no legacy file path. Not actionable for C18.

---

## Recommendations Summary

- **A — Extension surface**: opencode is **in-process TS plugin API only**; no command hooks. Plugins load from `.opencode/plugins/` or the `plugin:[]` array; typed mutable hooks + an observe-only event bus.
- **B — Parity**: 4/7 have direct typed equivalents (UserPromptSubmit=`chat.message`, PreToolUse=`tool.execute.before`, PostToolUse=`tool.execute.after`, PreCompact=`experimental.session.compacting`); SessionStart/Stop are bus-derived (`session.created`/`session.idle`); SubagentStart is derived from child `session.created`+`parentID`. **Every event needs a shape shim — the Claude stdin schema cannot be consumed directly.** Parity gap = **SubagentStart retrieval-injection** (observe-only bus event, cannot inject) + no stdin contract at all + degraded Stop/SessionStart derivation + PreCompact on an experimental API.
- **C — Ingestion**: **(a) command-hook mirror is impossible** (no mechanism — Codex model does not generalize); **(b) TS plugin shim → `unimatrix hook --provider opencode` is recommended** (reuses the full Rust pipeline); **(c) `--provider opencode` branch is required as a component of (b), not an alternative**. `source_domain` needs a `resolve_source_domain`/domain-pack `opencode` mapping, not just the flag.
- **D — Installer (C17 extension)**: net-new opencode detect + plugin provisioning (`.opencode/plugins/` + `.opencode/package.json`, or `plugin:[]` merge). Reuse the nan-004 non-clobber merge *principle*, not its per-event matcher logic (different surface). **Preserve `mcp.unimatrix` retrieval / local STDIO byte-for-byte** (C17 regression sentinel).
- **E — Doing better (ranked)**: (1 lead) **harness-attested trusted identity** — better than Claude at the source (validated `session.agent`, LLM out of loop) and the consumption seam (`external_identity`) already exists + PL-4 fail-closed flip; the open question is the per-call MCP delivery channel (resolvable via an MCP-proxy shim). (2) per-model `source_domain` attribution incl. local models (C14), feasible now. (3) richer in-process behavioral/permission signal for `unimatrix-observe`. (4) tighter typed session/cycle binding.
- **Overall for the uni-zero decision**: opening a C18 cycle is justified. The ingestion path is feasible and reuse-heavy (plugin shim + provider branch). Scope it as **design-then-delivery**, not straight delivery — the trusted-identity leg (E1) carries a real design decision (delivery channel) and the installer leg (D) is net-new. The measured parity gap (SubagentStart injection + no stdin contract) is inherent to opencode's architecture and should be recorded as C18's `done_when`, not treated as a defect to close.
