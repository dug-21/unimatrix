# Risk-Based Test Strategy: nan-011

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | config.toml default values diverge from compiled config.rs defaults | High | High | Critical |
| R-02 | boosted_categories/adaptive_categories show Rust Default value instead of serde default | High | High | Critical |
| R-03 | rayon_pool_size shown as a fixed integer instead of a dynamic formula | High | Med | High |
| R-04 | Protocol dual-copy drift: .claude/protocols/uni/ and protocols/ differ after edits | High | High | Critical |
| R-05 | Bare MCP tool invocations missed by manual grep — AC-10 passes with false confidence | High | Med | High |
| R-06 | config.toml uncommented fields produce TOML parse errors | High | Med | High |
| R-07 | uni-init CLAUDE.md block lists fewer than 14 skills or wrong skill names | Med | High | High |
| R-08 | npm pack --dry-run not run or run from wrong directory — AC-13 unverified | Med | Med | Med |
| R-09 | protocols/ README.md omits context_cycle example or uses stale parameter names | Med | Med | Med |
| R-10 | Vision statement verbatim check fails — any character diff from approved text | Med | Med | Med |
| R-11 | Stale NLI references survive in README, protocols, or skill files | Med | Med | Med |
| R-12 | uni-retro distributed to npm carries forward its own bare invocation violations | Med | Low | Med |
| R-13 | PRODUCT-VISION.md W1-5 and HookType status fixes applied to wrong rows | Low | Low | Low |
| R-14 | uni-seed idempotency warning absent or placed after first tool call | Low | Med | Low |
| R-15 | skills/ directory at repo root absent — uni-retro copy has no landing path | Med | Low | Med |

---

## Risk-to-Scenario Mapping

### R-01: config.toml default values diverge from compiled config.rs defaults

**Severity**: High
**Likelihood**: High
**Impact**: Operators configure Unimatrix with wrong default values. Silent misconfiguration: values written in config.toml are not the compiled fallback values, so operator expectations are incorrect. Pattern #3817 (dual-site config defaults) and #4044 (hidden inference-config sites) confirm this is a persistent failure mode across features.

**Test Scenarios**:
1. For each uncommented field in config.toml, read the corresponding `default_*` function in `config.rs` and assert the values are identical (exact string/type match, not approximate).
2. Parse the numeric and boolean fields (u32, f64, bool) from config.toml and compare programmatically against the compiled defaults table in ADR-002.
3. Verify that `[agents]` session_capabilities uses capital R/W/S (`"Read"`, `"Write"`, `"Search"`) — case-sensitive per ADR-002.

**Coverage Requirement**: Every uncommented field in every section must have a confirmed match against `config.rs`. No section may be accepted on assertion alone.

---

### R-02: boosted_categories/adaptive_categories show Rust Default value instead of serde default

**Severity**: High
**Likelihood**: High
**Impact**: config.toml shows `[]` (the Rust Default value) instead of `["lesson-learned"]` (the serde default). Operators who omit these fields get a different behavior than what the config documentation implies. ADR-002 and the architecture brief explicitly call this out as a known two-site discrepancy.

**Test Scenarios**:
1. Confirm `boosted_categories = ["lesson-learned"]` appears in config.toml (not `[]`).
2. Confirm `adaptive_categories = ["lesson-learned"]` appears in config.toml (not `[]`).
3. Confirm ADR-002 annotation is present in comments near these fields explaining the serde-vs-Default distinction.

**Coverage Requirement**: Both fields must show the serde default value. A comment must distinguish serde behavior from programmatic Default.

---

### R-03: rayon_pool_size shown as a fixed integer instead of a dynamic formula

**Severity**: High
**Likelihood**: Med
**Impact**: Operator reads a fixed value (e.g., `4`) and interprets it as the universal default. In reality, the value is `(num_cpus::get() / 2).max(4).min(8)` — a machine-dependent formula. The implementer may naively paste a platform-specific value.

**Test Scenarios**:
1. Confirm rayon_pool_size in config.toml is either commented out entirely or includes the formula `(num_cpus / 2).max(4).min(8)` in a comment rather than a bare integer.
2. If an integer is shown, confirm a comment states "dynamically computed at startup" and gives the formula.

**Coverage Requirement**: No bare integer for rayon_pool_size without the formula explanation.

---

### R-04: Protocol dual-copy drift — .claude/protocols/uni/ and protocols/ differ after edits

**Severity**: High
**Likelihood**: High
**Impact**: Distributed users receive protocol files that do not match what active swarms use internally. Protocol drift is a silent correctness failure — the distributed copy may reference stale MCP signatures, removed features, or old binary names. SR-03 (High/High scope risk) maps directly to this. NFR-4 requires a diff-verification step.

