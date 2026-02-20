# Learning and Knowledge Evolution Architecture

## Executive Summary

Agentic development platforms face a fundamental challenge: each AI coding session generates valuable knowledge---about what works, what fails, which patterns suit a codebase, and which conventions the team prefers---but that knowledge typically evaporates when the session ends. The cost is paid repeatedly in wasted tokens, repeated mistakes, and agents that never get smarter.

This document presents a practical architecture for **continuous learning in multi-project agentic development**, designed specifically for Unimatrix. The architecture operates across four levels (session, team, project, global), implements knowledge lifecycle management with active pruning of outdated patterns, uses structured feedback loops to convert corrections into durable improvements, and progressively reduces human oversight as the system proves reliability.

Key design principles:

- **Every correction is a learning opportunity**: When a human corrects an agent, the correction is captured, generalized, and stored so no agent makes the same mistake twice.
- **Knowledge has a lifecycle**: Patterns are created, validated, promoted, evolved, and eventually deprecated or pruned. Stale knowledge is more dangerous than no knowledge.
- **Trust is earned through evidence**: Human review gates decrease only when metrics demonstrate sustained quality. Trust escalation is reversible.
- **Token budgets are finite**: The learning system must reduce context consumption over time, not increase it. Retrieval must be precise, not exhaustive.

---

## The Learning Challenge in Agentic Development

### Why Current Approaches Fall Short

Most AI coding tools today have rudimentary or no cross-session learning. The dominant approaches and their limitations:

| Approach | Example | Limitation |
|----------|---------|------------|
| Static rule files | `.cursorrules`, `CLAUDE.md` | Manual maintenance, no automatic evolution, no validation of effectiveness |
| Session memory | Claude Code auto-memory | Limited to ~200 lines loaded per session; no cross-agent sharing; no pruning |
| Prompt engineering | System prompts with conventions | Token-expensive, scales poorly, no feedback on what actually works |
| Fine-tuning | Custom model training | Expensive, slow iteration, difficult to version or roll back |

