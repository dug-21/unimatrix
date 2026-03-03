# Test Plan: ServiceLayer + Types

## Unit Tests

### TS-18: ServiceError display preserves context (R-11)
- Test each variant's Display output includes key details:
  - `ContentRejected { category: "InstructionOverride", description: "..." }` -> message includes category
  - `ValidationFailed("query too long")` -> message includes "query too long"
  - `Core(CoreError::Store(...))` -> message includes underlying error
  - `EmbeddingFailed("model not loaded")` -> message includes "model not loaded"
  - `NotFound(42)` -> message includes "42"

### TS-18b: ServiceError to rmcp::ErrorData conversion
- Convert each variant and verify:
  - ContentRejected -> code -32001, message includes category
  - ValidationFailed -> code -32602, message includes detail
  - Core -> delegates to existing ServerError conversion
  - EmbeddingFailed -> code -32603
  - NotFound -> code -32602

### TS-18c: ServiceError to ServerError conversion
- Convert each variant and verify correct ServerError variant produced

### TS-24: All service methods accept AuditContext (AC-11)
- Code inspection test: grep all `pub(crate) async fn` and `pub(crate) fn` in services/*.rs
- Verify each has `audit_ctx: &AuditContext` parameter (except is_quarantined which is static, and recompute which is fire-and-forget)

### TS-09: AuditSource visibility (AC-12)
- Compile-time test: verify `AuditSource` and `AuditSource::Internal` are `pub(crate)`
- This is a code structure test, not a runtime test

## ServiceLayer Construction

### TS-24b: ServiceLayer::new constructs all services
- Action: Construct ServiceLayer with all dependencies
- Verify: No panic
- Verify: search, store_ops, confidence fields are accessible
