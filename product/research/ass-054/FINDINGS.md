# FINDINGS: Workflow Documentation, Manual Processes & Security Process Expansion

**Spike**: ass-054
**Date**: 2026-04-24
**Approach**: investigation + documentation
**Confidence**: directional

---

## Findings

### Q: What Mermaid diagrams best communicate the Design Session and Delivery Session workflows to human contributors?

**Answer**: Three diagram files were written to `docs/workflows/`. Each uses a split-diagram approach — one high-level phase flow per session plus one zoomed-in sub-diagram for the complex parts.

**Evidence**: Read both session protocols in full, the bugfix protocol, all three skill files (uni-zero, uni-retro, uni-review-pr), the uni-architect and uni-scrum-master agent definitions.

**Design choices made**:

1. `docs/workflows/design-session.md`: Two Mermaid diagrams. Diagram 1 shows the full phase flow: Human intent → Init with context_cycle start → Phase 1 researcher → human SCOPE.md approval gate → Phase 1b risk strategist → context_cycle phase-end scope → Phase 2a parallel spawn → Phase 2a+ risk strategist → context_cycle phase-end design → Phase 2b vision guardian → Phase 2c synthesizer → context_cycle phase-end design-review → Human receives 8 artifacts → decision. Diagram 2 zooms into Phase 2 only, showing the parallel Architect/Specification spawn and the sequential downstream agents.

2. `docs/workflows/delivery-session.md`: Two diagrams. Diagram 1 covers the full stage flow (Init → Stage 3a parallel spawn → Component Map update → Gate 3a → Stage 3b waves → Gate 3b → Stage 3c → Gate 3c → Phase 4) with rework loops (up to 2 retries labeled) and SCOPE FAIL exits. Diagram 2 zooms into Phase 4 (push PR, documentation trigger decision, security review, cycle close).

3. `docs/workflows/pm-governance.md`: One diagram covering the entire governance loop — uni-zero → Design → human design review → Delivery → Merge → uni-retro → human retro review → optional protocol update → next feature.

**What was deliberately omitted for legibility**: Internal protocol parameters (`next_phase` values, exact context_cycle call syntax), individual Unimatrix search queries that agents use inside Stage 3a/3b, and all cargo truncation commands. These are protocol details that belong in the protocol files, not in overview diagrams. The diagrams are at the "phase/gate" level of abstraction, not the "exact tool call" level.

**What was split into sub-diagrams**: Phase 2 of Design (too many sequential agents to be legible in the overall flow at readable font size). Phase 4 of Delivery (conditional logic for documentation + security review is distinct enough to warrant its own diagram). Wave planning in Stage 3b is described in a prose table below the delivery diagram rather than in the diagram itself — representing arbitrary wave counts in static Mermaid is not possible cleanly.

**Recommendation**: The most likely drift points when protocols evolve: Phase 2 agent order (if new agents are inserted), gate outcomes (if SCOPE FAIL handling changes), Phase 4 documentation trigger criteria. Update the diagrams when those sections of the protocols change. The split-diagram structure makes this easier — Phase 4 can be updated without touching the main stage flow diagram.

---

### Q: How does the human product-management layer interact with the automated swarm workflows, and how should this be documented?

**Answer**: The governance loop has three mandatory human re-entry points per feature (scope approval, design artifact review, PR review/merge) and one optional re-entry (retro review → protocol update). Protocol updates are intentionally human-gated and slow. The `pm-governance.md` diagram is the missing documentation for this entire layer.

**Evidence**: uni-zero SKILL.md (scope, vision curation, GitHub Issue creation; explicitly cannot run design/delivery protocols), uni-retro SKILL.md (knowledge extraction after merge; hotspot detection triggers human review, not automated action), design protocol (SCOPE.md approval gate: "Do not proceed to Phase 1b until the human approves SCOPE.md"), delivery protocol (human starts Session 2 by providing IMPLEMENTATION-BRIEF path; session cannot start without it).

**What triggers each manual process**:

