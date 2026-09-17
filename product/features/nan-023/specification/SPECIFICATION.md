# SPECIFICATION — nan-023: Install Options — Per-Harness Idempotent Wiring, Non-Destructive Definition Install

> Feature: nan-023 | Issue: #990 | Capability: **C14 (multi-LLM harness parity)** | Goal: `personal-cloud` (#4946)
> Source of truth: `product/features/nan-023/SCOPE.md` (16 ACs, decided policy block) and `SCOPE-RISK-ASSESSMENT.md` (SR-01..SR-12).
> **Scope boundary (binding):** this feature governs what the **installation package** provisions *into consumer repos* via the JS client / dogfood mechanism. It does NOT touch or "fix" this repo's own local harness config.

---

## 1. Objective

Give the Unimatrix installation package a non-destructive definition-install policy and a per-harness, idempotent **wiring** path so that claude-code, opencode, and codex-cli each reach C14 multi-LLM harness parity in a consumer repo. Parity means the package writes each harness's MCP/retrieval and hook config non-destructively AND that, after wiring, a `context_*` retrieval call against the wired slug **returns** and wired hooks **fire** — not merely that config is present. gemini-cli is deferred (out of scope).

---

## 2. Domain Models / Ubiquitous Language

| Term | Definition |
|------|------------|
| **Harness** | A supported coding-agent runtime the package wires: `claude-code`, `opencode`, `codex-cli`. `gemini-cli` is a known harness but **out of scope** this feature. |
| **Wiring surface** | The set of config artifacts a harness reads for MCP/retrieval and hooks. Per harness: claude-code = `.mcp.json` + `.claude/settings.json`; opencode = `opencode.json` (`mcp.unimatrix`) + `.opencode/plugins/` shim + `opencode.json` `plugin[]`; codex-cli = `.codex/config.toml` (`[mcp_servers.unimatrix]`) + Codex hooks. |
| **Definition** | A Unimatrix-owned content file installed into the consumer repo. **Scope = skills only** this feature (`.claude/skills/…`). Protocols and agents are explicitly NOT definitions here. |
| **wire vs init** | `init` = definition install + wiring + DB/validation for a fresh setup. `wire` = a distinct verb that runs the wiring layer **only** and touches zero definition files, no DB steps. |
| **Detection marker** | A project-local filesystem marker that signals a harness is present: `.mcp.json`/`.claude/` (claude-code), `opencode.json`/`.opencode/` (opencode), `.codex/` (codex-cli), `.gemini/` (gemini, ignored). Absence of a marker ⇒ that harness leg is a no-op. |
| **JS hook client target** | The executable every Claude AND Codex hook command invokes: `packages/unimatrix/lib/hook-client/index.js`, run as `node <clientPath> <EVENT> …`. Hooks target this client, **never** the `unimatrix` binary. |
| **Provider attribution (`--provider codex-cli`)** | A mandatory flag on every Codex hook command. Codex shares event names with claude-code; omitting the flag silently mislabels events as `claude-code` (vnc-013 ADR-006). |
| **Retrieval slug** | The wired MCP server identity a `context_*` retrieval call resolves against per harness (`mcpServers.unimatrix`, `mcp.unimatrix`, `[mcp_servers.unimatrix]`). The **return path** asserts against this slug. |
| **Install-if-absent** | Default definition-install policy: write a Unimatrix-owned definition only when it does not already exist on disk; never overwrite an existing one. |
| **`--force`** | Definition-install override: overwrite Unimatrix-owned definitions (skills) with shipped versions. **Definitions-only** — it never re-asserts or overwrites wiring. |
| **Foreign key / entry** | Any config content the package does not own (foreign MCP servers, foreign hooks, foreign `[mcp_servers.*]` tables, the opencode Ollama `provider` block). Must survive **byte-for-byte**. |
| **Non-clobbering merge** | The `mergeSettings`/`opencode-install.js` principle: detect → parse-safe → additive/install-if-absent merge → format-preserving write → warn-and-skip on malformed. |
| **Containment guard** | `isWithinProject`/`detectProjectRoot` (`.git` walk): no write ever lands outside the resolved project root. |
| **Return path** | A behavioral assertion that a `context_*` retrieval call against the wired slug **returns** from the wired command after wiring. Load-bearing: config presence alone does not satisfy parity. |
| **Fire (hook firing)** | A behavioral assertion that a wired hook event reaches the JS hook client (claude-code, codex-cli) or the in-process TS plugin (opencode). |

