# ASS-015: Signal Taxonomy for Passive Knowledge Acquisition

## Problem Statement

Unimatrix currently acquires knowledge through explicit `context_store` calls: an agent decides something is worth storing, spends tokens composing an entry, and calls the MCP tool. ASS-013 demonstrated that agents consistently under-store workflow mechanics, build patterns, and structural conventions -- knowledge they generate as a side effect of working but never explicitly record.

The question: can Unimatrix observe agent behavior signals and extract knowledge entries automatically, without consuming agent context window?

This document provides a complete taxonomy of observable signals, their quality characteristics, the knowledge extractable from each, and a framework for separating signal from noise.

---

## Part 1: Signal Classification

### 1.1 Search Signals

Signals emitted when agents query the knowledge base or codebase.

| Signal ID | Signal | Source | Raw Data |
|-----------|--------|--------|----------|
| S-SRC-01 | Search query text | `context_search` PreToolUse input | Natural language query string |
| S-SRC-02 | Search result set | `context_search` PostToolUse response | Entry IDs, similarity scores, ranking order |
| S-SRC-03 | Search result selection | `context_get` following `context_search` | Which entry ID was fetched after search |
| S-SRC-04 | Search result ignored | Absence of `context_get` after search | Entry IDs returned but never fetched |
| S-SRC-05 | Search miss (zero results) | `context_search` PostToolUse with empty results | Query that found nothing |
| S-SRC-06 | Search reformulation | Sequential `context_search` calls with similar but modified queries | Query pair (original, reformulated) |
| S-SRC-07 | Lookup filter patterns | `context_lookup` PreToolUse input | Topic, category, tags used as exact filters |
| S-SRC-08 | Briefing parameters | `context_briefing` PreToolUse input | Role + task combination |
| S-SRC-09 | Codebase search queries | Grep/Glob PreToolUse input | Pattern, path, file type filters |
| S-SRC-10 | Codebase search misses | Grep/Glob PostToolUse with zero matches | Patterns that returned nothing |
| S-SRC-11 | Search-via-Bash | Bash PreToolUse containing grep/find/rg | Misrouted search commands |

### 1.2 Access Signals

Signals from how agents consume knowledge entries and files.

| Signal ID | Signal | Source | Raw Data |
|-----------|--------|--------|----------|
| S-ACC-01 | Entry access | `context_get` PreToolUse | Entry ID |
| S-ACC-02 | Entry re-access | Multiple `context_get` for same entry ID within a session | Entry ID, access count, timestamps |
| S-ACC-03 | Entry co-access | Two entries fetched within N-minute window | Entry ID pair, time gap |
| S-ACC-04 | File access | Read PreToolUse | File path |
| S-ACC-05 | File co-access | Two files read within N-minute window | File path pair, time gap |
| S-ACC-06 | File re-read | Same file read multiple times | File path, read count, timestamps |
| S-ACC-07 | Read-before-edit | Read followed by Edit/Write to same file | File path, time between read and edit |
| S-ACC-08 | Cross-crate read | Read of file in crate A followed by edit of file in crate B | Source path, target path, time gap |
| S-ACC-09 | Comprehension read (no mutation follows) | Read with no subsequent Edit/Write to that file | File path |
| S-ACC-10 | Entry helpful vote | `helpful` parameter on context tools | Entry ID, boolean |
| S-ACC-11 | Entry access after briefing | `context_get` following `context_briefing` | Entry ID, briefing role+task |

### 1.3 Behavioral Signals

Signals from agent tool call sequences and patterns.

| Signal ID | Signal | Source | Raw Data |
|-----------|--------|--------|----------|
| S-BEH-01 | Tool call sequence | Ordered PreToolUse events within a session | Tool names in order |
| S-BEH-02 | Tool call frequency | Count of each tool type per phase | Tool -> count map |
| S-BEH-03 | Parallel tool calls | PreToolUse events with identical timestamps | Tool names, count |
| S-BEH-04 | Reformulation/retry | Same tool called multiple times with modified input | Tool, input diff, attempt count |
| S-BEH-05 | Permission friction | PreToolUse count exceeds PostToolUse count for a tool | Tool, excess count |
| S-BEH-06 | Compile cycle | Sequence of cargo check/test invocations | Command, success/fail, count |
| S-BEH-07 | Output parsing struggle | Same cargo command with different pipe filters | Commands, filter variations, time window |
| S-BEH-08 | Sleep workaround | Any `sleep` command in Bash | Duration, context |
| S-BEH-09 | Edit cycle (write-fail-rewrite) | Write followed by cargo check fail followed by Edit | File, error type, iteration count |
| S-BEH-10 | Warmup pattern | First 3 tool calls after SubagentStart | Tool sequence, agent type |
| S-BEH-11 | Exploration burst | Cluster of Read/Grep/Glob within short window | Tool count, time window |
| S-BEH-12 | Production burst | Cluster of Write/Edit within short window | Tool count, time window, files touched |
| S-BEH-13 | WebSearch clustering | Multiple WebSearch calls within seconds | Query list, time window |
| S-BEH-14 | Context loading surge | Large volume of Read responses in short window | Total KB, time window, file count |

### 1.4 Content Signals

Signals derived from what agents write, not just that they write.

| Signal ID | Signal | Source | Raw Data |
|-----------|--------|--------|----------|
| S-CON-01 | File creation path | Write PreToolUse input file_path | Full path of new file |
| S-CON-02 | Directory structure | Aggregated Write paths per feature | Directory tree created |
| S-CON-03 | File naming pattern | Regex match on Write paths | File name template (e.g., `ADR-NNN-kebab.md`) |
| S-CON-04 | Commit message | Bash command containing `git commit -m` | Message text |
| S-CON-05 | PR description | Bash command containing `gh pr create --body` | PR body text |
| S-CON-06 | Branch name | Bash command containing `git checkout -b` | Branch name string |
| S-CON-07 | Cargo.toml dependencies | Write/Edit to Cargo.toml files | Dependency changes |
| S-CON-08 | Module declarations | Write/Edit to lib.rs files | Module structure |
| S-CON-09 | Test structure | Write paths under `tests/` or `mod tests` in source | Test organization pattern |
| S-CON-10 | GH issue creation | Bash command containing `gh issue create` | Issue title, labels, body |
| S-CON-11 | Knowledge entry content | `context_store` PostToolUse response | Stored entry content |
| S-CON-12 | Edit target patterns | Edit PreToolUse old_string/new_string | What gets changed and how |

### 1.5 Outcome Signals

Signals from results of agent actions.