**Test Scenarios**:
1. Run `diff .claude/protocols/uni/uni-design-protocol.md protocols/uni-design-protocol.md` — must produce zero output.
2. Run the same diff for all four protocol files: uni-delivery, uni-bugfix, uni-agent-routing.
3. Confirm `protocols/README.md` exists and is distinct from any source (it has no source equivalent).
4. Run `grep -rn "NLI\|MicroLoRA\|unimatrix-server\|HookType" protocols/` — must return zero matches.

**Coverage Requirement**: All four file diffs must be empty. The search for stale references must return zero matches.

---

### R-05: Bare MCP tool invocations missed by manual grep — AC-10 passes with false confidence

**Severity**: High
**Likelihood**: Med
**Impact**: A skill file contains a bare `context_search(` or `context_store(` invocation not caught by a naive grep. Agent execution fails with "tool not found" in contexts where the MCP server name is part of resolution. ADR-004 identifies that bare invocations in spawn-prompt strings (uni-retro lines ~146, ~161) require a two-pass pattern — a single grep may miss one form.

**Test Scenarios**:
1. Run the two-pass pattern from ADR-004 across all 14 SKILL.md files:
   - Pass 1: `` grep -rn '`context_[a-z_]*(` .claude/skills/*/SKILL.md ``
   - Pass 2: `grep -rn 'context_[a-z_]*(' .claude/skills/*/SKILL.md | grep -v 'mcp__unimatrix__'`
2. Manually review each match from Pass 2 to confirm it is an invocation (not prose). Confirm zero unresolved invocations remain.
3. Verify the distributed `skills/uni-retro/SKILL.md` at repo root (the npm copy) also passes both patterns — it is a copy and must be fixed independently if it differs.

**Coverage Requirement**: Both passes must return zero uninvestigated matches. The npm copy of uni-retro must also be clean.

---

### R-06: config.toml uncommented fields produce TOML parse errors

**Severity**: High
**Likelihood**: Med
**Impact**: An invalid config.toml is functionally useless — Unimatrix fails to start or ignores the file. Type errors (integer without quotes vs. string with quotes, float without decimal, wrong array syntax) are easy to introduce in a hand-authored file.

**Test Scenarios**:
1. Run `python3 -c "import tomllib; tomllib.load(open('config.toml','rb'))"` (or equivalent toml-rs check) against the produced file — must produce no errors.
2. Temporarily uncomment the `[[observation.domain_packs]]` example block and re-run the parser — must parse without error (required fields are present in the example).
3. Temporarily uncomment the NLI sub-block and re-run the parser — must parse without error.
4. Temporarily uncomment the `[confidence]` custom weights block and re-run the parser — must parse without error and the six weights must sum to 0.92.

**Coverage Requirement**: config.toml must parse cleanly in both its default form and with each commented example uncommented.

---

### R-07: uni-init CLAUDE.md block lists fewer than 14 skills or wrong skill names

**Severity**: Med
**Likelihood**: High
**Impact**: New projects initialized with `uni-init` get an incomplete or stale Available Skills reference. Operators don't know about skills that exist, or reference skills that don't exist. ADR-004 identifies this as a content gap (currently only 2 of 14 listed).

**Test Scenarios**:
1. Grep or read the uni-init SKILL.md and extract the skills list from the CLAUDE.md append block. Confirm exactly 14 skill names are present.
2. Cross-reference each listed name against the actual 14 skill directories in `.claude/skills/`: uni-git, uni-release, uni-review-pr, uni-init, uni-seed, uni-store-lesson, uni-store-adr, uni-store-pattern, uni-store-procedure, uni-knowledge-lookup, uni-knowledge-search, uni-query-patterns, uni-zero, uni-retro.
3. Confirm no skill is listed twice and no non-existent skill appears.

**Coverage Requirement**: Exact match of 14 names against the canonical list.

---

### R-08: npm pack --dry-run not run or run from wrong directory — AC-13 unverified

**Severity**: Med
**Likelihood**: Med
**Impact**: The package.json is updated but the verification step is skipped or run from the repo root (which would succeed trivially). The actual npm package may omit protocols/ or uni-retro without detection. SR-02 notes that a missing Node.js toolchain could block this entirely.

**Test Scenarios**:
1. Confirm Node.js and npm are available in the dev environment before delivery (`node --version`, `npm --version`).
2. Run `npm pack --dry-run` from `packages/unimatrix/` (not repo root). Capture output.
3. Confirm output lists at least one file from `protocols/` (e.g., `protocols/README.md`).
4. Confirm output lists `skills/uni-retro/SKILL.md`.
5. Confirm `uni-release/SKILL.md` does NOT appear in the output (it must not be distributed).