---

## 3. Functional Requirements

Each FR is testable. IDs referenced by the acceptance criteria in §6.

### Command surface

- **FR-01** — `unimatrix init` performs definition install (skills, install-if-absent) + wiring for detected harnesses + existing DB/validation steps. The claude-code common path (`.mcp.json` + `.claude/settings.json`) remains byte-identical to today's output.
- **FR-02** — `unimatrix init --force` overwrites Unimatrix-owned skill files with shipped versions and performs no wiring re-assertion beyond the normal always-additive wiring pass. Foreign files under skill directories are untouched.
- **FR-03** — `unimatrix wire` is a distinct verb that runs the wiring layer only: ensures MCP + hooks + retrieval for every detected harness and writes/modifies zero definition files and performs no DB steps.
- **FR-04** — `unimatrix wire --harness <name>` targets a single named harness (`claude-code` | `opencode` | `codex-cli`). It is the explicit opt-in required to write a **new** MCP/retrieval entry into a user-owned config (see FR-14).
- **FR-05** — `unimatrix wire --dry-run` and `unimatrix init --dry-run` compute and print every intended action prefixed `[dry-run]` and write nothing to disk.
- **FR-06** — Argument routing in `bin/unimatrix.js` dispatches `init`, `init --force`, `wire`, `wire --harness <name>`, and `--dry-run` on either verb. An ambiguous or unrecognized invocation prints help/usage rather than silently falling through to a default install (#960).

### Definition install (skills only)

- **FR-07** — Definition install replaces the current blanket `copySkills` overwrite with a per-file existence check: a Unimatrix-owned skill that already exists on disk is left byte-for-byte unchanged (install-if-absent).
- **FR-08** — `--force` restores shipped versions of Unimatrix-owned skill files only. Non-Unimatrix (foreign) files in the same directories are never read, moved, or written.
- **FR-09** — Definition install never touches protocols (`.claude/protocols/`) or agents (`.claude/agents/`). Help/output states the skills-only boundary explicitly (SR-05).

### Per-harness wiring — retrieval / MCP

- **FR-10** — **claude-code**: wiring ensures `mcpServers.unimatrix` in `.mcp.json`, preserving foreign MCP servers (existing behavior, retained non-destructively).
- **FR-11** — **opencode**: wiring ensures the `mcp.unimatrix` retrieval entry in `opencode.json`. An existing `mcp.unimatrix`, the Ollama `provider` block, and all foreign keys are preserved byte-for-byte (vnc-049 sentinel). This retrieval entry is **new** installer behavior (opencode-install.js provisions only the plugin today).
- **FR-12** — **codex-cli**: wiring writes/merges `[mcp_servers.unimatrix]` into project-local `.codex/config.toml` using a format-preserving TOML writer, preserving foreign `[mcp_servers.*]` tables and all other TOML keys, comments, and ordering. Transport key follows deployment: `command` (stdio, local) or `url` (streamable HTTP, cloud).

### Per-harness wiring — hooks

- **FR-13** — **codex-cli**: wiring provisions Codex hooks in a Claude-like, event-driven shape (`type: "command"`), preserving foreign hook entries. Every hook command:
  - (a) targets the JS hook client `packages/unimatrix/lib/hook-client/index.js` (`node <clientPath> <EVENT> …`), **not** the `unimatrix` binary — asserted against the written command string;
  - (b) carries `--provider codex-cli` — asserted present on every command string.
  - opencode hook/observation stays the in-process TS plugin (unchanged surface); claude-code hooks stay `.claude/settings.json` (unchanged surface).

### Detection + intent model (#960)

- **FR-14** — Writing a **new** MCP/retrieval entry into a user-owned config (`opencode.json`, `.codex/config.toml`) requires explicit `--harness`/opt-in. Merging additively into a surface the user already has requires no extra opt-in.
- **FR-15** — Every harness leg is gated on its project-local detection marker. A harness with no marker present produces no writes and no error (silent no-op is correct only for *undetected* harnesses; see FR-17 for detected-but-skipped).

### Behavioral / parity verification

- **FR-16** — After wiring a harness, the package exposes a verifiable **return path**: a `context_*` retrieval call against that harness's wired slug returns. This is asserted from the wired command path, in both local and cloud deployment where feasible.
- **FR-17** — A wiring leg that is skipped for any reason other than "marker absent" (e.g. malformed config) is **visibly reported** (warning surfaced), never a silent success (SR-08).

### Cross-cutting

- **FR-18** — Every writer (new and existing) honors the containment guard: no write outside the resolved project root, ever.
- **FR-19** — Every harness leg is malformed-safe: invalid JSON/TOML input is left unchanged, that leg is skipped with a warning, and init/wire never throw or partially write because of it.

---

## 4. Non-Functional Requirements

- **NFR-01 (Idempotence)** — Running any wiring path (`init` wiring pass or `wire`) twice in succession produces byte-identical config output for every supported harness. Measurable: byte-diff of each wiring surface between run 1 and run 2 is empty.
- **NFR-02 (Non-destructive / byte-for-byte preservation)** — Foreign content in every touched config survives byte-for-byte: foreign MCP servers (`.mcp.json`), the opencode `mcp.unimatrix` + Ollama `provider` block + foreign keys (`opencode.json`), foreign `[mcp_servers.*]` tables + all keys/comments/ordering (`.codex/config.toml`), and foreign hook entries. Measurable: byte-diff of foreign regions is empty before/after.
- **NFR-03 (Malformed-safe / warn-and-skip)** — On invalid JSON/TOML, the leg is skipped with a surfaced warning; input preserved unchanged; no throw, no partial write. Mirrors `opencode-install.js` fail-safe posture. A skip is a distinct, reported, asserted outcome — not a silent pass.
- **NFR-04 (Containment / project-scoped only)** — Root resolved via the existing `.git` walk (`detectProjectRoot`). No path writes to any config outside the resolved root — asserted specifically for `~/.codex/`, `~/.claude/`, `~/.gemini/`, `~/.config/`.
- **NFR-05 (Local AND cloud deployment)** — The JS-client install path — including Codex hook wiring, the retrieval return path, and transport selection (stdio vs HTTP) — functions under both local and cloud deployment. Behavioral assertions (return path, hook fire) run in both where feasible.
- **NFR-06 (Backward compatibility)** — The existing claude-code `.mcp.json` + `.claude/settings.json` output stays byte-identical for the common path after refactor into a per-harness layer (golden-file before refactor, SR-07). Install-if-absent must not break first-run installs.
- **NFR-07 (Attribution correctness)** — Every written Codex hook command carries `--provider codex-cli`; a missing flag is a fail-loud defect, not a warning (SR-10).
- **NFR-08 (Format preservation — TOML)** — The Codex TOML writer preserves foreign keys, comments, and formatting/ordering across a read-modify-write round trip (SR-01). Treated as first-class acceptance, not best-effort.
- **NFR-09 (Trust precondition surfaced)** — Codex `.codex/config.toml`/hooks load only for a trusted `.codex/` layer. This precondition is surfaced to the consumer; if trust is absent, the leg fails loud / warns — never a silent no-op that reads as success (SR-02).
- **NFR-10 (Dry-run fidelity)** — The `--dry-run` action set matches the real write path exactly (same set of intended actions), and writes nothing. Dry-run must not diverge from the real path (SR-12).

---

## 5. User Workflows / Use Cases (entry-point → outcome)

Outcomes are what the consumer OBSERVES, path-independent (aligned with SCOPE-RISK-ASSESSMENT entry-point table).

### W1 — claude-code consumer, fresh repo
`unimatrix init` → Unimatrix skills installed (install-if-absent); `.mcp.json` `mcpServers.unimatrix` + `.claude/settings.json` hooks present, byte-identical to today. Return path: a `context_*` call against the stdio MCP slug returns. Firing: claude hooks reach the JS hook client.

### W2 — Any consumer, re-run after editing an installed skill
`unimatrix init` (edited skill on disk) → the edited skill survives byte-for-byte; no skill clobbered (AC-01).

### W3 — Consumer refreshing shipped skills
`unimatrix init --force` → Unimatrix-owned skills replaced with shipped versions; foreign files untouched; wiring is NOT re-asserted/overwritten by `--force` (AC-02).

### W4 — Consumer refreshing wiring without touching definitions
`unimatrix wire` → MCP + hooks + retrieval ensured for every detected harness; zero definition files written (AC-03). Re-run yields byte-identical config (AC-04).

### W5 — opencode consumer
`unimatrix wire` (opencode detected) → `mcp.unimatrix` retrieval entry ensured in `opencode.json`; existing entry, Ollama `provider` block, foreign keys preserved byte-for-byte; a `context_*` call still returns (AC-05/AC-10). Observation/firing is via the in-process TS plugin (plugin-level assertion, not JS-hook-client-level).

### W6 — codex-cli consumer (full parity leg)
`unimatrix wire --harness codex-cli` (explicit opt-in) → `[mcp_servers.unimatrix]` written/merged into `.codex/config.toml` preserving foreign tables; Claude-like Codex hooks written targeting the JS hook client with `--provider codex-cli` on every command; in a trusted `.codex/` environment, a `context_*` call **returns** AND a wired Codex hook event **actually fires** to the JS hook client (AC-06/07/08/09/10). Asserted in local and cloud where feasible.

### W7 — Any consumer, preview
`unimatrix wire --dry-run` (or `init --dry-run`) → intended actions printed with `[dry-run]`; nothing written (AC-14).

### W8 — Consumer with an undetected harness
`unimatrix wire` on a repo with no `.codex/` marker → no Codex writes, no error (AC-13). A *malformed* detected config, by contrast, is skipped with a surfaced warning (AC-15).

---

## 6. Acceptance Criteria (tracing SCOPE AC-01..AC-16)

Verification method is stated per AC. **Behavioral ACs (AC-05 return, AC-08/09 codex hooks, AC-10 return) are asserted from the wired command path — never discharged by a config-presence/proxy check (SR-09 ceremonial-wiring guard).**

| AC | Requirement | FR | Verification method |
|----|-------------|-----|---------------------|
| **AC-01** | `unimatrix init` never overwrites an existing Unimatrix-owned skill by default; a modified existing skill survives a re-run byte-for-byte. | FR-07 | Fixture repo with a modified installed skill; run `init`; byte-diff the skill file = empty. |
| **AC-02** | `init --force` overwrites Unimatrix-owned skills with shipped versions; foreign files under same dirs untouched; `--force` re-asserts/overwrites **no** wiring. | FR-02, FR-08 | Modified skill + foreign file + pre-existing wiring; run `--force`; skill == shipped, foreign byte-identical, wiring surfaces byte-identical to pre-run (assert zero wiring changes, SR-04). |
| **AC-03** | `unimatrix wire` ensures MCP + hooks + retrieval for every detected harness and writes/modifies zero definition files. | FR-03 | Multi-harness fixture; run `wire`; assert wiring surfaces changed as expected AND skill/protocol/agent trees byte-identical. |
| **AC-04** | Wire path run twice → byte-identical config for every supported harness (idempotent). | NFR-01, FR-03 | Run `wire` twice; byte-diff each wiring surface between runs = empty. |
| **AC-05** | opencode detected → `mcp.unimatrix` ensured; existing entry + Ollama `provider` block + foreign keys preserved byte-for-byte; **retrieval still returns**. | FR-11, NFR-02 | Run against opencode fixture; byte-diff sentinel regions = empty; then issue a `context_*` retrieval against the wired slug and assert it RETURNS (vnc-049 sentinel, SR-06). |
| **AC-06** | Codex detected → `[mcp_servers.unimatrix]` written/merged into `.codex/config.toml`; foreign `[mcp_servers.*]` and all other keys preserved. | FR-12, NFR-08 | TOML fixture with foreign tables/comments; run wire; assert `[mcp_servers.unimatrix]` present AND foreign tables/keys/comments/ordering byte-preserved (SR-01). |
| **AC-07** | Codex detected → hooks in Claude-like, event-driven shape with `--provider codex-cli` on every command; foreign hook entries preserved. | FR-13 | Parse written Codex hooks; assert event-driven `type:"command"` shape, `--provider codex-cli` on every command, foreign hooks byte-preserved. |
| **AC-08** | Codex hook commands target the **JS hook client** (`hook-client/`), not the unimatrix binary — asserted against the written command string. | FR-13(a) | String-assert each written command invokes `node <…/hook-client/index.js> …`; assert it does NOT invoke the `unimatrix` binary. |
| **AC-09** | Package-wired Codex hooks **actually fire** — a wired hook event reaches the JS hook client (behavioral, not presence). Asserted in local AND cloud where feasible. | FR-13, FR-16, NFR-05, NFR-09 | In a trusted `.codex/` env, trigger the wired hook event and assert it arrives at the JS hook client (observed ingress), local and cloud. Not satisfiable by config presence (SR-09). |
| **AC-10** | After wiring, a `context_*` retrieval against the wired slug **returns** (not just config written). Per-harness: claude-code required; codex-cli required (trust-gated); opencode required via `mcp.unimatrix` (firing = plugin-level). Local AND cloud where feasible. | FR-16, NFR-05 | Per harness, issue a `context_*` retrieval against the wired slug from the wired command path and assert a non-empty RETURN. Config-presence check is explicitly insufficient (SR-09). |
| **AC-11** | Writing a **new** MCP/retrieval entry into a user-owned config requires explicit `--harness`/opt-in; additive merge into an existing surface needs none. | FR-04, FR-14 | With no `--harness`, assert no new `mcp.unimatrix`/`[mcp_servers.unimatrix]` written into a config lacking it; with `--harness <x>`, assert it is written. Additive merge into existing surface proceeds without opt-in (SR-11). |
| **AC-12** | No path writes any config outside the resolved project root (asserted for `~/.codex/`, `~/.claude/`, `~/.gemini/`, `~/.config/`). | FR-18, NFR-04 | Run init/wire in a sandbox; assert zero writes to the named global paths (SR-12). |
| **AC-13** | Wiring gated on project-local markers; a harness with no marker → no writes, no error. | FR-15 | Repo missing a harness marker; run wire; assert no writes for that leg and exit success. |
| **AC-14** | Both definition install and wiring honor `--dry-run`: intended actions printed with `[dry-run]`, nothing written. | FR-05, NFR-10 | Run each verb with `--dry-run`; assert `[dry-run]`-prefixed output AND zero filesystem changes; action set matches real-path action set. |
| **AC-15** | Malformed config (invalid JSON/TOML) for any harness → preserved unchanged, leg skipped with a warning; init/wire never throw or partially write. | FR-19, NFR-03, FR-17 | Inject malformed config per harness; assert input byte-preserved, warning surfaced (visible, not swallowed — SR-08), no throw, no partial write. |
| **AC-16** | Command surface + per-harness wiring consistent with #960 — no path silently installs an unintended surface; ambiguous invocations surface help. | FR-06, FR-14 | Assert no silent new-surface install; assert ambiguous/unknown invocation prints help rather than defaulting to install. |

**AC coverage:** AC-01..AC-16 all present. No AC is discharged by a tautology or config-presence proxy; AC-05/08/09/10 carry explicit behavioral-assertion verification (SR-09 guard).

---

## 7. Constraints

- **C-01 Project-scoped only** — root via existing `.git` walk (`detectProjectRoot`); never write global/user config (NFR-04).
- **C-02 New TOML reader/writer** — `.codex/config.toml` is the first non-JSON writer for the JS installer; must be format-preserving (foreign keys/comments/ordering). Library vs. minimal in-house writer is a design-phase decision (open question). (SR-01, NFR-08)
- **C-03 Codex hooks target the JS hook client** — every Codex hook command invokes `packages/unimatrix/lib/hook-client/index.js` (`node <clientPath> <EVENT>`), mirroring Claude; never the `unimatrix` binary. (AC-08)
- **C-04 `--provider codex-cli` mandatory** on every Codex hook command (vnc-013 ADR-006); missing = fail-loud defect. (NFR-07, SR-10)
- **C-05 vnc-049 retrieval sentinel** — `mcp.unimatrix` + Ollama `provider` block in `opencode.json` survive byte-for-byte; regression test asserts retrieval still returns after wiring. (AC-05, SR-06)
- **C-06 C14 return path is load-bearing** — a leg is not "wired" for parity unless a `context_*` retrieval returns after wiring; config presence alone is insufficient. (AC-10, SR-09)
- **C-07 Local AND cloud deployment** — JS-client install path (incl. Codex hook wiring, retrieval return, stdio-vs-HTTP transport) must work under both; ACs assert both where feasible. (NFR-05, SR-03)
- **C-08 Codex trust-gating** — project-local `.codex/config.toml`/hooks load only for a trusted `.codex/` layer; ship target is functional hooks + returning retrieval verified in a trusted env; trust surfaced as a precondition, never a silent no-op. (NFR-09, SR-02)
- **C-09 Non-clobbering / fail-safe** — every leg warns-and-skips on malformed input, never throws mid-init; mirrors `opencode-install.js`. (NFR-03)
- **C-10 Backward compatibility** — claude-code common-path output byte-identical after per-harness refactor; install-if-absent must not break first-run. (NFR-06, SR-07)
- **C-11 Idempotence + dry-run + containment** apply to every new writer. (NFR-01, NFR-04, NFR-10)
- **C-12 `--force` blast radius** — definitions-only (skills); wiring always-additive; `--force` writes zero wiring changes. (AC-02, SR-04)
- **C-13 Definition scope = skills only** — protocols/agents excluded; boundary stated in help/output; do not silently widen. (FR-09, SR-05)

---

## 8. Dependencies

- **JS installer package** — `packages/unimatrix/lib/init.js` (`copySkills` at :130, hook wiring at :502–518), `bin/unimatrix.js` (arg routing), `merge-settings.js` (non-clobber merge principle), `opencode-install.js` (additive/fail-safe/containment reference), `resolve-binary.js`.
- **JS hook client** — `packages/unimatrix/lib/hook-client/index.js` (the executable Claude and Codex hooks target); `normalize.js` (split-brain mirror of Rust `hook.rs` normalizer — a new provider arm requires editing both, guarded by parity corpus; noted for downstream, not a wiring change here).
- **TOML dependency (new)** — a format-preserving TOML reader/writer for `.codex/config.toml` (C-02).
- **vnc-049 / ADR-006 (Unimatrix #5743)** — non-clobbering OpenCode branch + C17 retrieval regression sentinel.
- **#960** — explicit-intent install model (intent gating for new user-owned-config entries).
- **#16732 (upstream, FIXED)** — Codex MCP-tool-call hooks fire; assumed fixed (SCOPE Decided §7); if regressed, AC-09 unmeetable (SR-02/SR-09 escalate).
- **vnc-013 ADR-006** — provider attribution requirement (`--provider codex-cli`).
- **Deployment** — local and cloud Unimatrix server (stdio and streamable-HTTP MCP transports).

---

## 9. NOT in Scope (explicit exclusions)

- **gemini-cli parity** — deferred; hand-authored reference config stands.
- **Definition install beyond skills** — protocols (`.claude/protocols/`) and agents (`.claude/agents/`) NOT brought under install-if-absent/`--force`.
- **Hash-tracked / 3-way-merge definition updates** — rejected in favor of install-if-absent + `--force`.
- **Writing to any global/user-level config** — `~/.codex/config.toml`, `~/.claude/`, `~/.config/`, `~/.gemini/` are never written.
- **Changing the dogfood definition source-of-truth flow** — definitions flow into the package via `/uni-release`; this feature governs consumer repos only.
- **New observation/retrieval capability per harness** — depth of what each harness observes is separate per-harness work (C18/vnc-049); this feature only *wires* existing surfaces.
- **Fixing this repo's own local harness config** — scope is the installation package's output into consumer repos, not this repo's config.
- **`--force` re-asserting wiring** — wiring is always-additive; `--force` is definitions-only.

---

## 10. Open Questions (for architect / design phase)

1. **TOML writer implementation** — library vs. minimal in-house format-preserving writer for `.codex/config.toml`, subject to NFR-08 (foreign key/comment/ordering preservation). Design-phase decision, not a scope blocker. (SCOPE Open Q1)
2. **Codex hook event coverage** — the exact Claude-mirroring lifecycle event set the Codex hook writer emits. Design-phase detail within the decided Claude-like approach. (SCOPE Open Q2)
3. **Cloud return-path/firing feasibility per harness** — SCOPE marks local+cloud assertions "where feasible"; the architect/risk-strategist should pin exactly which of AC-09/AC-10 run green in cloud vs local-only, and whether a pre-tag real-server exercise is needed to avoid the multi-round tag tax (SR-03).
4. **Trust-precondition surfacing mechanism** — how the consumer is told a `.codex/` layer must be trusted for AC-09/10 to hold (warn text, exit behavior when untrusted) — needs a concrete surface (NFR-09, SR-02).

---

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — returned prior installer/harness knowledge: #5743 (vnc-049 ADR-006 non-clobbering OpenCode branch + retrieval sentinel), #5737 (adding an observation harness = three coupled touchpoints; opencode has no command-hook, plugin-only; provider≠source_domain), #5755 (opencode plugin HookInput shim trap), #4925 (nan-016 fixed install-dir ADR). Applied to domain models, constraints, and dependencies. No net-new generalizable pattern to store (spec decisions are feature-specific; read-only tier).
