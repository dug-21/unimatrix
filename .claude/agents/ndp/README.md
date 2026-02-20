# Unimatrix Agent Team

Project-specific agents for the Unimatrix. These agents know the project's patterns, conventions, and architecture.

**Creating a new agent?** See [AGENT-CREATION-GUIDE.md](./AGENT-CREATION-GUIDE.md) for requirements and best practices.

## When to Use Unimatrix Agents

**ALWAYS use Unimatrix agents instead of generic agents for this project.** See `.claude/protocols/agent-routing.md` for swarm composition templates and full routing tables.

## Agent Roster

### Mandatory (every swarm)
| Agent | Role | What It Does |
|-------|------|-------------|
| `ndp-scrum-master` | Coordinator (queen) | Reads protocol, spawns workers with Agent IDs, manages waves, updates GH Issues |
| `ndp-validator` | Validation gate | Discovers completions from shared memory, runs validation tiers, records trust entries |

No swarm runs without these two.

### Planning (planning swarms only)
| Agent | Scope | When to Use |
|-------|-------|-------------|
| `ndp-pseudocode` | Specialized | Per-component pseudocode with Rust/SQL/scripting knowledge, integration surface awareness |
| `ndp-vision-guardian` | Broad | After planning agents complete, checks SPARC artifacts against product vision |

### Core Team
| Agent | Scope | When to Use |
|-------|-------|-------------|
| `ndp-architect` | Broad | Architecture decisions, ADRs, cross-cutting concerns |
| `ndp-rust-dev` | General | Any Rust development following Unimatrix patterns |
| `ndp-tester` | Specialized | Testing strategy, integration tests, coverage |

### Domain Scientists
| Agent | Scope | When to Use |
|-------|-------|-------------|
| `ndp-meteorologist` | Specialized | NWS data interpretation, forecast evaluation, weather domain logic |
| `ndp-air-quality-specialist` | Specialized | AQI calculations, sensor calibration, EPA standards, health thresholds |

### Data Engineering
| Agent | Scope | When to Use |
|-------|-------|-------------|
| `ndp-parquet-dev` | Narrow | Bronze layer, Parquet operations, WAL, storage |
| `ndp-timescale-dev` | Narrow | Silver layer, TimescaleDB, SQL queries |
| `ndp-dq-engineer` | Specialized | Layered DQ strategy, transparency tables, quality monitoring |
| `ndp-analytics-engineer` | Specialized | Silver→Gold transforms, domain logic in SQL, analytics views |

### ML & Predictions
| Agent | Scope | When to Use |
|-------|-------|-------------|
| `ndp-feature-engineer` | Narrow | Time-series features, aggregations, windowing |
| `ndp-ml-engineer` | Narrow | ruv-FANN models, training pipelines, inference |

### Visualization & Alerts
| Agent | Scope | When to Use |
|-------|-------|-------------|
| `ndp-grafana-dev` | Narrow | Grafana dashboards, panels, visualizations |
| `ndp-alert-engineer` | Narrow | Rust-based triggers, thresholds, notifications |

## Agent Behavior

All Unimatrix agents follow this protocol:

### Before Implementation

1. Use `get-pattern` skill to find relevant patterns for your domain
2. Read referenced architecture and procedure documents
3. Understand existing conventions before writing code

### During Implementation

1. Apply design principles from your agent definition
2. Follow established conventions (naming, error handling, etc.)
3. Track what you learn - gaps in patterns, new approaches that work

### After Implementation

1. Use `reflexion` skill to record whether patterns helped (REQUIRED)
2. Use `save-pattern` skill if you discovered a reusable approach
3. Ensure tests follow project patterns

### Git Operations (REQUIRED)

ALL git operations MUST use `ndp-github-workflow` skill:
- Branch naming: `feature/{phase}-{NNN}`
- Commits: `{type}({scope}): {description}`
- PRs: Use project template

## Key Project Patterns

Agents should know these patterns exist (use get-pattern for details):

- `architecture:domain-adapter-pattern` - Hexagonal architecture
- `architecture:channel-ownership-adr-001` - mpsc channel ownership
- `architecture:source-factory-pattern-adr-002` - Dynamic source creation
- `architecture:response-parser-trait` - HTTP parsing pattern
- `data-flow:ingestion-pipeline` - Source → Channel → Storage
- `deployment:docker-minimal-changes` - Extend without restructuring
- `conventions:naming` - Stream/field naming rules

## Spawning Unimatrix Agents

Agents are spawned by `ndp-scrum-master` (the coordinator) with an Agent ID that activates their `## Swarm Coordination` block. See `.claude/protocols/agent-routing.md` for composition templates.

```
# The scrum-master spawns workers with Agent IDs — agents self-coordinate via shared memory
Task(subagent_type="ndp-timescale-dev", prompt="Your agent ID: dp-004-agent-1-silver\n...")
Task(subagent_type="ndp-parquet-dev", prompt="Your agent ID: dp-004-agent-2-bronze\n...")
```

## Related Skills

| Skill | Purpose | When |
|-------|---------|------|
| `ndp-github-workflow` | Branch, commit, PR conventions | ALL git operations (REQUIRED) |
| `get-pattern` | Retrieve project patterns | BEFORE implementation (REQUIRED) |
| `reflexion` | Record pattern feedback | AFTER implementation (REQUIRED) |
| `save-pattern` | Store new patterns | AFTER discoveries |
| `learner` | Auto-discover patterns from history | User-invoked after feature completion |
| `pattern-manage` | Pattern lifecycle (delete, deprecate, update, stats) | Cleanup, auditing, deduplication |

## Directory

```
.claude/agents/ndp/
├── README.md                       # This file
├── AGENT-CREATION-GUIDE.md         # How to create new Unimatrix agents
├── ndp-scrum-master.md             # Feature lifecycle coordination
├── ndp-architect.md                # Architecture decisions
├── ndp-rust-dev.md                 # General Rust development
├── ndp-tester.md                   # Testing specialist
├── ndp-meteorologist.md            # Weather domain scientist
├── ndp-air-quality-specialist.md   # Air quality domain scientist
├── ndp-parquet-dev.md              # Bronze/Parquet layer
├── ndp-timescale-dev.md            # Silver/TimescaleDB layer
├── ndp-dq-engineer.md              # Data quality engineering
├── ndp-analytics-engineer.md       # Analytics transformations
├── ndp-feature-engineer.md         # Feature engineering
├── ndp-ml-engineer.md              # ML/ruv-FANN
├── ndp-grafana-dev.md              # Grafana dashboards
├── ndp-alert-engineer.md           # Alerts/triggers
├── ndp-pseudocode.md              # Per-component pseudocode (planning phase)
├── ndp-vision-guardian.md          # Vision alignment reviewer
└── ndp-validator.md               # Validation gate (planning + implementation)

.claude/skills/
├── ndp-github-workflow/      # Git conventions (branches, commits, PRs)
├── get-pattern/              # Pattern retrieval (BEFORE work)
├── reflexion/                # Pattern feedback (AFTER work)
├── save-pattern/             # Pattern storage (new discoveries)
├── learner/                  # Auto-discovery (user-invoked)
├── validate/                 # 4-tier validation (used by ndp-validator for impl)
├── validate-plan/            # 5-check validation (used by ndp-validator for planning)
├── trust-dashboard/          # Bayesian trust scores (reads ndp-validator entries)
├── shadow-judge/             # Human judgment calibration
├── align/                    # Vision alignment check (planning sessions)
└── pattern-manage/           # Pattern lifecycle management (GH Issue #42)
```
