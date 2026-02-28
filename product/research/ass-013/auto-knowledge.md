# ASS-013: Auto-Knowledge Extraction from Observation Telemetry

## The Opportunity

Unimatrix currently relies on agents explicitly spending tokens to store knowledge via `context_store`. If durable project knowledge can be extracted automatically from tool-call telemetry — without consuming agent context window — the knowledge base populates itself as a side effect of normal development work.

## The Risk

Noise pollution. Failed experiments, one-off workarounds, and feature-specific decisions could be recorded as repeatable patterns, degrading the knowledge base over time.

## What's Extractable: Three Tiers

### Tier 1: Structural Conventions (High Confidence, Rule-Based)

Patterns that emerge from file creation and naming, verifiable against existing project structure. These are the safest to auto-extract because they're cross-validated by multiple data points and existing files.

| Signal | Evidence | Extraction Method |
|--------|----------|-------------------|
| Feature directory structure | 34 files created in fixed hierarchy across 7 subdirectories | Match Write paths against `product/features/{id}/` template |
| ADR naming `ADR-NNN-kebab.md` | 4 ADRs, perfectly consistent | Regex on Write paths in `architecture/` |
| Crate naming `unimatrix-{name}` | 6 crates, all follow pattern | Match `mkdir` or Write under `crates/` |
| Component 1:1:1 mirror (pseudocode/test-plan/source) | 8 components, perfect alignment | Stem-match across three directories |
| Inline `mod tests {}` blocks (no separate test files) | 42 tests, 0 `tests/` directories | Absence of `tests/` in Write paths |
| Commit format `type(scope): desc (#N)` | 4 commits, consistent | Parse `git commit -m` from Bash |
| Branch naming `feature/{phase}-{NNN}` | 1 branch, matches convention | Parse `git checkout -b` from Bash |
| Workspace glob `members = ["crates/*"]` | Root Cargo.toml read 6x, never edited | Read-without-edit of root Cargo.toml during crate creation |

**Noise risk**: Very low. These are structural, not behavioral. A failed experiment doesn't create a wrong directory structure — it creates wrong code inside the right structure.

**Auto-extraction approach**: After each feature cycle, compare Write paths against known templates. New templates that appear consistently across 3+ features get proposed as conventions.

### Tier 2: Procedural Knowledge (Medium Confidence, Sequence-Based)

Step-by-step processes extracted from ordered tool-call sequences. Higher value but higher noise risk — need cross-feature validation before promoting to knowledge.

#### Server Integration Procedure (6 files, 10+ steps)

Extracted from the edit sequence during Stage 3b implementation. Every prior crate integration (embed, coaccess, coherence, adapt) followed this same file order:

1. Add dependency to `crates/unimatrix-server/Cargo.toml`
2. Add `use` import to `server.rs`
3. Add field to `UnimatrixServer` struct (`server.rs`)
4. Wire into constructor (`server.rs`)
5. Add adaptation/service calls in tool handlers (`tools.rs`)
6. Modify search reranking pipeline (`tools.rs`)
7. Modify store/correct paths to feed data (`tools.rs`)
8. Instantiate service in `main.rs`
9. Add lifecycle field to `ShutdownHandles` (`shutdown.rs`)
10. Wire constructor call in `main.rs`

**Evidence**: 29 edit events across 5 files in fixed order. Same file-order pattern confirmed for crt-001 through crt-005.

**Noise risk**: Medium. The procedure itself is reliable, but the specific edit content is feature-dependent (which tool handlers, which reranking step). The file ORDER is the durable knowledge; the edit CONTENT is not.

#### Crate Bootstrapping Sequence

1. Read existing crate Cargo.toml files (convention check)
2. `mkdir -p crates/unimatrix-{name}/src`
3. Write `Cargo.toml` with workspace inheritance
4. Write `src/lib.rs` with module declarations
5. Write source modules in dependency order (leaves first, facade last)
6. `cargo check -p unimatrix-{name}` immediately after all files written
7. Fix compilation errors, iterate

**Evidence**: 10 files written in dependency order. `cargo check` always follows immediately.

#### Incremental Verification Sequence

```
cargo check -p {new-crate}
cargo check -p unimatrix-server
cargo test -p {new-crate} -p unimatrix-server
cargo test --workspace
```

