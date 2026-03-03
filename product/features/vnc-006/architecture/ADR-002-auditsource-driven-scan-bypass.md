## ADR-002: AuditSource-Driven Content Scan Bypass

### Context

The UDS path currently calls `write_auto_outcome_entry` to write system-generated outcome entries. With the unified StoreService, these writes would pass through S1 content scanning. System-generated content should not be scanned for injection/PII patterns because:

1. The content is constructed by the server itself, not from user input
2. False positives on system-generated content would silently break auto-outcomes
3. Scanning adds unnecessary overhead to internal writes

However, S3 validation (structural correctness) and S5 audit (traceability) should still apply to internal writes.

### Decision

`StoreService::insert()` checks `audit_ctx.source` to determine S1 behavior:

```rust
pub(crate) fn validate_write(&self, title: &str, content: &str, ..., audit_ctx: &AuditContext) -> Result<(), ServiceError> {
    // S3: Always validate structure
    validate_title_length(title)?;
    validate_content_length(content)?;
    validate_no_control_chars(title)?;
    // ...

    // S1: Content scan — skip for Internal callers
    match &audit_ctx.source {
        AuditSource::Internal { .. } => { /* skip scan */ }
        _ => {
            ContentScanner::global().scan(content).map_err(|r| ServiceError::ContentRejected { ... })?;
            ContentScanner::global().scan_title(title).map_err(|r| ServiceError::ContentRejected { ... })?;
        }
    }

    Ok(())
}
```

`AuditSource::Internal` is `pub(crate)`, preventing external code from constructing it and bypassing scans.

### Consequences

- **Easier**: Internal writes (auto-outcome, future service-initiated entries) don't need special write paths. Single `StoreService::insert()` handles all callers.
- **Harder**: Must audit `AuditSource::Internal` usage — any new call site claiming Internal must be reviewed for whether it truly handles only system-generated content.
