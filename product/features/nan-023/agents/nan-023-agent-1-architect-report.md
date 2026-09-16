# Agent Report — nan-023-agent-1-architect

## Task
Produce architecture + ADRs for nan-023 (per-harness idempotent wiring + non-destructive definition install; C14 parity for claude-code, opencode, codex-cli).

## Deliverables
- `product/features/nan-023/architecture/ARCHITECTURE.md`
- `architecture/ADR-001-per-harness-wiring-layer.md` (Unimatrix #5763)
- `architecture/ADR-002-codex-toml-surgical-writer.md` (Unimatrix #5764)
- `architecture/ADR-003-codex-hooks-js-client-provider.md` (Unimatrix #5765)
- `architecture/ADR-004-definition-install-if-absent.md` (Unimatrix #5766)
- `architecture/ADR-005-wire-verb-harness-intent.md` (Unimatrix #5767)
- `architecture/ADR-006-c14-verification-spine.md` (Unimatrix #5768)

## Key decisions
1. Per-harness wiring layer: `lib/wire.js` orchestrator + one non-clobbering writer per (harness x surface); each writer returns a structured `WireLeg`; aggregated **manifest** is the single input to the C14 verifier, dry-run, and summary.
2. Codex TOML: minimal in-house **surgical block writer** (byte-for-byte foreign-key preservation by construction; zero new dependency) — not a round-trip library.
3. Codex hooks target the **JS hook client**, requiring a new `--provider <name>` argv hint in `index.js` (it currently only infers provider -> would mislabel codex as claude-code; SR-10). Emitted event set defined.
4. Definition install-if-absent + `--force` (skills only, Unimatrix-owned manifest scope, never touches wiring or foreign files).
5. `unimatrix wire` verb + `--harness` routing; #960 intent gate (new user-owned-config entry requires explicit opt-in).
6. C14 verification spine: assert retrieval-RETURNS / hook-FIRES from the wired manifest, local+cloud, with a pre-tag real-server exercise (SR-03/SR-09).

## Modularity flags (PL-10 / #693)
- No file pushed over the 1,000 code-line cap. Wire + codex logic go to NEW modules (`lib/wire.js`, `lib/codex-install.js`), keeping `init.js` (691) under 800.
- **Tight constraint:** `lib/hook-client/` raw total 189,946 B vs 200,000 B backstop (~10 KB headroom). The `--provider` addition to `index.js` must be lean; trim comment prose, never raise the gate (#5372 / #4780).

## Open questions (for design-leader / human, tester)
1. Codex cloud MCP transport — mirror the token-free `.mcp.json` `command="node" args=[bridge,hash]` shape as a TOML `command`/`args` entry, or use `url=`? Needed before AC-10 codex cloud return path.
2. Codex trust-gating in the C14 verifier env — confirm the local+cloud CI fixture can mark `.codex/` trusted, else AC-09 firing is unmeetable (SR-02).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_get -- reused vnc-049 ADR-006 (#5743, non-clobber opencode + retrieval sentinel), per-harness config-surface matrix (#5762), hook-client size gate lesson/decision (#4780/#5372), parity-corpus discipline (#4751), observation-harness three-touchpoints incl. source_domain forcing trap (#5737), vnc-013 ADR-006 (--provider mandate).
- Stored: entries #5763–#5768 (ADR-001..006) via context_store (category decision, topic nan-023). ADR-003 (#5765) asserted 2 Prerequisite edges: ->#5737 (source_domain trap) and ->#5372 (size gate). No supersession required — vnc-013 ADR-006 is a binary-targeting reference config, parallel to this package-written JS-client form, not invalidated.
