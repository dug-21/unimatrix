# Pseudocode: SecurityGateway (services/gateway.rs)

## Struct

```
struct SecurityGateway {
    audit: Arc<AuditLog>,
}
```

## Constructor

```
fn new(audit: Arc<AuditLog>) -> Self:
    SecurityGateway { audit }

#[cfg(test)]
fn new_permissive() -> Self:
    // Create a gateway with a no-op AuditLog for unit testing
    SecurityGateway { audit: Arc::new(AuditLog::new_noop()) }
```

## ScanWarning Type

```
struct ScanWarning {
    category: String,
    description: String,
    matched_text: String,
}
```

## S1 + S3: validate_search_query

```
fn validate_search_query(&self, query: &str, k: usize, audit_ctx: &AuditContext)
    -> Result<Option<ScanWarning>, ServiceError>:

    // S3: Input validation
    if query.len() > 10_000:
        return Err(ServiceError::ValidationFailed("query exceeds 10000 characters"))

    if k == 0 || k > 100:
        return Err(ServiceError::ValidationFailed("k must be between 1 and 100"))

    // S3: Control character check (allow \n, \t)
    for ch in query.chars():
        if ch.is_control() && ch != '\n' && ch != '\t':
            return Err(ServiceError::ValidationFailed("query contains control characters"))

    // S1: Content scan (warn mode -- do NOT reject)
    // Skip scan for Internal callers
    match &audit_ctx.source:
        AuditSource::Internal { .. } => return Ok(None),
        _ => {}

    let scanner = ContentScanner::global()
    match scanner.scan(query):
        Err(scan_result) =>
            let warning = ScanWarning {
                category: scan_result.category.to_string(),
                description: scan_result.description.to_string(),
                matched_text: scan_result.matched_text,
            }
            // S5: Log the warning via audit
            self.emit_audit(AuditEvent {
                operation: "security_scan_warning",
                detail: format!("search query scan warning: {}", warning.category),
                ...from audit_ctx
            })
            return Ok(Some(warning))
        Ok(()) =>
            return Ok(None)
```

## S1 + S3: validate_write

```
fn validate_write(&self, title: &str, content: &str, category: &str, tags: &[String],
    audit_ctx: &AuditContext) -> Result<(), ServiceError>:

    // S3: Structural validation
    if title.len() > 500:
        return Err(ServiceError::ValidationFailed("title exceeds 500 characters"))

    if content.len() > 50_000:
        return Err(ServiceError::ValidationFailed("content exceeds 50000 characters"))

    if title.is_empty():
        return Err(ServiceError::ValidationFailed("title cannot be empty"))

    if content.is_empty():
        return Err(ServiceError::ValidationFailed("content cannot be empty"))

    // S3: Control character check on title (allow \n, \t in content)
    for ch in title.chars():
        if ch.is_control() && ch != '\n' && ch != '\t':
            return Err(ServiceError::ValidationFailed("title contains control characters"))

    // S3: Tag validation
    for tag in tags:
        if tag.len() > 100:
            return Err(ServiceError::ValidationFailed("tag exceeds 100 characters"))
        if tag.is_empty():
            return Err(ServiceError::ValidationFailed("empty tag"))

    // S1: Content scan -- skip for Internal callers (ADR-002)
    match &audit_ctx.source:
        AuditSource::Internal { .. } => { /* skip scan */ },
        _ => {
            // Scan content for injection + PII
            if let Err(scan_result) = ContentScanner::global().scan(content):
                return Err(ServiceError::ContentRejected {
                    category: scan_result.category.to_string(),
                    description: scan_result.description.to_string(),
                })

            // Scan title for injection only
            if let Err(scan_result) = ContentScanner::global().scan_title(title):
                return Err(ServiceError::ContentRejected {
                    category: scan_result.category.to_string(),
                    description: scan_result.description.to_string(),
                })
        }

    Ok(())
```

## S4: is_quarantined

```
fn is_quarantined(status: &Status) -> bool:
    *status == Status::Quarantined
```

Note: This is a static method (no &self) -- it does not need gateway state.

## S5: emit_audit

```
fn emit_audit(&self, event: AuditEvent):
    // Fire-and-forget -- log but never block
    let _ = self.audit.log_event(event)
```

## Tests (in gateway.rs #[cfg(test)] mod)

See test-plan/gateway.md for full test specifications.
