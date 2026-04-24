# ASS-054: Workflow Documentation, Manual Processes & Security Process Expansion

**Date**: 2026-04-24
**Tier**: 2 — process improvement; no delivery gating
**Feeds**: docs/workflows/ (new), uni-workflow-review skill evolution, security posture

---

## Goal

Answer the following questions:

1. **Workflow diagrams**: What Mermaid diagrams best communicate the Design Session (Session 1) and Delivery Session (Session 2) workflows to human contributors — showing the full agent swarm lifecycle, gate structure, and Unimatrix integration points (context_cycle, context_briefing, ADR storage) in a way that is visually digestible rather than technically exhaustive?

2. **PM process flow**: How does the human product-management layer (uni-zero vision alignment + uni-retro post-merge feedback loop) interact with the automated swarm workflows, and how should this be documented as a diagram that other contributors can use to understand the full governance model?

3. **Security process gaps**: Given the current security posture (uni-security-reviewer at PR stage, cargo audit mentioned in protocol but not enforced as a gate, no injection testing), what specific process changes would meaningfully improve supply-chain and code-vulnerability coverage? How should dependabot alerts be triaged and integrated into the existing workflow?

4. **Injection testing**: What would it look like to test Unimatrix's existing injection protections? What is the gap between "protections exist" and "protections are tested"?

---

## Why It Matters

The protocols are sophisticated but invisible to anyone new to the project. Diagrams are the fastest path to comprehension. Without them, contributors spend session time re-reading protocol files instead of understanding the system's intent.

The two human-maintained processes (uni-zero, uni-retro) are undocumented outside the skill files themselves. Anyone trying to understand how workflow evolution happens — how a retro recommendation becomes a protocol change — has no map.

Dependabot shows 4 open HIGH-severity findings against openssl (buffer overflow, AES key wrap, memory leak, digest_final overwrite) plus medium/low findings against rustls-webpki and rand. These are currently untracked in GitHub Issues and have no defined triage process. The delivery protocol mentions `cargo audit` in a truncation example, not as an enforced gate step.

The injection protections in Unimatrix (input validation at MCP boundaries, content size caps, quarantine) have never been adversarially tested. This is a known gap the human wants quantified before deciding on a testing approach.

---

## Breadth

`code-only` — all artifacts are in-repo:
- `.claude/protocols/uni/` — all four protocols
- `.claude/agents/uni/` — all agent definitions
- `.claude/skills/uni-retro/`, `uni-zero/`, `uni-review-pr/`
- `.github/workflows/ci.yml`
- `crates/unimatrix-server/src/` — injection protection implementation
- Dependabot alert data (already retrieved via gh CLI — no external research needed)

---

## Approach

`investigation + documentation`

1. Map each workflow phase and agent to a Mermaid diagram layer — prioritizing visual clarity over completeness. Each diagram covers one workflow (Design, Delivery). Unimatrix touchpoints are called out explicitly (context_cycle start/stop, phase-end calls, context_briefing in agent prompts, ADR storage by architect).

2. Map the human governance layer as a separate PM process diagram — showing where uni-zero feeds into scope approval, where uni-retro feeds back into protocol evolution, and what the human-approval gate looks like for both.

3. Audit the security surface: current gates vs. gaps, dependabot alert severity distribution, what cargo audit covers vs. what it misses, where injection protections live in code and what they protect against.

4. Produce concrete recommendations — not implementations. Each recommendation states: what to add, where it fits in the existing workflow (which phase/gate), what it would catch that current process misses, and rough effort level.

---

## Confidence Required

`directional` — diagrams are output, not decisions. Security recommendations require human approval before any process change. No PoC or measurement required.

---

## Target Outputs

`FINDINGS.md` containing:

1. **Design Session diagram** — Mermaid flowchart covering Phase 1 (researcher + scope approval), Phase 1b (scope risk), Phase 2a (architect + spec parallel), Phase 2a+ (risk strategist), Phase 2b (vision guardian), Phase 2c (synthesizer), Phase 2d (return to human). Unimatrix context_cycle calls annotated. File saved to `docs/workflows/design-session.md`.

2. **Delivery Session diagram** — Mermaid flowchart covering Stage 3a (pseudocode + test plan), Gate 3a, Stage 3b (wave-based implementation), Gate 3b, Stage 3c (testing), Gate 3c, Phase 4 (PR + security review + cycle close). Unimatrix context_cycle calls annotated. File saved to `docs/workflows/delivery-session.md`.

3. **PM governance diagram** — Mermaid flowchart showing: where uni-zero runs (pre-scope, pre-design, pre-merge), where uni-retro runs (post-merge), how retro recommendations feed back to protocol review (human-approved, slow change). File saved to `docs/workflows/pm-governance.md`.

4. **Security gap analysis** — assessment of: current security gates and what they cover, dependabot findings with severity mapping, cargo audit gap (mentioned but not gated), CI coverage gap (single check today), injection protection inventory and test gap.

5. **Security process recommendations** — ranked list of concrete additions, each with: proposed location in workflow, what it catches, effort estimate. Topics: dependabot triage process (GH issues? bugfix flow?), cargo audit as a CI gate, injection test harness concept.

---

## Constraints

**Hard**:
- Diagrams must be Mermaid (render natively in GitHub/VS Code)
- Diagrams target human readers, not agents — explanatory labels over technical precision
- Recommendations are not implementations — nothing changes without human approval
- uni-workflow-review skill itself does not need updating; the PM governance diagram IS the missing documentation

**Hypothesis** (subject to challenge by researcher):
- Dependabot HIGH severity findings should become GH Issues and flow through the bugfix protocol
- cargo audit should be a CI gate job alongside the existing inference-site check
- Injection testing belongs in a separate test suite (adversarial / fuzzing), not integrated into the current infra-001 harness

---

## Prior Art

- `.claude/protocols/uni/uni-design-protocol.md` — full Session 1 flow
- `.claude/protocols/uni/uni-delivery-protocol.md` — full Session 2 flow
- `.claude/protocols/uni/uni-bugfix-protocol.md` — bugfix flow (reference only)
- `.claude/skills/uni-retro/SKILL.md` — retro process
- `.claude/skills/uni-zero/SKILL.md` — vision guide process
- `.claude/skills/uni-review-pr/SKILL.md` — current PR security review
- `.claude/agents/uni/uni-security-reviewer.md` — security reviewer capabilities
- `.github/workflows/ci.yml` — current CI (single job: inference site check)
- Dependabot alert data: 4 HIGH openssl CVEs (buffer overflow, AES, PSK, digest_final), 1 medium rustls-webpki CRL matching, 2 low openssl, 4 low rand/rustls-webpki
- `crates/unimatrix-server/src/` — injection protection implementation (content size caps, input validation)
- `product/research/ass-050/` — security model foundation (MCP identity, audit attribution)

---

## Dependencies

None — this spike is self-contained. No other spike must complete before this one begins.

What this spike unblocks:
- `docs/workflows/` directory creation and population
- Security process GH issues (dependabot triage, cargo audit CI gate)
- PM governance documentation for onboarding