| Signal ID | Signal | Source | Raw Data |
|-----------|--------|--------|----------|
| S-OUT-01 | Compilation success/failure | `cargo check`/`cargo build` PostToolUse | Exit code, error output |
| S-OUT-02 | Test pass/fail | `cargo test` PostToolUse | Exit code, test counts |
| S-OUT-03 | Gate pass/fail | TaskUpdate with gate result status | Gate ID, result, reason |
| S-OUT-04 | Feature completion | Final TaskUpdate with status "completed" | Feature ID, timestamp |
| S-OUT-05 | Post-completion work | Tool calls after S-OUT-04 | Tool count, duration, activities |
| S-OUT-06 | Follow-up issues created | `gh issue create` after S-OUT-04 | Issue content |
| S-OUT-07 | PR merge status | `gh pr merge` or `gh pr view` status | PR number, merge state |
| S-OUT-08 | Rework event | TaskUpdate status from completed to in_progress | Task ID, rework trigger |

### 1.6 Meta Signals

Contextual information that enriches other signals.

| Signal ID | Signal | Source | Raw Data |
|-----------|--------|--------|----------|
| S-MET-01 | Agent role | SubagentStart `agent_type` field | Role name (e.g., "uni-scrum-master") |
| S-MET-02 | Session identity | `session_id` on all records | UUID |
| S-MET-03 | Feature cycle | File paths containing feature ID, task subjects | Feature ID (e.g., "crt-006") |
| S-MET-04 | Phase context | TaskCreate/TaskUpdate subject prefix | Phase name (e.g., "3a", "3b") |
| S-MET-05 | Task decomposition | TaskCreate events | Task count, subjects, dependencies |
| S-MET-06 | Coordinator identity | SubagentStart for coordinator types | Coordinator type, spawn count |
| S-MET-07 | Session duration | First/last record timestamps | Duration in seconds |
| S-MET-08 | Feature type | Feature ID prefix (ass/nxs/col/vnc/alc/crt) | Phase classification |

### 1.7 Hook Signals

Signals from Claude Code hook infrastructure itself.

| Signal ID | Signal | Source | Raw Data |
|-----------|--------|--------|----------|
| S-HOK-01 | PreToolUse/PostToolUse pair completeness | Matched Pre/Post events | Tool, match status |
| S-HOK-02 | SubagentStart/SubagentStop pair | Matched start/stop events | Agent type, duration |
| S-HOK-03 | Hook latency | Time between Pre and Post for same tool call | Duration in ms |
| S-HOK-04 | Missing agent type | SubagentStop with empty agent_type | Count of anonymous stops |
| S-HOK-05 | Session file completeness | Session file exists and is parseable | Parse success/failure |
| S-HOK-06 | Timestamp gaps | Gaps > threshold between consecutive records | Gap duration, surrounding context |

---

## Part 2: Signal Quality Assessment

### Quality Dimensions

Each signal is rated on four dimensions, each on a 1-5 scale:

- **Signal-to-noise ratio (S/N)**: How much useful knowledge can be extracted per unit of signal? 5 = nearly every instance is meaningful, 1 = mostly noise.
- **Latency tolerance**: Can it wait for batch processing, or must it be real-time? 5 = batch is fine (hours/days), 1 = must be real-time (sub-second).
- **Volume**: Expected throughput per feature cycle. 5 = very low (< 10 per feature), 1 = very high (> 1000 per feature).
- **Reliability**: How consistently does this signal indicate knowledge? 5 = deterministic, 1 = highly context-dependent.

### Search Signals

| Signal ID | S/N | Latency | Volume | Reliability | Notes |
|-----------|-----|---------|--------|-------------|-------|
| S-SRC-01 | 3 | 5 | 3 | 3 | Queries reveal intent but are noisy. Many exploratory. |
| S-SRC-02 | 2 | 5 | 3 | 4 | Result sets are deterministic but selection is what matters. |
| S-SRC-03 | 4 | 5 | 4 | 4 | Selection = validated relevance. Strong signal. |
| S-SRC-04 | 3 | 5 | 3 | 2 | Ignoring could mean irrelevant OR agent context was full. Ambiguous. |
| S-SRC-05 | 5 | 5 | 4 | 5 | Zero results = definite gap. Highest confidence search signal. |
| S-SRC-06 | 4 | 5 | 4 | 4 | Reformulation = query didn't work. Good gap/quality signal. |
| S-SRC-07 | 3 | 5 | 5 | 3 | Filter patterns show categorization mental models. Low volume. |
| S-SRC-08 | 4 | 5 | 5 | 4 | Role+task pairs reveal workflow entry points. Very low volume. |
| S-SRC-09 | 2 | 5 | 2 | 2 | Codebase searches are highly contextual. Mostly feature-specific. |
| S-SRC-10 | 3 | 5 | 3 | 3 | Codebase search miss could be structural ignorance or premature search. |
| S-SRC-11 | 5 | 5 | 4 | 5 | Compliance signal. Always indicates tool misuse. |

### Access Signals

| Signal ID | S/N | Latency | Volume | Reliability | Notes |
|-----------|-----|---------|--------|-------------|-------|
| S-ACC-01 | 3 | 4 | 3 | 4 | Every access is a data point, but individual accesses aren't knowledge. |
| S-ACC-02 | 4 | 5 | 4 | 4 | Re-access = content is important enough to refetch. Strong. |
| S-ACC-03 | 4 | 5 | 3 | 4 | Already used by co-access boosting (crt-004). Proven signal. |
| S-ACC-04 | 2 | 5 | 1 | 3 | Very high volume, low individual signal. |
| S-ACC-05 | 3 | 5 | 2 | 3 | File co-access patterns are structural. Need cross-feature validation. |
| S-ACC-06 | 4 | 5 | 3 | 4 | Re-reads indicate files that resist single-pass comprehension. |
| S-ACC-07 | 3 | 5 | 2 | 4 | Platform-required pattern (must read before edit). Filter needed. |
| S-ACC-08 | 4 | 5 | 4 | 4 | Cross-crate dependencies are stable. High knowledge value. |
| S-ACC-09 | 3 | 5 | 2 | 3 | Distinguishing comprehension from platform-mandated reads requires heuristic. |
| S-ACC-10 | 5 | 3 | 5 | 5 | Explicit human/agent judgment. Already integrated into confidence. |
| S-ACC-11 | 4 | 5 | 5 | 4 | Post-briefing access = briefing was useful for that entry. |

### Behavioral Signals

