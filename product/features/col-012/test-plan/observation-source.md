# Test Plan: observation-source

## Risk Coverage

This component defines the trait only. Risk coverage is in sql-implementation.

## Test Scenarios

### T-OS-01: Trait is object-safe and usable as dyn
**Type**: Unit (compile-time check)

Assert: `Box<dyn ObservationSource>` compiles (trait is object-safe if needed).

Actually, the trait methods take `&self` and return concrete types, so it should be object-safe.
This is verified implicitly by compilation.

## Implementation Notes

- The trait definition itself is trivially testable -- the real tests are on the implementation
- Ensure the trait uses unimatrix-observe's own Result type, not unimatrix-store's
