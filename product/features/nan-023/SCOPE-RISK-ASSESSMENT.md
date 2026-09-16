# Scope Risk Assessment: nan-023

## User-Facing Entry Points & Behavioral Outcomes

The behavioral contract architecture and tests must BOTH satisfy — authored from SCOPE alone, independent of any implementation path. Outcomes are what the consumer OBSERVES.

| Entry point (as a consumer runs it) | Path-independent outcome they must observe |
|---|---|
| `unimatrix init` (fresh repo) | Unimatrix skills installed; claude-code `.mcp.json`+`.claude/settings.json` present |
| `unimatrix init` (re-run, edited skill exists) | The edited skill survives byte-for-byte; no skill clobbered (AC-01) |
| `unimatrix init --force` | Unimatrix-owned skills replaced with shipped versions; foreign files untouched; wiring NOT re-asserted (AC-02) |
| `unimatrix wire` | MCP+hooks+retrieval ensured for every detected harness; zero definition files written (AC-03) |
| `unimatrix wire` (run twice) | Second run yields byte-identical config for every harness (AC-04) |
| `unimatrix wire` (opencode detected) | `mcp.unimatrix` ensured; existing entry, Ollama `provider` block, foreign keys survive byte-for-byte; retrieval still returns (AC-05) |
| `unimatrix wire --harness codex-cli` | `[mcp_servers.unimatrix]` in `.codex/config.toml`; Claude-like hooks targeting the JS hook client with `--provider codex-cli`; a `context_*` call returns AND a wired hook event actually fires (AC-06/07/09/10) |
| `unimatrix wire --dry-run` | Intended actions printed with `[dry-run]`; nothing written (AC-14) |

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | New TOML reader/writer (first non-JSON writer) must preserve foreign `[mcp_servers.*]` and all keys/comments/formatting; naive round-trip drops or reorders them | High | Med | Choose a format-preserving TOML approach; treat foreign-key survival as an acceptance assertion, not a hope (AC-06) |
| SR-02 | "Wired" is defined as retrieval RETURNS + hooks FIRE, but Codex retrieval/hooks are trust-gated; assertions run in a trusted env while a real consumer's `.codex/` may be untrusted — green in test, inert for the user | High | Med | State trust as a precondition surfaced to the consumer; assert return/fire from the actual command path; if trust absent, fail loud / warn, never silent no-op |
| SR-03 | Cloud + local return-path/firing gates are release-only and have never run green on a tag; per #5267 (nan-019/nan-020) such chains fail in sequence, one tag round each | High | High | Budget multiple tag rounds; prioritize a PRE-TAG real-server exercise of codex hook-fire + retrieval-return so layers surface before release |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | `--force` blast radius: SCOPE decides definitions-only (skills), but a writer that also re-asserts wiring on `--force` silently violates AC-02 | Med | Med | Keep `--force` definitions-only; wiring always-additive; assert `--force` writes zero wiring changes |
| SR-05 | Definition-install scope is skills-only; protocols/agents excluded — a consumer expecting full parity may read "parity" as all definitions | Low | Med | Keep the skills-only boundary explicit in help/output; do not silently widen |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | vnc-049 sentinel regression: adding `mcp.unimatrix` to `opencode.json` disturbs the Ollama `provider` block or foreign keys | High | Med | Reuse the `opencode-install.js` non-clobber principle; byte-for-byte preservation test + retrieval-still-returns regression (AC-05) |
| SR-07 | Backward compat: claude-code `.mcp.json`/`.claude/settings.json` common-path output must stay byte-identical after refactor into a per-harness layer | High | Med | Golden-file the current claude-code output before refactor; assert unchanged |
| SR-08 | warn-and-skip fail-safe posture can mask a leg that never wired; a swallowed warning reads as success (cf. #4473 warn+continue masking failure paths) | Med | Med | A skipped leg must be visibly reported; malformed-input skip is a distinct, asserted outcome (AC-15), not a silent pass |

## Path-Divergence Risks

The "works on path A, user invokes path B" class the closed design→test→gate loop cannot catch alone (vnc-047/#944; ceremonial-seam #4974).

| Risk ID | Entry point | Divergence (path that works vs. path the user invokes) | Recommendation |
|---|---|---|---|
| SR-09 | `unimatrix wire --harness codex-cli` | Config/hook block is WRITTEN (seam green) but the hook never fires and/or `context_*` never returns from the wired command — ceremonial wiring | Assert firing from the wired hook path and return from the wired slug — never discharge with a config-presence check (AC-08/09/10) |
| SR-10 | `unimatrix wire` (codex hooks) | `--provider codex-cli` omitted on a command; hook still fires but events are silently mislabeled `claude-code` (vnc-013 ADR-006) | Assert every written hook command carries `--provider codex-cli`; missing flag is a fail-loud defect |
| SR-11 | `unimatrix wire --harness <x>` vs auto-detect | New MCP entry into a user-owned config written WITHOUT explicit `--harness`/opt-in — silent install the user did not intend (#960) | Gate new-entry writes on explicit opt-in; additive merge into existing surface needs none; ambiguity surfaces help (AC-11/16) |
| SR-12 | `unimatrix wire --dry-run` | Dry-run path diverges from real path and writes (or real path writes outside project root) | Assert `--dry-run` writes nothing and containment guard blocks writes outside resolved root (AC-12/14) |

## Assumptions

- **#16732 is fixed** (SCOPE Background/Decided §7) — Codex MCP-tool-call hooks fire upstream. If regressed or environment-dependent, AC-09 (firing) is unmeetable; SR-02/SR-09 escalate.
- **Trust-gating is a precondition, not a blocker** (Constraints, "Codex trust-gating") — assumes firing/return assertions run in a trusted `.codex/` layer. If real consumers commonly run untrusted, the parity claim is conditional (SR-02).
- **`.git`-walk root resolution is correct in cloud** (Constraints, project-scoped) — assumes the resolved project root is the same under local and cloud deployment; if not, containment/writes diverge (SR-12).

## Design Recommendations

- Make the C14 verifier the design's spine: every leg's acceptance is a return/fire assertion FROM the wired command, not config presence (SR-02, SR-09). Ceremonial wiring passes N=1 and only fails in a real consumer.
- Land a pre-tag, real-server exercise of codex hook-fire + retrieval-return before the release chain (SR-03, evidence #5267) to avoid the multi-round tag tax.
- Golden-file existing claude-code output before the per-harness refactor (SR-07); byte-for-byte opencode sentinel test (SR-06).
- Treat the TOML writer's foreign-key/comment preservation as first-class acceptance (SR-01).
