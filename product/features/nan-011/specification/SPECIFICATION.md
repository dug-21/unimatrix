# Specification: nan-011 — Release Preparation: Documentation, Configuration, and Distribution

Feature directory: `product/features/nan-011/`
GH Issue: TBD

---

## Objective

Unimatrix's user-facing surface — README, vision statement, default `config.toml`, skills,
and reference protocols — has drifted from the current implementation across multiple
shipping cycles. This feature corrects all inaccuracies, expands the default configuration
to document all eight operator-facing sections, audits all 14 skills for correct MCP tool
call format, packages reference protocols and the `uni-retro` skill into the npm
distribution, and updates `uni-seed` for the current blank-database installation path.
No Rust code changes are in scope.

---

## Functional Requirements

### FR-1: README — Vision Statement Replacement

FR-1.1: The README opening paragraph (the "what is Unimatrix" definition) must be
replaced with the approved vision statement below, used verbatim:

> Unimatrix is a workflow-aware, self-learning knowledge engine built for agentic
> software delivery. It captures the knowledge that emerges from doing work —
> decisions, patterns, lessons, conventions — and makes it trustworthy, retrievable,
> and continuously improving. As agents move through delivery cycles, Unimatrix learns
> what matters at each phase and delivers the right knowledge dynamically, before
> agents need to ask for it. Knowledge retention becomes a first-class citizen of the
> delivery process, not a side effect.
>
> Unimatrix is not an orchestration engine. It does not coordinate agents, schedule
> work, or manage workflows. It is a knowledge engine that understands workflow context
> — your current phase, what your team has been doing, what comes next — and uses that
> understanding to surface relevant knowledge at exactly the right moment.
>
> The key mental model: workflow definitions, agent definitions, and skill definitions
> are static — they live in your tooling and change infrequently. Architecture
> decisions, patterns, and lessons-learned are dynamic — they evolve with every
> feature, every delivery, every failure. Unimatrix was designed to manage the dynamic
> layer. Every architectural pivot, every hard-won lesson, every reusable pattern is
> captured, attributed, and made available to every future agent that needs it.
>
> Built for agentic software delivery. Configurable for any workflow-centric domain.

FR-1.2: Immediately after the vision statement block, the README must include a
one-sentence qualifier in the vision section contextualizing the phrase "before agents
need to ask for it":

> This workflow-phase-conditioned delivery means knowledge is surfaced at phase
> transitions based on what the engine has learned about each phase — it is not
> unconditional injection into every prompt.

This qualifier is required by SR-06 to prevent the vision statement from being read as
a claim of fully hook-injected context delivery on every prompt.

FR-1.3: The same approved vision statement is used verbatim in PRODUCT-VISION.md,
replacing the current opening Vision section paragraph.

### FR-2: README — Stale Section Removal

FR-2.1: The "Semantic Search with NLI Re-ranking" sub-section must be removed in its
entirety and replaced with a new "Graph-Enhanced Retrieval" section (see FR-3.1).

FR-2.2: The "Contradiction Detection and NLI Edge Classification" sub-section must
be removed and replaced with a one-paragraph accurate description:
- Cosine Supports detection (threshold >= 0.65)
- Contradiction density as the Lambda dimension using the periodic scan
- Manual contradiction management via `context_correct`

FR-2.3: No other sections referencing NLI as a shipped, active capability may
remain. Prose may reference NLI only in the context of the opt-in config block or
future roadmap.

### FR-3: README — Section Additions

FR-3.1: Add a "Graph-Enhanced Retrieval" section (one to two paragraphs) that
describes the unified retrieval model:
- HNSW vector similarity locates initial candidates
- PPR (Personalized PageRank) co-access traversal expands the pool to surface
  cross-category entries that pure vector search misses
- Phase-conditioned category affinity stratifies results by workflow phase
- Co-access ranking promotes entries historically retrieved together
- The three layers compose: semantic similarity → graph expansion → phase/co-access
  ranking
- PPR expansion contributes a confirmed +0.0122 MRR improvement

FR-3.2: Add at least one paragraph each for:
- Behavioral signal delivery and goal-conditioned briefing (crt-046, Group 6)
- Domain-agnostic observation pipeline (W1-5, col-023): `source_domain` guard on all
  detection rules, domain pack registration via config, "claude-code" pack always
  active, any domain's event stream connects without code changes

### FR-4: README — Binary Name Fix

FR-4.1: All references to `unimatrix-server` must be updated to `unimatrix`.

FR-4.2: The build output path must be shown as `target/release/unimatrix`.

FR-4.3: Any MCP server configuration examples using `unimatrix-server` as the
binary name must use `unimatrix`.

