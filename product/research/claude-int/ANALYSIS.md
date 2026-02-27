# Raw Message Interception: Strategic Analysis for Unimatrix

**Research Question:** Should Unimatrix gain access to raw Claude message interactions, and what benefits would this provide over the current model of behavioral driving through `.claude/` configuration?

**Date:** 2025-02-25

---

## 1. Current Model: Active Agent Participation

Unimatrix currently relies on agents **actively participating** in the knowledge lifecycle. The behavioral driving chain is:

1. **MCP server `instructions`** tell agents to "search before work, store after decisions"
2. **CLAUDE.md** reinforces with project-level rules
3. **Agent definition files** mandate orientation lookups and outcome reporting
4. **Tool responses** inject behavioral guidance as footer text

This is a **pull-based model** — agents must choose to call `context_search`, `context_store`, `context_briefing`. If an agent ignores the instructions (or the instructions get compacted out of context), knowledge is lost. The system only learns what agents explicitly tell it.

**What Unimatrix sees today:** Tool call arguments and nothing else. When an agent calls `context_store(topic: "auth", content: "Use JWT...")`, Unimatrix sees the store request. It never sees the 40-turn conversation that led to that decision, the alternative approaches considered and rejected, the errors encountered, or the implicit patterns the agent followed without storing them.

---

## 2. What "Raw Message Access" Means

Claude Code provides several mechanisms to observe the raw message stream:

| Mechanism | Access Level | What You See |
|-----------|-------------|--------------|
| **PostToolUse hooks** | Tool outputs after execution | Every tool result (file reads, grep results, test output) |
| **PreToolUse hooks** | Tool inputs before execution | Every tool call Claude is about to make |
| **SubagentStart/Stop hooks** | Agent lifecycle | Spawn prompts and final results |
| **Claude Agent SDK streaming** | Full message stream | Every user/assistant message, all tool_use/tool_result blocks |
| **CLI headless mode** | Structured JSON output | Complete conversation transcript |
| **Network proxy** | Raw HTTP | API requests/responses including system prompt |

The most actionable mechanisms for Unimatrix are **PostToolUse hooks** (immediate, no SDK dependency) and **Agent SDK streaming** (comprehensive, requires wrapping Claude Code invocations).

---

## 3. The Fundamental Shift: Active to Passive Knowledge Acquisition

The current model is **explicit, active, opt-in**:
```
Agent decides to store --> Unimatrix receives --> Knowledge captured
```

Raw message access enables **implicit, passive, comprehensive**:
```
Agent works normally --> Messages flow --> Unimatrix observes --> Knowledge extracted
```

This is the difference between a **questionnaire** (you only learn what people choose to tell you) and **observation** (you learn from behavior). Both have value. The question is whether the observational layer adds enough value to justify the complexity.

---

## 4. Six Potential Benefits

### 4.1 Implicit Knowledge Extraction

**The problem:** Agents make dozens of micro-decisions per feature — naming conventions, error handling patterns, API design choices — that never get stored because they aren't "big enough" to warrant an explicit `context_store` call. These micro-decisions ARE the project's accumulated wisdom.

**What message access enables:** A PostToolUse hook on `Write` and `Edit` tool calls would capture every code change. A background process could analyze diffs against the existing knowledge base, detecting new patterns that match or diverge from stored conventions. When an agent consistently uses `Result<T, AppError>` instead of `anyhow::Result`, that's a convention — even if nobody stored it.

**Value assessment:** HIGH. This addresses the single largest knowledge gap in the current system. The behavioral driving chain achieves ~90% compliance for explicit store/search calls, but captures maybe 10% of the implicit patterns embedded in actual code changes.

### 4.2 Outcome Inference for Process Intelligence

**The problem:** Milestone 5 (Collective phase) requires agents to explicitly store structured outcomes at each phase/gate for the retrospective pipeline. This adds cognitive overhead and depends on agent compliance. The `col-001` design assumes agents will call something like `context_store_outcome(phase: "gate-3a", result: "fail", reason: "interface errors")`.

**What message access enables:** By observing the validator agent's conversation, Unimatrix could INFER the outcome without explicit reporting. A gate-3a failure produces a specific pattern: the validator reads test results, finds failures, writes a report with "REWORKABLE FAIL" in it. Observing this pattern is more reliable than requiring the validator to separately call a store function.

**Value assessment:** HIGH for M5 specifically. Outcome inference from message observation would make the retrospective pipeline work even with imperfect agent compliance. The system could track: which files were touched, which tests failed, how many rework cycles occurred, what the final gate result was — all from observation.

### 4.3 Context Utilization Measurement

**The problem:** Unimatrix serves context via `context_briefing` and `context_search`, but has no visibility into whether that context was actually used. The confidence system tracks helpful/unhelpful votes (explicit signal), but doesn't know if a returned entry was ignored, partially used, or central to the agent's work.

