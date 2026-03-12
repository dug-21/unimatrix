# Agent Report: nan-003-researcher

Feature: nan-003 — Unimatrix Onboarding Skills
Agent: nan-003-researcher (uni-researcher)
Date: 2026-03-11

## Output

SCOPE.md written to: `product/features/nan-003/SCOPE.md`

## Key Findings

### 1. Skill File Format Confirmed
Skills live at `.claude/skills/{name}/SKILL.md`. YAML frontmatter (`name`, `description`) + markdown body. Directory name = slash command. Both new skills require new directories: `.claude/skills/unimatrix-init/` and `.claude/skills/unimatrix-seed/`.

### 2. Naming: Two Distinct Things Both Called "init"
- `uni-init` agent (`.claude/agents/uni/uni-init.md`) — batch brownfield bootstrap, reads `.claude/agents/` and protocol files, stores duties/conventions. Scoped to Unimatrix's own repo artifacts.
- `/unimatrix-init` skill (nan-003) — deterministic CLAUDE.md setup + read-only agent orientation recommendations. Scoped to ANY target repo.
Spec and documentation must be explicit about this distinction to prevent confusion.

### 3. Idempotency via Sentinel Marker
The appended CLAUDE.md block should begin with `<!-- unimatrix-init: DO NOT REMOVE THIS LINE -->`. This is the idempotency check: if the sentinel is present, init is a no-op. Standard pattern for file-append skills.

### 4. Seed Quality Lesson (From uni-init Prototype)
The prototype generated 67 entries, all deprecated. Root cause: fully automated extraction with no human validation. `/unimatrix-seed` must be human-gated: Level 0 auto-runs (2-4 entries max), all deeper levels require explicit opt-in and per-entry (or batch) approval. This is the core design differentiator.

### 5. Skills Are Instructions, Not Scripts
Skills are markdown files Claude reads and follows. They cannot execute code themselves. All MCP calls, file reads/writes are performed by Claude following skill instructions. This means `unimatrix-seed`'s quality depends on model state management across a multi-turn conversation — a genuine platform constraint worth noting.

### 6. Target Repo CLAUDE.md May Not Exist
Must handle both append-to-existing and create-new cases. The block should be self-contained (readable by a developer with no prior Unimatrix knowledge).

### 7. MCP Availability Is a Hard Prerequisite
`/unimatrix-seed` calls `context_store`. If MCP is not wired, it fails. Skills should detect this and fail gracefully with a clear message, not silently produce nothing.

## Scope Boundaries (Rationale)

**In scope**: Two skill files + their complete behavioral specifications. Idempotency logic. Agent scan + recommendation output. Seed level model with human-gating.

**Out of scope (nan-004)**: Binary installation, settings.json wiring, ONNX model. These are explicitly decoupled to keep nan-003 deliverable as pure skill files.

**Out of scope (by design)**: Auto-editing agent files. Renaming existing skills. Seeding from `.claude/agents/` (that's `uni-init` agent territory).

## Open Questions for Human

1. Which skills should the CLAUDE.md block list? Just the two new `unimatrix-*` ones, or ALL installed skills (including store-adr, retro, etc.)?
2. Agent recommendation report: printed to terminal only, or also written to a file (e.g., `.unimatrix-init-recommendations.md`)?
3. `/unimatrix-seed` re-run behavior: warn if entries already exist, or always proceed and rely on server-side dedup?
4. Should the sentinel include a version number to support future `/unimatrix-init --update`?

## Knowledge Stewardship

- Queried: `/query-patterns` for skill file format, CLAUDE.md onboarding — no directly relevant patterns found (this is new territory)
- Stored: Two patterns attempted (sentinel marker, human-gated depth model) — **FAILED: agent lacks Write capability**. Patterns documented in SCOPE.md background research for future storage by authorized agent.
