# FINDINGS: Graph & Execution Model (Track R1)

**Spike**: ass-063
**Date**: 2026-05-29
**Approach**: investigation
**Confidence**: directional

---

## Findings

### Q: Can the existing typed graph (categories, typed edges, traversal modes) represent workflow DAGs — steps, gates, agent assignments, input/output contracts? What new categories, edge types, and entry schemas are needed? How do workflow definitions compose (e.g., design workflow reuses scoping steps from research workflow)?

**Answer**: The existing graph can represent workflow DAGs with targeted extensions — no fundamental redesign required. The graph already supports directed typed edges, multi-hop traversal, cycle detection, and 16 relation types. Workflow representation requires three new entry categories, three new edge types, and a structured content schema convention for workflow entries.

**Evidence**:

The current data model (`schema.rs`) stores entries with: id, title, content (free-form string), topic, category (free-form string, 8 known categories), tags (string array), status (Active/Deprecated/Proposed/Quarantined), confidence, and feature_cycle. The `graph_edges` table stores directed typed edges with: source_id, target_id, relation_type (string), weight, metadata (nullable TEXT), and a UNIQUE constraint on (source_id, target_id, relation_type). The `RelationType` enum has 16 variants, stored as strings — extensible without schema migration.

Current categories (entry #241): decision, convention, pattern, outcome, lesson-learned, duties, procedure, reference. None represent workflow structure.

Current edge types relevant to workflow: `Prerequisite` (reserved for W3-1, no write path yet), `Advances` (source advances target goal), `Motivates` (source is rationale behind target). These are semantically adjacent but not workflow-specific.

**Proposed extensions**:

*Three new categories*:
- `workflow` — A workflow definition entry. Content is a structured JSON document describing the workflow's purpose, trigger conditions, and composition rules. One entry per workflow (e.g., "design-session", "delivery-session", "bugfix-session", "research-spike"). The entry's `tags` carry version info and session-type identifiers.
- `step` — A single step within a workflow. Content is a structured document containing: step name, agent type (e.g., "uni-researcher", "uni-architect"), instruction template, input contract (what artifacts/data the step requires), output contract (what the step must produce), and completion criteria. Each step is its own entry with `topic` set to the parent workflow name.
- `gate` — A validation checkpoint between workflow stages. Content describes: gate name, pass/fail criteria, max rework iterations, and what happens on failure (rework vs. abort vs. escalate). Gates are entries, not inline metadata, because they need independent status tracking and can be shared across workflows.

*Three new edge types* (added to `RelationType` enum):
- `HasStep` — workflow entry -> step entry. Directional. The edge `weight` field encodes execution order (1.0, 2.0, 3.0...) and `metadata` JSON stores `{"parallel_group": "2a"}` for steps that execute concurrently within the same phase.
- `GatedBy` — step entry -> gate entry. The step's output must pass this gate before the workflow advances. A step with no `GatedBy` edge auto-advances on completion.
- `Requires` — step entry -> step entry. Data dependency: this step requires the output of the target step before it can begin. Distinct from `HasStep` ordering — `Requires` expresses a data dependency that may cross workflow boundaries (enabling composition).

*Structured content schema for step entries* (convention, not enforced at storage layer):
```json
{
  "agent_type": "uni-architect",
  "instruction_template": "Read SCOPE.md at {scope_path}. Produce ARCHITECTURE.md...",
  "inputs": [
    {"name": "scope_path", "type": "file_path", "required": true},
    {"name": "risk_assessment_path", "type": "file_path", "required": false}
  ],
  "outputs": [
    {"name": "architecture_path", "type": "file_path"},
    {"name": "adr_paths", "type": "file_path_list"}
  ],
  "completion_criteria": "ARCHITECTURE.md exists and is non-empty"
}
```

This schema lives in the `content` field as a JSON string. The storage layer does not need to understand it — the workflow execution layer (MCP tools) parses it at runtime. This is the same pattern used by existing structured entries (e.g., outcome entries with structured tags).

*Gate content schema*:
```json
{
  "criteria": "cargo build --workspace succeeds with zero errors",
  "max_rework": 2,
  "on_fail": "rework",
  "on_exhaust": "escalate_to_human"
}
```

**Composition model**: Workflow composition operates through `Requires` edges that cross workflow boundaries. Example: the design workflow's "scope validation" step is a standalone `step` entry. The research workflow defines its own steps but creates a `Requires` edge from its "investigation" step to the shared "scope validation" step. When the workflow engine encounters a `Requires` edge pointing to a step in a different workflow, it checks whether that step's output artifacts already exist (from a prior workflow run). If they do, the dependency is satisfied without re-execution. If not, the engine can either fail with a clear message or trigger the dependency workflow.

This is strictly more flexible than the current protocol-file approach, where composition is achieved by copy-pasting step descriptions between protocol markdown files (observable in the design and research protocols, which both contain scope validation logic).

The existing graph traversal modes support all needed queries:
- `neighbors` (outgoing, edge_type=HasStep) — get all steps for a workflow
- `neighbors` (outgoing, edge_type=GatedBy) — find the gate for a step
- `neighbors` (outgoing, edge_type=Requires) — find data dependencies
- `path` — find the execution path between two steps
- `subgraph` — visualize an entire workflow's structure
- `filter` (category=step, edge_types=GatedBy, max_edge_count=0) — find ungated steps
- `inverse` (category=step, missing_edge_types=Requires) — find independent steps (parallelizable)

**Recommendation**: Extend the `RelationType` enum with `HasStep`, `GatedBy`, and `Requires`. Add three categories (`workflow`, `step`, `gate`) to the category convention. Define content schemas as JSON conventions documented in a Unimatrix `convention` entry — not enforced at the storage layer. This approach requires zero schema migrations (categories and edge types are free-form strings), approximately 20 lines of code change to `RelationType` (3 new match arms in `as_str` and `from_str`), and no changes to the graph_edges table or any existing traversal logic.

---

### Q: What does "Unimatrix controls the workflow" mean concretely? Options range from passive (LLM queries for next step) to active (Unimatrix dispatches instructions). What MCP tools are needed (e.g., `workflow_next`, `workflow_complete_step`, `workflow_status`)? How are step completion, gate pass/fail, rework loops, and workflow abort represented and enforced?

**Answer**: The guided model is the correct choice — Unimatrix returns structured step instructions with input/output contracts when queried, but does not initiate LLM sessions. The LLM remains the caller; Unimatrix is the authority on what happens next. This requires five new MCP tools and a new set of tables for execution state.

**Evidence**:

Three execution models evaluated:

**Model A — Passive**: The LLM calls `workflow_next(workflow_id)` and receives a text hint like "do the architecture step next." The LLM interprets this hint and self-navigates, much as it does today with protocol files. This model provides minimal benefit over the status quo — it reduces token cost (the LLM loads one step's instructions instead of the full protocol) but does not solve compliance drift. The LLM can still skip steps, ignore gates, or invent shortcuts. Gate enforcement is advisory.

**Model B — Guided (recommended)**: The LLM calls `workflow_start` to begin a workflow run. Unimatrix creates a run record and returns the first step's full instruction payload (agent type, instruction template with variables resolved, input artifacts, output contract, completion criteria). The LLM executes the step, then calls `workflow_complete_step` with the step's outputs. Unimatrix validates the outputs against the step's output contract, records completion, checks for a gate (via `GatedBy` edge), and either advances to the next step or returns gate instructions. The LLM cannot skip ahead — `workflow_next` only returns the next eligible step based on completion state and dependency resolution.

Key property: the LLM calls Unimatrix to advance. Unimatrix never calls the LLM. The MCP protocol (as implemented by rmcp) is request-response where the LLM is always the initiator. Active dispatch (Model C) would require Unimatrix to maintain outbound connections to LLM APIs, which is architecturally alien to the current server design — `unimatrix-server` is a pure MCP server that responds to tool calls.

**Model C — Active**: Unimatrix initiates LLM sessions, dispatching step instructions to specific providers. This requires an outbound HTTP client, provider API keys, session lifecycle management, and retry logic — turning Unimatrix from a knowledge server into an orchestration engine. The complexity is at least 3x the guided model. The SCOPE.md constraint ("honest about build cost") and the product vision question ("is this a different product?") both counsel against this for a minimum viable version. Model C can be built atop Model B later — the guided model's workflow state tracking is a prerequisite for active dispatch regardless.

**Proposed MCP tool surface** (5 tools):

**1. `workflow_start`** — Begin a workflow run.
```
Parameters:
  workflow_id: u64        // Entry ID of the workflow definition (category=workflow)
  context: {              // Runtime context — variable bindings for instruction templates
    feature_id: string,
    scope_path: string,
    issue_number: string,
    ...                   // Workflow-specific key-value pairs
  }
  agent_id: string
  session_id: string

Returns:
  run_id: u64             // Unique run identifier
  first_step: {           // The first eligible step's full instruction payload
    step_id: u64,
    step_name: string,
    agent_type: string,
    instructions: string, // Template with context variables resolved
    inputs: [{name, type, value}],   // Resolved input artifacts
    outputs: [{name, type}],         // Expected output contract
    completion_criteria: string
  }
```
Internally: creates a row in `workflow_runs` table, resolves step ordering from `HasStep` edges (sorted by weight), resolves `Requires` edges for the first step, and returns the first step whose dependencies are all satisfied.

**2. `workflow_complete_step`** — Report step completion and advance.
```
Parameters:
  run_id: u64
  step_id: u64
  outputs: {              // Actual outputs produced by the step
    architecture_path: "/path/to/ARCHITECTURE.md",
    adr_paths: ["/path/to/ADR-001.md"]
  }
  agent_id: string

Returns one of three outcomes:

  // Outcome A: Next step available
  next_step: { ... }      // Same shape as first_step above

  // Outcome B: Gate check required
  gate: {
    gate_id: u64,
    gate_name: string,
    criteria: string,
    instructions: string  // What to check and how
  }

  // Outcome C: Workflow complete
  complete: true
  summary: { steps_completed: N, duration_secs: N }
```
Internally: records step completion in `workflow_runs`, stores output artifacts as metadata. Checks for `GatedBy` edge on the completed step. If a gate exists, returns gate instructions instead of the next step. If no gate, resolves the next eligible step via topological sort of `HasStep` ordering with `Requires` dependency satisfaction.

**3. `workflow_gate_result`** — Report gate pass/fail.
```
Parameters:
  run_id: u64
  gate_id: u64
  result: "pass" | "fail"
  detail: string          // Evidence for pass/fail
  agent_id: string

Returns:
  // On pass: same as workflow_complete_step Outcome A or C

  // On fail with rework remaining:
  rework: {
    step_id: u64,         // Step to rework
    attempt: 2,           // Current attempt number
    max_attempts: 2,      // From gate.max_rework
    feedback: string      // The detail from the fail, forwarded as rework context
  }

  // On fail with rework exhausted:
  escalation: {
    reason: "rework_exhausted",
    gate_name: string,
    attempts: N,
    action: "escalate_to_human"  // From gate.on_exhaust
  }
```
Internally: if result is "fail" and rework count < max_rework, increments the rework counter for this gate and returns the gated step for re-execution with the failure detail as additional context. If rework is exhausted, returns the escalation action from the gate definition. On "pass", advances to the next step.

**4. `workflow_status`** — Query current workflow run state.
```
Parameters:
  run_id: u64
  agent_id: string

Returns:
  workflow_name: string
  current_step: { step_id, step_name, status: "pending"|"active"|"complete"|"failed" }
  completed_steps: [{ step_id, step_name, completed_at, outputs }]
  pending_gates: [{ gate_id, gate_name, attempts, max_attempts }]
  context: { ... }        // The runtime context from workflow_start
```
This tool is read-only. It allows the LLM (or a different LLM in a multi-provider scenario) to understand the current state of a workflow run without having been present for earlier steps. Critical for multi-LLM routing (RQ-5): a new LLM session can call `workflow_status` to orient itself before executing its assigned step.

**5. `workflow_abort`** — Terminate a workflow run.
```
Parameters:
  run_id: u64
  reason: string
  agent_id: string

Returns:
  aborted: true
  steps_completed: N
  steps_remaining: N
```
Records the abort with reason. The run remains queryable via `workflow_status` with a terminal state. Aborted runs do not auto-resume.

**Execution state storage**:

A new set of tables is needed. The existing `entries` table is not suitable for mutable execution state — entries are append-only knowledge with supersession chains, not stateful run trackers:

```sql
CREATE TABLE workflow_runs (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  workflow_id     INTEGER NOT NULL,   -- FK to entries.id (category=workflow)
  status          TEXT NOT NULL,       -- 'active', 'complete', 'aborted'
  context         TEXT NOT NULL,       -- JSON runtime context
  created_at      INTEGER NOT NULL,
  updated_at      INTEGER NOT NULL,
  created_by      TEXT NOT NULL        -- agent_id that started the run
);

CREATE TABLE workflow_step_runs (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id          INTEGER NOT NULL,    -- FK to workflow_runs.id
  step_id         INTEGER NOT NULL,    -- FK to entries.id (category=step)
  status          TEXT NOT NULL,       -- 'pending', 'active', 'complete', 'failed', 'skipped'
  attempt         INTEGER NOT NULL DEFAULT 1,
  outputs         TEXT,                -- JSON of produced artifacts
  started_at      INTEGER,
  completed_at    INTEGER
);

CREATE TABLE workflow_gate_runs (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id          INTEGER NOT NULL,    -- FK to workflow_runs.id
  gate_id         INTEGER NOT NULL,    -- FK to entries.id (category=gate)
  step_id         INTEGER NOT NULL,    -- The step this gate guards
  result          TEXT,                -- 'pass', 'fail', NULL (pending)
  attempt         INTEGER NOT NULL DEFAULT 1,
  detail          TEXT,
  evaluated_at    INTEGER
);
```

This is mutable state — distinct from the immutable knowledge graph. The separation is deliberate: workflow definitions (what the workflow is) live in the knowledge graph as entries and edges. Workflow runs (what is happening right now) live in these tables. This mirrors the distinction in CI/CD systems between pipeline definitions and pipeline executions.

**Enforcement semantics**: The guided model enforces workflow compliance through state gating, not advisory text:

1. **Step sequencing**: `workflow_complete_step` only returns the next step whose `Requires` dependencies are all satisfied. The LLM cannot request an arbitrary step — it receives only what it is allowed to do next.

2. **Gate enforcement**: When a step has a `GatedBy` edge, `workflow_complete_step` returns gate instructions instead of the next step. The LLM must call `workflow_gate_result` to proceed. There is no API to skip a gate.

3. **Rework loops**: Gate failure increments the attempt counter. The step is re-presented with the failure detail as additional context. The LLM gets a fresh attempt with the original step instructions plus gate feedback.

4. **Workflow abort**: Only `workflow_abort` terminates a run. If the LLM session dies (context window exhaustion, crash), the run remains in `active` status. A new LLM session can call `workflow_status` to find the active run and resume from the current step.

5. **Output contract validation**: `workflow_complete_step` can validate that the declared outputs match the step's output contract schema (e.g., required fields present, file paths non-empty). Lightweight structural check — content quality validation is the gate's job.

**Capability mapping**: All five workflow tools require the existing `Read` capability (for workflow_status) or `Write` capability (for state-mutating tools). No new capability variant needed. The existing `TrustLevel` hierarchy applies — `Restricted` agents can query workflow_status but cannot start or advance workflows without `Write`.

**Recommendation**: Implement the guided model (Model B) with five MCP tools: `workflow_start`, `workflow_complete_step`, `workflow_gate_result`, `workflow_status`, `workflow_abort`. Add three tables (`workflow_runs`, `workflow_step_runs`, `workflow_gate_runs`) for mutable execution state, separate from the knowledge graph. Start with a single workflow (design session) as the proof case. Gate enforcement is the critical differentiator from the status quo — without it, the system is just a more expensive way to store protocol files.

---

## Unanswered Questions

None. Both assigned questions (RQ-1 and RQ-2) are fully addressed.

---

## Out-of-Scope Discoveries

1. **Workflow versioning via supersession chains**: Workflow, step, and gate entries are regular knowledge entries with supersession support. When a protocol evolves, the workflow entry is corrected via `context_correct`, creating a supersession chain. Active runs continue on their original version; new runs pick up the latest active version. Natural consequence of the data model — should be validated during design.

2. **Session state convergence**: The existing `SessionState` (`session.rs`) already tracks `current_phase`, `rework_events`, and `current_goal` per session. The proposed `workflow_runs` tables overlap with this in-memory state. During design, the relationship must be resolved: either `SessionState` becomes a read-through cache of `workflow_runs`, or `SessionState` is deprecated in favor of persistent workflow state. The latter is likely correct — persistent state survives session crashes.

3. **Category allowlist implications**: Entry #3775 references a `CategoryAllowlist` (crt-031) that may enforce which categories are valid. Adding `workflow`, `step`, and `gate` categories may require allowlist configuration changes.

4. **Template variable resolution security**: Instruction templates contain `{variable}` placeholders resolved from the `context` parameter. If the context contains user-supplied values, template injection is possible. The design phase should specify a safe interpolation mechanism.

5. **Parallel step execution model**: The current MCP protocol is strictly request-response. If two steps are in the same parallel group, the LLM must spawn subagents or execute them serially. The workflow engine can declare parallelism (via `parallel_group` in HasStep metadata) but cannot enforce it — the LLM decides how to execute parallel-eligible steps. Acceptable for the guided model; worth noting if the active model (Model C) is pursued later.

---

## Recommendations Summary

- **RQ-1**: Extend the graph with 3 new categories (workflow, step, gate) and 3 new edge types (HasStep, GatedBy, Requires). Zero schema migration required — categories and edge types are free-form strings. Workflow composition works through cross-workflow `Requires` edges. Approximately 20 lines of Rust change to `RelationType` enum.
- **RQ-2**: Implement the guided execution model — LLM calls Unimatrix to advance, Unimatrix returns structured per-step instructions and enforces sequencing/gates. Five new MCP tools (workflow_start, workflow_complete_step, workflow_gate_result, workflow_status, workflow_abort) plus three new SQL tables for mutable run state. Gate enforcement is the critical differentiator; without it, the system adds complexity without solving compliance drift.