**What message access enables:** By observing the assistant's responses after receiving Unimatrix context, the system could detect:
- **Direct reference:** Agent quotes or paraphrases a stored entry → strong positive signal
- **Behavioral alignment:** Agent's code follows a stored convention → moderate positive signal
- **Contradiction:** Agent does the opposite of stored guidance → negative signal, potential knowledge issue
- **Silence:** Agent never references the context → weak signal (might not have been relevant)

**Value assessment:** MEDIUM-HIGH. This would dramatically improve confidence scoring. The current Wilson score needs 5+ explicit votes to be useful. Implicit utilization signals accumulate much faster and don't require agent action. The 6-factor confidence composite could gain a 7th factor: "observed utilization rate."

### 4.4 Agent Behavior Profiling

**The problem:** Unimatrix tracks usage per agent via the AGENT_REGISTRY, but only sees which tools agents call. It doesn't understand HOW agents reason, what mistakes they make, or where they struggle.

**What message access enables:** Observing full conversations reveals:
- **Reasoning patterns:** Does the architect consider 3 alternatives or jump to the first solution?
- **Error recovery:** When a test fails, does the rust-dev fix the root cause or patch the symptom?
- **Context utilization efficiency:** Does the tester actually read the risk strategy before writing tests?
- **Rework indicators:** How many edit-test-fix cycles does implementation take?

**Value assessment:** MEDIUM. Interesting for long-term process intelligence, but the analysis is complex and the signal is noisy. Most useful as input to the M5 retrospective pipeline rather than a standalone capability.

### 4.5 Briefing Optimization

**The problem:** `context_briefing` compiles orientation context, but the token budget is fixed. Unimatrix doesn't know which parts of the briefing were valuable and which were wasted tokens.

**What message access enables:** By tracking which briefing content appears in subsequent agent reasoning, Unimatrix could learn that "the architect always uses ADR references but ignores convention entries" or "the rust-dev benefits from code patterns but not design rationale." Future briefings could be optimized per-role, per-phase, per-task.

