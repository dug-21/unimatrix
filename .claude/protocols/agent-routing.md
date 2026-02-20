# Agent Routing and Swarm Composition

## Unimatrix Agent Preference

Always use Unimatrix-specific agents over generic ones:

| Instead of | Use | Why |
|------------|-----|-----|
| `coder` | `ndp-rust-dev` | Knows Rust patterns, project structure |
| `system-architect` | `ndp-architect` | Knows Domain Adapter pattern, ADRs |
| `tester` | `ndp-tester` | Knows test patterns, mocking approach |
| `planner` | `ndp-scrum-master` | Knows feature lifecycle, reads protocols |
| `reviewer` | `ndp-validator` | Runs /validate or /validate-plan, glass box reports, trust recording |

---

## Every Swarm Has These Two Agents

| Agent | Role | Spawned By |
|-------|------|------------|
| `ndp-scrum-master` | **Coordinator (queen)** — reads protocol, spawns workers, manages waves, updates GH Issues | Primary agent |
| `ndp-validator` | **Validation gate** — discovers completions from shared memory, runs tier checks, records trust | Scrum-master (end of each wave) |

These are non-negotiable. No swarm runs without a coordinator and no swarm completes without validation.

---

## Complete Agent Roster

### Coordination (2 agents — mandatory on every swarm)

| Agent | Type | What It Does |
|-------|------|-------------|
| `ndp-scrum-master` | coordinator | Reads protocol file, inits hive, spawns workers with Agent IDs, drift checks, GH Issue lifecycle |
| `ndp-validator` | gate | Memory-driven discovery of agent completions, 4-tier impl validation OR 5-check plan validation, trust recording |

### Planning (5 agents — planning swarms only, wave-ordered)

| Agent | Type | Wave | What It Produces |
|-------|------|------|-----------------|
| `ndp-architect` | specialist | 1 | ARCHITECTURE.md with ADRs + Integration Surface, stores ADRs via /save-pattern |
| `ndp-specification` | specialist | 1 | SPECIFICATION.md, TASK-DECOMPOSITION.md |
| `ndp-pseudocode` | specialist | 2 | pseudocode/OVERVIEW.md + per-component pseudocode files (reads Wave 1 output) |
| `ndp-tester` | specialist | 2 | test-plan/OVERVIEW.md + per-component test plan files (reads Wave 1 output) |
| `ndp-synthesizer` | synthesizer | 3 | IMPLEMENTATION-BRIEF.md, ACCEPTANCE-MAP.md, LAUNCH-PROMPT.md, GH Issue (fresh context) |

Wave 1 (spec + arch) → Wave 2 (pseudocode + test plan) → Wave 3 (vision guardian, synthesizer, validator — sequential).

### Core Implementation (2 agents — most implementation swarms)

| Agent | Type | When to Include |
|-------|------|----------------|
| `ndp-rust-dev` | general | Any Rust code changes — the default implementation agent |
| `ndp-tester` | specialized | When new tests are needed or test strategy changes |

### Data Layer (4 agents — include when touching their layer)

| Agent | Type | Layer | When to Include |
|-------|------|-------|----------------|
| `ndp-parquet-dev` | narrow | Bronze | WAL, Parquet files, snapshot logic, `core/src/bronze/` |
| `ndp-timescale-dev` | narrow | Silver | Hypertables, continuous aggregates, ETL, `apps/silver-etl/` |
| `ndp-analytics-engineer` | specialized | Gold | Materialized views, domain transforms, `tools/ndp-gold-ddl/` |
| `ndp-dq-engineer` | specialized | Cross-layer | Data quality rules, transparency tables, schema validation |

### Domain Scientists (2 agents — include when their domain is involved)

| Agent | Type | When to Include |
|-------|------|----------------|
| `ndp-meteorologist` | specialized | NWS data, forecast schemas, atmospheric science, weather stream config |
| `ndp-air-quality-specialist` | specialized | AQI calculations, EPA standards, sensor calibration, health thresholds |

### ML & Features (2 agents)

| Agent | Type | When to Include |
|-------|------|----------------|
| `ndp-feature-engineer` | narrow | Time-series features, windowing, aggregations, ML-ready data |
| `ndp-ml-engineer` | narrow | ruv-FANN models, training pipelines, inference integration |

