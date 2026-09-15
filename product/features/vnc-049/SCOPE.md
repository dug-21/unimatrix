# vnc-049 — OpenCode observation harness: behavioral-signal parity + local-model attribution (C18)

## Problem Statement
OpenCode is already wired for **retrieval** (MCP `context_*` via `opencode.json`, C10 — proven) but
emits **no behavioral signal** — it has no hooks. This is the missing half of **C18** and the last
red item on personal-cloud's floor. OpenCode is the strategic on-ramp for local LLMs and the forcing
function for harness-agnostic observation. This feature delivers OpenCode as the **fourth observation
harness** (after claude-code, gemini-cli, codex-cli), reusing the existing Rust ingestion pipeline
unchanged.

Grounding: `product/research/ass-106/SCOPE.md`, `product/research/ass-106/ass-106-findings.md` (GH #982).
Feature intent: GH #986. Design-cycle scope boundary set by uni-zero 2026-09-15 (honored below exactly).

## Goals
1. OpenCode emits all 7 canonical behavioral events into the existing pipeline with correct provider
   and `source_domain` attribution.
2. Subagent telemetry is captured and aligned to the owning feature/cycle (the show-stopper leg).
3. The C17 installer provisions the OpenCode plugin without clobbering existing retrieval config.
4. Local-model activity is distinguishable via per-model `source_domain` attribution (E2 —
   architect-sized).

## Non-Goals
- **SubagentStart retrieval-injection.** OpenCode surfaces subagent spawns only as an observe-only bus
  event (`session.created` + `parentID`); it cannot inject Unimatrix retrieval into the new subagent's
  context the way Claude's `SubagentStart` hook stdout does. This is an **accepted parity gap**, not a
  defect. Observation of the spawn is in scope; injection is not.
- **Full E1 trusted-identity design/delivery.** Harness-attested agent identity → capability
  enforcement is demand-pulled under trusted-identity #5734. Only a **forward-compat constraint**
  applies here (see Constraints), not design or delivery.
- Command-hook mirror (`.codex/hooks.json` model) — architecturally impossible; OpenCode has no
  command-hook mechanism (ass-106 finding A/C).
- Bringing the whole of `background.rs` or `listener.rs` under the 500-line cap (a targeted carve is
  an open design decision, below — not a requirement).

## Background Research
Verified against the codebase (paths absolute under `/workspaces/unimatrix`):

- **Extension surface.** OpenCode is **in-process TypeScript plugin API only** (no stdin/exit-code
  command-hook contract). Plugins load from `.opencode/plugins/` or the `plugin:[]` array in
  `opencode.json`. `@opencode-ai/plugin@1.18.31` is already the sole dep in `.opencode/package.json`;
  no plugin code exists yet. `opencode.json` today has `mcp.unimatrix` (retrieval) and a local Ollama
  `provider` block, but **no `hooks`/`plugin` wiring**.
- **The pipeline reuses unchanged.** A TS plugin shim maps each OpenCode event to a Claude-shaped
  `HookInput` and shells to `unimatrix hook <EVENT> --provider opencode`. `HookInput`/`ImplantEvent`
  live in `crates/unimatrix-engine/src/wire.rs`; the CLI hook path in
  `crates/unimatrix-server/src/uds/hook.rs`.
- **Provider allowlist (the exact touchpoint).** `crates/unimatrix-server/src/uds/hook.rs:158` —
  `const KNOWN_PROVIDERS: &[&str] = &["claude-code", "gemini-cli", "codex-cli"];`. Adding `"opencode"`
  requires a matching arm in `map_to_canonical`/`normalize_event_name`.
- **Split-brain (col-022).** The Rust normalizer (`hook.rs:66-105`) is mirrored by
  `packages/unimatrix/lib/hook-client/normalize.js` ("exact port of hook.rs:50-105"). Both must gain
  the `opencode` arm, and the parity corpus test (`crates/unimatrix-server/src/uds/parity_corpus_uds.rs`)
  must cover it. Changing one without the other is a known defect class.
- **`source_domain` is event-derived, not provider-derived.** `resolve_source_domain(event_type)`
  (`crates/unimatrix-observe/src/domain/mod.rs:180`) matches the event against a `DomainPack`; the hook
  ingress path forces `DEFAULT_HOOK_SOURCE_DOMAIN = "claude-code"`
  (`crates/unimatrix-server/src/services/observation.rs:572`). So `--provider opencode` alone sets
  only `ImplantEvent.provider`; correct `source_domain` requires an **explicit `opencode` domain-pack /
  resolution path** — a named requirement, not automatic. `source_domain` format is
  `^[a-z0-9_-]{1,64}$`.
- **No model-id carrier exists today.** `ImplantEvent` (wire.rs) carries `provider` but no
  `model_id`/backend-model field; per-model attribution (E2) needs a carrier (payload field or new
  typed field) plumbed from the plugin's in-process `model:{providerID,modelID}`.
- **Line-cap context.** The authoritative rule is the **500 code-line cap (tests excluded)**;
  #966/#965 are stated against that code-line cap. Raw line counts (`background.rs` = 5075,
  `uds/listener.rs` = 10131, `uds/hook.rs` = 4403) are **raw-line figures that must be re-measured in
  code-lines** before judging any carve — do not treat them as code-line counts. C18 touches regions of
  these; the targeted-carve decision is judged against the code-line cap.