**Value assessment:** MEDIUM. Token efficiency matters at scale (the founder's vision emphasizes reducing from 50-130K to 10-15K tokens per request), but the current briefing is already concise. This becomes more valuable in the multi-project future (M7) where token pressure increases.

### 4.6 Anti-Gaming Layer 3 Completion

**The problem:** The confidence system's Layer 3 anti-gaming defense is designed but unimplemented. It requires "implicit outcome correlation + agent diversity + anomaly detection" — all of which need observational data.

**What message access enables:** Implicit outcome correlation means: did the agent's code pass tests, did the gate succeed, did the feature ship? These outcomes can be inferred from message observation. Agent diversity means: is the same agent repeatedly voting for its own entries? Anomaly detection means: is a pattern of votes statistically unusual? All three signals come from observing behavior, not from explicit reporting.

**Value assessment:** MEDIUM. Anti-gaming matters for trust, but the current Layer 1+2 defenses are adequate for a single-user system. Layer 3 becomes important if Unimatrix ever serves multiple users or untrusted agents.

---

## 5. Three Implementation Approaches

### 5.1 PostToolUse Hook Pipeline (Minimal)

```
PostToolUse hook fires → Shell script writes JSON to a spool directory →
Background process reads spool → Extracts signals → Updates Unimatrix
```

**What you get:** Visibility into all tool inputs and outputs. Code changes (Write/Edit), test results (Bash), file reads (Read), search queries (Grep/Glob). No visibility into assistant reasoning.

**Complexity:** Low. Hook configuration in `.claude/settings.json`, a shell script, and a spool consumer. Could be a Rust binary that shares the redb database.

**Coupling:** Moderate. Tied to Claude Code hook format, but hooks are a stable public API.

### 5.2 Agent SDK Wrapper (Moderate)

```
Orchestrator uses Claude Agent SDK → Streams messages →
Tee to Unimatrix observer → Observer extracts signals → Updates Unimatrix
```

**What you get:** Full message stream including assistant reasoning. Complete visibility into the conversation flow.

**Complexity:** Moderate. Requires wrapping subagent invocations in SDK calls instead of (or alongside) the `Task` tool. The orchestrator becomes a custom application rather than a pure Claude Code session.

**Coupling:** High. Requires building outside Claude Code's native agent model. The orchestrator is no longer a Claude Code agent — it's a custom SDK application that happens to use Claude.

### 5.3 Hybrid: Hooks for Signals, SDK for Research (Recommended)

```
Production: PostToolUse hooks → spool → background consumer → Unimatrix
Research: SDK wrapper on select sessions → full transcript capture → offline analysis
```

**What you get:** Low-overhead continuous signal collection via hooks, plus deep analysis capability for specific sessions via SDK.

**Complexity:** Moderate overall, but each piece is simple.

**Coupling:** Hooks are Claude Code stable API. SDK usage is confined to research/analysis sessions, not the critical path.

---

## 6. Risks and Concerns

### 6.1 Complexity Budget

Unimatrix's strength is architectural simplicity: embedded Rust, single redb file, no cloud dependencies. A message interception pipeline adds a new subsystem (spool directory, background consumer, signal extraction logic). This must be weighed against the knowledge gains.

**Mitigation:** Start with PostToolUse hooks only. No SDK wrapper. No assistant reasoning capture. Just tool I/O signals flowing into a spool. This is a ~2-day feature, not a new subsystem.

### 6.2 Signal-to-Noise Ratio

Most tool calls are noise. An agent reads 50 files per session — only 2-3 of those reads produce actionable knowledge signals. The signal extraction logic must be selective.

**Mitigation:** Filter by tool type. `Write` and `Edit` calls contain code changes (high signal). `Bash` calls with test output contain outcome data (high signal). `Read` and `Grep` calls are mostly noise unless they indicate which knowledge domains the agent needed.

### 6.3 Platform Coupling

Hooks are Claude Code-specific. If Unimatrix needs to support other MCP clients, hook-based observation won't work.

**Mitigation:** The MCP protocol itself is the stable abstraction. Hook-based observation is an acceleration layer on top. The core Unimatrix system remains a standard MCP server. If a different client connects, Unimatrix degrades to the active participation model (which already works).

### 6.4 Storage Volume

Capturing tool I/O for every session produces significant data. A single feature session might generate 500+ tool calls.

**Mitigation:** Don't store raw messages. Extract signals and discard the source. "Agent wrote code matching convention X" is a 100-byte signal extracted from a 10KB code change. The observation pipeline is a funnel, not a warehouse.

---

## 7. Strategic Assessment

### Where This Fits in the Roadmap

Raw message access is most valuable as an **accelerator for M5 (Collective phase — Process Intelligence)**. The M5 features require:

- `col-001` (Outcome Tracking): Message observation provides implicit outcomes
- `col-002` (Retrospective Pipeline): Richer signal from observation vs explicit reporting
- `col-003` (Process Proposals): More evidence = better proposals
- `col-004` (Feature Lifecycle): Gate results inferable from validator conversations

Without message access, M5 depends entirely on agents explicitly storing outcomes. With message access, M5 can cross-reference explicit reports against observed behavior — making the retrospective pipeline more accurate and more robust to agent non-compliance.

### The Philosophical Question

The research question frames this as "raw message access vs. rules/guidelines in .claude directories." This is a false dichotomy. The two mechanisms serve different purposes:

| Mechanism | Purpose | Direction |
|-----------|---------|-----------|
| `.claude/` rules | **Prescriptive**: Tell agents what to DO | System → Agent |
| Message observation | **Descriptive**: Learn what agents DID | Agent → System |

Rules drive behavior. Observation measures behavior. A learning system needs both. You prescribe conventions, then observe whether they're followed, then refine the conventions based on evidence. This is the full improvement loop that M5 envisions — and message observation is the measurement mechanism that closes it.

### The Core Insight

The deepest benefit isn't any single capability from sections 4.1-4.6. It's the transition from **Unimatrix as a passive database that agents query** to **Unimatrix as an active observer that learns from the development process**. This is the difference between a library (you go to it) and a colleague (they watch you work and get smarter over time).

The product vision statement says Unimatrix delivers "the right context to the right agent at the right workflow moment." To do this optimally, Unimatrix needs to know what "right" means — and that can only come from observing what actually helps. Message access is the observational substrate that makes the self-learning promise real.

---

## 8. Recommendation

**Pursue message observation as a pre-M5 capability (new feature: `col-000` or `ass-010`).**

Phase 1 (minimal, ~2 days):
- PostToolUse hook on `Write`, `Edit`, `Bash` tool calls
- Shell script writes filtered events to a JSON-lines spool file
- Manual analysis of spool files to validate signal quality

Phase 2 (integrated, ~1 week):
- Background consumer reads spool, extracts signals
- Signals feed into existing confidence factors (new "observed utilization" factor)
- Outcome inference from test results and gate reports

Phase 3 (M5 integration):
- Observation signals feed directly into `col-001` outcome tracking
- Cross-referencing explicit reports vs observed behavior
- Retrospective pipeline uses both active and passive signals

This incremental approach validates the concept before committing to deep integration, keeps complexity bounded, and positions M5 for success.

---

## 9. Open Questions

1. **Hook performance at scale:** PostToolUse hooks add ~7-15ms per tool call. At 500+ calls per session, this is 3.5-7.5 seconds of overhead. Acceptable? Need to measure.

2. **Signal extraction fidelity:** Can we reliably detect "agent used this convention" from code diffs alone, or does this require assistant reasoning visibility (which hooks don't provide)?

3. **Storage isolation:** Should observation data live in the main redb database or a separate spool? Mixing operational and observational data could affect query performance.

4. **Subagent visibility:** PostToolUse hooks fire for subagents spawned via `Task` tool, but the current hook system may not distinguish which subagent generated which event. Need to verify.

5. **SDK vs hooks long-term:** If the Agent SDK matures and Unimatrix moves toward custom orchestration (beyond Claude Code's native `Task` tool), hooks become unnecessary. Should we plan for this transition or stay with hooks?
