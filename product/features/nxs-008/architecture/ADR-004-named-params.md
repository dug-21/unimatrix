# ADR-004: Mandatory Named Parameters for All Multi-Column SQL Statements

**Status**: Accepted
**Context**: nxs-008
**Mitigates**: SR-02 (24-Column Bind Parameter Accuracy)

## Decision

All SQL statements with 4 or more bind parameters MUST use rusqlite's `named_params!{}` macro instead of positional `?` placeholders.

### Rationale

The ENTRIES table has 24 columns. An INSERT with 24 positional `?` placeholders is extremely error-prone — a single column-order mismatch causes silent data corruption (e.g., `created_at` written to `updated_at`). Compilation succeeds because all integer columns accept any i64 value.

`named_params!{}` binds values by name (`:id`, `:title`, `:content`), making column-order bugs impossible. The parameter names must match the SQL placeholders, creating a compile-visible contract.

### Example

```rust
// BEFORE (positional — error-prone at 24 columns)
conn.execute(
    "INSERT INTO entries (id, title, content, ...) VALUES (?1, ?2, ?3, ...)",
    rusqlite::params![id, title, content, ...],
)?;

// AFTER (named — column-order bugs impossible)
conn.execute(
    "INSERT INTO entries (id, title, content, ...) VALUES (:id, :title, :content, ...)",
    rusqlite::named_params! {
        ":id": id as i64,
        ":title": &record.title,
        ":content": &record.content,
        // ... 21 more named params
    },
)?;
```

## Scope

- ENTRIES INSERT (24 params): mandatory
- ENTRIES UPDATE (24 params): mandatory
- SESSIONS INSERT/UPDATE (9 params): mandatory
- AGENT_REGISTRY INSERT (8 params): mandatory
- AUDIT_LOG INSERT (8 params): mandatory
- SIGNAL_QUEUE INSERT (6 params): mandatory
- INJECTION_LOG INSERT (5 params): mandatory
- CO_ACCESS INSERT (4 params): mandatory
- Statements with 1-3 params: positional `?` is acceptable

## Consequences

- Slightly more verbose SQL strings
- Column-order bugs eliminated at the source
- Easier code review — reviewer can verify param names match column names without counting positions
