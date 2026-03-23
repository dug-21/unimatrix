## ADR-003: Graceful Degradation Contract Scope

### Context

SR-07 from the risk assessment identifies the degradation boundary as high-severity: "a
mis-scoped `?` or early return could silently skip injection in non-failure cases." Lesson
#699 (referenced in SR-07) documents a historical case where a hardcoded `None` in the
hook pipeline silently broke the entire feedback loop with no test failure.

The hook has a hard invariant: **FR-03.7 — always exit 0**. Any transcript read path that
can fail must be wrapped in a degradation boundary that prevents the failure from
propagating to the server round-trip or the `BriefingContent` write.

Three scoping options were considered:

**Option A: Skip entire PreCompact output on transcript failure.** If transcript extraction
fails, write nothing to stdout. Reasoning: a partial restoration block is worse than none.

Rejected: This violates SR-07 explicitly. Skipping the `BriefingContent` (Unimatrix
knowledge index) because a local file is unreadable is disproportionate — the agent loses
both task continuity AND knowledge context when only the first is unavailable. The
knowledge context is always available (it comes from the server); skipping it due to a
local I/O error is a bug, not a feature.

**Option B: Fail the entire hook on transcript failure.** Return a non-zero exit code.
Claude Code would then block compaction or surface an error to the user.

Rejected: Violates FR-03.7 unconditionally. Transcript read errors are not hook-critical
failures — they are local file I/O uncertainties (missing file, race condition with write
flush, malformed records). Blocking compaction due to a missing transcript file would
degrade the agent's ability to compact its context window, which is far more harmful than
losing transcript restoration.

**Option C: Skip only the transcript block on failure; always emit BriefingContent.**
If transcript extraction returns `None` (for any reason), the `BriefingContent` response
from the server is written to stdout exactly as it would be if the transcript feature did
not exist. The agent receives knowledge context without task continuity — strictly better
than receiving nothing.

Selected. This is the minimal, correctly scoped degradation boundary.

**Implementation contract**:

`extract_transcript_block(path: &str) -> Option<String>` is the entire degradation
envelope. All I/O operations inside it are wrapped with `.ok()?` or `?` within an inner
closure/function that returns `Option<String>`. No error type escapes this boundary.

The call site in `run()` is:

```rust
let transcript_block: Option<String> = hook_input
    .transcript_path
    .as_deref()
    .filter(|p| !p.is_empty())
    .and_then(|p| extract_transcript_block(p));
// Execution ALWAYS continues here, transcript_block is None on any failure
```

The `and_then` chain means `extract_transcript_block` is never called if `transcript_path`
is `None` or empty. The return value is always `Option<String>` — there is no `Result`
type that could propagate. Downstream code operates on `Option<String>` — if `None`,
the briefing is written unmodified.

**Explicit failure classes** that all map to `transcript_block = None`:

| Failure | How it becomes None |
|---------|-------------------|
| `transcript_path` is `None` | `and_then` on `None` short-circuits |
| `transcript_path` is `""` | `.filter(|p| !p.is_empty())` returns `None` |
| `File::open` fails (not found, permissions) | `.ok()?` in inner closure |
| `file.metadata()` fails | `.ok()?` in inner closure |
| `file.seek()` fails | `.ok()?` in inner closure |
| All JSONL lines malformed | `build_exchange_pairs` returns `vec![]`; format returns `None` |
| No user/assistant pairs in window | `build_exchange_pairs` returns `vec![]`; format returns `None` |
| Budget fills with zero turns | format function returns `None` |

**Logging**: `extract_transcript_block` does NOT write to stderr on failure. Transcript
unavailability is a normal operating condition (empty new sessions, post-compaction
sessions, sessions where Claude Code has not yet written the file). Logging a warning for
every such case would pollute the hook's stderr output. If debugging is needed, the
implementer may add a debug-mode log behind a compile-time or env-var flag.

### Decision

Degradation boundary is `extract_transcript_block(path: &str) -> Option<String>`. All
failures inside this function return `None`. The call site never propagates any error from
this function. `BriefingContent` is always written to stdout regardless of whether
`transcript_block` is `Some` or `None`. Hook exit code is always 0 (FR-03.7).

### Consequences

- Agents with unreadable transcript files receive the Unimatrix knowledge briefing as
  before crt-028 was delivered — no regression.
- Agents with readable transcripts receive both task continuity and knowledge context.
- A future bug that accidentally makes `extract_transcript_block` panic (not `None`) would
  be caught by the FR-03.7 invariant test — which must exist and test that the hook exits 0
  even when the transcript path points to a non-existent file.
- The degradation contract is structurally enforced by the `Option<String>` return type —
  not by convention. A reviewer can verify correctness by checking that `extract_transcript_block`
  has no `Result` return type and no `.unwrap()` calls.
- SR-07 explicit test requirement: a test must verify that when `transcript_block = None`,
  `BriefingContent` with non-empty content still produces non-empty stdout. This test
  verifies the boundary is correctly scoped and briefing is not silently suppressed.
