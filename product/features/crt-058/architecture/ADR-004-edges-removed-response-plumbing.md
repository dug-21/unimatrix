## ADR-004: `edges_removed` Signal Plumbing Through `format_status_change`

### Context

The caller must be told inline how many edges were removed (AC-02), including a zero count when there were none (AC-05), while a delete failure must omit the advisory (AC-06). The formatter `format_status_change` (`mutations.rs:16`) serves three status changes (deprecate/quarantine/restore) across three formats (Summary/Markdown/Json) and currently has no advisory slot. Threading a value through a multi-format, multi-caller formatter is exactly where a count can be silently dropped by one format and still ship green — string/call-count tests are blind to argument threading (SR-04, #5427).

### Decision

Add one parameter `edges_removed: Option<u64>` to `format_status_change`, positioned before `format`. The `Option` encodes **ran-vs-failed**; the value encodes **count**:
- `Some(n)` — the eager delete ran; render `n` in **every** format, including `Some(0)` (AC-05).
- `None` — the delete failed, or the caller does not delete edges; omit the advisory in all formats.

Per-format rendering:
- Summary: append ` | {n} edges removed` to the existing line.
- Markdown: add a `**Edges removed:** {n}` line.
- Json: add an `"edges_removed": n` field.

`format_deprecate_success` gains an `edges_removed: Option<u64>` param and forwards it. `format_quarantine_success` and `format_restore_success` pass `None` (they delete no edges) — a single formatter, no branching by action. The `context_deprecate` handler passes `Some(count)` on success, `None` on eager-delete failure.

Enforcement (SR-04): a **behavioral per-format matrix** test asserts, for each of the three formats, that `Some(n)` surfaces the count, `Some(0)` renders `0`, and `None` omits the advisory entirely — plus an assertion on audit-record content. Not a call-count or bare string-presence check.

### Consequences

Easier: one additive, backward-compatible parameter; one formatter still serves all three status changes; the `Some(0)` vs `None` distinction cleanly separates "ran, found nothing" from "failed / not applicable" without a second flag.

Harder: four call sites update in lockstep (the formatter, three wrappers) plus the handler; the per-format matrix test is mandatory to prevent a silently-dropped count in one format. `Option<u64>` at every non-deprecate call site is slight boilerplate (`None`), accepted to keep a single formatter.