| Signal ID | S/N | Latency | Volume | Reliability | Notes |
|-----------|-----|---------|--------|-------------|-------|
| S-BEH-01 | 2 | 5 | 1 | 2 | Raw sequences are too feature-specific. Need cross-feature comparison. |
| S-BEH-02 | 3 | 5 | 3 | 4 | Aggregate frequencies are stable by phase type. Good baseline material. |
| S-BEH-03 | 2 | 5 | 2 | 3 | Parallelization rates correlate with efficiency but don't encode knowledge. |
| S-BEH-04 | 4 | 5 | 4 | 4 | Retries indicate something is wrong. Clear signal. |
| S-BEH-05 | 5 | 5 | 4 | 5 | Permission friction is deterministic. Already tracked. |
| S-BEH-06 | 3 | 5 | 3 | 3 | Compile cycles vary by feature complexity. Cross-feature comparison needed. |
| S-BEH-07 | 4 | 5 | 5 | 4 | Output parsing struggle = platform interaction problem. |
| S-BEH-08 | 5 | 5 | 5 | 5 | Sleep = workaround. Always a signal. |
| S-BEH-09 | 3 | 5 | 3 | 3 | Write-fail-rewrite could be normal iteration or API misunderstanding. |
| S-BEH-10 | 3 | 5 | 4 | 3 | Warmup patterns are phase-predictive (ASS-013) but need calibration. |
| S-BEH-11 | 2 | 5 | 2 | 3 | Exploration bursts are natural, not anomalous. |
| S-BEH-12 | 2 | 5 | 2 | 3 | Production bursts are natural, not anomalous. |
| S-BEH-13 | 3 | 5 | 5 | 3 | Research intensity marker, not knowledge source. |
| S-BEH-14 | 3 | 5 | 3 | 3 | Context surges may indicate scope issue or normal design-phase behavior. |

### Content Signals

| Signal ID | S/N | Latency | Volume | Reliability | Notes |
|-----------|-----|---------|--------|-------------|-------|
| S-CON-01 | 4 | 5 | 3 | 5 | File paths are structural truth. No interpretation needed. |
| S-CON-02 | 5 | 5 | 4 | 5 | Directory trees are conventions. Cross-feature comparison yields reliable patterns. |
| S-CON-03 | 5 | 5 | 4 | 5 | Naming patterns are verifiable against existing files. |
| S-CON-04 | 3 | 5 | 4 | 3 | Commit messages encode intent, but quality varies. |
| S-CON-05 | 3 | 5 | 5 | 3 | PR descriptions are feature-specific, not reusable. |
| S-CON-06 | 4 | 5 | 5 | 5 | Branch names follow conventions. Low volume, high reliability. |
| S-CON-07 | 3 | 5 | 4 | 4 | Dependency patterns across features reveal project norms. |
| S-CON-08 | 3 | 5 | 4 | 4 | Module structure patterns are structural. |
| S-CON-09 | 5 | 5 | 4 | 5 | Test structure is verifiable. "Inline mod tests, never separate files." |
| S-CON-10 | 4 | 5 | 5 | 4 | Issues reveal knowledge gaps or tech debt. |
| S-CON-11 | 5 | 3 | 5 | 5 | Already stored. Source of truth. |
| S-CON-12 | 2 | 5 | 1 | 2 | Edit content is feature-specific. Very high volume, low generalizability. |

### Outcome Signals

| Signal ID | S/N | Latency | Volume | Reliability | Notes |
|-----------|-----|---------|--------|-------------|-------|
| S-OUT-01 | 3 | 5 | 3 | 4 | Compilation results are deterministic but transient. |
| S-OUT-02 | 4 | 5 | 3 | 4 | Test pass/fail is strong when correlated with specific changes. |
| S-OUT-03 | 5 | 5 | 5 | 5 | Gate results are designed for knowledge extraction. |
| S-OUT-04 | 5 | 5 | 5 | 5 | Feature completion is a lifecycle boundary. |
| S-OUT-05 | 4 | 5 | 4 | 4 | Post-completion work reveals scope miscalculation. |
| S-OUT-06 | 5 | 5 | 5 | 4 | Follow-up issues = concrete knowledge gaps. |
| S-OUT-07 | 3 | 5 | 5 | 5 | PR status is binary. Clean signal, low information density. |
| S-OUT-08 | 5 | 5 | 5 | 5 | Rework events always indicate something went wrong. |

### Meta Signals

| Signal ID | S/N | Latency | Volume | Reliability | Notes |
|-----------|-----|---------|--------|-------------|-------|
| S-MET-01 | 4 | 5 | 5 | 3 | Agent role is only available on top-level spawns (platform constraint). |
| S-MET-02 | 5 | 5 | 5 | 5 | Session identity is clean and reliable. |
| S-MET-03 | 4 | 5 | 5 | 4 | Feature attribution from file paths is proven (ASS-013 data pipeline). |
| S-MET-04 | 4 | 5 | 4 | 4 | Phase context from task subjects is stable. |
| S-MET-05 | 3 | 5 | 4 | 4 | Task decomposition reveals workflow complexity. |
| S-MET-06 | 4 | 5 | 5 | 3 | Coordinator identity has the nested-agent-type-blank problem. |
| S-MET-07 | 3 | 5 | 5 | 5 | Duration is a simple, reliable metric. |
| S-MET-08 | 4 | 5 | 5 | 5 | Feature type classification is deterministic from ID prefix. |

---

## Part 3: Knowledge Extraction Patterns

Each pattern describes: (1) the input signals, (2) the extraction logic, (3) the knowledge produced, (4) the confidence level, and (5) the cross-feature validation requirement.

### Pattern KE-01: Knowledge Gap Detection

**Input signals**: S-SRC-05 (search miss), S-SRC-06 (search reformulation), S-SRC-10 (codebase search miss)

**Extraction logic**: Collect queries that returned zero results. Cluster by semantic similarity (embed the queries, group by cosine distance < 0.3). Each cluster represents a topic agents searched for but couldn't find.

**Knowledge produced**: Category `gap`, with the clustered query terms as content. Example: "Agents searched for 'server integration procedure' across 3 features but found no knowledge entry."

**Confidence**: Medium-high. Zero results is unambiguous. The interpretation of what should exist is the uncertain part.

**Cross-feature validation**: 2+ features with similar zero-result queries before proposing as a gap. Single-feature misses go to the gap-candidate queue.

**Noise risk**: Low. Premature searches (before a file exists) inflate counts, but the cluster deduplication across features filters one-offs.

### Pattern KE-02: Structural Convention Discovery

**Input signals**: S-CON-01 (file creation paths), S-CON-02 (directory structure), S-CON-03 (file naming), S-CON-06 (branch names), S-CON-09 (test structure)

**Extraction logic**: After each feature cycle, extract the set of created paths. Match against known templates (regex). When a new template appears consistently across 3+ features, propose it as a convention.

Template matching examples:
- `product/features/{phase}-{NNN}/SCOPE.md` -- feature scope document
- `crates/unimatrix-{name}/src/lib.rs` -- crate entry point
- `ADR-NNN-kebab-case.md` -- architecture decision record
- `feature/{phase}-{NNN}` -- branch naming

**Knowledge produced**: Category `convention`, with the template and supporting evidence (which features, how many instances).

**Confidence**: High. Structural patterns are verifiable against the filesystem. If 5 features all created `product/features/{id}/SCOPE.md`, that is a convention.

