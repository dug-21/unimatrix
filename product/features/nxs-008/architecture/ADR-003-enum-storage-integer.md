# ADR-003: Enum Storage as INTEGER with Display-Name Comments

**Status**: Accepted
**Context**: nxs-008, Open Question #3 from SCOPE.md
**Mitigates**: SR-04, SR-07 (Enum-to-Integer Mapping Stability)

## Decision

All enum types are stored as INTEGER columns using their `#[repr(u8)]` discriminant values. TEXT storage is rejected.

### Rationale

1. **Existing code already uses INTEGER**: All seven enums (`Status`, `SessionLifecycleStatus`, `SignalType`, `SignalSource`, `Outcome`, `TrustLevel`, `Capability`) have `#[repr(u8)]` and existing `as u8` conversions. The current bincode serialization stores them as their discriminant values. Using INTEGER maintains consistency with existing data and eliminates any migration conversion.

2. **bincode compatibility**: bincode v2 with serde serializes `#[repr(u8)]` enums as their discriminant integer. The v5-to-v6 migration deserializes full records via bincode, then writes the enum field as `record.status as u8 as i64`. The deserialized value IS the `repr(u8)` value. No conversion risk (addresses SR-07).

3. **Compactness**: INTEGER columns use 1-2 bytes per value vs 6-12 bytes for TEXT names. At current scale this is irrelevant, but there is no benefit to TEXT.

4. **Query ergonomics**: `WHERE status = 0` is marginally less readable than `WHERE status = 'Active'`, but this is mitigated by:
   - SQL comments in schema DDL: `-- 0=Active, 1=Deprecated, 2=Proposed, 3=Quarantined`
   - Named constants in Rust code for all discriminant values
   - Future queries will use the Rust API, not raw SQL

5. **Type safety**: `TryFrom<u8>` already exists for `Status`. Adding it for other enums provides runtime validation on read. TEXT parsing would require string matching, which is more error-prone.

### Rejected Alternative: TEXT

TEXT storage would require:
- New `FromStr`/`Display` impls for 6 enums that don't have them
- Migration code to convert integer discriminants to strings
- Every query to use string comparison instead of integer comparison
- Risk of case-sensitivity bugs (`"Active"` vs `"active"`)

## Implementation

1. Each enum column is declared as `INTEGER NOT NULL` in the DDL
2. Schema DDL includes a comment mapping values to names
3. All enums get `TryFrom<u8>` if they don't already have it (Status already does)
4. The `entry_from_row()` helper reads enum columns as `row.get::<_, u8>(n)?` and converts via `TryFrom`
5. Write paths use `enum_value as u8 as i64` (rusqlite binds i64)

## Consequences

- No conversion step in migration (bincode discriminant = repr(u8) = stored INTEGER)
- Future enum variant additions require appending to the end of the enum (existing contract)
- If TEXT readability is needed later, a VIEW can provide `CASE WHEN status=0 THEN 'Active' ...`