**Evidence**: 4 occurrences of this exact sequence. The pattern is: verify new code compiles → verify integration compiles → test new + integration → full regression.

#### Gate Validation Procedures

Each gate (3a, 3b, 3c) follows a distinct but repeatable command sequence. Gate 3c always includes:
- `cargo test -p {crate} -- --list` (enumerate tests for risk mapping)
- `grep -r 'forbid(unsafe_code)'` (safety verification)
- `cargo check --workspace | grep 'warning|error'` (zero-warning check)

**Noise risk for procedures**: The biggest risk is extracting a procedure from a feature where the agent did something unusual (e.g., wrong order due to a mistake, then corrected). The correction sequence looks like a valid procedure even though it's an error recovery path.

**Mitigation**: Only promote procedures that appear identically across 3+ feature cycles. Single-feature procedures stay as "observed" not "confirmed."

### Tier 3: Dependency Knowledge (Medium-High Confidence, Graph-Based)

File dependency relationships extracted from read-before-edit patterns. This is the "to modify X, you need to understand Y and Z" knowledge.

#### Stable Dependency Chains (from 5-min lookback analysis)

| To edit... | Always read first (100%) | Usually read first (>50%) |
|------------|------------------------|--------------------------|
| `server.rs` | self, `shutdown.rs` | `tools.rs`, `lib.rs`, `main.rs`, `Cargo.toml`, `normalize.rs` |
| `tools.rs` | self, `server.rs`, `shutdown.rs` | `normalize.rs`, `lib.rs`, `main.rs`, full new crate |
| `main.rs` | — | `tools.rs`, `server.rs`, `shutdown.rs` |
| `shutdown.rs` | self, `tools.rs`, `server.rs` (15-min window) | full server module + full new crate |
| `service.rs` (facade) | all sibling modules (100%) | — |

**Cross-crate signal**: `unimatrix-embed/src/normalize.rs` was read before 55-85% of server edits. It defines `l2_normalize()` — the embedding normalization function. This is a stable cross-crate dependency.

**API surface lookups**: Agents grepped for specific function signatures before edits:
- `l2_normalize|l2_normalized` (12 greps) — most-searched API
- `embed_service.embed|embed_handle|.embed(` — locating embed call sites
- `UnimatrixServer::new` — confirming constructor signature before adding parameters
- `record_usage_for_entries` — locating function ownership across files
- `TODO|todo!|unimplemented!` — anti-stub verification at end of implementation