**Cross-feature validation**: Minimum 3 features with identical template before auto-proposing. First observation goes to observed-patterns buffer.

**Noise risk**: Very low. Structural patterns don't have false positives -- a directory either exists in that shape or it doesn't.

### Pattern KE-03: Procedural Knowledge from Tool Sequences

**Input signals**: S-BEH-01 (tool call sequence), S-CON-01 (file creation paths), S-ACC-07 (read-before-edit pairs)

**Extraction logic**: Extract ordered sequences of file mutations (Write/Edit) per phase. Normalize to file roles (e.g., `Cargo.toml` -> "dependency declaration", `server.rs` -> "server struct"). Align sequences across features using edit distance. Sequences with alignment score > 0.8 across 3+ features are candidate procedures.

Example: The "server integration procedure" from ASS-013:
1. Edit `Cargo.toml` (add dependency)
2. Edit `server.rs` (add import, struct field, constructor)
3. Edit `tools.rs` (add tool handler logic)
4. Edit `main.rs` (wire service)
5. Edit `shutdown.rs` (add lifecycle)

**Knowledge produced**: Category `procedure`, with step sequence and the file-role normalization.

**Confidence**: Medium. The FILE ORDER is durable; the EDIT CONTENT varies per feature. The procedure captures "which files in what order," not "what to write."

**Cross-feature validation**: Minimum 3 features with the same file-role sequence. Single-feature observations stay in the observed-procedures buffer. Error-recovery sequences (write-fail-edit loops) are excluded.

**Noise risk**: Medium. The main risk is extracting a procedure from a feature where the agent did something unusual. The 3-feature threshold and error-recovery exclusion mitigate this.

### Pattern KE-04: File Dependency Graph

**Input signals**: S-ACC-05 (file co-access), S-ACC-06 (file re-read), S-ACC-07 (read-before-edit), S-ACC-08 (cross-crate read)

**Extraction logic**: Build a weighted graph where nodes are files and edges represent co-access frequency. Weight edges by: (1) temporal proximity (reads within 5 minutes), (2) directionality (A read before B edited), (3) cross-feature consistency. Prune edges that appear in fewer than 2 features. Apply community detection to find stable file clusters.

Example clusters from ASS-013:
- `server.rs <-> shutdown.rs <-> tools.rs` (always co-read, 100% correlation)
- `normalize.rs -> server edits` (cross-crate dependency, 55-85% correlation)

**Knowledge produced**: Category `pattern`, capturing file clusters and their directionality. "To modify the server integration surface, agents always co-read server.rs, shutdown.rs, and tools.rs."

**Confidence**: Medium-high for clusters that appear across 3+ features. The structural dependency is real; the question is whether future features will follow the same pattern.

**Cross-feature validation**: Edges must appear in 2+ features. Clusters must be stable across 3+ features.

**Noise risk**: Low for the file-cluster level, medium for specific API references (those are feature-dependent).

### Pattern KE-05: Lesson Extraction from Gate Failures

**Input signals**: S-OUT-03 (gate pass/fail), S-OUT-08 (rework event), S-OUT-05 (post-completion work), S-OUT-06 (follow-up issues)

**Extraction logic**: When a gate fails, capture: (1) the gate ID, (2) the failure reason (from TaskUpdate input), (3) what changed between failure and success (file diffs in the window), (4) the rework duration. When the same gate fails for similar reasons across 2+ features, extract a lesson.

**Knowledge produced**: Category `lesson-learned`. Example: "Gate 3b code review consistently flags missing error handling in tool handler functions. 3/5 features required rework for this."

**Confidence**: High for the fact of failure, medium for the causal attribution. The "why" requires interpreting diffs.

**Cross-feature validation**: Same gate, similar failure reason, 2+ features.

**Noise risk**: Low for failure detection (binary), medium for lesson interpretation (requires understanding what the diff means).

### Pattern KE-06: Agent Workflow Phase Profile

**Input signals**: S-BEH-02 (tool call frequency), S-BEH-10 (warmup pattern), S-MET-04 (phase context), S-MET-07 (session duration)

**Extraction logic**: For each workflow phase type (design, implementation, testing, delivery), compute the average tool distribution profile across features. Normalize to percentages. Store as a "phase profile" that characterizes expected behavior.

Example phase profile from ASS-013:
```
design:    explore=46%, produce=8%, execute=18%, orchestrate=15%, knowledge=10%
delivery:  explore=37%, produce=28%, execute=21%, orchestrate=13%, knowledge=0%
```

**Knowledge produced**: Category `pattern`. Phase profiles serve as baselines for anomaly detection.

**Confidence**: Medium. Profiles converge with more data. First 3 features are noisy baselines; 10+ features produce stable profiles.

**Cross-feature validation**: Profiles are inherently cross-feature (computed as averages). Single-feature profiles are informational only.

**Noise risk**: Medium. Feature type matters -- research spikes have different profiles than full implementations. Must normalize by S-MET-08 (feature type).

### Pattern KE-07: Implicit Convention from Consistency

**Input signals**: S-CON-07 (Cargo.toml dependencies), S-CON-08 (module declarations), S-CON-04 (commit messages)

**Extraction logic**: For each structural element, check if the same pattern holds across all features. If yes, it's an implicit convention that agents follow but never documented.

Examples:
- Workspace inheritance: `edition.workspace = true, rust-version.workspace = true` in every crate Cargo.toml
- Inline tests: `mod tests {}` in every source file, zero `tests/` directories
- Commit format: `type(scope): desc (#N)` in every commit message

**Knowledge produced**: Category `convention`. "All crates use workspace = true for edition, rust-version, and license. Root Cargo.toml is never edited during crate creation."

**Confidence**: High when 100% consistent across features. Drops sharply with any deviation.

**Cross-feature validation**: Must hold across ALL observed features (100% consistency). Any exception converts the convention to a "common practice" with lower confidence.

**Noise risk**: Very low. Either every feature does it or not.

### Pattern KE-08: Search-to-Action Correlation

**Input signals**: S-SRC-01 (search query), S-SRC-03 (result selection), S-ACC-01 (entry access), S-BEH-12 (production burst)

**Extraction logic**: Track the chain: search query -> entry accessed -> subsequent file mutations. This reveals how knowledge entries influence agent behavior. Entries that consistently precede specific types of work get tagged with their "application context."

**Knowledge produced**: Enrichment on existing entries. "Entry #181 (adaptive embedding pattern) was accessed before implementation work in 5/5 sessions. Application context: pre-implementation reference."

**Confidence**: Medium. Correlation is not causation. The entry might have been accessed and ignored.

**Cross-feature validation**: Same entry -> same type of work across 3+ features.

**Noise risk**: Medium. The causal link between reading an entry and writing code is assumed, not proven.

