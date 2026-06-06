## ADR-001: Parity Corpus Generated from the Rust Hook as Oracle, Committed Goldens, CI Drift Check

### Context

SR-01 (High/High): `build-request.js` ports nontrivial logic from the 4,183-line
`hook.rs` — PostToolUse rework extraction (`is_bash_failure`, `extract_file_path`,
MultiEdit fan-out), the MIN_QUERY_WORDS=5 gate, PreToolUse `context_cycle` interception
(mcp_context promotion, `validate_cycle_params`, goal truncation), the PostToolUseFailure
explicit arm, and the SubagentStart JSONL tail-parse (RQ-6, `transcript_block.rs`).
Rust/JS divergence hides in edge cases: malformed JSONL, UTF-8 boundaries, missing
fields, whitespace-only prompts, non-string `tool_input`. The accepted scope review made
the parity corpus a first-class design artifact doubling as F6 (hook.rs retirement)
evidence. Hand-written expected outputs would just be a second hand-port — the same drift
risk twice.

### Decision

The Rust implementation is the oracle. A Rust dev-test in `unimatrix-server` (additive;
no production code change) generates the corpus goldens, which are **committed** under
`packages/unimatrix/test/fixtures/parity/` and **drift-checked in CI** by regenerating
and diffing — the same pattern as the ts-rs bindings gate (vnc-024 ADR-001, entry #4726).

Corpus structure per case: `{name}/stdin.json` (+ optional `transcript.jsonl`) →
goldens `expected-request.json` (from `normalize_event_name` + `build_request` + the
SubagentStart fallback) and, for the stdout layer, `expected-stdout.bin` (from
`write_stdout` / `write_stdout_subagent_inject` given a fixture `HookResponse`).

Mandatory edge-case inventory (the SR-01 enumeration):
- all 13 canonical events; Gemini aliases (`BeforeTool`/`AfterTool`/`SessionEnd`);
  unknown-event passthrough (raw name preserved); empty stdin; malformed stdin JSON;
  missing `session_id` (ppid fallback); missing `cwd`.
- UserPromptSubmit: empty, whitespace-only, 4-word vs 5-word boundary, long multi-word.
- PostToolUse: Bash exit_code 0/non-zero/missing/non-integer, `interrupted` true/false,
  Edit (`tool_input.path`), Write (`tool_input.file_path`), MultiEdit (normal, empty
  `edits`, missing `edits`, non-array `edits`), non-rework tools, non-claude-code provider.
- PostToolUseFailure: explicit arm, empty/null `extra`, missing `tool_name`, null error.
- PreToolUse: `context_cycle` bare + `mcp__unimatrix__` prefixed, near-miss injection
  names (must NOT intercept), invalid cycle params (fall through), `mcp_context`
  promotion, goal > MAX_GOAL_BYTES with multi-byte boundary.
- SubagentStart: `prompt_snippet` present/empty/whitespace; transcript tail variants —
  malformed JSONL lines, window starting mid-line, multi-byte char split at window edge,
  thinking-only turns, tool_use/tool_result adjacent pairing, missing transcript file,
  empty `transcript_path`.
- Adversarial content: control chars, emoji, lone-surrogate-adjacent sequences, embedded
  quotes/backslashes (feeds ADR-002's envelope byte-parity too).

Comparison rules: request goldens compared structurally after normalizing volatile
fields (`timestamp` → 0, `ppid-\d+` session ids → `ppid-X`); stdout goldens compared
**byte-identical** (AC-04/AC-05 Layer 1).

### Consequences

- Easier: SR-01 collapses from "trust the port" to "diff against the oracle"; F6
  retirement evidence accumulates for free; any future hook.rs behavior change breaks the
  CI drift check loudly instead of silently diverging the JS client.
- Harder: a deliberate Rust behavior change requires regenerating goldens (one command)
  — an explicit, reviewable step. The generator test adds a small Rust test artifact that
  must be able to write into `packages/` (path wired via env var in the dev-test, CI runs
  it from the workspace root).
- The corpus is the contract between F3 and F6; deleting or thinning it later
  invalidates the retirement gate.