**Coverage Requirement**: dry-run output must be recorded in the PR description or delivery checklist. All three assertions (protocols present, uni-retro present, uni-release absent) must pass.

---

### R-09: protocols/README.md omits context_cycle example or uses stale parameter names

**Severity**: Med
**Likelihood**: Med
**Impact**: Distributed users have no worked example of how to integrate context_cycle into their workflow. The protocols/ README is the entry point for new users — a missing or wrong example defeats the distribution purpose.

**Test Scenarios**:
1. Confirm `protocols/README.md` exists and contains the string `context_cycle`.
2. Confirm the example shows all three call types: `type: "start"`, `type: "phase"`, `type: "stop"`.
3. Confirm no deprecated parameter names appear in the context_cycle call signatures (e.g., no `phase_id` instead of `type`).
4. Confirm the README mentions the generalizability note (not Claude Code-specific).

**Coverage Requirement**: All four scenario checks must pass.

---

### R-10: Vision statement verbatim check fails — any character diff from approved text

**Severity**: Med
**Likelihood**: Med
**Impact**: AC-01 requires zero character differences between the README/PRODUCT-VISION.md opening block and the approved text in SCOPE.md. Word substitutions, em-dash vs. regular-dash, trailing whitespace, or reordered sentences all fail this AC.

**Test Scenarios**:
1. Extract the vision block from README.md and diff character-by-character against the verbatim approved text in SCOPE.md §Proposed Approach. Zero diff.
2. Extract the vision block from PRODUCT-VISION.md and perform the same diff. Zero diff.
3. Confirm FR-1.2's qualifier sentence appears immediately after the vision block in README.md (the phrase-conditioned delivery clarifier).

**Coverage Requirement**: Character-level diffs must be empty for both files. The qualifier sentence is mandatory.

---

### R-11: Stale NLI references survive in README, protocols, or skill files

**Severity**: Med
**Likelihood**: Med
**Impact**: Operators read that NLI re-ranking or NLI contradiction detection is an active shipped feature. They may attempt to use or rely on behavior that was removed in crt-038.

**Test Scenarios**:
1. Run `grep -i "nli re-rank\|nli cross-encoder\|nli contradiction\|nli re-ranker\|nli sort" README.md` — zero matches.
2. Run `grep -rn "NLI\|MicroLoRA\|unimatrix-server\|HookType" protocols/` — zero matches.
3. Run `grep -rn "HookType\|closed.enum\|UserPromptSubmit\|SubagentStart\|PreCompact\|PreToolUse\|PostToolUse\|Stop hook" .claude/skills/uni-retro/SKILL.md` — zero matches.
4. Confirm the opt-in NLI block in config.toml is fully commented out (not an active field).

**Coverage Requirement**: All four grep checks must return zero matches.

---

### R-12: uni-retro distributed to npm carries forward its own bare invocation violations

**Severity**: Med
**Likelihood**: Low
**Impact**: The `skills/uni-retro/SKILL.md` at repo root is a copy made during delivery. If the copy is made before the bare invocations are fixed in `.claude/skills/uni-retro/SKILL.md`, the distributed version ships broken. The source file must be fixed first, then copied.

**Test Scenarios**:
1. Run Pass 2 grep from R-05 against `skills/uni-retro/SKILL.md` (repo root, not .claude/skills/) — zero matches.
2. Diff `skills/uni-retro/SKILL.md` against `.claude/skills/uni-retro/SKILL.md` — must be identical.

**Coverage Requirement**: Both checks must pass on the repo-root copy specifically.

---

### R-13: PRODUCT-VISION.md W1-5 and HookType status fixes applied to wrong rows

**Severity**: Low
**Likelihood**: Low
**Impact**: A trivially wrong row gets updated while the correct row remains stale. Unlikely given the specificity of the AC, but a wrong-row edit is visually indistinguishable in a PR diff without checking the surrounding context.

**Test Scenarios**:
1. Read the W1-5 section in PRODUCT-VISION.md and confirm it contains "COMPLETE", "col-023", "PR #332", and "GH #331" in the same block.
2. Read the Domain Coupling table and find the row containing "HookType" — confirm Status = "Fixed" with col-023 / W1-5 / PR #332 reference.

**Coverage Requirement**: Both checks must confirm the correct rows were edited.

---

### R-14: uni-seed idempotency warning absent or placed after first tool call

**Severity**: Low
**Likelihood**: Med
**Impact**: Operator re-runs uni-seed on an established installation, duplicating knowledge entries. FR-10.2 requires the warning to be "readable before the user begins execution" — placement matters, not just presence.