### Pattern KE-09: Friction-Induced Lesson

**Input signals**: S-BEH-05 (permission friction), S-BEH-08 (sleep workaround), S-BEH-07 (output parsing struggle), S-BEH-04 (reformulation/retry)

**Extraction logic**: When friction events recur across features (same tool, same pattern), extract a lesson about the interaction surface. The friction itself is the knowledge.

**Knowledge produced**: Category `lesson-learned`. "context_store calls frequently trigger permission friction (10 retries across crt-006). Agents should batch stores or handle denial gracefully."

**Confidence**: High for the friction observation. Medium for the recommended action.

**Cross-feature validation**: Same friction pattern in 2+ features.

**Noise risk**: Low. Friction events are deterministic -- PreToolUse minus PostToolUse is arithmetic.

### Pattern KE-10: Emergent Topic Clusters

**Input signals**: S-SRC-01 (search queries), S-ACC-03 (entry co-access), S-CON-11 (stored entry content)

**Extraction logic**: Embed all search queries from a feature cycle. Cluster them. Compare clusters to existing Unimatrix topics. Clusters that don't map to existing topics represent emergent knowledge areas.

**Knowledge produced**: Category `pattern`. "Agents in cortical-phase features consistently search for concepts around 'confidence scoring,' 'Wilson score,' and 'threshold convergence' -- these form a natural topic cluster not currently labeled."

**Confidence**: Low-medium. Cluster quality depends on query volume and diversity.

**Cross-feature validation**: Same cluster appears in 3+ features.

**Noise risk**: Medium-high. Small query volumes produce unreliable clusters.

---

## Part 4: The "GREAT" Bar

What separates passive knowledge acquisition that actively improves the system from a noise generator that degrades it?

### 4.1 Precision: False Positive Rate

**Target**: < 10% false positive rate for auto-proposed entries.

A false positive is an auto-extracted entry that, when reviewed by a human, is judged as not-reusable-knowledge (too specific, incorrect, or obvious).

**How to achieve it**:
- Cross-feature validation as a hard gate. No knowledge promoted from a single feature observation, ever.
- Structural signals (KE-02, KE-07) have near-zero false positive rates because they're filesystem-verifiable.
- Procedural signals (KE-03) and behavioral signals (KE-06) have higher false positive rates and need stricter thresholds (3+ features minimum).
- Content interpretation signals (KE-05, KE-08) are the highest risk and should start as "proposed" status, not "active."

**Measurement**: Track human review outcomes on auto-proposed entries. Compute precision = accepted / (accepted + rejected). If precision drops below 90%, raise the cross-feature threshold.

### 4.2 Coverage: Capture Rate

**Target**: Capture > 60% of knowledge that agents would have stored if asked explicitly.

ASS-013 found that agents stored 8 entries explicitly but generated material for 6+ more entries they didn't store. The gap was entirely workflow mechanics and build patterns -- exactly what structural and procedural extraction targets.

**How to measure**: Periodically audit feature cycles. Have a human identify "knowledge that should have been stored" and check whether passive acquisition captured it.

**Coverage by extraction pattern**:

| Pattern | Coverage Target | Rationale |
|---------|----------------|-----------|
| KE-02 (structural conventions) | 90%+ | Filesystem is complete truth |
| KE-03 (procedures) | 50-70% | Some procedures are too feature-specific |
| KE-04 (file dependencies) | 70-80% | Stable clusters are discoverable |
| KE-01 (knowledge gaps) | 40-60% | Only captures gaps agents searched for |
| KE-05 (gate failure lessons) | 60-80% | Failures are recorded, interpretation varies |
| KE-06 (phase profiles) | 80%+ | Profiles are aggregate, not individual |
| KE-07 (implicit conventions) | 90%+ | 100%-consistent patterns are trivially detectable |
| KE-08 (search-to-action) | 30-50% | Correlation signal is noisy |

### 4.3 Freshness: Incorporation Latency

**Target**: New knowledge available to agents within 1 feature cycle of first observation.

Passive acquisition operates on a batch cadence aligned with retrospectives:

```
Feature N completes
  -> Retrospective runs (batch analysis)
  -> Signals added to observation buffer
Feature N+1 completes
  -> Retrospective runs
  -> Cross-feature validation against buffer
  -> Entries meeting threshold promoted to "proposed"
Feature N+2 completes
  -> Retrospective runs
  -> "proposed" entries with 3-feature support promoted to "active"
```

Minimum latency: 2 feature cycles for structural conventions (high confidence, lower threshold), 3 cycles for procedural knowledge.

Maximum acceptable latency: 5 feature cycles. If knowledge takes 5+ features to detect, the cross-feature threshold is too high or the signal is too weak.

**Real-time exceptions**: Some signals have immediate value without cross-feature validation:
- Knowledge gaps (S-SRC-05): Can surface "frequently searched, never found" in the current session's retrospective.
- Gate failures (S-OUT-03): Can record the lesson immediately as "proposed" status.

### 4.4 Relevance: Utility to Future Agents

**Target**: > 70% of auto-extracted entries receive at least one access within 10 feature cycles of creation.

An entry that exists but is never accessed is storage waste. Auto-extracted entries must be things future agents actually need.

**How to measure**: Track `access_count` on auto-extracted entries. Compare to agent-authored entries. If auto-extracted entries have significantly lower access rates, the extraction is producing noise.

