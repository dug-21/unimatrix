# Pseudocode: C1 Status Extension

## File: crates/unimatrix-store/src/schema.rs

### Status enum

```
enum Status {
    Active = 0,
    Deprecated = 1,
    Proposed = 2,
    Quarantined = 3,    // NEW
}
```

### TryFrom<u8>

```
impl TryFrom<u8> for Status:
    match value:
        0 => Active
        1 => Deprecated
        2 => Proposed
        3 => Quarantined    // NEW
        other => Err(InvalidStatus(other))
```

### Display

```
impl Display for Status:
    match self:
        Active => "Active"
        Deprecated => "Deprecated"
        Proposed => "Proposed"
        Quarantined => "Quarantined"    // NEW
```

### status_counter_key

```
fn status_counter_key(status):
    match status:
        Active => "total_active"
        Deprecated => "total_deprecated"
        Proposed => "total_proposed"
        Quarantined => "total_quarantined"    // NEW
```

## File: crates/unimatrix-server/src/confidence.rs

### base_score

```
fn base_score(status):
    match status:
        Active => 0.5
        Proposed => 0.5
        Deprecated => 0.2
        Quarantined => 0.1    // NEW (ADR-001)
```

## File: crates/unimatrix-server/src/response.rs

### status_to_str

```
fn status_to_str(status):
    match status:
        Active => "active"
        Deprecated => "deprecated"
        Proposed => "proposed"
        Quarantined => "quarantined"    // NEW
```

## File: crates/unimatrix-server/src/validation.rs

### parse_status

```
fn parse_status(s):
    match s.to_lowercase():
        "active" => Active
        "deprecated" => Deprecated
        "proposed" => Proposed
        "quarantined" => Quarantined    // NEW
        _ => Err(invalid status)
```

## File: crates/unimatrix-store/src/schema.rs (tests)

### Update test for TryFrom invalid

```
// Current: assert try_from(3u8) is Err
// Must change to: assert try_from(3u8) is Ok(Quarantined)
// Update invalid test to use 4u8 instead
```