- **uni-zero**: Human invokes `/uni-zero`. No automated trigger. Used for roadmap planning, aligning on a new feature, or as a thinking partner before committing to scope.
- **Design Session**: Human invokes the design protocol after deciding to build something. The CLAUDE.md rule (`IMPLEMENTATION-BRIEF.md` not present → use design) prevents bypassing this gate.
- **Delivery Session**: Human starts it after reviewing and approving design artifacts. Hard precondition: IMPLEMENTATION-BRIEF.md must exist.
- **uni-retro**: Human invokes `/uni-retro {feature-id} {pr-number} {issue-number}` after a PR merges. No automated trigger.

**Human decision points per feature**:

1. After SCOPE.md is presented: approve (→ Phase 1b), reject and iterate (→ researcher revises), defer (feature not pursued).
2. After all 8 design artifacts are returned: proceed to Delivery, revise scope (back to uni-zero), or defer.
3. After PR opens and security review returns: approve and merge, address blocking security findings, or close PR.
4. After uni-retro presents findings: accept findings as informational, act on a specific recommendation (file GH Issue, update protocol), or ignore.

**What "slow-moving workflow change" looks like operationally**: A retro identifies that `coordinator_respawns` has been above threshold for two consecutive features. Human reads both sessions' retrospective data and concludes the protocol needs a more explicit handoff point. Human edits `.claude/protocols/uni/uni-delivery-protocol.md` directly — there is no approval gate, PR, or review process for protocol files. The change takes effect on the next session. The slowness comes from the human's deliberate decision process, not from any bureaucratic mechanism. Protocol files are not versioned through the feature workflow.

**Why uni-retro hotspot detection is human-reviewed, not automated**: The retrospective data surfaces patterns (e.g., `orphaned_calls`, `sleep_workarounds`, `post_completion_work`) with thresholds. A single outlier session could be noise — a difficult feature, an unusual model behavior, a test environment quirk. Only the human can judge whether a pattern across multiple sessions represents a real protocol problem versus a one-off. Automated protocol edits based on single-session hotspot data would make the system fragile to noise. The retro is designed to surface evidence; the human supplies judgment.

**Recommendation**: `docs/workflows/pm-governance.md` is now the canonical reference for this governance model. It should be the first diagram shown to a new contributor — it provides the mental model that makes the Design and Delivery diagrams meaningful.

---

### Q: What specific process changes would meaningfully improve supply-chain and code-vulnerability coverage? How should dependabot alerts be triaged?

**Answer**: Three structural gaps exist: no cargo audit gate in CI, no defined triage process for Dependabot alerts, and no test execution in CI at all. The current Dependabot HIGH openssl alerts are low-reachability but should be resolved via version bump, tracked as GH Issues, and processed through the existing bugfix protocol. Ranked recommendations follow.

**Evidence**: Read `.github/workflows/ci.yml` (single job: bash inference site check — no cargo test, no cargo audit, no cargo build, no clippy). Read delivery protocol (cargo audit appears only in the "Cargo Output Truncation" examples section, not in any gate step). Analyzed Dependabot alert data.

**Current Security Gate Map**:

| Gate | What it covers | What it misses |
|------|---------------|----------------|
| uni-security-reviewer (PR stage) | OWASP injection, access control, input validation, blast radius, dependency safety (narrative) | Not a blocking gate on CVEs; no cargo audit run; relies on reviewer judgment on known CVEs |
| Gate 3b validator | Code matches pseudocode; compiles; tests pass | No supply-chain check |
| Gate 3c validator | Integration smoke tests pass; risk coverage | No supply-chain check |
| CI (ci.yml) | Inference site convention check only | No `cargo test`; no `cargo build`; no `cargo audit`; no clippy; zero correctness enforcement |

**Dependabot Alert Reachability Assessment**:

The openssl crate in this project is almost certainly transitive through the `ort` crate (ONNX inference) or its native library dependencies. The Unimatrix server uses stdio MCP transport — no TLS server, no HTTPS listener, no PSK mode, no explicit key derivation in any production path.