**Test Scenarios**:
1. Read uni-seed SKILL.md and confirm the idempotency warning appears before the first `mcp__unimatrix__context_store` call.
2. Confirm the warning text matches: "Do not re-run on an established installation — seed entries will duplicate existing knowledge."

**Coverage Requirement**: Warning present AND positioned before first tool invocation.

---

### R-15: skills/ directory at repo root absent — uni-retro copy has no landing path

**Severity**: Med
**Likelihood**: Low
**Impact**: The `package.json` `files` array already contains `"skills/"`. If no `skills/` directory exists at repo root, creating `skills/uni-retro/SKILL.md` implicitly creates the directory — but if the implementer does not realize this, they may write the file to the wrong location (e.g., `.claude/skills/` instead of `skills/`). Architecture open question 1 explicitly flags this uncertainty.

**Test Scenarios**:
1. Confirm `skills/uni-retro/SKILL.md` exists at repo root (not `.claude/skills/uni-retro/SKILL.md`).
2. Confirm the file is not a symlink (`ls -la skills/uni-retro/SKILL.md` shows a regular file).
3. Run `npm pack --dry-run` from `packages/unimatrix/` and confirm `skills/uni-retro/SKILL.md` appears in the manifest.

**Coverage Requirement**: Physical file at correct path, confirmed by npm pack output.

---

## Integration Risks

**config.rs → config.toml** (R-01, R-02, R-03): The only runtime-affecting integration in this feature. No compilation validates the mapping — the implementer must read source and verify manually. The serde-vs-Default discrepancy for boosted_categories and adaptive_categories (ADR-002) is the single most dangerous field-level mistake.

