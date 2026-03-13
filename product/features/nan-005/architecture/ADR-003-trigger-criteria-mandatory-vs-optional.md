## ADR-003: Documentation Step Trigger Criteria — Mandatory for User-Visible Changes

### Context

SCOPE.md AC-08 states the documentation step is "optional" — the Delivery Leader determines whether the feature warrants documentation updates. The risk assessment (SR-05) flagged this as a decay risk: pure optionality means Delivery Leaders may always skip the step, defeating the purpose.

Two extremes are wrong:
- **Always mandatory**: Forces the agent to run on test-only or infrastructure-only features where it produces no useful output. Adds latency for no value.
- **Always optional**: Relies on human judgment under delivery pressure. The documentation decay problem that motivated nan-005 will recur.

The right model is a decision rule, not human discretion. The Delivery Leader should not need to judge — the rule should be deterministic based on observable feature properties.

### Decision

The documentation step is **mandatory** when any of the following are true:
- Feature adds or modifies an MCP tool (any change to `context_*` handler or its parameters)
- Feature adds or modifies a skill (any change to `.claude/skills/*/SKILL.md`)
- Feature adds or modifies a CLI subcommand or global flag
- Feature adds a new knowledge category to the allowlist
- Feature introduces a new operational constraint users must know (e.g., new session boundaries, naming conventions)
- Feature changes user-visible behavior of an existing capability

The documentation step is **skipped** when:
- Feature is internal-only (refactor, performance improvement, test infrastructure, CI/CD, build system)
- Feature is test-only
- Feature is documentation-only (nan-005 itself)

The Delivery Leader evaluates these criteria against the feature's SCOPE.md "Goals" section. If goals mention any of the mandatory triggers, the step runs. If goals are entirely internal, the step is skipped.

The protocol modification includes this decision table explicitly so the Delivery Leader has a deterministic rule, not a judgment call.

### Consequences

- Documentation decay is prevented for all user-facing features that go through the delivery protocol.
- Internal features (majority of infrastructure work) skip the step without overhead.
- The decision rule is in the protocol, not in agent definitions — the Delivery Leader applies it, not uni-docs.
- Features delivered outside the protocol (hotfixes, emergency patches) remain undocumented until the next protocol delivery touches the same area. Accepted risk per SCOPE.md Constraint 2.
- Edge cases: a feature that only adds tests for an existing tool does not trigger mandatory documentation. A feature that changes tool behavior (even without adding a new tool) does trigger it.