**Noise risk**: Low for the dependency chains (they're structural), but the specific API lookups are feature-dependent. `l2_normalize` was searched because crt-006 specifically needed the embed pipeline — other features wouldn't search for the same thing.

**Durable knowledge**: The file-cluster dependency (server.rs ↔ shutdown.rs ↔ tools.rs are always co-read) is project-level truth. The specific API names are feature-level detail.

## What's NOT Extractable (Noise Sources)

### Dead Ends That Look Like Patterns

1. **Write→Edit→Write cycles**: `lora.rs` was written, failed `cargo check` (wrong `rand` v0.9 API assumed), then rewritten entirely. A naive extractor sees "correction pattern" but the cause (rand API migration from `StandardNormal` to `Normal`) is version-specific and one-off.

2. **Test flakiness false positives**: `test_compact_search_consistency` FAILED in workspace test, PASSED in isolation. No edits between failure and success. An extractor looking for "failure→fix→success" cycles would attribute the gate report writes as a "fix" — a completely false causal chain.

3. **Context store retries**: 18 PreToolUse calls for `context_store`, only 8 PostToolUse confirmations. The unmatched 10 are hook capture artifacts from multi-agent concurrency, not MCP failures. Auto-extraction MUST require PostToolUse confirmation.

4. **One-shot investigation commands**: 82 of 118 unique Bash commands are exploratory (grep, find, ls, cat) driven by the specific unknown at that moment. Zero repeatability signal.

5. **Settings.json hook corrections**: Two rapid edits to fix a misconfigured hook matcher. One-off setup mistake, not a pattern.

### Feature-Specific vs. Project-Universal

The telemetry contains both, interleaved. Distinguishing them requires understanding WHAT was being built, not just HOW.

| Feature-specific (DO NOT extract as project patterns) | Project-universal (safe to extract) |
|-------------------------------------------------------|--------------------------------------|
| MicroLoRA weight initialization approach | Crate bootstrapping file order |
| `rand_distr` dependency choice | Server integration file sequence |
| EWC++ regularization structure | Incremental verification sequence |
| Adaptive embedding pipeline architecture | Feature directory hierarchy |
| Specific ADR content (ndarray, bincode, RwLock) | ADR naming convention |

## What Agents Store vs. What They Do

### Agents explicitly stored 8 entries:
- 1 architecture pattern (MicroLoRA + Contrastive + EWC++)
- 4 ADRs (ndarray, bincode, independent persistence, RwLock)
- 2 outcomes (session 1 result, hook deployment)
- 1 design trade-off analysis (episodic vs co-access)

### What agents DID but did NOT store:
- Server integration procedure (5-file, 10-step sequence)
- Crate bootstrapping sequence
- Incremental verification pattern
- File dependency chains (server.rs ↔ shutdown.rs ↔ tools.rs)
- Convention discovery read sequence (12-15 files read before any Write)
- Component 1:1:1 mirror pattern (pseudocode/test-plan/source)

The gap is almost entirely **workflow mechanics and build patterns**. Agents correctly prioritize domain knowledge (architecture decisions, research findings) over process knowledge. But the process knowledge IS durable and valuable — it just isn't worth an agent's tokens to store it explicitly when it could be derived.

## Proposed Extraction Strategy

### Confidence Levels for Auto-Extraction

```
CONFIRMED (auto-store after 3+ features with same pattern)
  └─ Structural conventions
  └─ File dependency clusters
  └─ Naming conventions

OBSERVED (present to human on /retrospective, don't auto-store)
  └─ Procedural sequences from single feature
  └─ API surface lookups
  └─ One-off investigation patterns

DISCARDED (never extract)
  └─ Compilation fix cycles
  └─ Test flakiness false positives
  └─ Feature-specific design decisions
  └─ One-shot Bash commands
  └─ Context store retry artifacts
```

### Noise Prevention Rules

1. **Require PostToolUse confirmation**: Any knowledge derived from a tool call must have a matching PostToolUse with non-error response.

2. **Cross-feature validation**: No procedural pattern promoted to "confirmed" from a single feature. Minimum 3 features showing the same sequence.

3. **Exclude fix cycles**: Write→fail→Edit→Write sequences are corrections, not patterns. The FINAL state has value; the journey does not.

4. **Separate structural from behavioral**: File naming/structure is verifiable (compare to existing files). Tool call sequences are behavioral (may vary by feature complexity, agent model, or human feedback).

5. **Human confirmation gate**: Even "confirmed" patterns should be presented in `/retrospective` before being auto-stored. The human says "yes, this is real" or "no, that's coincidence."

### What This Could Populate (After 5+ Features)

| Category | Example | Source |
|----------|---------|--------|
| `convention` | "Server integration touches 5 files in order: Cargo.toml → server.rs → tools.rs → main.rs → shutdown.rs" | Edit sequence analysis, confirmed across 5 features |
| `convention` | "New crates use `workspace = true` for edition/rust-version/license, never edit root Cargo.toml" | Write analysis + absence of root edits |
| `pattern` | "Facade modules (service.rs) require reading ALL sibling modules before edit" | Read-before-edit dependency chain |
| `pattern` | "Incremental verification: check new → check server → test both → test workspace" | Bash command sequence |
| `convention` | "Tests are inline `mod tests {}`, never separate `tests/` directories" | Write path analysis + glob absence |

## Open Questions

1. **Storage location**: Should auto-extracted knowledge go into Unimatrix entries (category: "convention", source: "observation") or a separate namespace to distinguish from agent-authored knowledge?

2. **Confidence bootstrapping**: Auto-extracted entries start at what confidence? Lower than agent-authored (since they haven't been human-validated)?

3. **Staleness**: Conventions evolve. If a future feature breaks the pattern (e.g., uses a `tests/` directory), does the auto-extracted convention get deprecated automatically, or does the deviation get flagged as anomaly?

4. **Feature-type normalization**: Research spikes vs. full implementations have very different telemetry profiles. Should they be treated as separate populations for pattern extraction?

5. **Multi-session features**: When a feature spans multiple human sessions (as crt-006 did with its timeout/restart), how to attribute patterns to the feature vs. the session?
