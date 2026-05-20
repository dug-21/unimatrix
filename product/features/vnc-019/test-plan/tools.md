# Test Plan: `tools.rs`

## Scope

The only change to `tools.rs` in vnc-019 is a tool description string update for
`context_graph`. No logic changes. No new route. Tool count remains 14.

Verification is primarily a code review checklist. One unit test asserts the string
contains the four required disclosure categories (AC-13).

---

## Verification Approach

### 1. Code Review Checklist (AC-13, R-11, ADR-004)

Reviewer reads the `context_graph` tool description in `tools.rs` and checks all items.

#### (a) subgraph Mode Section Present

- [ ] Tool description includes a `subgraph` mode section distinct from chain/current/neighbors
- [ ] Section documents parameters: `seed_ids`, `max_depth`, `max_nodes`, `edge_types`,
      `direction`, `resolve_supersessions`
- [ ] Section documents `max_depth` default (3) and valid range (1..=10)
- [ ] Section documents `max_nodes` valid range (1..=200) and that values above 200
      are rejected with a validation error

#### (b) Staleness Disclosure (FR-19, ADR-004, AC-13a)

- [ ] Description states that subgraph mode uses the in-memory graph cache
- [ ] Description states the cache is rebuilt each tick (typically 30-60 seconds)
- [ ] Description states that edges written within the current tick may not appear
- [ ] Description cross-references or equates to the same staleness contract as
      neighbors mode at depth>1

#### (c) depth_reached and truncated Semantics (AC-13b)

- [ ] Description explains what `depth_reached` represents (actual max BFS depth traversed)
- [ ] Description explains what `truncated: true` means (max_nodes cap reached)

#### (d) Unknown Seed Behavior (AC-13c, AC-17)

- [ ] Description states that seed IDs not present in the graph return an empty result,
      not an error

#### (e) EdgeRecord.direction Always "outgoing" (AC-13d, AC-03)

- [ ] Description explicitly states that the `direction` field in returned EdgeRecords
      is always `"outgoing"`, reflecting the canonical stored direction
      (`source_id → target_id`)

#### (f) No graph_rebuilt_at Field (ADR-004, C-08)

- [ ] Description does NOT promise a `graph_rebuilt_at` or `graph_age_ms` field
      in the response (these fields do not exist)

#### (g) Exact FR-19 Required Text

Verify the description includes, verbatim or equivalent:

> "subgraph mode uses the in-memory graph cache for BFS traversal. The cache is
> rebuilt each tick (typically 30-60 seconds). Edges written within the current
> tick interval may not appear in the result. This is the same staleness contract
> as neighbors mode at depth>1. The `depth_reached` field reports the actual
> maximum BFS depth traversed; `truncated: true` indicates the `max_nodes` cap
> was reached before BFS completed. Seed IDs not present in the graph return an
> empty result — not an error. The `direction` field in returned EdgeRecords is
> always `outgoing`, reflecting the canonical stored direction (source_id →
> target_id) regardless of the traversal direction parameter. `max_nodes` must
> be in range 1..=200; values above 200 are rejected with a validation error."

All facts from this text must be present; exact wording may vary.

---

### 2. Unit Test: String Assertions (AC-13, R-11)

As defined in `test-plan/graph_read.md`, Section 3:

```rust
#[test]
fn test_tool_description_contains_staleness_disclosures() {
    // AC-13, R-11: all four required disclosures present in tool description.
    // (a) tick-window staleness
    assert!(CONTEXT_GRAPH_DESCRIPTION.contains("tick"), "missing tick disclosure");
    // (b) depth_reached semantics
    assert!(CONTEXT_GRAPH_DESCRIPTION.contains("depth_reached"), "missing depth_reached");
    assert!(CONTEXT_GRAPH_DESCRIPTION.contains("truncated"), "missing truncated semantics");
    // (c) unknown seed behavior
    assert!(CONTEXT_GRAPH_DESCRIPTION.contains("empty result"), "missing empty result disclosure");
    // (d) direction always outgoing
    assert!(CONTEXT_GRAPH_DESCRIPTION.contains("outgoing"), "missing outgoing disclosure");
    // max_nodes 200 limit
    assert!(CONTEXT_GRAPH_DESCRIPTION.contains("200"), "missing max_nodes=200 limit");
}
```

**Implementation note for delivery agent**: expose the description as
`pub(crate) const CONTEXT_GRAPH_DESCRIPTION: &str = "..."` so the test can reference it.
If this is impractical (e.g., the description is built dynamically), the test must be
placed inside a `tools_tests.rs` companion file with `use super::*`.

---

### 3. Integration Test: tools/list reflects updated description

```python
# In suites/test_protocol.py or test_tools.py

def test_graph_tool_description_includes_subgraph(server):
    """context_graph tool description in tools/list mentions subgraph mode."""
    response = server.call("tools/list")
    tools = {t["name"]: t for t in response["tools"]}
    assert "context_graph" in tools
    desc = tools["context_graph"]["description"]
    assert "subgraph" in desc, f"description missing subgraph: {desc[:200]}"
    assert "tick" in desc, f"description missing staleness disclosure: {desc[:200]}"
    assert "outgoing" in desc, f"description missing direction disclosure: {desc[:200]}"
```

---

### 4. Tool Count Unchanged

**This is a critical non-regression check** (C-07, lesson #4437).

The existing `test_protocol.py` has a test asserting the exact tool count. That count
is currently **14** (vnc-018 added context_graph as the 14th tool). vnc-019 must NOT
change this count.

Verify:
- [ ] No new tool is added in `tools.rs`
- [ ] The `test_list_tools_returns_fourteen` (or equivalent) test in `test_protocol.py`
      still passes without modification

If the count test fails, it means a new tool was accidentally registered or an existing
one was removed — both are bugs.

---

## Assertions Checklist

- [ ] Tool description contains the word "subgraph" (mode documented)
- [ ] Tool description contains "tick" (staleness disclosure present)
- [ ] Tool description contains "depth_reached" (traversal depth semantics)
- [ ] Tool description contains "truncated" (cap semantics)
- [ ] Tool description contains "empty result" (unknown seed behavior)
- [ ] Tool description contains "outgoing" (EdgeRecord.direction disclosure)
- [ ] Tool description contains "200" (max_nodes upper bound)
- [ ] Tool description does NOT contain "graph_rebuilt_at" (ADR-004 compliance)
- [ ] Tool count in `tools/list` response remains 14
- [ ] No logic changes in `tools.rs` (pure string update)