### FR-5: PRODUCT-VISION.md — Targeted Status Fixes

FR-5.1: W1-5 row must be marked COMPLETE with reference to `col-023`, PR #332, GH #331.

FR-5.2: The Domain Coupling table row for "HookType enum tied to Claude Code events"
must have its Status column changed to `Fixed — col-023 / W1-5 (PR #332)`.

No other changes to PRODUCT-VISION.md content are in scope.

### FR-6: config.toml — Full Eight-Section Rewrite

FR-6.1: The current `config.toml` (26 lines, `[retention]` only) must be rewritten
to cover all 8 TOML sections of `UnimatrixConfig`, in this order:
1. `[profile]`
2. `[knowledge]`
3. `[server]`
4. `[agents]`
5. `[retention]`
6. `[observation]`
7. `[confidence]` (inside a clearly marked `## Advanced Configuration` block)
8. `[inference]` (inside the advanced block)

FR-6.2: Section `[profile]` must document the `preset` field with all four accepted
values and a one-line description of each:
- `collaborative` — balanced weights, suited for team-based agentic delivery
- `authoritative` — elevated trust and usage signals, suited for structured pipelines
- `operational` — freshness and correction signals elevated, suited for ops domains
- `empirical` — helpfulness and co-access signals elevated, suited for research domains

FR-6.3: Section `[knowledge]` must document:
- `categories` — list of active entry categories; defines the category allowlist
- `boosted_categories` — categories that receive a ranking boost in search results
- `adaptive_categories` — categories eligible for adaptive lifecycle management

The advanced sub-block for `[knowledge]` must include `freshness_half_life_hours`,
commented out.

FR-6.4: Section `[server]` must document the `instructions` field — the system prompt
injected into context briefings.

FR-6.5: Section `[agents]` must document:
- `default_trust` — trust level assigned to auto-enrolling unknown agents
- `session_capabilities` — capabilities granted per-session to known agents

FR-6.6: Section `[retention]` must document the three fields already present
(`activity_detail_retention_cycles`, `max_cycles_per_tick`,
`audit_log_retention_days`). This is an update, not a rewrite — existing comments may
be retained and improved.

FR-6.7: Section `[observation]` must include a complete commented example of the
`[[observation.domain_packs]]` table format showing all four fields:
- `source_domain` — string identifier for the domain
- `event_types` — list of accepted event type strings for this domain
- `categories` — knowledge categories used by this domain's detection rules
- `rule_file` — path to a custom detection rule file for this domain

The example must include a comment noting that the built-in "claude-code" domain pack
is always active and requires no configuration.

FR-6.8: The `[confidence]` advanced block must list all six scoring components:
`base`, `usage`, `fresh`, `help`, `corr`, `trust`. Each must have a comment stating
its purpose. A note must state that component weights must sum to 0.92 and are only
active when `preset = "custom"`.

FR-6.9: The `[inference]` advanced block must include the following NLI sub-block,
fully commented out:

```toml
# NLI cross-encoder (opt-in). Requires an external ONNX NLI cross-encoder model
# file. Not bundled with Unimatrix. See documentation for model acquisition.
# nli_enabled = false
# nli_model_name = "..."
# nli_model_path = "..."
# nli_model_sha256 = "..."
# nli_top_k = 20
# nli_entailment_threshold = 0.5
# nli_contradiction_threshold = 0.5
```

FR-6.10: The `[inference]` block must include the user-facing fields `rayon_pool_size`,
`phase_freq_lookback_days`, and `min_phase_session_pairs` as uncommented documented fields.

FR-6.11: PPR parameters, graph inference parameters, Informs detection thresholds,
and fusion weight fine-tuning fields in `[inference]` must either be:
- Omitted entirely, OR
- Present in a clearly-marked block with comment: "Internal tuning — do not change
  unless directed by a support issue."

FR-6.12: Every user-facing field that appears uncommented must have:
- A comment explaining its purpose and effect
- The accepted values or valid range
- The default value

FR-6.13: The `[profile]` section must include a commented-out `preset = "custom"`
block with a note explaining that `preset = "custom"` activates the `[confidence]`
custom weights block.

### FR-7: config.toml — Default Value Accuracy

FR-7.1: Every uncommented field value shown in `config.toml` must exactly match the
compiled default from the corresponding `default_*` function in
`crates/unimatrix-server/src/infra/config.rs`.

FR-7.2: The implementer must verify each default by directly reading `config.rs` — no
defaults may be asserted from memory or inferred from field names. A verification
artifact (inline comment or separate table in the delivery checklist) is required.