### Visualization & Alerts (2 agents)

| Agent | Type | When to Include |
|-------|------|----------------|
| `ndp-grafana-dev` | narrow | Grafana dashboards, panels, data sources |
| `ndp-alert-engineer` | narrow | Rust-based triggers, thresholds, notifications |

### Alignment (1 agent — planning swarms only)

| Agent | Type | When to Include |
|-------|------|----------------|
| `ndp-vision-guardian` | specialist | After planning agents complete, before generating brief |

**Total: 17 agents** (2 coordination + 4 planning + 11 implementation/specialist)

---

## Swarm Composition Templates

Use these as starting points. Add/remove specialists based on the specific task.

### Planning Swarm

```
Coordinator:  ndp-scrum-master
Wave 1:       ndp-architect, ndp-specification              (parallel)
              ndp-architect stores ADRs via /save-pattern
Wave 2:       ndp-pseudocode, ndp-tester                (parallel, after Wave 1)
Wave 3:       ndp-vision-guardian (alignment)            (sequential)
              ndp-synthesizer (brief + maps + GH Issue)  (fresh context window)
              ndp-validator (5-check)
```

Produces: SPECIFICATION.md, TASK-DECOMPOSITION.md, ARCHITECTURE.md (with Integration Surface + ADRs stored in pattern store), pseudocode/OVERVIEW.md + per-component pseudocode, test-plan/OVERVIEW.md + per-component test plans, ALIGNMENT-REPORT.md, ACCEPTANCE-MAP.md, LAUNCH-PROMPT.md, IMPLEMENTATION-BRIEF.md (with Component Map), GH Issue.

### Feature Implementation (General Rust)

```
Coordinator:  ndp-scrum-master
Workers:      ndp-rust-dev, ndp-tester
Post-wave:    ndp-validator (4-tier)
```

The baseline. Most features start here.

### Data Pipeline (Bronze → Silver → Gold)

```
Coordinator:  ndp-scrum-master
Workers:      ndp-parquet-dev, ndp-timescale-dev, ndp-analytics-engineer, ndp-dq-engineer
Post-wave:    ndp-validator (4-tier)
```

Add domain scientist if pipeline involves domain-specific logic.

### Schema / ETL Change

```
Coordinator:  ndp-scrum-master
Workers:      ndp-architect, ndp-timescale-dev, ndp-dq-engineer
Post-wave:    ndp-validator (4-tier)
```

Always include `ndp-architect` for cross-cutting schema changes.

### New Data Source

```
Coordinator:  ndp-scrum-master
Workers:      ndp-architect, ndp-rust-dev, ndp-parquet-dev, {domain-scientist}
Post-wave:    ndp-validator (4-tier)
```

Domain scientist validates the data interpretation.

### ML / Predictions

```
Coordinator:  ndp-scrum-master
Workers:      ndp-feature-engineer, ndp-ml-engineer, ndp-rust-dev
Post-wave:    ndp-validator (4-tier)
```

### Dashboard / Visualization

```
Coordinator:  ndp-scrum-master
Workers:      ndp-grafana-dev, ndp-analytics-engineer
Post-wave:    ndp-validator (4-tier)
```

### Alerts / Triggers

```
Coordinator:  ndp-scrum-master
Workers:      ndp-alert-engineer, ndp-rust-dev, {domain-scientist}
Post-wave:    ndp-validator (4-tier)
```

### Bug Fix

```
Coordinator:  ndp-scrum-master
Workers:      ndp-rust-dev, ndp-tester
Post-wave:    ndp-validator (4-tier)
```

For single-file bugs, skip the swarm entirely — just fix and /validate.

---

## Composition Rules

1. **Every swarm**: ndp-scrum-master (coordinator) + ndp-validator (gate). No exceptions.
2. **Domain data work**: include the relevant domain scientist (meteorologist or air-quality-specialist).
3. **Schema/ETL changes**: include ndp-dq-engineer for data quality impact.
4. **Cross-cutting changes**: include ndp-architect for ADR decisions.
5. **Skip swarm for**: single-file edits, 1-2 line fixes, config changes, docs, exploration.
6. **Max wave size**: 5 workers. Split into waves if more agents needed.
