## ADR-004: `transcript_candidates` Attached at Response-Assembly Level, Outside the Memoized Struct

### Context
SR-07 (High) / AC-06: crt-033 ADR-001 (#3793) memoizes the cycle-review report **synchronously** to
`cycle_review_index` via `store_cycle_review()` (`write_pool_server`, before the handler returns). The
report struct it persists is `RetrospectiveReport` (`unimatrix-observe/src/types.rs:381`), a long
additive-optional-field struct. If `transcript_candidates` were a field **on** `RetrospectiveReport`,
every first-call review would write raw transcript excerpts into SQL — a direct breach of the
in-memory-only secrets guarantee (#4721, vnc-024 ADR-005: retention governs raw transcript only, never
distilled knowledge; no content redactor exists). The pinned constraint mandates the invariant (AC-06),
not a specific mechanism: candidates must never persist, and a forced re-review of the stored record
must return it with no stale candidates.

### Decision
Candidates are **response-transient** and live **outside** the memoized struct. Mechanism:

- `RetrospectiveReport` (the memoized type) gains **no** candidate field. `store_cycle_review()` and the
  `cycle_review_index` row are byte-unchanged.
- The cycle-review **response** (the wire/MCP result the handler assembles) carries the additive
  optional section:
  ```rust
  #[serde(skip_serializing_if = "Option::is_none")]
  transcript_candidates: Option<TranscriptCandidatesSection>,
  ```
  attached by ADR-005's helper at **response-assembly level** — after the report is computed and
  memoized, on the path to returning to the caller. Absent (not null/empty) when no session yields
  candidates (AC-04).
- This split is enforced structurally (the field does not exist on the persisted type) AND by a
  content-leak gate: a grep/log test extending vnc-025 AC-12 asserts no candidate or buffer content
  reaches any SQL write, file write, or log line in the new code paths (AC-06). A re-review test loads
  the stored record and asserts the returned report carries no candidates.

On a memoization HIT, candidates are still distilled fresh from whatever buffer content is present at
call time (ADR-005, OQ-4) and attached to the response — they may differ from the cached report, which
is acceptable and documented in the consumer guidance (AC-05).

Choosing assembly-level-attach over persist-path-strip: attach-outside makes the leak **structurally
impossible** (the persisted type has no slot), whereas strip-on-persist relies on remembering to strip
at every persist site. Structural impossibility is the stronger guarantee for a secrets invariant.

### Consequences
Easier: AC-06 holds by construction — the persisted type cannot carry candidates; the re-review and
content-leak gates are mechanical; cache-hit semantics (OQ-4) fall out (fresh candidates, unchanged
cached report). Harder: the response type and the memoized report type now diverge (the response is the
report plus an out-of-band section) — the handler's assembly step must be explicit that candidates are
added after memoization, and reviewers must police that no future field migration folds candidates back
onto `RetrospectiveReport`. Cross-refs: ADR-005 (the helper that attaches), crt-033 ADR-001 #3793,
vnc-024 ADR-005 #4721, vnc-025 AC-12, SR-07.
