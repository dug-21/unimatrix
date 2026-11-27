# 1036-agent-1-pseudocode report

Output: `product/features/1036/pseudocode/` — OVERVIEW.md + 17 component files (Component Map filenames from IMPLEMENTATION-BRIEF): config-dirs, trust-writer-core, safe-write, wire-plan, git-exclude, wire, claude-json, merge-settings, codex-install, opencode-install, hook-ownership, install-store, init, credstore, status, atomic-write-removal, test-infrastructure.

Wave order (OVERVIEW): W1 writer core + API with FR-33 test port while private writers still exist (C1, C11 parallel); W2 consumers migrate (C7–C10, C12–C14) + C5; W3 C4, C6, C15; W4 deletions (C16) + grep/parity/real-binary tests (C17). `scrubbedEnv` lands in W1.

Open questions: Q1–Q7 in OVERVIEW.md (notably Q2 git-exclude leg harness/surface naming conflict between ARCHITECTURE §6 and SPEC FR-46).

Unimatrix entries consulted: #5763, #5767, #5803 (briefing). ADR content taken from ARCHITECTURE.md/ADR-INDEX.md summaries (#5937 #5960 #5962 #5959 #5955 #5943 #5961); not individually fetched.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- returned nan-023 ADR-001/005 (wire manifest, wire verb), lesson #5803 (dogfood tooling wrote tracked settings.json; informs scrubbedEnv/test isolation)
- Deviations from established patterns: `updateFileSync` result gains additive `before` field (Q1); `ConfigDirs` gains per-harness `value` (Q3)