FR-7.3: If any discrepancy between the config file default and the `config.rs`
compiled default is discovered, the config file must match `config.rs`. This is a
non-negotiable constraint: `config.rs` is the authority.

### FR-8: Skills — MCP Format Audit (All 14)

The 14 skills subject to audit are:
```
uni-git, uni-release, uni-review-pr, uni-init, uni-seed,
uni-store-lesson, uni-store-adr, uni-store-pattern, uni-store-procedure,
uni-knowledge-lookup, uni-knowledge-search, uni-query-patterns,
uni-zero, uni-retro
```

FR-8.1: No bare Unimatrix tool name invocation — defined as the tool name followed
immediately by `(` (open parenthesis), appearing in a code block or formatted as a
function call — may appear in any `SKILL.md` file without the `mcp__unimatrix__`
prefix.

The following bare names are prohibited in invocation context:
- `context_search(`
- `context_store(`
- `context_get(`
- `context_lookup(`
- `context_correct(`
- `context_deprecate(`
- `context_status(`
- `context_briefing(`
- `context_enroll(`
- `context_quarantine(`
- `context_cycle(`
- `context_cycle_review(`

FR-8.2: Prose references in descriptive text (e.g., "call `context_search` to find
entries") are exempt from this requirement. The format rule applies only to code
blocks and invocation-format tool calls.

FR-8.3: The detection pattern for the audit is: the bare tool name immediately followed
by `(` in any context. Prose occurrences that lack the `(` are not violations.

Known existing violation (identified during specification): `uni-seed` SKILL.md line 49
contains `context_status()` as a bare invocation call without the `mcp__unimatrix__`
prefix. This must be corrected.

### FR-9: Skills — Accuracy Audit (4 Targeted Skills)

FR-9.1: `uni-release` SKILL.md must be updated to:
- Reference the binary as `unimatrix` (not `unimatrix-server`) wherever applicable
- Add a step to copy `protocols/` into the npm package distribution area before
  the release commit (see FR-11)
- Add a step to include `uni-retro` SKILL.md in the npm package distribution area
  before the release commit (see FR-11)
- Verify the release process steps are accurate against the current workflow

FR-9.2: `uni-init` SKILL.md must be updated to:
- List all 14 current skills by name accurately and completely in the CLAUDE.md
  block it appends (the "Available Skills" table or equivalent)
- Verify any Unimatrix tool call examples in the skill use the full
  `mcp__unimatrix__` prefix
- The server binary reference `unimatrix-server` in the Prerequisites section must
  be updated to `unimatrix`

FR-9.3: `uni-retro` SKILL.md must be updated to:
- Remove any references to `HookType`, closed-enum event type matching, or any
  col-023 predecessor concept (none found in current content as of specification —
  implementer must verify)
- Verify the retro invocation pattern works with the current domain-agnostic
  observation pipeline

FR-9.4: `uni-seed` SKILL.md must be updated per FR-10.

### FR-10: uni-seed Skill — Update for Current Installation Path

FR-10.1: All Unimatrix tool calls in `uni-seed` SKILL.md must use the full
`mcp__unimatrix__context_*` prefix. Known violation: `context_status()` on line 49
must become `mcp__unimatrix__context_status({})` (or equivalent full-prefix call).

FR-10.2: The skill must include a prominent idempotency warning, readable before the
user begins execution:

> Run once per new project before the first delivery session. Do not re-run on an
> established installation — seed entries will duplicate existing knowledge.

FR-10.3: The skill description and category list must reflect the current
`INITIAL_CATEGORIES` from
`crates/unimatrix-server/src/infra/categories/mod.rs`.

The current `INITIAL_CATEGORIES` as of this specification:
```
"lesson-learned", "decision", "convention", "pattern", "procedure"
```

The implementer must verify this list directly from `categories/mod.rs` at delivery
time — this file is the authority. The `INITIAL_CATEGORIES` array is the correct
reference; runtime-added categories from config are not.

FR-10.4: Any seed entries or skill text referencing removed features (NLI, MicroLoRA
as a seeding topic) must be removed or updated to reflect what actually shipped.

FR-10.5: The skill must accurately state the blank-installation use case: a fresh
Unimatrix install starts with an empty database; this skill provides an initial
curated knowledge set.

### FR-11: Protocol Packaging — protocols/ Directory

FR-11.1: A `protocols/` directory must be created at the repository root containing
exactly these files:
- `uni-design-protocol.md`
- `uni-delivery-protocol.md`
- `uni-bugfix-protocol.md`
- `uni-agent-routing.md`
- `README.md`

FR-11.2: The four protocol files in `protocols/` are independent copies of the files
in `.claude/protocols/uni/`. They are not symlinks (symlinks do not survive `npm pack`).

FR-11.3: Any accuracy corrections needed during protocol validation (FR-12) must be
applied to both the `.claude/protocols/uni/` source and the `protocols/` copy. The
`.claude/protocols/uni/` directory is the source of truth; `protocols/` is the
distributed copy. After all edits, both directories must be identical in content for
each corresponding file.

FR-11.4: `protocols/README.md` must cover:
- What the four protocols are and how they relate to Unimatrix's `context_cycle` tool
- How `context_cycle(type: "start" | "phase" | "stop")` works and why it enables
  workflow-conditioned knowledge delivery
- A minimal illustrative example of a two-phase cycle (design → delivery) showing
  the three `context_cycle` calls: start, phase transition, and stop
- A note that these are Claude Code + Unimatrix reference implementations and that
  the `context_cycle` pattern generalizes to any workflow-centric domain

### FR-12: Protocol Validation — Accuracy Corrections

FR-12.1: All four protocols must be reviewed and corrected to:
- Remove any references to NLI as a required or default capability
- Remove any references to MicroLoRA as a configuration requirement
- Remove any references to `unimatrix-server` as the binary name (replace with
  `unimatrix`)
- Remove any references to `HookType` closed enum

FR-12.2: `context_cycle` call signatures in all four protocols must match the current
MCP tool signature.

FR-12.3: Choreography logic, phase structure, gate definitions, and agent spawn
sequences in the protocols must not be modified. Only factual inaccuracies and
removed-feature references are corrected.

### FR-13: npm Package — Distribution Update

FR-13.1: `packages/unimatrix/package.json` `files` array must be updated to include:
- `"protocols"` — the `protocols/` directory at repo root
- The path to the distributed `uni-retro` skill (see FR-13.2)

FR-13.2: `uni-retro` skill must be made available at a distributable location, for
example `skills/uni-retro/SKILL.md` at the repo root (mirroring the existing
`skills/` directory in `packages/unimatrix/`). The exact path is an implementation
decision; the requirement is that the skill appears in the package manifest.

FR-13.3: `uni-release` SKILL.md must document the steps to include both artifacts
(protocols, uni-retro) in the npm package as part of the standard release process.

FR-13.4: `uni-release` must NOT appear in the npm `files` array. It is internal
tooling only.

FR-13.5: The `npm pack --dry-run` verification step, run from the
`packages/unimatrix/` directory, must confirm that both `protocols/` and the
`uni-retro` skill path appear in the package manifest.

---

## Non-Functional Requirements

### NFR-1: config.toml Validity

The `config.toml` file must be valid TOML. Every field that appears uncommented must
parse without error. Commented-out examples must be syntactically valid TOML when
uncommented. Field types must be respected: strings use quotes, integers are bare,
floats use decimal notation (e.g., `0.92`), arrays use `[...]`, tables-of-tables use
`[[...]]`.

### NFR-2: Default Value Fidelity

All default values in `config.toml` must match the compiled defaults in `config.rs`.
No defaults may be assumed from documentation, field names, or memory. This applies to
all eight sections. The `default_*` functions in `config.rs` are the sole authority.

### NFR-3: npm Pack Verification

The `npm pack --dry-run` command, run from `packages/unimatrix/`, must confirm that
`protocols/` and the `uni-retro` skill file appear in the package manifest. This
verification must be performed and its output recorded before the PR is opened.

### NFR-4: Dual-Copy Protocol Maintenance

The `.claude/protocols/uni/` directory is the source of truth for all four protocols.
The `protocols/` directory is a distributed copy. After all edits:
- Apply all corrections to `.claude/protocols/uni/` first
- Copy the corrected files to `protocols/`
- Diff both directories to confirm they are identical in content for each file
  before the PR is opened

### NFR-5: No Choreography Changes

The protocols' wave structure, agent spawning order, gate logic, and phase definitions
must not be modified. This is a functional constraint: the delivery protocol governs
active swarms. Changes to choreography require a separate scoped feature.

### NFR-6: No Rust Code Changes

No changes to any `.rs` file, `Cargo.toml`, migration SQL, or any compiled artifact
are in scope. If an acceptance criterion reveals a code issue, a separate GitHub issue
must be filed.

---

## Acceptance Criteria

### AC-01: Vision Statement Verbatim
**Requirement**: README and PRODUCT-VISION.md opening section uses the approved vision
statement verbatim.

**Pass**: The exact four-paragraph approved text from SCOPE.md appears at the start of
the "what is Unimatrix" section in both files. A diff of the vision block against the
approved text shows zero character differences.

**Fail**: Any word substitution, sentence reorder, or truncation. Omitting either file.

### AC-02: NLI References Removed from README
**Requirement**: README contains zero mentions of "NLI re-ranking", "NLI
cross-encoder", "NLI contradiction", or any variant used to describe an active or
shipped feature.

**Pass**: `grep -i "nli re-rank\|nli cross-encoder\|nli contradiction\|nli re-ranker\|nli sort" README.md` returns zero matches. "Adaptive Embeddings (MicroLoRA)" section is retained.

**Fail**: Any match on the prohibited patterns in the non-opt-in, non-roadmap context.
The opt-in NLI config block (in the config.toml section or installation docs) and
future roadmap references are permitted.

### AC-03: New README Sections Present
**Requirement**: README contains a "Graph-Enhanced Retrieval" section covering the
semantic + graph + SQL model, plus at least one paragraph each for behavioral signal
delivery and domain-agnostic observation pipeline.

**Pass**: All three additions are present. The Graph-Enhanced Retrieval section
mentions PPR expansion, phase-conditioned category affinity, and co-access ranking.
The behavioral signal delivery paragraph is present. The domain-agnostic observation
pipeline paragraph is present (mentioning `source_domain`, domain packs, and
"claude-code" pack).

**Fail**: Any of the three additions is absent. The Graph-Enhanced Retrieval section
omits any of the three required elements.

### AC-04: Binary Name Fixed in README
**Requirement**: All binary name references in README use `unimatrix`. Build path shown
as `target/release/unimatrix`.

**Pass**: `grep "unimatrix-server" README.md` returns zero matches. `grep "target/release/unimatrix"` returns at least one match.

**Fail**: Any occurrence of `unimatrix-server` in README.md.

### AC-05: PRODUCT-VISION.md Status Fixes
**Requirement**: PRODUCT-VISION.md correctly marks W1-5 as COMPLETE and the HookType
domain coupling gap as Fixed.

**Pass**: The W1-5 section heading contains "COMPLETE" and references `col-023`, PR
#332, GH #331. The HookType row in the Domain Coupling table has `Status` = Fixed with
reference to col-023/W1-5/PR #332.

**Fail**: W1-5 still marked "IN PROGRESS". HookType row still marked "In progress".

### AC-06: config.toml Covers All 8 Sections
**Requirement**: `config.toml` covers all 8 sections: `[profile]`, `[knowledge]`,
`[server]`, `[agents]`, `[retention]`, `[observation]`, advanced block with
`[confidence]` and `[inference]`. Every user-facing field has a comment.

**Pass**: All 8 sections are present. Every uncommented field has at least one comment
line immediately preceding or following it.

**Fail**: Any section absent. Any uncommented field without a comment.

### AC-07: config.toml domain_packs Example
**Requirement**: `config.toml` contains a complete commented `[[observation.domain_packs]]`
example showing all four fields with explanatory comments.

**Pass**: A commented example with `source_domain`, `event_types`, `categories`, and
`rule_file` is present. A comment states the "claude-code" pack is always active.

**Fail**: Any of the four fields absent from the example. No mention of the built-in
"claude-code" pack.

### AC-08: config.toml Valid TOML and Accurate Defaults
**Requirement**: `config.toml` is valid TOML. All uncommented fields parse without
error. All shown default values match compiled defaults in `config.rs`.

**Pass**: Running a TOML parser (e.g., `python3 -c "import tomllib; tomllib.load(open('config.toml','rb'))"`)
against the file produces no errors. Each uncommented field value is confirmed against
the corresponding `default_*` function in `config.rs`.

**Fail**: Any parse error. Any field value that differs from the compiled default.

### AC-09: NLI Sub-block Present but Commented Out
**Requirement**: The NLI sub-block in `[inference]` is present but fully commented out
with a note that it requires an external model file not bundled with Unimatrix.

**Pass**: Lines for `nli_enabled`, `nli_model_name`, `nli_model_path`,
`nli_model_sha256`, `nli_top_k`, `nli_entailment_threshold`, and
`nli_contradiction_threshold` are all present as comments. The block includes the
note: "Requires an external ONNX NLI cross-encoder model file. Not bundled."

**Fail**: Any NLI field is uncommented (active). Block is absent entirely.

### AC-10: Zero Bare Tool Name Invocations in Skills
**Requirement**: No bare Unimatrix tool name invocations (without `mcp__unimatrix__`
prefix) appear across all 14 skill files.

**Pass**: The following command, run from the repository root, returns zero matches:
```
grep -rn "context_status()\|context_search(\|context_store(\|context_get(\|context_lookup(\|context_correct(\|context_deprecate(\|context_briefing(\|context_enroll(\|context_quarantine(\|context_cycle(\|context_cycle_review(" .claude/skills/
```
(Matching lines that begin with `mcp__unimatrix__` on the same line are not violations.
Matches within prose text without `(` are not violations.)

**Fail**: Any match returned where the bare tool name appears as an invocation (with `(`
immediately following) without the `mcp__unimatrix__` prefix.

### AC-11: uni-init Lists All 14 Skills
**Requirement**: `uni-init` SKILL.md lists all 14 current skills by name, accurately
and completely.

**Pass**: The CLAUDE.md block appended by `uni-init` (the "Available Skills" table or
equivalent) lists all 14 skill names:
`uni-git`, `uni-release`, `uni-review-pr`, `uni-init`, `uni-seed`,
`uni-store-lesson`, `uni-store-adr`, `uni-store-pattern`, `uni-store-procedure`,
`uni-knowledge-lookup`, `uni-knowledge-search`, `uni-query-patterns`,
`uni-zero`, `uni-retro`.

**Fail**: Any skill absent. Any skill listed that does not exist. Count is not exactly 14.

### AC-12: uni-retro Contains No HookType or Predecessor References
**Requirement**: `uni-retro` SKILL.md contains no references to `HookType`,
closed-enum event type matching, or any col-023 predecessor concept.

**Pass**: `grep -n "HookType\|closed.enum\|event_types enum\|UserPromptSubmit\|SubagentStart\|PreCompact\|PreToolUse\|PostToolUse\|Stop hook" .claude/skills/uni-retro/SKILL.md` returns zero matches.

**Fail**: Any match on the prohibited patterns.

Note: The Claude Code hook names (`UserPromptSubmit` etc.) are valid as configuration
examples in other contexts but must not appear in `uni-retro` as fixed event type
identifiers. If `uni-retro` references them as examples of domain events (not as a
closed vocabulary), this is acceptable — the implementer must judge by context.

### AC-13: uni-release and package.json Updated for Distribution
**Requirement**: `uni-release` SKILL.md includes steps to (a) copy `protocols/` into
the npm package and (b) include `uni-retro` in the npm package. `package.json` `files`
array includes both artifacts. `npm pack --dry-run` confirms both appear.

**Pass**: `uni-release` SKILL.md contains explicit steps for both packaging actions.
`packages/unimatrix/package.json` `files` array contains `"protocols"` and the
`uni-retro` skill path. Running `npm pack --dry-run` from `packages/unimatrix/`
produces output listing files from the `protocols/` directory and the `uni-retro`
SKILL.md.

**Fail**: Either step absent from `uni-release`. Either entry absent from `files` array.
`npm pack --dry-run` does not list the expected files.

### AC-14: protocols/ Directory Exists with README
**Requirement**: `protocols/` directory exists at repo root containing all 4 protocol
files and a `README.md`. The README includes a `context_cycle` usage example.

**Pass**: All 5 files present: `uni-design-protocol.md`, `uni-delivery-protocol.md`,
`uni-bugfix-protocol.md`, `uni-agent-routing.md`, `README.md`. The README contains a
code block showing `context_cycle` calls for start, phase, and stop.

**Fail**: Any file absent. README lacks a `context_cycle` example.

### AC-15: Protocols Contain Zero Stale Feature References
**Requirement**: All 4 protocols contain zero references to removed features (NLI,
MicroLoRA, `unimatrix-server`). `context_cycle` call signatures in protocols match
the current MCP tool.

**Pass**: `grep -rn "NLI\|MicroLoRA\|unimatrix-server\|HookType" protocols/` returns
zero matches. `context_cycle` call signatures use the current parameter format:
`type: "start" | "phase" | "stop"`.

**Fail**: Any match on the prohibited patterns. Any `context_cycle` call using a
deprecated parameter name.

### AC-16: uni-seed Uses Full Prefix and Has Idempotency Warning
**Requirement**: `uni-seed` SKILL.md uses `mcp__unimatrix__context_store` for all
tool calls, describes the blank-installation use case, and warns against re-running
on an established installation.

**Pass**: `grep -n "context_store\|context_status\|context_search" .claude/skills/uni-seed/SKILL.md | grep -v "mcp__unimatrix__"` returns zero matches. The file contains a visible warning about re-running on established installations. The skill description mentions the blank-database first-run use case.

**Fail**: Any bare invocation. No idempotency warning. No mention of blank-database
installation.

### AC-17: uni-seed Categories Match INITIAL_CATEGORIES
**Requirement**: All seed entry categories in `uni-seed` use categories present in the
current `INITIAL_CATEGORIES` list in
`crates/unimatrix-server/src/infra/categories/mod.rs`.

**Pass**: The categories used by `uni-seed` entries (`convention`, `pattern`,
`procedure`) are all present in the `INITIAL_CATEGORIES` array in `categories/mod.rs`.
No seed entry uses a category not in that array.

**Fail**: Any seed entry uses a category string not present in `INITIAL_CATEGORIES`.

---

## Domain Model

### Files Touched by Each Deliverable

| Deliverable | File | Change Type |
|---|---|---|
| D1 — Vision & README | `README.md` | Rewrite (opening section, section removals, section additions, binary fix) |
| D1 — Vision & README | `product/PRODUCT-VISION.md` | Update (two targeted status fixes) |
| D2 — config.toml | `config.toml` | Rewrite (from 26 lines covering 1 section to full 8-section file) |
| D3 — Skills Audit | `.claude/skills/uni-seed/SKILL.md` | Update (bare prefix fix, idempotency warning) |
| D3 — Skills Audit | `.claude/skills/uni-init/SKILL.md` | Update (skill list completeness, binary name fix) |
| D3 — Skills Audit | `.claude/skills/uni-release/SKILL.md` | Update (binary name, packaging steps) |
| D3 — Skills Audit | `.claude/skills/uni-retro/SKILL.md` | Update (verify no HookType refs — likely no change needed) |
| D3 — Skills Audit | Up to 10 other `SKILL.md` files | Update (format fix only if bare invocations found) |
| D4 — Protocol Packaging | `protocols/uni-design-protocol.md` | Create (copy from `.claude/protocols/uni/`) |
| D4 — Protocol Packaging | `protocols/uni-delivery-protocol.md` | Create (copy) |
| D4 — Protocol Packaging | `protocols/uni-bugfix-protocol.md` | Create (copy) |
| D4 — Protocol Packaging | `protocols/uni-agent-routing.md` | Create (copy) |
| D4 — Protocol Packaging | `protocols/README.md` | Create (new) |
| D4 — Protocol Packaging | `.claude/protocols/uni/*.md` (all 4) | Update (if accuracy corrections found) |
| D4 — Distribution | `packages/unimatrix/package.json` | Update (add protocols and uni-retro to `files`) |
| D4 — Distribution | `skills/uni-retro/SKILL.md` (repo root) | Create (copy from `.claude/skills/uni-retro/`) |
| D5 — uni-seed | `.claude/skills/uni-seed/SKILL.md` | Update (same file as D3 — single edit covers both) |

### Ubiquitous Language

| Term | Definition |
|---|---|
| bare invocation | A Unimatrix tool call using the short form `context_*({...})` without the `mcp__unimatrix__` prefix. Bare invocations fail in agent execution contexts where MCP server name is part of tool resolution. |
| full-prefix invocation | A Unimatrix tool call using the complete form `mcp__unimatrix__context_*({...})`. The required format for all skill files. |
| INITIAL_CATEGORIES | The compile-time default category allowlist in `crates/unimatrix-server/src/infra/categories/mod.rs`. At time of specification: `["lesson-learned", "decision", "convention", "pattern", "procedure"]`. |
| source of truth (protocols) | The `.claude/protocols/uni/` directory. All edits to protocol content are made here first; `protocols/` receives a copy. |
| distributed copy (protocols) | The `protocols/` directory at repo root. Included in the npm package. Must be identical in content to the source of truth after all edits. |
| vision section qualifier | The one-sentence note required by SR-06, placed immediately after the approved vision statement in README, clarifying that proactive delivery is workflow-phase-conditioned. |
| Wave 1A | The Adaptive Intelligence Pipeline wave: PPR expansion, phase-conditioned affinity, proactive delivery (crt-024 through crt-051). All Wave 1A items are complete and must be accurately represented in the README. |

---

## User Workflows

### Workflow 1: Operator Configuring a New Deployment

1. Operator installs Unimatrix via npm or from source.
2. Operator reads `config.toml` at the repository root.
3. Operator finds all configurable sections documented with comments.
4. Operator edits the sections relevant to their domain (e.g., `[knowledge]` for
   custom categories, `[observation]` for domain packs, `[agents]` for trust levels).
5. Operator reads the advanced block and leaves `[confidence]` unchanged unless using
   `preset = "custom"`.
6. Operator reads the NLI sub-block, understands it is opt-in and requires an external
   model, and leaves it commented out.
7. Operator has a complete, accurate configuration reference without needing to read
   Rust source code.

### Workflow 2: New User Evaluating Unimatrix

1. User reads README.md.
2. User reads the approved vision statement and understands Unimatrix as a
   workflow-aware, self-learning knowledge engine.
3. User reads the one-sentence qualifier and understands that proactive delivery is
   phase-conditioned, not unconditional injection.
4. User reads the Graph-Enhanced Retrieval section and understands the semantic +
   graph + SQL access model.
5. User reads about behavioral signal delivery and the domain-agnostic observation
   pipeline.
6. User sees no references to removed capabilities (NLI re-ranking as a default
   feature, NLI contradiction classification).
7. User follows the Getting Started instructions using the correct binary name
   `unimatrix`.

### Workflow 3: Agent Running /uni-seed on a Blank Installation

1. Agent reads the `uni-seed` SKILL.md description and immediately sees the
   idempotency warning.
2. Agent understands this skill is for blank-database first-run use only.
3. Agent calls `mcp__unimatrix__context_status({})` (full prefix) for pre-flight.
4. Agent proceeds through the gated exploration flow using full-prefix tool calls.
5. All seed entries use only categories from the `INITIAL_CATEGORIES` allowlist.

### Workflow 4: Adopter Using Distributed Protocols

1. Adopter installs Unimatrix via npm.
2. Adopter finds the `protocols/` directory in the installed package.
3. Adopter reads `protocols/README.md` to understand the `context_cycle` integration
   pattern.
4. Adopter copies the desired protocol files into `.claude/protocols/` in their
   repository.
5. Adopter wires `context_cycle` calls at the appropriate workflow transitions.

---

## Constraints

- No changes to any `.rs` file, `Cargo.toml`, migration SQL, or any compiled artifact.
  If any acceptance criterion would require a code change to pass, a separate GitHub
  issue must be filed.
- `config.toml` default values must match compiled defaults in `config.rs`. The
  implementer must read the `default_*` functions directly.
- `config.toml` must be valid TOML. All uncommented fields parse correctly.
- Protocol choreography (phase structure, gate logic, agent spawn sequences) must not
  be modified. Only accuracy corrections are in scope.
- `uni-release` must NOT appear in the npm `files` array.
- `protocols/` files are independent copies, not symlinks.
- Unimatrix knowledge base entries #4163 and #4164 are out of scope for this delivery.
  Update via `context_correct` in a uni-zero session after merge.
- The "Invisible Delivery" README bullet copy correction (the overselling of
  hook-injected context) is addressed by the SR-06 qualifier requirement (FR-1.2) only.
  A full rewrite of that bullet is not in scope.

---

## Dependencies

| Dependency | Type | Notes |
|---|---|---|
| `crates/unimatrix-server/src/infra/config.rs` | Read-only reference | Authority for all `config.toml` default values. Implementer must read `default_*` functions. |
| `crates/unimatrix-server/src/infra/categories/mod.rs` | Read-only reference | Authority for `INITIAL_CATEGORIES` allowlist. Implementer must read at delivery time. |
| `packages/unimatrix/package.json` | Modified | `files` array update to include `protocols/` and `uni-retro` path. |
| Node.js >= 18 / npm | Runtime | Required for `npm pack --dry-run` verification (AC-13). Confirm available in dev environment before delivery. See SR-02. |
| `.claude/protocols/uni/` (all 4 files) | Source | Source of truth for protocol content. Copied to `protocols/`. |
| `.claude/skills/uni-retro/SKILL.md` | Source | Copied to distributable location for npm packaging. |

---

## NOT in Scope

- Any changes to Rust crates, compiled binaries, or MCP tool implementations.
- Schema migrations.
- New capabilities or MCP tool additions.
- Changes to protocol choreography, phase structure, or gate logic.
- Changes to skills that are already format-correct and factually accurate — only
  skills with MCP format errors or material inaccuracies are changed.
- Unimatrix knowledge base entries #4163 and #4164 — update in a uni-zero session.
- Changes to `InferenceConfig` defaults or any compiled-in Rust defaults.
- A full rewrite of the "Invisible Delivery" README bullet — the SR-06 qualifier
  is the in-scope correction.
- `uni-release` skill in the distributed npm package.
- Any new skill files beyond the `uni-retro` distribution.

---

## Open Questions

None. All design decisions were resolved in the design session with the project owner.
See SCOPE.md §Open Questions for the decision log.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 17 results; relevant hits
  included nan-005 ADR-001 (README as single file, capability-first order), nan-005
  ADR-004 (README vs CLAUDE.md content boundary), nan-003 decisions (uni-seed
  server availability requirement), entry #4148 (lesson-learned about config.rs
  field type divergence from IMPLEMENTATION-BRIEF.md — directly informed SR-01
  handling). No directly reusable patterns for specification authoring returned;
  findings are feature-specific and not promoted.
