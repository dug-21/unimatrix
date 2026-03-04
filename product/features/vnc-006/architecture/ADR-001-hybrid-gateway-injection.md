## ADR-001: Hybrid Gateway Injection Pattern

### Context

The Security Gateway (S1-S5) must be integrated with services (SearchService, StoreService, ConfidenceService). Three patterns were considered:

1. **Decorator pattern**: Gateway wraps each service, intercepting calls. Clean separation but heavy boilerplate — each service method needs a wrapper.
2. **Free functions**: Gateway logic as standalone functions called by services. Simple but not testable via injection — hard to mock in unit tests.
3. **Injected struct**: SecurityGateway as a struct injected into services via constructor. Services call `self.gateway.validate_search_query()` internally.

### Decision

Use the hybrid injected struct pattern (option 3). `SecurityGateway` is constructed once during `ServiceLayer::new()`, wrapped in `Arc`, and shared across all services. Services call gateway methods at the appropriate points in their pipelines.

```rust
pub(crate) struct SecurityGateway {
    audit: Arc<AuditLog>,
}

// In SearchService:
impl SearchService {
    pub(crate) async fn search(&self, params: ServiceSearchParams, audit_ctx: &AuditContext) -> Result<...> {
        let scan_warning = self.gateway.validate_search_query(&params.query, params.k, audit_ctx)?;
        // ... pipeline ...
        self.gateway.emit_audit(event);
        Ok(results)
    }
}
```

For testing, `SecurityGateway::new_permissive()` creates a gateway with a no-op audit log, allowing service unit tests to bypass security concerns when testing business logic.

### Consequences

- **Easier**: Testing services independently (mock or permissive gateway). Adding new security gates (add method to SecurityGateway). vnc-008 can enforce gateway references at module visibility boundary.
- **Harder**: Services must remember to call gateway methods at correct points — not compiler-enforced. Gateway bypass is possible within `pub(crate)` scope until vnc-008 module reorganization.