| Alert Cluster | Severity | Reachability in Unimatrix | Triage |
|--------------|----------|--------------------------|--------|
| #10, #12, #11, #14 — openssl buffer overflow (CVE-2026-41681), AES key wrap (CVE-2026-41678), PSK trampoline (#11), key derivation (CVE-2026-41676) | HIGH | LOW — none of these code paths are exercised by Unimatrix production code | One GH Issue with reachability assessment; version bump via bugfix protocol. Priority: normal, not urgent. |
| #13 — openssl OOB read in PEM password callback (CVE-2026-41677) | LOW | LOW — same reasoning | Include in the openssl cluster issue |
| #6, #5 — rustls-webpki URI name constraints, wildcard name constraints | LOW | VERY LOW — verify via `cargo tree -i rustls-webpki`; if dev/test dependency only, close with "not production-reachable" | Separate GH Issue; verify dependency path first |
| #1 — rustls-webpki CRL matching | MEDIUM | VERY LOW — same as above | Include in rustls-webpki issue |
| #15/#9/#8/#7 — rand unsound with custom logger | LOW | LOW — vulnerability requires a custom global logger implementing Logger that calls rand::rng(); Unimatrix uses tracing | Separate GH Issue; update rand crate; low urgency |

**Challenge to SCOPE.md hypothesis on automatic HIGH → GH Issue**: Partially correct. HIGH severity alerts should not be silently ignored, but filing four separate HIGH-urgency GH Issues without reachability context creates false urgency. The right approach: one GH Issue per dependency cluster with a reachability assessment table, then process through the existing bugfix protocol at appropriate priority. The bugfix protocol handles version bumps cleanly (Phase 1 investigator confirms reachability and fix approach, Phase 2 rust-dev bumps version, Phase 3 tester verifies no regression).

**Challenge to creating a new "security triage session type"**: Unnecessary. Dependabot alerts are a narrow subtype of bugfix — a dependency version bump with CVE context. The existing bugfix protocol covers this entirely. Adding a new session type adds protocol complexity without adding value.

**Ranked Security Process Improvements**:

**Rank 1 — cargo audit as a CI gate (~2 hours)**
Add a `cargo-audit` job to `.github/workflows/ci.yml`. This closes the supply-chain gap permanently and turns Dependabot into an early-warning system rather than the only gate. Every future dependency CVE is caught at PR time. Instruction: `cargo install cargo-audit && cargo audit`. Include `--ignore RUSTSEC-XXXX` flags for alerts that have been assessed as non-reachable and tracked in a GH Issue.

**Rank 2 — Dependabot alerts → clustered GH Issues → bugfix protocol (1 hour triage + 1 bugfix session per cluster)**
Create three GH Issues (openssl cluster, rustls-webpki cluster, rand). Include a reachability assessment table in each issue body. Assign normal priority, not urgent, unless reachability is confirmed. Process through the standard bugfix protocol as bandwidth allows.

**Rank 3 — cargo test --workspace as a CI gate (~2 hours for CI job)**
Currently zero test execution in CI. A PR that breaks compilation passes CI. This is a larger correctness gap than cargo audit. A `cargo test --workspace -- --test-threads=1` job (or with --jobs flag to control parallelism) should be added alongside cargo audit. Some test suite tuning may be needed to avoid flakiness in the CI environment.

**Rank 4 — clippy -D warnings as a CI gate (~4 hours including fixing any existing warnings)**
Catches code quality issues including some security-adjacent patterns (unchecked arithmetic, unsafe blocks, integer overflow candidates). Lower urgency than the above since clippy runs during Gate 3b validator already in the delivery pipeline.

---

### Q: What would it look like to test Unimatrix's injection protections? What is the gap between "protections exist" and "protections are tested"?

**Answer**: The protections are real, multi-layered, and substantially tested at the unit level. The gap is adversarial coverage (evasion techniques not in the known-pattern set) and end-to-end wire testing (no test sends crafted JSON-RPC over the MCP stdio transport). A separate adversarial harness is the right approach — not infra-001 integration.

