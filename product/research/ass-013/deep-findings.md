# ASS-013: Deep Findings (Pass 2+)

Continued analysis beyond initial-findings.md. Focus on signals with retrospective value.

## Finding: Edit Responses Are 44% of All Context Load

Edit tool responses echo the entire file back to the agent. 17 edits to `tools.rs` (91KB) and `server.rs` (68-71KB) generated **1,793 KB** of response data — 44% of all tool response data in the session.

| File | Edits >50KB | Avg Response |
|------|------------|-------------|
| `tools.rs` | 7 | ~91 KB each |
| `server.rs` | 9 | ~69 KB each |
| `SCOPE.md` | 1 | 52 KB |

**Total response data across all tools: 4.0 MB** (~1M tokens consumed by the agent's context).

This is a platform behavior, not something Unimatrix controls. But it's a **context pressure amplifier** — each edit to a large file burns ~70-90KB of context window. This explains why the agent re-reads files: edits push earlier content out.

**Detection**: Rule-based. Flag files where edit responses exceed a threshold (e.g., >50KB). These are "context-expensive files" where each edit costs disproportionately.

**Baseline metric**: Total edit response KB per feature cycle, ratio of edit responses to total response data.

## Finding: Permission Friction Is Measurable

10 of 18 `context_store` PreToolUse calls never received a PostToolUse response. The agent retried the same call until the user approved.

- "Adaptive Embedding Pipeline": 4 attempts → 1 success (attempt 4)
- "Session activity capture hooks": 6 attempts → 1 success (attempt 6)
- "Episodic Augmentation analysis": 3 attempts → 1 success (attempt 3)

**Detection**: `Pre count - Post count` per tool = friction score. Zero model needed.

**Baseline metric**: Friction events per tool per feature. Starting threshold: >2 retries for same tool = friction flag.

## Finding: Agents Use Bash for Search (Compliance Issue)

10+ Bash commands were search operations that should have used Grep/Glob:
```
find ... | xargs grep -l "crt-006"
find ... | xargs grep -l "adaptive"
grep -r "adaptive embedding" --include="*.md"
find ... -type d -name "crt-*"
find ... -name "*.rs" -path "*/test*"
find ... -name "*.rs" | xargs grep -l "#[test]"
```

System prompt explicitly says: "Use Grep instead of grep, Glob instead of find."

**Detection**: Regex match Bash commands against `^find |^grep |^rg |xargs grep|xargs rg`. Rule-based.

**Baseline metric**: Search-via-Bash count / total Bash count = compliance ratio. Starting threshold: >5% = flag.

## Finding: Cold Restart Cost Is Quantifiable

Post-gap recovery at 12:34 loaded 17 files (123 KB, ~31K tokens) plus 17 Bash commands (git diffs, cargo tests, branch checkout). Pure overhead — re-establishing context that existed before the timeout.

**Detection**: Gap > threshold in timestamp sequence, followed by reads to files already accessed earlier. Rule-based.

**Baseline metric**: Cold restart events per feature, total KB re-loaded per restart. Starting threshold: gap > 30 min = potential restart.

## Finding: Nested Subagent Types Are Invisible

26 of 31 SubagentStop records have empty `agent_type`. Only top-level spawns carry type labels. The actual worker agents (researcher, architect, spec-writer, tester, coder) spawned by the scrum-master are anonymous.

**Impact**: Cannot attribute context load, file access, or duration to specific agent roles. Hotspot detection works at session level but not agent-role level.

**Telemetry gap**: Hooks need to capture agent_type on SubagentStop, or better: the agent prompt/description field on both Start and Stop.

## Finding: Scrum-Master Respawn Pattern

The scrum-master was spawned 5 times during the feature cycle:

| Run | Duration | Trigger |
|-----|----------|---------|
| 1 | 5 min | Initial scope exploration |
| 2 | 5 min | Human feedback: redo research |
| 3 | 6 min | Human feedback: add integration tests |
| 4 | 25 min | Approved → Phase 1b through Session 1 end |
| 5 | 237 min | Session 2: entire delivery phase |

Runs 1-3 are human-feedback-driven restarts. Each one is a full cold start — re-read protocol, re-brief, rebuild context.

**Detection**: Count SubagentStart per agent_type per feature. Rule-based.

**Baseline metrics**: Respawn count per coordinator, average duration. Starting threshold: >3 respawns = human iteration was high.

## Finding: Post-Delivery Review Phase Exists (Undocumented)

After all tasks were completed (06:46), the session continued with 4+ subagent investigations:

| Time | Activity |
|------|----------|
| 12:34 | Cold restart — full code review |
| 12:44 | l2_normalize integration check |
| 13:02 | Co-access vs episodic overlap analysis |
| 13:06 | EntryRecord / Store API exploration |
| 13:15 | Created GH issue + stored pattern |
| 13:24 | Created 2nd GH issue (RNG seed) |
| 13:26 | Wrote memory file |

This is an organic review/retrospective phase not defined in the protocol. It produced 2 issues and 1 knowledge entry.

**Detection**: Tool calls after final task marked completed. Rule-based.

**Baseline metric**: Post-completion tool calls, issues created, knowledge stored.

## Finding: Build/Test Loop Pattern

During Stage 3b implementation (04:19-04:34), the agent ran 8 cargo check/test invocations in 15 minutes. Three `cargo test --workspace` runs within 2 minutes used different output filters:
```
cargo test --workspace 2>&1 | grep 'test result:'
cargo test --workspace 2>&1 | grep 'FAILED\|failures'
cargo test --workspace 2>&1 | grep 'test result:'
```

This indicates output parsing difficulty — the agent struggles to determine pass/fail status from cargo output.

**Detection**: Repeated `cargo test` with different pipe filters within N minutes. Rule-based.

**Baseline metric**: Compile cycles per implementation phase. Starting threshold: >6 cycles = iteration pressure.

## Finding: WebSearch Is Tightly Clustered

4 WebSearch calls within 13 seconds, all during scope research:
- ndarray vs nalgebra performance
- MicroLoRA rank selection scaling
- InfoNCE batch size scaling
- EWC continuous learning scaling

Research happens in bursts. Not an anti-pattern — a natural workflow signal.

**Baseline metric**: WebSearch count per phase. Useful for characterizing research intensity.

## Finding: `sleep` Workaround

At 13:15:37, an agent ran `sleep 10 && echo "ready"` — likely working around a race condition with Unimatrix server after a failed `context_store`.

**Detection**: Any `sleep` command in Bash. Rule-based. `sleep` in agent output almost always indicates a workaround.

**Baseline metric**: Sleep command count per feature. Starting threshold: any occurrence = flag.

## Finding: Thinking Time Reveals Where LLM Does Heavy Reasoning

**Total thinking time: 128 min. Total tool execution time: 4 min.** The LLM spends 97% of wall-clock time reasoning, not waiting for tools.

Thinking time distribution:
- <1s: 162 calls (batched/parallel calls, no deliberation)
- 1-5s: 226 calls (quick decisions)
- 5-30s: 136 calls (moderate reasoning)
- 30s-5min: 54 calls (heavy reasoning — document generation)
- \>5min: 7 calls (stalls — likely GH API latency or context compression)

**Long pauses (>60s) correlate with document generation.** Every pause >60s was followed by a Write — the LLM was composing SCOPE.md (194s), ARCHITECTURE.md (86s), SPECIFICATION.md (103s), RISK-TEST-STRATEGY.md (915s!), test plans (30-48s each), and source files (42-81s each).

The 915s (15 min) pause before writing RISK-TEST-STRATEGY.md is notable — this was the most complex document to compose, cross-referencing risks against architecture against test strategy.

**Detection**: Long-pause events (>60s) before Writes. Rule-based. Could indicate document complexity or context pressure.

**Baseline metric**: Avg thinking time per document type. Starting threshold: >5min for a single document = complexity flag.

## Finding: 32% Search Miss Rate

18 of 57 Grep/Glob searches returned empty results.

Notable misses:
- `product/features/crt-006/**` — feature directory didn't exist yet (searched before creation)
- `product/roadmap*`, `docs/*roadmap*` — agent looking for roadmap files that don't exist
- `crt-006|Adaptive Embedding` in PRODUCT-VISION.md — feature not yet in vision doc
- `registry.rs` function searches — agent exploring non-existent API surface

**Detection**: PostToolUse response_size < 100 bytes for Grep/Glob. Rule-based.

**Baseline metric**: Search miss rate per feature. Starting threshold: >30% = agent orientation is inefficient (searching for things that don't exist).

**Retrospective value**: High miss rates early in a phase suggest the agent doesn't know the codebase structure. Unimatrix could help here — if it stored "project directory structure" patterns, agents would search less blindly.

## Finding: 25% Parallel Tool Call Rate

149 of 600 PreToolUse calls occurred in parallel groups (same second). Parallel patterns:
- Glob+Glob+Glob (3-way file discovery)
- Read+Read (parallel file reads)
- Bash+Bash (parallel command execution)
- TaskCreate+TaskCreate (batch task creation)

75% of calls are sequential. This is a potential efficiency signal — more parallelization could reduce wall-clock time.

**Detection**: Count calls with identical timestamps. Rule-based.

**Baseline metric**: Parallelization rate per feature. Starting threshold: informational only — need multiple features to establish whether higher rates correlate with faster delivery.

## Finding: Activity Profile Shifts by Phase

| Phase | Explore | Produce | Execute | Orchestrate | Knowledge |
|-------|---------|---------|---------|-------------|-----------|
| Design | 46% | 8% | 18% | 15% | 10% |
| Delivery | 37% | 28% | 21% | 13% | 0% |
| Post-delivery | 41% | 1% | 49% | 4% | 4% |

- Design is exploration-heavy (46%) — agents reading, searching, understanding
- Delivery shifts to production (28% vs 8%) — agents writing code
- Post-delivery is execution-heavy (49%) — mostly git/gh/cargo commands
- **Knowledge interaction drops to 0% during delivery** — agents don't consult Unimatrix during implementation

**Detection**: Activity category ratios per phase. Rule-based.

**Baseline metric**: Explore/Produce/Execute ratios per phase. Deviation from established ratios = phase running differently than usual.

**Interesting signal**: Zero knowledge interaction during delivery means agents rely entirely on design docs and live code. If this is consistent across features, it suggests Unimatrix has no delivery-phase role currently.

## Finding: Agent Warmup Patterns Are Phase-Predictive

First tool call after each task starts:

| Phase Type | First Call | Pattern |
|------------|-----------|---------|
| Research (task 1, runs 1-2) | Glob/Read | Exploration warmup |
| Risk/Spec/Architecture (tasks 3-5) | Read/Write | Quick read then produce |
| Synthesizer/Delivery (tasks 6-8) | Write | Immediate production |
| Branch init (task 9) | Bash | Git operations |
| Pseudocode (task 10) | Write | Immediate production (from design context) |
| Gate 3a (task 11) | Bash | Verification commands |
| Implementation (task 12) | Read x5 | Heavy exploration warmup |
| Gate 3b (task 13) | Read x5 | Heavy exploration warmup |
| Testing (task 14) | Read x2 → Bash | Read then test |
| Gate 3c (task 15) | Bash x5 | Pure execution |
| PR delivery (task 16) | Bash x5 | Pure git/gh operations |

**Gate/validation phases always start with exploration or execution.** Implementation always starts with heavy reads. This is predictable and could be validated — if an implementation phase starts with 5 Writes instead of Reads, something is wrong (agent producing without understanding).

**Detection**: Classify first 3 tool calls per phase as explore/produce/execute. Rule-based.

**Baseline metric**: Warmup pattern per phase type. Deviation = agent skipped expected warmup.

## Finding: GH API Latency Dominates Delivery Phase

The thinking-time analysis reveals that the longest "pauses" aren't thinking at all — they're GH API operations:

| Pause | Before | Actual cause |
|-------|--------|-------------|
| 4,432s (74 min) | `gh pr create` | PR creation (API + LLM composing PR body) |
| 17,901s (5 hours) | `gh pr view` | Session gap / timeout (not actual GH latency) |
| 1,055s (18 min) | `gh issue comment` | API latency or rate limiting |
| 912s (15 min) | `gh issue comment` | API latency or rate limiting |
| 604s (10 min) | `git status` | Session resumption delay |

Phase 4 (PR delivery) took 117 min, but most of that is GH API latency, not agent work.

**Detection**: Bash commands containing `gh ` with execution time >60s. Rule-based.

**Baseline metric**: GH API command duration. Starting threshold: >5 min = flag as latency issue.

## Finding: Session Identity Is Clean, Agent Identity Is Not (Platform Constraint)

Every telemetry record carries a `session_id` UUID. Sessions are fully separable — even when overlapping temporally:

| Sessions | Overlap Window | Separable? |
|----------|---------------|------------|
| d1d1a6a7 (human) + dc1e33f6 (knowledge subagent) | 02:10–02:13 | Yes — distinct session_ids |
| 66c19301 (swarm) + 1c6597fe (post-delivery) | 13:20–13:27 | Yes — distinct session_ids |

**For multi-session concurrent work, session_id is sufficient.** Two humans running two features simultaneously would have two session_ids. Partitioning telemetry by session is clean.

### Agent-Level Attribution: The Gap

Within a session, **all subagent tool calls share the parent session_id**. There is no per-subagent identifier on PreToolUse/PostToolUse records. When the scrum-master spawns a researcher, architect, and tester — their tool calls all carry `66c19301`.

What's available:
- `SubagentStart` fires with `agent_type` (e.g., `Explore`, `uni-scrum-master`) — 7 records
- `SubagentStop` sometimes has `agent_type` — only 5 of 31 stops carry it (all `uni-scrum-master`)
- **26 of 31 SubagentStop records have empty `agent_type`** — these are the nested worker agents (researcher, architect, spec-writer, tester, coder)

### Bracketing by Timestamp: Partially Works

Tool calls can be attributed to agents by timestamp windows between SubagentStart and SubagentStop events. This works for sequential, non-overlapping agents. Example:

```
uni-scrum-master #4 (03:09 → 03:34): 53 tool calls attributable
uni-scrum-master #5 (03:38 → 07:35): 30+ tool calls in first window
```

But it breaks when:
- **Nested agents overlap** — scrum-master spawns child agents; both parent and children make tool calls in the same time window
- **Child stops are anonymous** — when 4 sequential children stop between 03:45 and 04:41, we can't tell which SubagentStop belongs to which agent role without inferring from tool call patterns
- **No start event for nested children** — only the top-level `SubagentStart` fires from the hook; children spawned by the scrum-master don't emit their own `SubagentStart`

### Impact on Retrospective Analysis

- **Session-level hotspots**: Fully supported. Can detect "this session had high context load."
- **Agent-role hotspots**: Not directly supported. Can infer from tool patterns (e.g., an agent that only writes pseudocode files is probably the pseudocode agent) but this is heuristic, not definitive.
- **Per-agent context load**: Cannot measure directly. The "Stage 3b implementation agent loaded 190KB" finding required manual timestamp bracketing, not automated detection.

### This Is a Platform Constraint

Claude Code hooks expose `session_id` on all events but do not expose a per-subagent identifier on tool calls. The SubagentStart/SubagentStop hooks are the only agent lifecycle signals, and nested agents don't consistently populate `agent_type`. This is not something Unimatrix controls — it's a constraint of the hook API.

**Workarounds available within Unimatrix**:
1. **Timestamp bracketing** — attribute tool calls to the most recent SubagentStart. Works for sequential agents, fails for parallel spawns.
2. **Tool pattern inference** — classify agents by what they do (Write to pseudocode/ = pseudocode agent, cargo test = tester). Fragile but useful for hotspot detection.
3. **Feature-cycle level metrics only** — don't attempt per-agent attribution; report hotspots at session/feature granularity. This is sufficient for most retrospective value.

## Phase Duration Baseline (First Data Point)

| Phase | Duration | Notes |
|-------|----------|-------|
| Phase 1: Scope | 34 min | Includes 2 human-triggered reworks |
| Phase 1b: Risk Assessment | 1 min | |
| Phase 2a: Arch + Spec | 5 min | |
| Phase 2a+: Risk Strategy | 15 min | |
| Phase 2b: Vision Alignment | <1 min | |
| Phase 2c: Synthesizer | 3 min | |
| Phase 2d: Artifact Delivery | <1 min | |
| Stage 3a: Pseudocode + Tests | 11 min | |
| Gate 3a | 1 min | |
| Stage 3b: Implementation | 40 min | Longest phase, monolithic agent |
| Gate 3b | 3 min | |
| Stage 3c: Testing | 9 min | |
| Gate 3c | 4 min | |
| Phase 4: PR/Delivery | 117 min | Includes timeout/gap |
| **Design total** | **~60 min** | Human-in-loop |
| **Delivery total** | **~70 min** | Excluding timeout |