**Relevance boosters**:
- Structural conventions (KE-02) are accessed during every feature setup phase.
- Procedures (KE-03) are accessed during implementation phases.
- File dependency graphs (KE-04) are accessed during code comprehension.
- Knowledge gaps (KE-01) may never be "accessed" (they're signals for what to create, not reference material).

**Relevance red flags**:
- Auto-extracted entries with 0 access after 5 features -> candidate for deprecation.
- Auto-extracted entries that contradict agent-authored entries -> candidate for review.

### 4.5 Trust: Confidence Parity

**Target**: Auto-extracted entries achieve the same confidence scores as agent-authored entries within 10 feature cycles.

Agents and humans must trust auto-extracted knowledge as much as explicitly stored knowledge. This requires:

**Initial confidence bootstrapping**:
- Auto-extracted entries start at confidence 0.4 (below the 0.5 default for agent-authored entries).
- The "source" field is set to "observation" (distinguishing from "agent" or "human").
- The lower initial confidence means auto-extracted entries rank lower in search results until validated by access and helpfulness votes.

**Confidence convergence**:
- If an auto-extracted entry is accessed and voted helpful, its confidence rises through the normal confidence system (Wilson score, co-access boosting, decay).
- If it's never accessed, confidence decays toward 0.
- If it's accessed and voted unhelpful, it drops faster due to the lower starting point.

**Trust markers**:
- Category `convention` auto-entries with 100% cross-feature consistency: start at 0.55 (nearly matching agent-authored).
- Category `procedure` auto-entries: start at 0.35 (need validation through use).
- Category `gap` auto-entries: start at 0.3 (they're suggestions, not assertions).
- Category `lesson-learned` auto-entries: start at 0.4 (failure observations are reliable; lessons are interpretations).

---

## Part 5: Anti-Patterns and Risks

### 5.1 Knowledge Pollution

**Risk**: Extracting noise as knowledge. The knowledge base grows with low-quality entries that dilute search results and waste context window when surfaced to agents.

**Manifestation**: An auto-extractor records "crate bootstrapping sequence starts with reading rand_distr documentation" because crt-006 happened to use rand_distr. This is feature-specific, not a project convention.

**Mitigations**:
1. **Cross-feature validation gate**: No entry promoted from a single feature. This is the primary defense.
2. **Content-vs-structure separation**: File paths and directory structures are extracted (structural). Edit content and specific dependency names are not (content-specific).
3. **Confidence floor**: Auto-entries start below default confidence. They must earn their way up through access and votes. Unused entries decay to irrelevance.
4. **Bounded growth**: Cap auto-extracted entries at 20 per retrospective cycle. Force prioritization.
5. **Automatic deprecation**: Auto-entries with zero access after 10 feature cycles are auto-deprecated.

**Metric**: Knowledge pollution rate = (auto-entries deprecated unused) / (total auto-entries created). Target: < 20%.

### 5.2 Echo Chamber Effect

**Risk**: Auto-extracted patterns from early features reinforce themselves. Agents follow the extracted convention, which generates more signals that confirm the convention, even if the convention is suboptimal.

**Manifestation**: First 3 features use inline `mod tests {}`. This gets extracted as a convention. Future agents follow it because Unimatrix says to. Nobody ever tries a `tests/` directory approach that might be better for integration tests.

**Mitigations**:
1. **Convention entries are descriptive, not prescriptive**: "All observed features use inline mod tests" -- not "You must use inline mod tests." The entry describes what IS, not what SHOULD BE.
2. **Deviation flagging without blocking**: When an agent deviates from an auto-extracted convention, flag it in the retrospective as a data point -- don't prevent the deviation.
3. **Convention age tracking**: Auto-conventions created from early features (< 5 data points) carry a "low sample size" warning.
4. **Periodic review**: Every 20 features, review all auto-extracted conventions for staleness. Remove those that no longer match current practice.

**Metric**: Convention deviation rate = (features that deviate from convention) / (features where convention applies). Healthy range: 5-15%. Below 5% suggests lock-in; above 15% suggests the convention is wrong.

### 5.3 Sensitive Information Capture

**Risk**: Auto-extraction captures secrets, credentials, or private information from agent interactions.

**Manifestation**: An agent writes a `.env` file with an API key. The file path gets recorded in S-CON-01. The path itself is innocuous, but if the auto-extractor also captures content snippets (from edit inputs or response snippets), the key leaks into the knowledge base.

**Mitigations**:
1. **Path-only extraction for content signals**: Extract file paths and directory structures. Never extract file content.
2. **Blocklist for sensitive paths**: `.env`, `credentials.json`, `.ssh/`, `secrets/`, `*.pem`, `*.key` -- paths matching these patterns are excluded from all extraction.
3. **No response_snippet in knowledge entries**: The `response_snippet` field in ObservationRecords is used for hotspot detection but never propagated to auto-extracted entries.
4. **Content review gate**: Auto-extracted entries that include any text content (not just paths/metrics) go through "proposed" status and require human review before activation.

**Metric**: Sensitive-content-in-knowledge incidents. Target: zero. Any occurrence triggers immediate quarantine and root cause analysis.

### 5.4 Storage Bloat

**Risk**: Passive acquisition generates too many entries, overwhelming the knowledge base and degrading search performance.

**Manifestation**: Every feature generates 20+ auto-entries. After 50 features, the knowledge base has 1000+ auto-entries, most of which are minor variations of similar patterns.

**Mitigations**:
1. **Deduplication at extraction time**: Before proposing an auto-entry, embed its content and check similarity against existing entries. If cosine similarity > 0.85 with an existing entry, merge rather than create.
2. **Category budget**: Maximum auto-entries per category: conventions (50), procedures (30), patterns (30), gaps (20), lessons (20). Old, unused entries are deprecated when budget is exceeded.
3. **Compaction during retrospective**: Each retrospective checks for auto-entries that can be merged (same topic, similar content, both low-access). Merge into a single stronger entry.
4. **Observation buffer, not immediate storage**: Signals go into an observation buffer first. Only signals that survive cross-feature validation become entries. The buffer is fixed-size (last 20 features of observations).

**Metric**: Auto-entry count / total entry count. Target: < 30%. The knowledge base should remain primarily agent-authored.

### 5.5 Positive Feedback Loops

**Risk**: Auto-extracted knowledge influences agent behavior, which generates signals that extract more similar knowledge, creating a self-reinforcing loop.

**Manifestation**:
1. Feature 1-3: Agents happen to read `server.rs` before `tools.rs`.
2. Auto-extraction: "Convention: read server.rs before tools.rs."
3. Future agents: context_briefing surfaces this convention -> agents read server.rs before tools.rs.
4. Auto-extraction: "Convention confirmed with higher confidence" -> even more aggressive surfacing.
5. Result: An accidental ordering becomes a rigid workflow, even if the opposite order would be equally valid.

**Mitigations**:
1. **Source tagging**: All auto-extracted entries carry `source: "observation"`. The confidence system can weight observation-sourced entries differently from agent-authored ones.
2. **Observation-only confidence window**: For the first 5 features after extraction, an auto-entry's confidence is computed only from pre-extraction data. Post-extraction access patterns (which may be influenced by the entry itself) are excluded from confidence updates during this window.
3. **Novelty bonus for deviation**: When an agent does NOT follow an auto-extracted convention and succeeds (gate pass, feature completion), the deviation gets a positive signal. Conventions that are frequently deviated from successfully get their confidence reduced.
4. **Independent validation**: Periodically (every 10 features), a human reviews the top 10 most-accessed auto-entries. "Is this still accurate? Is it genuinely useful, or are agents just cargo-culting it?"

**Metric**: Self-referential access rate = (accesses to auto-entry that were preceded by a briefing surfacing that entry) / (total accesses to auto-entry). If > 80%, the entry may be in a feedback loop.

---

## Part 6: Implementation Architecture

### 6.1 Processing Model

Passive knowledge acquisition is a **batch process** that runs during retrospectives, not a real-time system. This is a deliberate choice:

| Property | Real-time | Batch (chosen) |
|----------|-----------|----------------|
| Latency | Immediate | 1-2 feature cycles |
| Accuracy | Low (single observation) | High (cross-feature validation) |
| Complexity | Stream processing, deduplication | Scan-compare-propose |
| Noise risk | High | Low |
| Infrastructure | Background daemon, message queue | Retrospective step |

The 1-2 cycle latency is acceptable because the knowledge being extracted is structural and procedural -- it doesn't change rapidly. A convention that takes 2 features to detect will remain stable for 50+ features.

### 6.2 Data Flow

```
Layer 1: Collection (existing)
  Hook events -> per-session JSONL files
  No changes needed.

Layer 2: Feature Attribution (existing)
  Session files -> feature mapping
  No changes needed.

Layer 3: Hotspot Detection (existing)
  Feature records -> metric vectors + hotspot findings
  No changes needed.

Layer 4: Signal Extraction (NEW)
  Feature records -> signal inventory
  Each extraction pattern (KE-01 through KE-10) runs against the feature's records.
  Output: candidate signals with evidence.

Layer 5: Observation Buffer (NEW)
  Candidate signals -> per-feature observation buffer
  Fixed-size ring buffer of last 20 features.
  Each entry: (feature_id, signal_type, evidence, content_hash).

Layer 6: Cross-Feature Validation (NEW)
  Observation buffer -> validated patterns
  On each retrospective, compare current feature's signals against the buffer.
  Patterns meeting the threshold are proposed.

Layer 7: Entry Proposal (NEW)
  Validated patterns -> Unimatrix entries (status: "proposed")
  Deduplication check against existing entries.
  Source: "observation".
  Confidence: per-category initial value.

Layer 8: Human Review (NEW, in retrospective conversation)
  Proposed entries presented in retrospective output.
  Human accepts/rejects/modifies.
  Accepted -> status: "active".
  Rejected -> feedback stored for threshold adjustment.
```

### 6.3 Observation Buffer Schema

```rust
struct ObservationBuffer {
    /// Ring buffer of per-feature observations. Max 20 entries.
    features: VecDeque<FeatureObservation>,
}

struct FeatureObservation {
    feature_id: String,
    computed_at: u64,
    /// Structural patterns observed (file paths, directory trees, naming patterns).
    structural_signals: Vec<StructuralSignal>,
    /// Procedural patterns observed (tool sequences for integration, bootstrap, etc.).
    procedural_signals: Vec<ProceduralSignal>,
    /// File dependency edges (weighted by temporal proximity and frequency).
    dependency_edges: Vec<DependencyEdge>,
    /// Knowledge gap queries (search queries with zero results).
    gap_queries: Vec<GapQuery>,
    /// Gate failure events with context.
    gate_failures: Vec<GateFailure>,
    /// Phase profiles (tool distribution by phase type).
    phase_profiles: HashMap<String, PhaseProfile>,
}

struct StructuralSignal {
    template: String,        // e.g., "product/features/{id}/SCOPE.md"
    instances: Vec<String>,  // actual paths that matched
    signal_type: StructuralType, // Directory, Naming, TestStructure, Branch
}

struct ProceduralSignal {
    /// File-role sequence (not specific paths).
    steps: Vec<String>,      // e.g., ["dep-declaration", "server-struct", "tool-handler"]
    /// Edit count per step.
    edit_counts: Vec<u32>,
    /// Total duration of the procedure.
    duration_secs: u64,
}

struct DependencyEdge {
    source: String,          // file path
    target: String,          // file path
    weight: f64,             // co-access frequency * temporal proximity
    direction: EdgeDirection, // SourceReadBeforeTargetEdit, Bidirectional, Undirected
}

struct GapQuery {
    query: String,
    embedding: Vec<f32>,     // for similarity clustering
    tool: String,            // context_search, Grep, Glob
    feature_phase: String,   // when in the feature lifecycle the gap was hit
}

struct GateFailure {
    gate_id: String,
    failure_reason: String,
    rework_duration_secs: u64,
    files_changed_in_rework: Vec<String>,
}
```

### 6.4 Cross-Feature Validation Logic

```
for each signal_type in [structural, procedural, dependency, gap, lesson]:
    current_signals = extract_signals(current_feature_records, signal_type)

    for each signal in current_signals:
        matches = buffer.find_similar(signal, signal_type)

        if matches.count >= threshold(signal_type):
            existing = knowledge_base.find_similar(signal.to_entry())
            if existing.similarity > 0.85:
                // Merge: update evidence, bump confidence
                knowledge_base.enrich(existing, signal)
            else:
                // New: propose entry
                knowledge_base.propose(
                    content: signal.to_knowledge(),
                    category: signal_type.category(),
                    source: "observation",
                    confidence: signal_type.initial_confidence(),
                    evidence: matches.to_evidence(),
                )

Thresholds by signal type:
  structural:  2 features (high reliability, verifiable)
  procedural:  3 features (medium reliability, needs pattern stability)
  dependency:  2 features (structural, but needs co-access consistency)
  gap:         2 features (zero-result is unambiguous)
  lesson:      2 features (failure is binary, lesson is interpretation)
  phase_profile: 5 features (statistical, needs sample size)
  convention:  all features (100% consistency required)
```

---

## Part 7: Signal Priority Matrix

Given finite engineering effort, which extraction patterns should be implemented first?

### Priority 1: Implement Immediately (High value, low risk, builds on existing infra)

| Pattern | Signals Used | Why First |
|---------|-------------|-----------|
| KE-01: Knowledge gaps | S-SRC-05, S-SRC-06 | Zero-result queries are already in JSONL. Just aggregate them. No false positive risk. |
| KE-02: Structural conventions | S-CON-01, S-CON-02, S-CON-03 | File paths are already in JSONL. Template matching is regex. Near-zero noise. |
| KE-07: Implicit conventions | S-CON-07, S-CON-08, S-CON-09 | 100%-consistency detection is trivial. Highest confidence. |

### Priority 2: Implement Next (High value, medium complexity)

| Pattern | Signals Used | Why Second |
|---------|-------------|------------|
| KE-04: File dependencies | S-ACC-05, S-ACC-07, S-ACC-08 | Co-access tracking exists (crt-004). Extend from entries to files. |
| KE-05: Gate failure lessons | S-OUT-03, S-OUT-08 | Gate events are in JSONL. Failure reason extraction needs parsing. |
| KE-09: Friction lessons | S-BEH-05, S-BEH-08 | Friction detection exists (21 rules). Just aggregate across features. |

### Priority 3: Implement Later (Medium value, higher complexity)

| Pattern | Signals Used | Why Later |
|---------|-------------|-----------|
| KE-03: Procedural knowledge | S-BEH-01, S-CON-01 | Sequence alignment across features requires normalization layer. |
| KE-06: Phase profiles | S-BEH-02, S-MET-04 | Needs 5+ features before profiles stabilize. |

### Priority 4: Defer (Low immediate value, research needed)

| Pattern | Signals Used | Why Defer |
|---------|-------------|-----------|
| KE-08: Search-to-action | S-SRC-01, S-ACC-01 | Correlation-not-causation problem. Needs careful design. |
| KE-10: Topic clusters | S-SRC-01, S-ACC-03 | Requires sufficient query volume. Small projects may never accumulate enough. |

---

## Part 8: Open Questions

### 8.1 Where Does the Observation Buffer Live?

Options:
- **Unimatrix entries (category: "observation-buffer")**: Uses existing storage. Searchable. But not designed for fixed-size ring buffers.
- **Dedicated SQLite table**: Fast, structured, indexed, queryable. Natural fit with existing backend.
- **File-based (JSON in observation directory)**: Simple, inspectable, no schema changes. But not queryable through MCP tools.

Recommendation: File-based for v1 (simplicity), dedicated table for v2 (performance at scale).

### 8.2 Who Reviews Proposed Entries?

Options:
- **Human during retrospective**: Naturally integrated. But adds review burden.
- **Auto-promotion after N features without rejection**: Reduces burden. But risks promoting unreviewed entries.
- **Coordinator agent with review authority**: Scalable. But agents reviewing agents is a trust question.

Recommendation: Human review for v1. Auto-promotion for structural conventions (KE-02, KE-07) after v1 proves their precision is consistently > 95%.

### 8.3 How to Handle Feature Type Normalization?

Research spikes (ass-*) and full implementations (nxs-*, crt-*) have fundamentally different telemetry profiles. Cross-feature validation that mixes them will produce bad results.

Options:
- **Separate observation buffers per feature type**: Clean. But reduces sample size per buffer.
- **Type-weighted comparison**: Use all features but discount cross-type matches.
- **Exclude research spikes**: Only extract from implementation features. Research spikes are inherently exploratory.

Recommendation: Exclude research spikes (ass-*) from procedural and phase profile extraction. Include them in structural and gap extraction (file structure conventions apply regardless of feature type).

### 8.4 What's the Minimum Feature Count for the System to Be Useful?

The cross-feature validation gates require N features before any auto-extraction occurs:
- Structural conventions: 2 features -> useful after feature 3
- Procedures: 3 features -> useful after feature 4
- Phase profiles: 5 features -> useful after feature 6

For a project with 5+ completed features (Unimatrix currently has 20+), the system can begin extracting immediately from the historical retrospective data.

For a new project: the system provides zero passive knowledge for the first 2-3 features. It operates in "observation only" mode, building the buffer. This is acceptable because new projects also don't have enough conventions to extract.

### 8.5 Interaction with Existing Confidence System

Auto-extracted entries participate in the existing confidence pipeline (crt-002 through crt-005):
- Wilson score helpfulness (once they accumulate votes)
- Co-access boosting (once they're accessed alongside other entries)
- Decay (if they go unaccessed)
- Contradiction detection (if they conflict with agent-authored entries)

The only difference is the initial confidence value, which is lower than agent-authored entries. Once in the system, they're treated identically.

Exception: the observation-only confidence window (5 features post-extraction) prevents feedback loops from inflating confidence before the entry has been independently validated.

---

## Appendix A: Signal Count Summary

| Category | Signal Count | Already Tracked | New |
|----------|-------------|----------------|-----|
| Search | 11 | 3 (S-SRC-05 via miss rate, S-SRC-11 via bash-for-search, S-SRC-03 via access tracking) | 8 |
| Access | 11 | 4 (S-ACC-01/02/03 via usage tracking, S-ACC-10 via helpful votes) | 7 |
| Behavioral | 14 | 6 (S-BEH-05/06/07/08 via detection rules, S-BEH-03 via parallel rate metric, S-BEH-02 via phase metrics) | 8 |
| Content | 12 | 1 (S-CON-11 via context_store) | 11 |
| Outcome | 8 | 5 (S-OUT-03/04/05/06/08 via detection rules and metrics) | 3 |
| Meta | 8 | 5 (S-MET-01/02/03/04/07 via attribution and metrics) | 3 |
| Hook | 6 | 2 (S-HOK-01 via friction metric, S-HOK-06 via cold restart detection) | 4 |
| **Total** | **70** | **26** | **44** |

Of 70 identified signals, 26 (37%) are already tracked by the existing observation and metrics infrastructure. The remaining 44 require new extraction logic, most of which operates on data already present in JSONL records.

## Appendix B: Knowledge Category Mapping

| Extraction Pattern | Output Category | Output Tags | Initial Confidence |
|--------------------|----------------|-------------|-------------------|
| KE-01: Knowledge gaps | `gap` | `[auto-extracted, gap-detection]` | 0.30 |
| KE-02: Structural conventions | `convention` | `[auto-extracted, structural]` | 0.55 |
| KE-03: Procedures | `procedure` | `[auto-extracted, procedural]` | 0.35 |
| KE-04: File dependencies | `pattern` | `[auto-extracted, dependency-graph]` | 0.40 |
| KE-05: Gate failure lessons | `lesson-learned` | `[auto-extracted, gate-failure]` | 0.40 |
| KE-06: Phase profiles | `pattern` | `[auto-extracted, phase-profile]` | 0.30 |
| KE-07: Implicit conventions | `convention` | `[auto-extracted, implicit]` | 0.55 |
| KE-08: Search-to-action | `pattern` | `[auto-extracted, application-context]` | 0.30 |
| KE-09: Friction lessons | `lesson-learned` | `[auto-extracted, friction]` | 0.40 |
| KE-10: Topic clusters | `pattern` | `[auto-extracted, topic-cluster]` | 0.25 |

## Appendix C: Relationship to Existing Infrastructure

| Existing Component | How Passive Acquisition Uses It |
|-------------------|---------------------------------|
| Observation JSONL (hooks) | Primary data source. No changes needed. |
| unimatrix-observe (parser) | Record parsing. No changes needed. |
| unimatrix-observe (detection rules, 21 rules) | Hotspot findings feed KE-05 and KE-09. |
| unimatrix-observe (metrics) | MetricVector feeds KE-06 phase profiles. |
| unimatrix-observe (attribution) | Feature mapping. No changes needed. |
| unimatrix-observe (baselines) | Historical comparison for anomaly detection in phase profiles. |
| unimatrix-observe (synthesis) | Narrative generation for proposed entries. |
| unimatrix-embed (embedding) | Query embedding for KE-01 gap clustering and KE-10 topic clusters. |
| unimatrix-store (entries) | Storage target for auto-extracted entries. |
| unimatrix-store (co-access) | KE-04 extends file-level co-access (currently entry-level). |
| Confidence pipeline (crt-002 through crt-005) | Auto-entries participate after extraction. |
| Retrospective pipeline (col-002) | Trigger point for batch extraction. |
| context_store MCP tool | Existing path for explicit knowledge. Auto-extraction is parallel, not replacement. |
