# Pseudocode: Duties Removal (response.rs)

## Overview

Remove all duties-related code from the Briefing struct and format_briefing function. This is a net-negative change (~30 lines removed).

## Changes

### 1. Briefing struct (response.rs ~line 459)

```rust
// BEFORE:
pub struct Briefing {
    pub role: String,
    pub task: String,
    pub conventions: Vec<EntryRecord>,
    pub duties: Vec<EntryRecord>,  // REMOVE THIS LINE
    pub relevant_context: Vec<(EntryRecord, f64)>,
    pub search_available: bool,
}

// AFTER:
pub struct Briefing {
    pub role: String,
    pub task: String,
    pub conventions: Vec<EntryRecord>,
    pub relevant_context: Vec<(EntryRecord, f64)>,
    pub search_available: bool,
}
```

### 2. format_briefing Summary format (~line 1043)

```rust
// BEFORE:
format!(
    "Conventions: {} | Duties: {} | Context: {}",
    briefing.conventions.len(),
    briefing.duties.len(),
    briefing.relevant_context.len()
)

// AFTER:
format!(
    "Conventions: {} | Context: {}",
    briefing.conventions.len(),
    briefing.relevant_context.len()
)
```

### 3. format_briefing Markdown format (~lines 1077-1085)

```rust
// REMOVE entire duties section:
text.push_str("### Duties\n\n");
if briefing.duties.is_empty() {
    text.push_str("No duties found for this role.\n\n");
} else {
    for entry in &briefing.duties {
        text.push_str(&format!("- **{}**: {}\n", entry.title, entry.content));
    }
    text.push('\n');
}
```

### 4. format_briefing JSON format (~lines 1104-1117)

```rust
// REMOVE duties array construction:
let duties: Vec<serde_json::Value> =
    briefing.duties.iter().map(entry_to_json).collect();

// REMOVE from JSON object:
"duties": duties,
```

### 5. Test helper make_briefing (~line 1979)

```rust
// BEFORE:
fn make_briefing(search_available: bool) -> Briefing {
    Briefing {
        role: "architect".to_string(),
        task: "design auth module".to_string(),
        conventions: vec![make_entry(1, "Convention 1", "Always use trait objects")],
        duties: vec![make_entry(2, "Duty 1", "Write ADRs")],
        relevant_context: vec![(make_entry(3, "Context 1", "Auth patterns"), 0.85)],
        search_available,
    }
}

// AFTER:
fn make_briefing(search_available: bool) -> Briefing {
    Briefing {
        role: "architect".to_string(),
        task: "design auth module".to_string(),
        conventions: vec![make_entry(1, "Convention 1", "Always use trait objects")],
        relevant_context: vec![(make_entry(3, "Context 1", "Auth patterns"), 0.85)],
        search_available,
    }
}
```

### 6. Test updates

All tests that reference `duties` or `Duties` must be updated:

- `test_format_briefing_summary`: Remove assertion on `Duties: 1`
- `test_format_briefing_markdown_all_sections`: Remove assertion on `### Duties` and `Duty 1`
- `test_format_briefing_json`: Remove assertion on `parsed["duties"]`
- `test_format_briefing_empty_sections`: Remove `duties: vec![]` from Briefing construction, remove assertion on `No duties found`

### 7. Briefing construction sites in tools.rs

The `context_briefing` handler constructs a `Briefing` struct. After duties removal, the `duties` field is gone:

```rust
// tools.rs (in context_briefing handler, after MCP rewiring):
let briefing = Briefing {
    role: params.role.clone(),
    task: params.task.clone(),
    conventions: result.conventions,
    // duties: REMOVED
    relevant_context: result.relevant_context,
    search_available: result.search_available,
};
```

## Impact Analysis

- `duties` field removed from struct: compilation will catch any missed references
- `format_briefing` output changes: summary line now shorter, markdown has no Duties section, JSON has no duties array
- MCP tool description string updated to not mention "duties"
- No runtime behavioral change: duties category has been empty in the knowledge base (0 active duties entries per project memory)