- **Installer.** `packages/unimatrix/lib/init.js` writes `.mcp.json` + `.claude/settings.json` only —
  **no OpenCode detection/writer**. nan-004 (#1201) prefix-match non-clobber merge is the *principle*
  to reuse; the surface differs (plugin dir + `plugin[]` array, not per-event matcher groups).

## Proposed Approach
Ingestion path **(b)+(c)** from ass-106: a TS plugin shim (b) constructs the Claude-shaped frame and
invokes `unimatrix hook --provider opencode`; a `--provider opencode` normalization arm (c) in both
`hook.rs` and `normalize.js` canonicalizes event names and stamps the provider; plus an explicit
`opencode` `source_domain` resolution path. The plugin is the normalization boundary the (nonexistent)
stdin contract cannot be. The installer gains a net-new additive OpenCode branch. Rationale: reuses the
entire proven Rust pipeline (UDS transport, queue, fail-open, drop-detector) with zero pipeline
rewrite; the only Rust deltas are the provider arm and the domain resolution.

## Acceptance Criteria
- **AC-01**: A TS plugin shim maps the 7 canonical events — SessionStart, UserPromptSubmit, PreToolUse,
  PostToolUse, SubagentStart, PreCompact, Stop — from OpenCode's typed hooks / event bus to
  Claude-shaped `HookInput` and delivers them via `unimatrix hook <EVENT> --provider opencode`.
  Reachable events fire and land as records; unreachable-by-architecture legs are documented as the
  measured parity gap, not failures.
- **AC-02**: Events carry correct **provider** (`"opencode"`) attribution: `"opencode"` added to
  `KNOWN_PROVIDERS` in `hook.rs` **and** the mirrored arm in `normalize.js`, with the parity corpus
  test (`parity_corpus_uds.rs`) extended and green (split-brain parity, col-022).
- **AC-03**: Events resolve to an **`opencode` `source_domain`** (via an explicit resolution/domain-pack
  path), not the `claude-code` hook-path default — validated by test.
- **AC-04** (show-stopper): Subagent telemetry — child `session.created` + `parentID` + validated
  `session.agent` — is captured and **aligned to the owning feature/cycle**, validated and tested.
- **AC-05**: The C17 installer detects OpenCode (presence of `opencode.json`/`.opencode/`) and
  provisions the plugin **non-clobbering** (`.opencode/plugins/` + `.opencode/package.json` dep, or
  `plugin:[]` append). `mcp.unimatrix` retrieval and the local STDIO command are preserved
  **byte-for-byte**; regression test asserts no retrieval regression.
- **AC-06** (E2, architect-sized — IN by default): Per-model `source_domain` attribution carries the
  real backend `model:{providerID,modelID}` (e.g. `ollama`/`qwen3-coder`) so local-model activity is
  distinguishable — C18's raison d'être. Requires a model carrier plumbed plugin→`ImplantEvent`→
  attribution. Architect may fast-follow if the cycle is too large (see Open Questions).
- **AC-07** (forward-compat constraint, E1): The delivery does **not foreclose** the `external_identity`
  seam (`build_context_with_external_identity`, currently always `None`), keeps a viable per-call MCP
  delivery-channel option open, and does **not** fold the validated `session.agent` into spoofable tool
  args. No E1 enforcement is built; the seam is only kept viable.

## Constraints
- **Split-brain (col-022):** any provider/normalizer change must land in `hook.rs` and `normalize.js`
  together, guarded by the parity corpus test.
- **C17 regression sentinel (#5582):** `mcp.unimatrix` retrieval / local STDIO must not regress —
  installer delta is strictly additive.
- **`source_domain` is event-derived server-side** and defaults to `claude-code` on the hook path;
  `--provider opencode` alone is insufficient for attribution (AC-03).
- **No stdin command contract in OpenCode** — every event must pass through the plugin shim; a
  `--provider opencode` arm is necessary but not sufficient on its own.
- **500 code-line file rule (tests excluded)** vs. C18-touched regions of
  `background.rs`/`listener.rs`/`hook.rs` — judged in code-lines, not raw lines (open decision, below).
- **PreCompact rests on `experimental.session.compacting`** (OpenCode flags it unstable) — stability
  risk on that one leg.
- **E1 forward-compat only** (AC-07) — demand-pulled under #5734; do not build enforcement.
- **Dependencies:** C17 installer provisioning (#5582) — prerequisite for the install leg (AC-05).
  C10 remote retrieval — proven, must be preserved.

## Open Questions
1. **E2 sizing (architect decision):** take per-model attribution (AC-06) inside this cycle, or
   fast-follow? Flagged as an architect sizing decision.
2. **Targeted carve (architect decision):** take a targeted carve of the `background.rs` (#966) region
   C18 touches under the **500 code-line cap (tests excluded)**? Same rule for `listener.rs` (#965)
   *only if it turns out edited, not reused-unchanged*. Judge against code-lines, not the raw counts
   above (which must be re-measured in code-lines). No requirement to bring the whole module under the
   cap. Note as an open design decision.
3. **`session.idle` → Stop semantics:** does it fire exactly once per Stop with derivable
   `duration`/`outcome`, or on every idle transition (SessionClose over-count risk)? Needs a PoC
   measurement (ass-106 unanswered Q).
4. **Subagent MCP connection:** do OpenCode child sessions share the parent's MCP client or get their
   own? Gates whether transport-attested identity could ever carry per-subagent identity without a proxy
   (affects the AC-07 seam viability, not this cycle's delivery).
5. **PreCompact experimental-API stability** — acceptable to depend on it now, or gate/flag it?

## Capability Mapping (C18)
- **C18** (personal-cloud capability, GH #5736 / feature GH #986): "OpenCode attaches as a full
  observation harness (fourth harness)." This feature's `done_when` = AC-01..AC-05 (behavioral-signal
  parity + attribution + subagent alignment + non-clobbering install), with AC-06 (local-model
  attribution) as the architect-sized E2 leg and AC-07 as the E1 forward-compat constraint. The
  measured parity gap (SubagentStart injection; degraded bus-derived Stop/SessionStart) is recorded as
  inherent to OpenCode's architecture, not a defect to close.
- **C18 ledger guardrail (do not defer-and-declare-victory):** C18 does **NOT** reach `proven` on
  AC-01..05 alone. "C18 proven" is tied to **AC-06** — local-model attribution behaviorally
  demonstrated. Whether or not E2 lands this cycle, if AC-06 is not behaviorally demonstrated C18 stays
  **`partial`** (an honest-visible-partial), and if E2 fast-follows C18 remains `partial` in the
  interim. Rationale: shipping AC-01..05 without AC-06 is a harness that observes but cannot distinguish
  ollama/qwen3 from a cloud model — the mechanism without C18's raison d'être. The E2-sizing latitude
  (architect's call) stands; this governs only the ledger status, removing the defer-and-declare-victory
  incentive.
- **C17** (#5582): extended by AC-05 (installer OpenCode branch). Prerequisite.
- **C10** (#5547): retrieval — proven; preserved as regression sentinel.

## Tracking
GH Issue: #986 (feature). Will be updated after Session 1.