As [Addy Osmani's 2026 workflow analysis](https://addyosmani.com/blog/ai-coding-workflow/) demonstrates, even sophisticated practitioners rely on manually maintained rule files and prompt plans. This is the state of the art---and it does not scale.

### The Compounding Cost Problem

Without learning, every agent session starts from near-zero context. Consider the cost trajectory:

- **Session 1**: Agent learns project uses pnpm, not npm. 500 tokens spent on correction.
- **Session 2**: Different agent makes same mistake. Another 500 tokens.
- **Session N**: The same correction has been made N times at 500 tokens each.

With 50 agents running across 10 projects, even small repeated mistakes compound into thousands of wasted tokens per day and significant developer frustration. The learning system's primary economic function is to **convert one-time corrections into permanent knowledge**, collapsing O(N) correction costs to O(1).

### The Staleness Danger

The inverse problem is equally dangerous. As documented in research on [knowledge decay in RAG systems](https://ragaboutit.com/the-knowledge-decay-problem-how-to-build-rag-systems-that-stay-fresh-at-scale/), stale knowledge actively degrades performance: an agent following deprecated API patterns or abandoned architectural conventions produces code that must be entirely rewritten. Knowledge freshness is not a nice-to-have---it is an architectural requirement ranked equal to knowledge capture.

---

## Multi-Level Learning Architecture

The learning system operates across four nested levels, each with distinct characteristics for knowledge scope, update frequency, validation requirements, and access patterns.

### Level 1: Session Learning

**Scope**: A single agent working on a single task within one coding session.

**What is captured**:
- Tool invocations and their outcomes (success/failure)
- Human corrections and the context that triggered them
- Execution traces: which approaches were tried and abandoned
- Final working solution and the path to reach it

**Storage**: Session transcripts stored as structured logs with extracted "learnings" tagged by category.

**Implementation pattern**:
```
session_learning:
  capture:
    - correction_events: { trigger, incorrect_action, correction, context }
    - tool_results: { tool, input, output, success, duration }
    - abandoned_approaches: { approach, reason_abandoned }
    - final_solution: { task, solution, validation_status }
  extraction:
    - on_session_end: extract_generalizable_patterns()
    - on_human_correction: capture_correction_immediately()
  promotion:
    - if pattern seen >= 2 sessions: promote_to_project_level()
    - if correction is universal: promote_to_global_level()
```

**Key insight from [Arize's self-improving agent research](https://arize.com/blog/closing-the-loop-coding-agents-telemetry-and-the-path-to-self-improving-software/)**: Traces are the primary documentation for agent-driven applications. An effective session learning system must instrument agent execution to capture not just what the agent did, but why---including intermediate reasoning, tool selection logic, and self-correction attempts.

### Level 2: Team Learning

**Scope**: Knowledge shared across multiple agents working within the same project simultaneously or sequentially.

**What is captured**:
- Patterns that multiple agents independently discover
- Coordination knowledge: "Agent A is modifying the auth module; avoid concurrent changes"
- Conflict resolutions: when two agents produce incompatible changes, how was it resolved?
- Collective velocity metrics: which task decompositions lead to faster completion?

**Storage**: Project-scoped shared knowledge store, updated in near-real-time.

**Implementation pattern**:
```
team_learning:
  shared_state:
    - active_modifications: { agent_id, files, estimated_completion }
    - discovered_patterns: { pattern, discovered_by, confidence, usage_count }
    - conflict_log: { agents, conflict_type, resolution, timestamp }
  propagation:
    - broadcast_on: pattern_discovery, conflict_resolution
    - query_on: task_start (check what other agents have learned)
  deduplication:
    - merge_equivalent_patterns()
    - resolve_contradictions_by_recency_and_confidence()
```

### Level 3: Project Learning

**Scope**: All accumulated knowledge about a specific project---its architecture, conventions, testing approach, deployment process, and historical decisions.

**What is captured**:
- Architecture decisions and rationale (ADRs, extracted or explicit)
- Code conventions with examples (not just rules, but why the rule exists)
- Testing patterns: what testing approach works for which module types
- Build and deployment specifics
- Common pitfalls specific to this codebase
- Dependency-specific knowledge (library versions, known issues, workarounds)

**Storage**: Version-controlled knowledge base co-located with the project repository.

**Implementation pattern**:
```
project_learning:
  knowledge_categories:
    architecture:
      - patterns: { name, description, where_applied, rationale, examples }
      - decisions: { decision, context, alternatives_considered, date }
      - anti_patterns: { pattern, why_bad, what_to_do_instead }
    conventions:
      - coding_style: { rule, examples, exceptions, rationale }
      - naming: { entity_type, convention, examples }
      - file_structure: { pattern, rationale }
    testing:
      - strategies: { module_type, testing_approach, example_tests }
      - known_flaky_tests: { test, flakiness_reason, mitigation }
    operations:
      - build_commands: { environment, commands, common_errors }
      - deployment: { process, gates, rollback_procedure }
  validation:
    - each_pattern_tracks: { usage_count, success_rate, last_validated }
    - stale_threshold: 90_days_without_validation
```

This level maps closely to how [Alfredo Perez's universal knowledge base](https://medium.com/ngconf/universal-knowledge-base-for-ai-2da5748f396c) structures cross-tool development knowledge---separating framework patterns, project guides, and tool configurations into composable units.

### Level 4: Global Learning

**Scope**: Universal patterns that apply across all projects regardless of technology stack, team, or domain.

**What is captured**:
- Language-level best practices (TypeScript patterns, Python idioms, etc.)
- Framework-specific knowledge (React patterns, Django conventions, etc.)
- Universal development practices (git workflow, PR conventions, error handling)
- Security patterns (input validation, auth patterns, secrets management)
- Performance patterns (caching strategies, query optimization, etc.)

**Storage**: Central knowledge base accessible to all projects, with strict validation gates before promotion.

**Implementation pattern**:
```
global_learning:
  promotion_criteria:
    - pattern must be validated in >= 3 distinct projects
    - pattern must have >= 90% success rate across uses
    - pattern must be reviewed by human maintainer
    - pattern must not conflict with existing global patterns
  categories:
    - language_patterns: { language, pattern, applicability, examples }
    - framework_patterns: { framework, version_range, pattern, examples }
    - universal_practices: { practice, rationale, exceptions }
    - security_patterns: { threat, mitigation, implementation_guide }
  versioning:
    - each pattern has: { version, effective_date, supersedes, changelog }
    - deprecated patterns link to: replacement_pattern, migration_guide
```

### Level Interaction and Knowledge Flow

Knowledge flows primarily upward (session -> team -> project -> global) through a promotion pipeline, and downward (global -> project -> session) through context injection at session start.

```
                    +------------------+
                    |   Global Level   |  Universal patterns
                    +--------+---------+
                             |
              promotion ^    | injection v
                             |
                    +--------+---------+
                    |  Project Level   |  Project-specific knowledge
                    +--------+---------+
                             |
              promotion ^    | injection v
                             |
                    +--------+---------+
                    |   Team Level     |  Cross-agent coordination
                    +--------+---------+
                             |
              promotion ^    | injection v
                             |
                    +--------+---------+
                    |  Session Level   |  Immediate learnings
                    +------------------+
```

**Critical design rule**: Downward injection must be budget-aware. An agent starting a new session does not receive all global + project + team knowledge. Instead, the system retrieves only the knowledge relevant to the current task, using the task description and target files to select applicable patterns. This keeps context windows lean and focused.

---

## Knowledge Lifecycle Management

Every piece of knowledge in the system progresses through a defined lifecycle. Treating knowledge as a living entity with birth, maturity, and retirement phases is essential to prevent the accumulation of stale or contradictory guidance.

### Lifecycle Stages

```
  PROPOSED -> VALIDATED -> ACTIVE -> AGING -> DEPRECATED -> ARCHIVED
                 |                     |          |
                 |                     |          +-> EVOLVED (new version)
                 |                     |
                 +-- rejected          +-- reinforced (reset aging clock)
```

#### 1. Proposed

A new pattern or learning enters the system, typically extracted from a session correction, an architectural discussion, or a code review finding.

```yaml
status: proposed
source: session_correction | code_review | manual_entry | automated_extraction
confidence: low
validation_count: 0
created_at: timestamp
proposed_by: agent_id | human_id
```

#### 2. Validated

The pattern has been confirmed to work correctly in at least one real scenario. For automatically extracted patterns, this requires either explicit human approval or successful application without subsequent correction.

```yaml
status: validated
confidence: medium
validation_count: 1+
validated_by: human_review | successful_application | test_pass
```

#### 3. Active

The pattern is in regular use, has been validated multiple times, and is automatically included in relevant agent context.

```yaml
status: active
confidence: high
usage_count: N
success_rate: percentage
last_used: timestamp
last_validated: timestamp
```

#### 4. Aging

The pattern has not been used or validated within a configurable threshold period (default: 90 days for project-level, 180 days for global-level). Aging patterns receive reduced priority in retrieval and are flagged for review.

```yaml
status: aging
days_since_last_use: N
days_since_last_validation: M
review_assigned_to: human_id | null
aging_started: timestamp
```

#### 5. Deprecated

The pattern has been explicitly superseded by a newer approach or flagged as no longer applicable. Deprecated patterns remain in the system with a pointer to their replacement, ensuring agents that encounter references to the old pattern can self-correct.

```yaml
status: deprecated
deprecated_at: timestamp
deprecated_by: human_id | automated_detection
reason: explanation
replacement: pattern_id | null
migration_guide: text | null
```

#### 6. Archived

Fully removed from active retrieval. Retained only for historical analysis and audit purposes.

### Knowledge Pruning Engine

The pruning engine runs on a configurable schedule (recommended: daily for project-level, weekly for global-level) and performs:

1. **Staleness detection**: Identify patterns that have not been used or validated within their threshold period. Flag for review or auto-transition to AGING.

2. **Contradiction detection**: Identify patterns within the same scope that give conflicting guidance. Flag for human resolution or auto-resolve using recency + confidence scoring.

3. **Redundancy detection**: Identify patterns that are subsets of broader patterns. Merge or archive the narrower pattern.

4. **Effectiveness analysis**: Identify patterns with low success rates (below 70%). Flag for review, evolution, or deprecation.

5. **Dependency checking**: When a technology version changes (e.g., React 18 -> React 19), flag all patterns that reference the old version for review.

```python
# Pseudocode for the pruning engine
def run_pruning_cycle(knowledge_base, scope):
    for pattern in knowledge_base.get_active_patterns(scope):
        # Staleness check
        if pattern.days_since_last_validation > scope.staleness_threshold:
            pattern.transition_to(AGING)
            notify_reviewers(pattern)

        # Effectiveness check
        if pattern.usage_count > 10 and pattern.success_rate < 0.70:
            flag_for_review(pattern, reason="low_effectiveness")

        # Contradiction check
        contradictions = knowledge_base.find_contradicting(pattern)
        if contradictions:
            resolve_or_flag(pattern, contradictions)

    # Dependency version check
    for dep_change in knowledge_base.get_recent_dependency_changes():
        affected = knowledge_base.find_patterns_referencing(dep_change.old_version)
        for pattern in affected:
            flag_for_review(pattern, reason="dependency_updated")
```

### Evolution Workflow

When a pattern needs to evolve rather than simply being deprecated:

1. **Fork**: Create a new version of the pattern with proposed changes.
2. **Test**: Run the new version alongside the old version (A/B style) if feasible, or validate in a controlled set of sessions.
3. **Promote**: If the new version performs better, transition the old version to DEPRECATED with a pointer to the new one.
4. **Migrate**: Update any derived rules, agent prompts, or documentation that referenced the old version.

This mirrors the hybrid conflict resolution approach documented in [BuCoR research](https://arxiv.org/html/2507.19432v1), where both example-based and rule-based strategies are used to handle evolving patterns.

---

## Feedback Loop Design Patterns

Effective learning requires closed feedback loops---mechanisms where outcomes inform future behavior. Based on research into [agentic design patterns](https://docs.cloud.google.com/architecture/choose-design-pattern-agentic-ai-system) and [self-improving software architectures](https://arize.com/blog/closing-the-loop-coding-agents-telemetry-and-the-path-to-self-improving-software/), the following feedback loop patterns are recommended for Unimatrix.

### Loop 1: Immediate Correction Loop

**Trigger**: Human corrects agent output during a session.
**Cycle time**: Seconds to minutes.
**Scope**: Session, potentially promoted to project/global.

```
Agent produces output
  -> Human identifies error
    -> Human provides correction
      -> System captures: { context, error, correction, category }
        -> Agent adjusts behavior for remainder of session
          -> Correction stored as PROPOSED pattern
            -> If correction is generalizable: flag for promotion
```

**Implementation detail**: The correction capture must include sufficient context to make the learning generalizable. A correction of "use `pnpm` not `npm`" without context is project-specific. A correction of "always check the lockfile type before assuming the package manager" is globally useful. The extraction engine should attempt both specific and general formulations.

### Loop 2: Build/Test Feedback Loop

**Trigger**: Agent-generated code is compiled, linted, or tested.
**Cycle time**: Minutes.
**Scope**: Session and team.

```
Agent writes code
  -> CI/linter/tests run automatically
    -> Results captured as structured feedback
      -> If failure: agent receives error + context
        -> Agent self-corrects
          -> If self-correction succeeds: pattern stored
          -> If self-correction fails: escalate to human
```

This is the most well-established feedback loop in current tools. As [GrowthBook's research](https://blog.growthbook.io/feedback-loops-are-the-next-breakthrough-in-agentic-coding/) demonstrates, the key advancement is extending this loop beyond technical correctness (does the code compile?) to business effectiveness (does the feature achieve its goal?).

### Loop 3: Code Review Feedback Loop

**Trigger**: Agent-generated code undergoes human review (or automated review).
**Cycle time**: Hours to days.
**Scope**: Project and global.

```
Agent submits PR
  -> Automated review checks run (lint, security, patterns)
    -> Human reviewer provides feedback
      -> Feedback categorized: { style, architecture, logic, security, performance }
        -> Patterns extracted from recurring feedback themes
          -> Patterns validated and promoted
```

**Implementation detail**: Review feedback is the richest source of project-level learning. The system should track review feedback themes over time. If a reviewer consistently flags the same type of issue, that signals a missing or ineffective pattern in the knowledge base.

### Loop 4: Retrospective Learning Loop

**Trigger**: Periodic (end of sprint, end of feature, end of incident).
**Cycle time**: Days to weeks.
**Scope**: Project and global.

```
Accumulated session data analyzed
  -> Success/failure patterns identified across sessions
    -> Knowledge base gaps identified
      -> New patterns proposed or existing patterns evolved
        -> Pruning cycle runs on stale patterns
          -> Updated knowledge base deployed to all agents
```

### Loop 5: Outcome Feedback Loop

**Trigger**: Deployed code produces observable outcomes (performance metrics, error rates, user behavior).
**Cycle time**: Days to months.
**Scope**: Project and global.

```
Feature deployed to production
  -> Telemetry collected (error rates, performance, adoption)
    -> Outcomes compared to predictions/goals
      -> If positive: reinforce patterns used in implementation
      -> If negative: flag patterns for review, capture anti-patterns
        -> Knowledge base updated with outcome data
```

This is the most valuable and most difficult loop to close. It requires integration with production telemetry systems and the ability to trace outcomes back to implementation decisions.

### Feedback Loop Priority and ROI

| Loop | Implementation Effort | Learning Value | Recommended Phase |
|------|----------------------|---------------|-------------------|
| Immediate Correction | Low | High | Phase 1 (MVP) |
| Build/Test | Low | High | Phase 1 (MVP) |
| Code Review | Medium | Very High | Phase 2 |
| Retrospective | Medium | High | Phase 2 |
| Outcome | High | Very High | Phase 3 |

---

## Trust Calibration and Progressive Autonomy

Drawing from the [five-level autonomy framework](https://knightcolumbia.org/content/levels-of-autonomy-for-ai-agents-1) and [supervised autonomy research](https://edge-case.medium.com/supervised-autonomy-the-ai-framework-everyone-will-be-talking-about-in-2026-fe6c1350ab76), the Unimatrix trust system should implement progressive autonomy that increases agent independence as reliability is demonstrated.

### Trust Levels

```
Level 0: SUPERVISED     - Every action requires human approval
Level 1: GUIDED         - Agent proposes, human approves before execution
Level 2: MONITORED      - Agent executes, human reviews all output
Level 3: AUDITED        - Agent executes, human reviews samples
Level 4: AUTONOMOUS     - Agent executes, human reviews exceptions only
Level 5: TRUSTED        - Agent executes and self-validates, human audits periodically
```

### Trust Dimensions

Trust is not a single scalar. An agent may be highly trusted for writing unit tests but not trusted for modifying database schemas. Trust is tracked per **capability domain**:

| Domain | Description | Example Actions |
|--------|-------------|-----------------|
| Code Generation | Writing new code | New functions, modules, components |
| Code Modification | Changing existing code | Refactoring, bug fixes, feature changes |
| Testing | Writing and running tests | Unit tests, integration tests, test infrastructure |
| Configuration | Modifying build/deploy config | Package.json, CI config, environment variables |
| Architecture | Structural decisions | New modules, API design, data model changes |
| Security | Security-sensitive changes | Auth code, input validation, secrets handling |
| Documentation | Writing docs and comments | README, API docs, inline comments |
| Operations | Deploy and infrastructure | Database migrations, deploy scripts |

### Trust Score Calculation

Each domain maintains a trust score computed from recent performance:

```python
def calculate_trust_score(domain, agent_id, project_id):
    recent_actions = get_actions(
        domain=domain,
        agent_id=agent_id,
        project_id=project_id,
        window=timedelta(days=30)
    )

    if len(recent_actions) < minimum_actions_for_trust[domain]:
        return TrustLevel.SUPERVISED  # Not enough data

    success_rate = sum(a.approved for a in recent_actions) / len(recent_actions)
    severity_weighted = weighted_success(recent_actions)  # Weight by impact
    trend = calculate_trend(recent_actions)  # Improving or declining?

    score = (
        0.5 * severity_weighted +
        0.3 * success_rate +
        0.2 * trend_score(trend)
    )

    return map_score_to_trust_level(score, domain)
```

### Trust Escalation Criteria

Transitioning to a higher trust level requires meeting all criteria:

| Transition | Minimum Actions | Success Rate | Sustained Period | Human Approval |
|-----------|----------------|--------------|-----------------|----------------|
| 0 -> 1 | 10 | 80% | 3 days | Required |
| 1 -> 2 | 25 | 85% | 7 days | Required |
| 2 -> 3 | 50 | 90% | 14 days | Required |
| 3 -> 4 | 100 | 93% | 30 days | Required |
| 4 -> 5 | 200 | 95% | 60 days | Required |

### Trust De-escalation (Circuit Breakers)

Trust must be revocable. De-escalation triggers:

- **Immediate drop to SUPERVISED**: Security vulnerability introduced, production incident caused, data loss.
- **Drop one level**: 3 consecutive rejections in code review, test failure rate exceeds threshold, pattern violation detected.
- **Reset to GUIDED**: Major architectural error, repeated same mistake after correction.

```python
def check_circuit_breakers(action_result, agent_id, domain):
    if action_result.severity == "critical" and not action_result.approved:
        set_trust_level(agent_id, domain, TrustLevel.SUPERVISED)
        notify_human_immediately(action_result)
        return

    recent_rejections = get_consecutive_rejections(agent_id, domain)
    if recent_rejections >= 3:
        decrease_trust_level(agent_id, domain, steps=1)
        log_deescalation(agent_id, domain, reason="consecutive_rejections")
```

### Progressive Autonomy in Practice

The practical effect of trust calibration is that human review gates adapt dynamically:

```
Week 1 (New project):
  - All PRs require human review
  - All architectural decisions require approval
  - Agent proposes, human decides

Week 4 (Trust building):
  - Simple code changes auto-merge after CI passes
  - Test additions auto-merge
  - Architecture changes still require review

Week 12 (Established trust):
  - Most code changes auto-merge
  - Only security-sensitive and architecture changes require review
  - Human reviews focus on sampling and auditing

Week 24+ (High trust):
  - Agent operates with minimal oversight
  - Human reviews are periodic audits, not per-change gates
  - Exception-based escalation only
```

---

## Automated Quality Gates and Validation

Quality gates serve as the objective validation mechanism that enables trust calibration. Without reliable quality measurement, progressive autonomy is impossible.

### Gate Architecture

Quality gates are organized as a pipeline, with each gate adding confidence:

```
Code Generation
  |
  v
Gate 1: Syntax & Lint       (instant, automated)
  |
  v
Gate 2: Type Checking        (fast, automated)
  |
  v
Gate 3: Unit Tests           (fast, automated)
  |
  v
Gate 4: Integration Tests    (medium, automated)
  |
  v
Gate 5: Pattern Compliance   (fast, automated)
  |
  v
Gate 6: Security Scan        (medium, automated)
  |
  v
Gate 7: Performance Check    (slow, automated)
  |
  v
Gate 8: Architectural Review (medium, automated + human for high-risk)
  |
  v
Gate 9: Human Review         (variable, trust-level dependent)
```

### Pattern Compliance Gate (Gate 5)

This is the novel gate enabled by the knowledge base. It checks agent output against active project and global patterns:

```python
def check_pattern_compliance(code_change, knowledge_base):
    violations = []
    applicable_patterns = knowledge_base.get_patterns_for(
        files=code_change.affected_files,
        categories=["conventions", "architecture", "security"]
    )

    for pattern in applicable_patterns:
        if pattern.has_automated_check:
            result = pattern.check(code_change)
            if not result.compliant:
                violations.append({
                    "pattern": pattern.id,
                    "violation": result.description,
                    "severity": pattern.severity,
                    "suggestion": pattern.fix_suggestion
                })

    return ComplianceResult(
        passed=len([v for v in violations if v["severity"] == "error"]) == 0,
        violations=violations
    )
```

### Self-Validation Pattern

Before submitting output for review, agents should run self-validation using the [reflection pattern](https://medium.com/@bijit211987/agentic-design-patterns-cbd0aed2962f) documented in agentic AI design literature:

```
Agent generates code
  -> Agent reviews own code against:
     - Task requirements (does it fulfill the specification?)
     - Known patterns (does it follow project conventions?)
     - Common errors (does it avoid known pitfalls?)
     - Test coverage (are there tests for new behavior?)
  -> If self-review identifies issues:
     - Agent self-corrects before submitting
     - Self-correction attempts are logged for learning
  -> If self-review passes:
     - Submit to automated quality gates
```

### Quality Metrics Tracked

| Metric | Description | Target |
|--------|-------------|--------|
| First-pass success rate | % of agent output that passes all gates without correction | > 85% |
| Self-correction rate | % of issues caught by agent self-review | > 60% |
| Human correction rate | % of output requiring human correction after gates | < 10% |
| Regression rate | % of changes that break existing functionality | < 2% |
| Pattern compliance | % of output that follows known project patterns | > 90% |
| Time to resolution | Average time from error detection to fix | < 5 min (automated), < 1 hr (human) |

### Regression Detection

Drawing from [2026 AI testing research](https://www.evozon.com/how-ai-is-redefining-software-testing-practices-in-2026), the system should implement:

1. **Baseline metrics**: Establish performance baselines for each agent across domains.
2. **Drift detection**: Monitor for statistical deviation from baselines.
3. **Root cause analysis**: When regression is detected, trace back to identify whether the cause is a knowledge base change, a model change, or a codebase change.
4. **Automated rollback**: If a knowledge base update causes regression, automatically revert the update and flag for human review.

---

## Knowledge Storage and Retrieval Patterns

### Storage Architecture

The knowledge base uses a layered storage approach optimized for both human readability and machine retrieval:

```
knowledge_store/
  global/
    languages/
      typescript/
        patterns.yaml
        anti_patterns.yaml
      python/
        patterns.yaml
    frameworks/
      react/
        v18/
          patterns.yaml
        v19/
          patterns.yaml
          migration_from_v18.yaml
    security/
      patterns.yaml
    universal/
      git_workflow.yaml
      error_handling.yaml

  projects/
    {project_id}/
      architecture/
        decisions.yaml
        patterns.yaml
        anti_patterns.yaml
      conventions/
        coding_style.yaml
        naming.yaml
        file_structure.yaml
      testing/
        strategies.yaml
        fixtures.yaml
      operations/
        build.yaml
        deploy.yaml
      team/
        velocity_patterns.yaml
        coordination.yaml

  sessions/
    {session_id}/
      transcript.log
      extracted_learnings.yaml
      corrections.yaml
```

### Knowledge Record Schema

Each knowledge record follows a standardized schema:

```yaml
id: "proj_react_component_pattern_001"
version: 3
status: active  # proposed | validated | active | aging | deprecated | archived
scope: project  # session | team | project | global

content:
  title: "React component file structure"
  description: "All React components use a flat file structure with co-located tests"
  rationale: "Reduces import complexity and makes component boundaries clear"
  examples:
    - context: "Creating a new React component"
      correct: |
        components/
          UserProfile/
            UserProfile.tsx
            UserProfile.test.tsx
            UserProfile.styles.ts
            index.ts
      incorrect: |
        components/
          UserProfile.tsx
        tests/
          UserProfile.test.tsx
        styles/
          UserProfile.styles.ts
  applicability:
    file_patterns: ["*.tsx", "*.jsx"]
    directories: ["src/components/**"]
    task_types: ["create_component", "refactor_component"]
  exceptions:
    - "Shared test utilities go in src/test-utils/"
    - "Global styles go in src/styles/"

metadata:
  created_at: "2026-01-15T10:00:00Z"
  created_by: "human:dev_lead"
  last_validated: "2026-02-10T14:30:00Z"
  usage_count: 47
  success_rate: 0.94
  confidence: 0.92
  tags: ["react", "file-structure", "conventions"]
  supersedes: "proj_react_component_pattern_001_v2"
  dependencies: ["proj_typescript_config_001"]
```

### Retrieval Strategy

Efficient retrieval is critical---loading irrelevant knowledge wastes tokens and can confuse agents. The retrieval system uses a multi-signal approach:

```
Task Description
  + Target Files/Directories
  + Technology Stack
  + Task Type (create, modify, test, deploy, etc.)
    |
    v
  Retrieval Engine
    |
    +-- File-path matching: patterns whose applicability.directories match
    +-- Tag matching: patterns whose tags match technology stack
    +-- Task-type matching: patterns whose task_types match current task
    +-- Recency weighting: recently validated patterns ranked higher
    +-- Confidence weighting: higher confidence patterns ranked higher
    |
    v
  Ranked Pattern List
    |
    +-- Token budget filter: include top-N patterns that fit within budget
    |
    v
  Context Injection (into agent prompt)
```

**Budget-aware retrieval**: Given a token budget of B for knowledge context, the system selects patterns greedily by relevance score until the budget is exhausted. Patterns are pre-tokenized so budget calculation is instant.

```python
def retrieve_knowledge(task, token_budget):
    candidates = score_and_rank_patterns(task)
    selected = []
    tokens_used = 0

    for pattern in candidates:
        pattern_tokens = pattern.pre_computed_token_count
        if tokens_used + pattern_tokens <= token_budget:
            selected.append(pattern)
            tokens_used += pattern_tokens
        elif pattern.priority == "critical":
            # Critical patterns (e.g., security) always included
            selected.append(pattern)
            tokens_used += pattern_tokens

    return selected, tokens_used
```

### Retrieval Optimization Over Time

The retrieval system itself learns. By tracking which retrieved patterns were actually useful (did the agent reference them? did the output comply with them?), the system can tune retrieval scoring:

- Patterns that are retrieved but never referenced get lower retrieval scores.
- Patterns that are retrieved and result in compliant output get reinforced.
- Patterns that are missing (agent violates a pattern that was not retrieved) get boosted for similar future tasks.

---

## Cross-Project Knowledge Sharing

### The Sharing-Isolation Tension

Cross-project knowledge sharing offers enormous value---a pattern discovered in Project A can immediately benefit Projects B through Z. But it also carries risks: project-specific conventions can leak across boundaries, creating confusion or incorrect behavior.

Drawing from [multi-tenant AI architecture research](https://learn.microsoft.com/en-us/azure/architecture/guide/multitenant/approaches/ai-machine-learning), the system implements a "shared core, isolated overlay" model.

### Architecture

```
+-----------------------------------------------------+
|                  Global Knowledge                     |
|  (Language patterns, security rules, universal best   |
|   practices -- available to all projects)             |
+-----------------------------------------------------+
        |               |               |
        v               v               v
+---------------+ +---------------+ +---------------+
| Project A     | | Project B     | | Project C     |
| Knowledge     | | Knowledge     | | Knowledge     |
|               | |               | |               |
| Inherits      | | Inherits      | | Inherits      |
| global +      | | global +      | | global +      |
| own patterns  | | own patterns  | | own patterns  |
|               | |               | |               |
| Can OVERRIDE  | | Can OVERRIDE  | | Can OVERRIDE  |
| global for    | | global for    | | global for    |
| local context | | local context | | local context |
+---------------+ +---------------+ +---------------+
```

### Sharing Mechanisms

#### 1. Promotion Pipeline

When a project-level pattern proves consistently effective, it can be promoted to global:

```
Project pattern with high success rate
  -> Nominated for promotion (auto or manual)
    -> Checked against other projects for applicability
      -> If applicable in >= 3 projects: promote to global
      -> If project-specific: remain project-level
```

#### 2. Cross-Project Recommendations

When a new project is created, the system recommends patterns from similar existing projects:

```python
def recommend_patterns_for_new_project(project_config):
    similar_projects = find_similar_projects(
        tech_stack=project_config.tech_stack,
        project_type=project_config.type,  # web app, API, library, etc.
        team=project_config.team
    )

    recommendations = []
    for project in similar_projects:
        high_value_patterns = project.knowledge_base.get_patterns(
            min_confidence=0.9,
            min_usage=20,
            status="active"
        )
        recommendations.extend(high_value_patterns)

    return deduplicate_and_rank(recommendations)
```

#### 3. Isolation Guarantees

To prevent knowledge leakage:

- **Namespace isolation**: Every pattern is namespaced to its scope. A project pattern cannot accidentally override a different project's pattern.
- **Explicit opt-in**: Cross-project patterns must be explicitly adopted, never auto-injected.
- **Override precedence**: Project-level patterns always override global patterns for that project. If a project has a specific convention that differs from the global default, the project convention wins.
- **Access control**: Projects can mark patterns as private (not eligible for promotion) for proprietary or sensitive patterns.

### Knowledge Taxonomy for Cross-Project Sharing

Not all knowledge types are equally shareable. The taxonomy below classifies knowledge by shareability:

| Category | Shareability | Examples |
|----------|-------------|----------|
| Language idioms | High (global) | TypeScript strict mode patterns, Python type hints |
| Framework patterns | Medium (framework-scoped) | React hooks patterns, Django model patterns |
| Architecture patterns | Medium (type-scoped) | Microservice communication, monorepo structure |
| Security patterns | High (global) | Input validation, auth flows, secrets management |
| Testing patterns | Medium (framework-scoped) | Jest patterns, Pytest fixtures |
| Code conventions | Low (project-specific) | Naming conventions, file structure |
| Business logic | None (project-private) | Domain-specific rules, proprietary algorithms |
| Operations | Low (infra-scoped) | CI/CD patterns, deployment procedures |

---

## Practical Implementation Approaches

### Phase 1: Foundation (Weeks 1-4)

**Goal**: Establish the basic capture and storage infrastructure.

**Deliverables**:
1. **Knowledge record schema** (YAML-based, version-controlled)
2. **Session correction capture**: Hook into agent session lifecycle to capture human corrections as structured events.
3. **Project-level knowledge files**: Establish the `knowledge_store/` directory structure in each project repository.
4. **Basic retrieval**: File-path and tag-based matching to inject relevant knowledge into agent context.
5. **Manual knowledge entry**: CLI/UI for developers to add patterns manually.

**Implementation notes**:
- Start with flat YAML files in the repository. No database needed yet.
- Use the existing `CLAUDE.md` / rules file paradigm but with structured schema.
- Retrieval is simple keyword + path matching at this stage.

### Phase 2: Automation (Weeks 5-10)

**Goal**: Automate knowledge extraction and begin building feedback loops.

**Deliverables**:
1. **Automated correction extraction**: When a human corrects an agent, the system automatically extracts a proposed pattern from the correction context.
2. **Build/test feedback loop**: Agent failures from CI are automatically captured and analyzed for recurring issues.
3. **Code review feedback extraction**: PR review comments are analyzed for pattern-worthy feedback.
4. **Pattern compliance gate**: Automated check that agent output follows active project patterns.
5. **Staleness detection**: Automated flagging of patterns that have not been validated recently.
6. **Basic trust scoring**: Track per-agent, per-domain success rates.

### Phase 3: Intelligence (Weeks 11-18)

**Goal**: Add intelligent retrieval, trust calibration, and cross-project sharing.

**Deliverables**:
1. **Semantic retrieval**: Use embeddings to match task descriptions to relevant patterns beyond keyword matching.
2. **Trust-based gating**: Implement progressive autonomy with configurable trust levels per domain.
3. **Cross-project promotion pipeline**: Patterns validated in multiple projects can be promoted to global.
4. **Knowledge evolution workflow**: Fork-test-promote pipeline for evolving patterns.
5. **Retrieval optimization**: Track which retrieved patterns are actually useful and tune scoring.
6. **Retrospective learning loop**: Periodic analysis of accumulated session data to identify systemic patterns.

### Phase 4: Maturity (Weeks 19-26)

**Goal**: Close the outcome feedback loop and achieve full knowledge lifecycle automation.

**Deliverables**:
1. **Outcome tracking**: Connect deployment outcomes (error rates, performance) back to implementation patterns.
2. **Automated knowledge graph**: Relationships between patterns (dependencies, supersession, conflicts) tracked automatically.
3. **Predictive pattern recommendation**: Based on task description and historical data, proactively recommend patterns before the agent starts.
4. **Self-improving retrieval**: The retrieval system tunes its own scoring based on pattern usefulness data.
5. **Dashboard and analytics**: Visibility into knowledge base health, trust levels, pattern effectiveness.

### Token Budget Strategy

A core Unimatrix principle is to "take away costs from token usage wherever possible." The learning system directly serves this goal:

| Mechanism | Token Savings |
|-----------|---------------|
| Precise retrieval (only relevant patterns) | Avoids loading full rule files (~2000-5000 tokens saved per session) |
| Correction prevention (agent does not make known mistakes) | Eliminates correction round-trips (~500-2000 tokens per avoided correction) |
| Precomputed solutions (known patterns injected, not discovered) | Agent does not spend tokens exploring known territory (~1000-3000 tokens saved) |
| Progressive autonomy (less human review overhead) | Reduces review request/response cycles (~200-500 tokens per skipped gate) |
| Knowledge pruning (stale patterns removed) | Avoids loading outdated context that confuses agents (~500-1500 tokens saved) |

**Estimated net impact**: A mature learning system should reduce average session token consumption by 20-40% compared to a static-rules baseline, while simultaneously improving output quality.

---

## Recommended Learning Architecture for Unimatrix

### Architecture Overview

```
+------------------------------------------------------------------+
|                        Unimatrix Platform                         |
|                                                                   |
|  +-------------------+    +-------------------+                   |
|  |   Agent Session   |    |   Agent Session   |  ...              |
|  |                   |    |                   |                   |
|  | +---------------+ |    | +---------------+ |                   |
|  | | Session       | |    | | Session       | |                   |
|  | | Learnings     | |    | | Learnings     | |                   |
|  | +-------+-------+ |    | +-------+-------+ |                   |
|  +---------|----------+    +---------|----------+                  |
|            |                        |                             |
|            v                        v                             |
|  +--------------------------------------------------+            |
|  |           Knowledge Ingestion Pipeline            |            |
|  |                                                    |            |
|  |  Correction Capture | Pattern Extraction |          |            |
|  |  Deduplication | Contradiction Detection            |            |
|  +----------------------+---------------------------+            |
|                         |                                         |
|                         v                                         |
|  +--------------------------------------------------+            |
|  |              Knowledge Store                      |            |
|  |                                                    |            |
|  |  +----------+  +----------+  +-----------+        |            |
|  |  |  Global  |  | Project  |  |   Team    |        |            |
|  |  | Patterns |  | Patterns |  | Patterns  |        |            |
|  |  +----------+  +----------+  +-----------+        |            |
|  |                                                    |            |
|  |  Lifecycle Engine | Pruning Engine                  |            |
|  +----------------------+---------------------------+            |
|                         |                                         |
|                         v                                         |
|  +--------------------------------------------------+            |
|  |           Knowledge Retrieval Engine               |            |
|  |                                                    |            |
|  |  Task Matching | Budget Optimization |              |            |
|  |  Relevance Scoring | Retrieval Learning            |            |
|  +----------------------+---------------------------+            |
|                         |                                         |
|                         v                                         |
|  +--------------------------------------------------+            |
|  |              Trust & Gating Engine                 |            |
|  |                                                    |            |
|  |  Trust Scores | Quality Gates | Circuit Breakers  |            |
|  |  Progressive Autonomy | Human Review Routing       |            |
|  +--------------------------------------------------+            |
|                                                                   |
|  +--------------------------------------------------+            |
|  |              Feedback Aggregation                  |            |
|  |                                                    |            |
|  |  CI Results | Review Feedback | Outcome Metrics    |            |
|  |  Retrospective Analysis                            |            |
|  +--------------------------------------------------+            |
+------------------------------------------------------------------+
```

### Core Components

#### 1. Knowledge Ingestion Pipeline

Responsible for converting raw signals (corrections, review feedback, CI results, telemetry) into structured knowledge records.

**Key design decisions**:
- **Asynchronous processing**: Knowledge extraction does not block the agent session. Corrections are captured immediately but pattern extraction runs asynchronously.
- **Human-in-the-loop for promotion**: Automatically extracted patterns start as PROPOSED. Promotion to VALIDATED requires either explicit human approval or demonstrated successful application.
- **Deduplication at ingestion**: Before creating a new pattern, check for semantic similarity with existing patterns. Merge if appropriate.

#### 2. Knowledge Store

The persistent storage for all knowledge across all levels.

**Key design decisions**:
- **Git-backed for project knowledge**: Project-level knowledge lives in the project repository, versioned alongside code. This ensures knowledge evolves with the codebase and can be reviewed in PRs.
- **Central store for global knowledge**: Global patterns live in a dedicated knowledge repository, with its own review and promotion process.
- **Structured YAML over free-text**: All knowledge records follow the standardized schema. This enables automated retrieval, compliance checking, and lifecycle management.

#### 3. Knowledge Retrieval Engine

Responsible for selecting the right knowledge for each agent session.

**Key design decisions**:
- **Budget-first design**: Every retrieval operation has a token budget. The engine never returns more knowledge than the budget allows.
- **Multi-signal ranking**: Combines file-path matching, tag matching, task-type matching, recency, and confidence into a unified relevance score.
- **Critical pattern override**: Security and correctness patterns marked as "critical" bypass budget limits and are always included.
- **Negative retrieval**: In addition to "what to do," the system retrieves relevant "what NOT to do" (anti-patterns and deprecated patterns with their replacements).

#### 4. Trust & Gating Engine

Manages progressive autonomy and quality validation.

**Key design decisions**:
- **Per-domain trust tracking**: Trust is not binary or singular. An agent has separate trust scores for code generation, testing, architecture, security, etc.
- **Evidence-based escalation**: Trust level increases require statistical evidence over a sustained period, plus human approval.
- **Automatic de-escalation**: Trust decreases are automatic when circuit breaker conditions are met.
- **Configurable per project**: Each project can set its own trust thresholds and gate configurations.

#### 5. Feedback Aggregation

Collects and processes signals from all feedback loops.

**Key design decisions**:
- **Structured signal format**: All feedback (CI results, review comments, corrections, telemetry) is normalized into a common event schema.
- **Batch analysis**: Retrospective analysis runs on accumulated data, not individual events, to identify systemic patterns.
- **Attribution tracking**: Each knowledge record tracks which feedback signals contributed to its creation and validation.

### Data Flow for a Typical Session

```
1. New session starts for task: "Add user avatar upload to profile page"

2. Retrieval Engine queries:
   - File patterns: src/components/Profile/*, src/api/user/*
   - Tags: react, file-upload, user-profile
   - Task type: feature_addition
   - Token budget: 3000 tokens

3. Retrieval returns (within budget):
   - Project pattern: "React component file structure" (200 tokens)
   - Project pattern: "File upload handling uses S3 presigned URLs" (350 tokens)
   - Project pattern: "User API endpoint conventions" (280 tokens)
   - Global pattern: "Image upload security: validate MIME type server-side" (180 tokens)
   - Project anti-pattern: "Do NOT use base64 encoding for images" (150 tokens)
   - Total: 1160 tokens (under budget, room for additional context)

4. Agent executes task with knowledge context injected.

5. Agent runs self-validation:
   - Checks output against retrieved patterns
   - Confirms S3 presigned URL approach used
   - Confirms MIME validation included
   - Confirms component file structure follows convention

6. Automated gates run:
   - Lint: pass
   - Types: pass
   - Tests: pass
   - Pattern compliance: pass
   - Security scan: pass

7. Trust-based gating check:
   - Agent trust for "Code Generation" in this project: Level 3 (AUDITED)
   - Decision: Auto-merge, add to review sample queue

8. Session learning captured:
   - No corrections needed (reinforces existing patterns)
   - Session duration and token usage logged
   - Pattern usage tracked (all 5 retrieved patterns were relevant)
```

---

## Metrics and Success Criteria

### Primary Metrics

| Metric | Definition | Target (6 months) | Target (12 months) |
|--------|-----------|-------------------|---------------------|
| **Correction Rate** | % of agent sessions requiring human correction | < 15% | < 8% |
| **Knowledge Reuse Rate** | % of sessions that use at least one retrieved pattern | > 70% | > 90% |
| **Pattern Effectiveness** | % of retrieved patterns that result in compliant output | > 80% | > 90% |
| **Token Efficiency** | Average tokens per session compared to baseline | -20% | -35% |
| **Trust Level Distribution** | % of agent-domain pairs at Level 3+ | > 30% | > 60% |
| **Knowledge Freshness** | % of active patterns validated within threshold | > 85% | > 95% |
| **First-Pass Gate Success** | % of agent output passing all automated gates on first attempt | > 75% | > 88% |

### Secondary Metrics

| Metric | Definition | Target |
|--------|-----------|--------|
| Knowledge base growth rate | New validated patterns per week | 5-15 per active project |
| Pattern deprecation rate | Patterns deprecated per month | < 10% of active patterns |
| Cross-project promotion rate | Patterns promoted to global per month | 2-5 |
| Contradiction detection rate | Contradictions found and resolved per month | Trending downward |
| Human review time saved | Hours of review avoided via trust escalation | Measurable after Phase 3 |
| Regression incidents | Cases where knowledge base update caused quality decrease | < 1 per month |

### Health Indicators

The following indicators signal systemic problems requiring intervention:

- **Knowledge base bloat**: Active pattern count growing faster than 20% per month without corresponding quality improvement. Indicates insufficient pruning.
- **Retrieval irrelevance**: More than 30% of retrieved patterns are not referenced by agents. Indicates retrieval scoring needs tuning.
- **Trust stagnation**: No agent-domain pairs advancing in trust level over 30 days. Indicates gates may be too strict or agents are not improving.
- **Correction concentration**: More than 50% of corrections are in the same category. Indicates a knowledge base gap in that category.
- **Staleness accumulation**: More than 15% of patterns in AGING status. Indicates review process is not keeping up.

### Measurement Infrastructure

To track these metrics, the system requires:

1. **Event logging**: Every agent action, gate result, human correction, and knowledge retrieval is logged as a structured event.
2. **Attribution tagging**: Each agent output is tagged with the knowledge records that were injected into its context.
3. **Outcome tracking**: Deployed features are tracked for quality outcomes (error rates, revert rates, incident involvement).
4. **Dashboard**: Real-time visibility into all primary and secondary metrics, with alerting on health indicator violations.

---

## References

- [Addy Osmani - My LLM Coding Workflow Going into 2026](https://addyosmani.com/blog/ai-coding-workflow/)
- [Arize - Closing the Loop: Coding Agents, Telemetry, and the Path to Self-Improving Software](https://arize.com/blog/closing-the-loop-coding-agents-telemetry-and-the-path-to-self-improving-software/)
- [GrowthBook - Feedback Loops Are the Next Breakthrough in Agentic Coding](https://blog.growthbook.io/feedback-loops-are-the-next-breakthrough-in-agentic-coding/)
- [Knight Columbia - Levels of Autonomy for AI Agents](https://knightcolumbia.org/content/levels-of-autonomy-for-ai-agents-1)
- [Supervised Autonomy: The AI Framework for 2026](https://edge-case.medium.com/supervised-autonomy-the-ai-framework-everyone-will-be-talking-about-in-2026-fe6c1350ab76)
- [Google Cloud - Choose a Design Pattern for Your Agentic AI System](https://docs.google.com/architecture/choose-design-pattern-agentic-ai-system)
- [Microsoft Azure - AI Agent Orchestration Patterns](https://learn.microsoft.com/en-us/azure/architecture/ai-ml/guide/ai-agent-design-patterns)
- [AWS - Agentic AI Patterns and Workflows](https://docs.aws.amazon.com/prescriptive-guidance/latest/agentic-ai-patterns/introduction.html)
- [Alfredo Perez - Universal AI Knowledge Base](https://medium.com/ngconf/universal-knowledge-base-for-ai-2da5748f396c)
- [Claude Code Memory Documentation](https://code.claude.com/docs/en/memory)
- [RAG About It - The Knowledge Decay Problem](https://ragaboutit.com/the-knowledge-decay-problem-how-to-build-rag-systems-that-stay-fresh-at-scale/)
- [Qodo - Code Quality Metrics for Large Engineering Orgs 2026](https://www.qodo.ai/blog/code-quality-metrics-2026/)
- [AWS - The Agentic AI Security Scoping Matrix](https://aws.amazon.com/blogs/security/the-agentic-ai-security-scoping-matrix-a-framework-for-securing-autonomous-ai-systems/)
- [Faros AI - Best AI Coding Agents for 2026](https://www.faros.ai/blog/best-ai-coding-agents-2026)
- [Microsoft - Enhancing Code Quality at Scale with AI-Powered Code Reviews](https://devblogs.microsoft.com/engineering-at-microsoft/enhancing-code-quality-at-scale-with-ai-powered-code-reviews/)
- [Cobbai - Content Freshness: Best Practices for Automating Updates and Deletions](https://cobbai.com/blog/knowledge-freshness-automation)
- [Microsoft Azure - Architectural Approaches for AI and ML in Multitenant Solutions](https://learn.microsoft.com/en-us/azure/architecture/guide/multitenant/approaches/ai-machine-learning)
