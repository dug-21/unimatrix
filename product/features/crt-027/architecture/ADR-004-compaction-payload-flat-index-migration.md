## ADR-004: CompactPayload Migrated to Flat Index Format

### Context

`handle_compact_payload` in `listener.rs` currently:
1. Calls `BriefingService::assemble()` to get `BriefingResult`
2. Partitions results into `CompactionCategories { decisions, injections, conventions }`
3. Passes `CompactionCategories` to `format_compaction_payload()` which produces a
   sectioned output with headers: `Decisions`, `Key Context`, `Conventions`

crt-027 removes `BriefingService`. The CompactPayload path must be migrated to the new
`IndexBriefingService`. This raises the question: what format should CompactPayload output
use after migration?

**Option A: Preserve section structure, adapt to new service.** Map `IndexEntry` back into
the three-section `CompactionCategories` structure based on `category` field. Retain
`format_compaction_payload` signature unchanged.

Rejected: This re-introduces the section structure that WA-4b explicitly removes. The
section structure is load-bearing for WA-5 if WA-5 depends on it — but the SCOPE.md
specifies that WA-5 "reads the flat table directly" and "can prepend without parsing section
structure." Preserving sections would force WA-5 to parse them anyway. The WA-5 dependency
is the primary reason to change the format here.

**Option B: Emit flat indexed table from CompactPayload.** Both `context_briefing` MCP
and `handle_compact_payload` emit the same `format_index_table()` format. WA-5 has a
single, consistent format contract.

Selected. The flat indexed table is defined by the typed `IndexEntry` struct (see ADR-005),
giving WA-5 a compile-time contract rather than a prose-described string format.

**Test invariants (SR-04):** 10 existing `format_compaction_payload` tests exist. The
function signature changes (drops `CompactionCategories`, gains `Vec<IndexEntry>`), so
all 10 tests must be rewritten — but the underlying invariants they protect mostly survive.
The section-ordering invariant (`format_payload_decisions_before_injections`) does NOT
survive — it is replaced by the confidence-descending sort invariant. The deprecated-
indicator test (`format_payload_deprecated_indicator`) does NOT survive — it is replaced
by an active-only invariant. All other 8 invariants survive in the new format.

The histogram block (`"Recent session activity:"`) is retained in the updated formatter.
It does not depend on section structure and WA-5 may optionally consume it.

### Decision

`handle_compact_payload` is migrated to use `IndexBriefingService::index()` directly.
`CompactionCategories` struct is deleted. `format_compaction_payload` is rewritten with
the signature:

```rust
fn format_compaction_payload(
    entries: &[IndexEntry],
    role: Option<&str>,
    feature: Option<&str>,
    compaction_count: u32,
    max_bytes: usize,
    category_histogram: &HashMap<String, u32>,
) -> Option<String>
```

The function produces:
1. Session context header block (Role, Feature, Compaction# — unchanged from current)
2. Flat indexed table from `format_index_table(entries)` within budget
3. Histogram block ("Recent session activity: ...") if non-empty, within remaining budget
4. Hard budget ceiling truncation via `truncate_utf8`

The query derivation in `handle_compact_payload` uses the same three-step priority as the
MCP path: (1) task param if present, (2) feature_cycle + top 3 topic_signals from session
state, (3) topic fallback. The `has_injection_history` path distinction is removed — the
new service does not distinguish between injection history and convention lookups; it runs
a unified indexed search.

**Test invariants rewritten (not deleted):**

| Old test | New test | Invariant |
|----------|----------|-----------|
| `format_payload_empty_categories_returns_none` | `format_payload_empty_entries_returns_none` | `Vec::new()` → `None` |
| `format_payload_header_present` | unchanged name | header line present |
| `format_payload_decisions_before_injections` | `format_payload_sorted_by_confidence` | confidence-desc sort |
| `format_payload_sorted_by_confidence` | merged into above | (same invariant now one test) |
| `format_payload_budget_enforcement` | unchanged name | `len() <= max_bytes` |
| `format_payload_multibyte_utf8` | unchanged name | valid UTF-8 at truncation |
| `format_payload_session_context` | unchanged name | Role/Feature/Compaction lines |
| `format_payload_deprecated_indicator` | `format_payload_active_entries_only` | all entries in output are Active |
| `format_payload_entry_id_metadata` | unchanged name | entry id in table |
| `format_payload_token_limit_override` | unchanged name | custom budget respected |
| `test_compact_payload_histogram_block_present_and_absent` | unchanged name | histogram block present/absent |

### Consequences

- `CompactionCategories` struct is deleted. Any code constructing it gets a compile error.
- `BriefingService.assemble()` call in `handle_compact_payload` is removed — the only UDS
  caller of `BriefingService` is migrated.
- WA-5 can prepend transcript content to `HookResponse::BriefingContent` without parsing
  section headers. The `IndexEntry` struct is the stable contract.
- The session injection history path (which filtered by previously-injected entry IDs) is
  removed from the CompactPayload path. The new index search returns the top-20 active entries
  ranked by fused score — this is a broader (not narrower) result set, which is appropriate
  for the "entering a phase" handoff use case.
- The `include_semantic: false` constraint from the old CompactPayload path is removed.
  `IndexBriefingService` always uses SearchService (semantic search). This is intentional:
  the new briefing is phase-conditioned by query (via topic_signals) rather than by injection
  history filtering.