**protocols/ ↔ .claude/protocols/uni/** (R-04): Dual-copy maintenance creates permanent divergence potential. Every correction applied to the source must be re-applied to the copy. The diff verification step (NFR-4) is the only guard.

**skills/uni-retro/ ↔ .claude/skills/uni-retro/** (R-12): The npm distribution copy must be made after the source is corrected, not before. Copy-then-fix ordering is the failure mode.

**npm files array ↔ actual artifacts on disk** (R-08, R-15): The `files` array references paths that must exist. If `protocols/` or `skills/uni-retro/` are not created, `npm pack` will silently produce a package without them — the `files` array is not validated at pack time.

---

## Edge Cases

- **config.toml comments-only section**: A section header (e.g., `[confidence]`) present with all fields commented out is still a valid TOML section — but a parser test that only checks for parse success will not catch a section that is accidentally omitted or misnamed (e.g., `[Confidence]` instead of `[confidence]`).
- **`[[observation.domain_packs]]` is a table-of-tables**: Its commented example must use `[[...]]` not `[...]`. If the comment character is placed after the key line, the TOML becomes invalid when uncommented. Test by temporarily uncommenting.
- **ConfidenceWeights sum**: When the `[confidence]` block is uncommented as an example, the six weights must sum to exactly 0.92 ± 1e-9. A round-number example (e.g., six × 0.15 = 0.90, not 0.92) would be invalid — the implementer must provide a valid example.
- **Binary name in code blocks vs. prose**: A code block containing `unimatrix-server` inside a shell command example is as much a violation of AC-04 as a prose reference. Grep must cover fenced code blocks.
- **`uni-init` lists uni-init itself**: The 14-skill list must include `/uni-init` — it is in the canonical list but an implementer may omit it as "already run."
- **protocols/README.md context_cycle example parameter names**: The current MCP signature uses `type:` not `phase_type:` or similar. A stale example with wrong parameter names would pass an existence check but fail at runtime.

---

## Security Risks

nan-011 contains no Rust code changes and accepts no runtime user input. The attack surface is limited to static file content consumed by agents.

**config.toml — operator input**: The config.toml ships as a template. Any operator-writable field that accepts a file path (`nli_model_path`, `rule_file` in domain_packs) could be used for path traversal if the server doesn't sanitize it. nan-011 does not change path handling — but the config comments should note that `rule_file` and `nli_model_path` accept file system paths and should be absolute or repo-relative.

**protocols/ and skills/ — agent execution surface**: Skills and protocols are consumed by Claude agents as instruction files. Introducing malicious or misleading instructions in the distributed copies would affect agent behavior. The risk is low (these are copies of internally-reviewed files) but the diff verification step (R-04, R-12) provides the control.

**npm package — supply chain**: The npm package ships skill and protocol files alongside the binary. A compromised `protocols/` copy could instruct agents to use wrong tool signatures, send data to wrong endpoints, or skip gates. The diff-verification step is the only control — there is no code signing on the content files.

**Blast radius if config.toml ships with wrong NLI block uncommented**: An uncommitted NLI field with `nli_enabled = true` and a nonexistent model path would cause Unimatrix to fail startup or produce errors on every query. Low likelihood given AC-09 test, but the consequence is service unavailability.

---

## Failure Modes

**config.toml parse failure**: Unimatrix logs a startup error and exits. The operator sees a TOML parse error with line number. Recovery: correct the config.toml field. The risk document should require that the TOML validity check be run before PR merge.

**config.toml value mismatch (no parse error)**: Unimatrix starts but uses the config.toml value instead of the compiled default. The operator's configured system behaves differently than documented. No visible error. Recovery requires re-reading config.rs and correcting the file. This is the silent failure mode — hardest to detect post-deploy.

**npm pack missing protocols/ or skills/uni-retro/**: The npm package is published without the artifacts. Users who install the package have no access to protocols or uni-retro. Recovery requires a patch release. The dry-run verification step (R-08) must be gating, not informational.

**Bare MCP invocation in distributed skill**: The npm package ships `skills/uni-retro/SKILL.md` with a bare `context_search(` invocation. Agents using the distributed skill fail with "tool not found" in prefixed contexts. Recovery requires a patch release. The post-copy grep check (R-12) prevents this.

**Dual-copy protocol drift**: The distributed `protocols/` files describe a different `context_cycle` signature than the internal `.claude/protocols/uni/` files. Agents using distributed protocols call the tool with wrong parameters. Silent failure (wrong parameters may not be immediately fatal depending on parameter validation). Recovery requires re-syncing copies and releasing a patch.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01: config.toml defaults diverge from config.rs | R-01, R-02, R-03 | ADR-002 provides a field-by-field verified defaults table. spec requires per-field verification artifact. R-01/R-02/R-03 test scenarios enforce it at gate. |
| SR-02: Node.js toolchain absent for npm pack verification | R-08 | R-08 adds environment pre-check (node --version, npm --version) as first scenario. If toolchain absent, AC-13 is blocked and must be flagged in PR. |
| SR-03: Dual-copy maintenance obligation easy to miss | R-04 | ADR-003 and NFR-4 require diff verification. R-04 test scenarios enforce four-file diff check before PR. uni-release skill adds explicit diff-verification step. |
| SR-04: AC-10 grep conflates prose and invocation contexts | R-05 | ADR-004 defines a two-pass grep pattern. R-05 test scenarios use that exact pattern and require manual review of each match. |
| SR-05: uni-seed categories against stale allowlist | R-14 | FR-10.3 specifies INITIAL_CATEGORIES as authority. Implementer must read categories/mod.rs at delivery time. R-14 verifies warning placement; category accuracy is part of R-07 coverage model. |
| SR-06: Vision statement "before agents need to ask" oversells capability | R-10 | FR-1.2 requires a qualifier sentence immediately after the vision block. R-10 test scenario 3 verifies the qualifier is present. |
| SR-07: uni-retro in npm creates versioning contract | R-12, R-08 | Addressed by distribution design (skill is a static file, not a compiled artifact). R-08 requires dry-run verification; R-12 ensures the distributed copy is identical to the corrected source. Minimum version note is out of scope for nan-011 — filed as follow-on concern. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 3 (R-01, R-02, R-04) | 10 scenarios minimum |
| High | 4 (R-03, R-05, R-06, R-07) | 10 scenarios minimum |
| Medium | 6 (R-08–R-12, R-15) | 14 scenarios minimum |
| Low | 2 (R-13, R-14) | 4 scenarios minimum |

---

## Knowledge Stewardship

- Queried: /uni-knowledge-search for "lesson-learned failures gate rejection documentation drift" — found #3611 (interface-sync lesson, nan-010), #4169 (stale xfail lesson), #4198 (spec-vs-ADR contradiction lesson); none directly apply to documentation-only feature pattern
- Queried: /uni-knowledge-search for "risk pattern configuration default value mismatch" — found #3817 (dual-site config default pattern) and #4044 (InferenceConfig hidden sites); both directly inform R-01 and R-02 severity elevation
- Queried: /uni-knowledge-search for "npm package distribution packaging verification dry-run" — found #1196 (workspace version SSOT), #1193 (optionalDependencies pattern), #4267 (ADR-003 itself); no new cross-feature patterns
- Stored: nothing novel to store — the dual-copy maintenance risk (R-04/SR-03) is feature-specific; the config dual-site pattern is already captured in #3817; no cross-feature pattern visible from nan-011 alone that is not already stored