**Evidence**: Read `crates/unimatrix-server/src/infra/scanning.rs` (ContentScanner: 25+ injection regex patterns + 6 PII patterns via OnceLock), `infra/validation.rs` (size caps on every field per tool; ~150 unit tests; multibyte boundary cases), `services/gateway.rs` (rate limiting, quarantine exclusion, audit emission), `services/store_ops.rs` (scanning pipeline integration).

**Protection Inventory**:

| Layer | Mechanism | Location | Tests |
|-------|-----------|----------|-------|
| S3 — bounds | Byte-based size caps on every field per tool | `infra/validation.rs` | ~150 unit tests; multibyte boundary cases; byte-limit regression (GH #561) |
| S3 — control chars | Reject U+0000–U+001F (except \n/\t) in content fields | `infra/validation.rs` | Per-field tests including null byte, \x01 |
| S1 — injection scan on writes | 25+ regex patterns: InstructionOverride, RoleImpersonation, SystemPromptExtraction, DelimiterInjection, EncodingEvasion; hard reject | `infra/scanning.rs` | Positive and negative unit tests per category |
| S1 — PII scan on writes | 6 patterns: email, phone, SSN, Bearer token, AWS key, GitHub token; hard reject | `infra/scanning.rs` | Positive tests per pattern |
| S1 — injection scan on searches | Same ContentScanner; warn-only (non-blocking) | `services/gateway.rs` | Unit test verifies warning emitted, not error |
| S2 — rate limiting | Sliding window: 300 search / 60 write per caller per hour; UDS exempt | `services/gateway.rs` | Under/over limit, per-caller isolation, UDS exemption, lazy eviction |
| S4 — quarantine exclusion | Quarantined entries excluded from all results | `services/gateway.rs` | True/false cases |
| S5 — audit | Every write, scan warning, and duplicate detection logged to AUDIT_LOG | `infra/audit.rs` | Panic-free unit test; integration coverage via store operations |
| Internal caller bypass | AuditSource::Internal skips S1; S3 still applies | `services/gateway.rs` | Scan skipped, structural checks still fail on invalid input |
| Byte-level content cap | Byte check before char check to prevent multibyte evasion | `infra/validation.rs` + `services/store_ops.rs` | Multibyte boundary cases; byte-fires-before-char regression test |

**Gaps — what is not tested**:

1. **Adversarial evasion of injection patterns**: Unicode homoglyphs, zero-width characters, right-to-left override (U+202E), mixed encoding that spells "ignore previous instructions" without matching the literal regex. None are in the current test suite.

2. **Partial-encoding evasion**: The EncodingEvasion patterns catch specific constructs (`base64 decode:`, `\uXXXX\uXXXX\uXXXX`, URL-encoded triple sequences). An attacker who encodes only a subset of characters in the key phrase can potentially bypass these patterns.

3. **End-to-end MCP wire injection**: Unit tests call validation functions directly with Rust strings. No test sends a crafted JSON-RPC message over the actual stdio MCP transport and verifies the full pipeline (deserialization → validation → scanning → error response). A deserialization quirk could allow a payload to reach scanning in a different form than expected.

4. **Rate limit bypass via agent_id rotation**: Rate limits are keyed by `CallerId::Agent(agent_id_string)`. The agent_id is caller-reported. An agent that rotates its agent_id on each request would bypass per-caller rate limits. The enrollment system enforces capabilities, but rate limiting does not consult the registry.

5. **Internal caller elevation**: The `AuditSource::Internal` classification that bypasses S1 scanning is set based on how `audit_ctx` is constructed from the MCP transport layer. Whether a caller can supply crafted parameters to cause Internal classification was not fully investigated. If that path is unguarded, the S1 bypass is a real exposure. **This gap should be investigated before the system is deployed in multi-tenant or networked configurations.**

6. **Scan bypass on read paths**: context_search/context_get/context_lookup do not scan returned content. If an internal caller stores agent-controlled content containing injection payloads, those payloads are returned to other agents without scanning. Currently low risk (internal callers are trusted subsystems), but becomes an exposure if any future internal write path accepts externally-sourced content.

**Recommended testing approach**:

The SCOPE.md hypothesis that injection testing belongs in a separate suite (not infra-001) is correct.

**Tier 1 — Extend unit tests in `infra/scanning.rs`** with adversarial evasion cases as they are identified. This is the right home for specific-pattern tests. Fast, deterministic, fits existing test infrastructure. Ongoing effort, ~2–4 hours per new evasion class.

**Tier 2 — Separate wire-level adversarial harness** in `product/test/` alongside infra-001. A small test client (Python or Rust) that sends crafted JSON-RPC messages over stdio to a running Unimatrix server and asserts correct rejection codes. Necessary because end-to-end wire behavior cannot be tested at the unit level. Scope before implementing: requires a design decision on harness language, startup/teardown protocol, and CI integration. Effort: ~1 sprint.

**Against fuzzing as the primary approach**: cargo-fuzz finds memory safety issues (OOM, panic, integer overflow) but is less effective at finding semantic injection bypasses. The ContentScanner patterns are specific phrase-matching, and a fuzzer is unlikely to generate adversarial phrases without significant guidance (structure-aware fuzzing would be required). A curated adversarial case library maintained manually — and extended after public jailbreak/injection disclosures — is more effective for this threat model.

**Against integrating into infra-001**: infra-001 tests MCP tool semantics (storage, retrieval, contradiction detection). Mixing adversarial tests into it muddies its purpose, slows the normal integration suite, and makes triage harder when tests fail.

---

## Unanswered Questions

**Internal caller elevation path**: How exactly does `AuditSource::Internal` get set from the MCP transport layer? Can a caller supply crafted parameters to cause Internal classification? Requires reading `crates/unimatrix-server/src/mcp/context.rs`. Not investigated in this spike. If unguarded, this is a real S1 bypass exposure and should be filed as a security investigation GH Issue.

---

## Out-of-Scope Discoveries

**CI has zero correctness enforcement**: The current `ci.yml` runs only the inference site convention bash script. No `cargo build`, no `cargo test`, no `cargo clippy`. A PR that does not compile passes CI. This is more severe than originally framed in the SCOPE.md — the gap is not just "cargo audit missing" but the entire CI correctness pipeline. Ranks 1 and 3 from the recommendations should be treated as a single CI overhaul effort rather than incremental additions.

**Scan bypass on read paths**: context_search and context_get do not scan returned content. Not currently a reachable vulnerability (internal callers are trusted subsystems), but becomes an exposure if any future internal write path accepts externally-sourced content (e.g., a tool that stores external API responses). Flag for security review when any new internal write path is added.

---

## Recommendations Summary

- **Q1 (workflow diagrams)**: Three files written to `docs/workflows/`. Design session: overall phase flow + Phase 2 agent detail. Delivery session: overall stage flow + Phase 4 detail. PM governance: single loop diagram covering uni-zero → Design → Delivery → uni-retro → protocol update cycle. Unimatrix touchpoints highlighted in yellow.
- **Q2 (PM process flow)**: `docs/workflows/pm-governance.md` is the canonical reference. Three mandatory human checkpoints per feature. Protocol changes are intentionally slow and human-gated — this is the control mechanism that keeps the swarm system trustworthy.
- **Q3 (security gaps)**: Rank 1: add `cargo audit` CI job (~2 hours). Rank 2: create one GH Issue per Dependabot alert cluster with reachability assessment, process through bugfix protocol. Do not create a new session type. The four HIGH openssl CVEs are low-reachability in Unimatrix's current threat model but warrant version bumps. Rank 3: add `cargo test --workspace` to CI — the larger correctness gap.
- **Q4 (injection testing)**: Extend unit tests with adversarial evasion cases (Tier 1, ongoing). Build a separate wire-level adversarial harness in `product/test/` (Tier 2, ~1 sprint, scope first). Do not integrate into infra-001. Investigate the internal caller elevation path (`mcp/context.rs`) before declaring the S1 bypass safe in networked deployments.
