# Test Plan — Reserved Slugs (`config.rs`)

> Component: `crates/unimatrix-server/src/infra/config.rs` (`RESERVED_SLUGS`, `is_reserved_slug`) · Surface: `src/infra/config.rs` tests · Risks: R-08 (High) · AC-02 (FR-13)

## Scope
The reserved set is re-derived from the new route grammar. `RESERVED_SLUGS = ["v1","health","observe","tools"]` value RETAINED; derivation re-documented (ADR-005). `tools` kept reserved (conservative — OQ-3). No registerable slug may shadow a live route segment.

## Unit Test Expectations

### Registration-rejection table (R-08 sc.1)
- `test_every_reserved_name_rejected` — attempt registration against EACH reserved name (`v1`, `health`, `observe`, `tools`); assert each is rejected at the parse edge (`is_reserved_slug` true → registration error).
- `test_non_reserved_slug_accepted` — a normal slug (`alpha`) passes the reserved check.

### Grammar-coupling (R-08 sc.2)
- `test_reserved_set_covers_route_segments` — assert every route segment the NEW grammar uses (`v1`, `health`, `observe`) is in `RESERVED_SLUGS`; no segment is both routable AND registerable. The reserved set is tested against the NEW grammar, not the old.
- `test_observe_is_reserved` — explicitly assert `observe` is reserved (now that `/v1/{slug}/observe` is a live segment) so a slug `observe` cannot shadow it.

### `tools` decision pin (R-08 sc.3 / OQ-3)
- `test_tools_reservation_locked` — lock the CHOSEN `tools` reservation state (currently reserved). A silent flip (un-reserving `tools` without intent) FAILS this test. If the human un-reserves `tools` (OQ-3), this test and the rejection table change by one row — documented, not silent.

## Edge Cases
- Slug exactly matching a reserved name (case-sensitive; reserved are lowercase, slug regex is lowercase) → rejected.
- A slug that is a SUPERSET of a reserved name (`v1x`, `observer`) → NOT reserved (only exact-segment names reserve); assert these register successfully.

## Coverage Requirement
No registerable slug can shadow a live route segment; the reserved set is derived from / consistent with the actual route segments of the new grammar; the `tools` decision is locked so a silent flip is caught.
