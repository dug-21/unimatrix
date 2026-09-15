## ADR-004: Ingestion = TS plugin shim + `--provider opencode` arm in both normalizers + parity corpus

### Context
OpenCode has **no command-hook mechanism** — no stdin/exit-code contract analogous to
`.claude/settings.json` hooks or `.codex/hooks.json` (ass-106 finding A/C). The `.codex/hooks.json`
mirror path is architecturally impossible and does not generalize. OpenCode's only extension surface is
an **in-process TypeScript plugin API** (`@opencode-ai/plugin@1.18.31`, already the sole dep in
`.opencode/package.json`). Pattern #5737 records that onboarding a harness is **three coupled
touchpoints, not one flag**, and col-022 records that the Rust normalizer (`hook.rs`) and its JS mirror
(`normalize.js`) are a split-brain pair — editing one side only is a known defect class (SR-06,
lesson #5670).

### Decision
Adopt ass-106 path **(b)+(c)**:

1. **TS plugin shim (C1)** in `.opencode/plugins/` (provisioned by the installer, ADR-006). It
   subscribes to the typed hooks (`chat.message`, `tool.execute.before/after`,
   `experimental.session.compacting`) and the event bus (`session.created`, `session.idle`), maps each
   to a Claude-shaped `HookInput`, and shells via `PluginInput.$` to
   `unimatrix hook <EVENT> --provider opencode --model <providerID>/<modelID>`. The plugin **is** the
   normalization boundary the missing stdin contract cannot be. It reuses the entire proven Rust
   pipeline unchanged (UDS transport, queue, fail-open, drop-detector). Optimization (later, not this
   cycle): open the per-project UDS socket in-process instead of per-event process spawn.
2. **Provider normalization arm (C2)** — add `"opencode"` to `KNOWN_PROVIDERS`
   (`uds/hook.rs:158`) and an OpenCode arm in `normalize_event_name`/`map_to_canonical`
   (`hook.rs:66-105`). The OpenCode event-name mapping table lives in a **new module**
   `uds/hook/opencode.rs` (ADR-005 thin-wiring); hook.rs gains a const entry + a delegating match arm.
3. **JS mirror (C3)** — the identical arm in `packages/unimatrix/lib/hook-client/normalize.js`, landed
   in the **same change** (col-022).
4. **Parity corpus (C8)** — extend `uds/parity_corpus_uds.rs` with OpenCode cases across all 7 canonical
   events (provider, model_id, subagent, degraded legs) and keep it green; the Rust hook is the oracle
   (vnc-026 ADR #4751). This is the AC-02 gate and the split-brain guard.

`--provider opencode` is **necessary but not sufficient** (it only stamps `ImplantEvent.provider`);
correct attribution requires ADR-001 persistence and ADR-002 model carrier — the other two touchpoints.

### Consequences
Easier: zero pipeline rewrite; the plugin absorbs all shape mapping; the corpus makes divergence a
loud CI failure. Harder: per-event process spawn has overhead (accepted for v1); the JS mirror and
corpus must move in lockstep with hook.rs or the split-brain defect recurs (lesson #5670); the plugin
is a new supply-chain artifact (`bun install` at startup — flagged for C17 security review).
Cross-references ADR-001, ADR-002, ADR-006. Prerequisite: pattern #5737 (three coupled touchpoints).
