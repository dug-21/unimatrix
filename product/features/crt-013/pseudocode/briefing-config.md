# Component: Configurable Briefing Neighbor Count

## Purpose

Replace hardcoded `k: 3` at `briefing.rs:228` with a configurable parameter. Default 3, env var override, clamped to [1, 20].

## Changes

### 1. Add semantic_k field to BriefingService

**File**: `crates/unimatrix-server/src/services/briefing.rs`

```
MODIFY struct BriefingService:
  ADD field: semantic_k: usize

MODIFY BriefingService::new():
  ADD parameter: semantic_k: usize
  SET self.semantic_k = semantic_k

MODIFY assemble() at line 228:
  REPLACE: k: 3
  WITH:    k: self.semantic_k
```

### 2. Add parse_semantic_k helper

```
/// Parse UNIMATRIX_BRIEFING_K env var.
/// Returns default (3) if unset or unparseable. Clamps to [1, 20].
/// Read once at construction time — runtime changes to the env var are ignored.
fn parse_semantic_k() -> usize {
    match std::env::var("UNIMATRIX_BRIEFING_K") {
        Ok(val) => match val.parse::<usize>() {
            Ok(k) => k.clamp(1, 20),
            Err(_) => {
                tracing::warn!(
                    value = %val,
                    "UNIMATRIX_BRIEFING_K: invalid value, using default 3"
                );
                3
            }
        },
        Err(_) => 3,
    }
}
```

### 3. Update construction site

**File**: `crates/unimatrix-server/src/services/mod.rs` (line 248-252)

```
MODIFY:
  let semantic_k = briefing::parse_semantic_k();
  let briefing = BriefingService::new(
      Arc::clone(&entry_store),
      search.clone(),
      Arc::clone(&gateway),
      semantic_k,
  );
```

**Note**: `parse_semantic_k` needs to be `pub(crate)` or called from within the briefing module.

### 4. Update test helper

**File**: `crates/unimatrix-server/src/services/briefing.rs` (test module)

```
MODIFY make_briefing_service():
  let service = BriefingService::new(
      Arc::clone(&entry_store),
      search,
      gateway,
      3,  // default semantic_k for existing tests
  );
```

## Error Handling

- Invalid env var: log warning via tracing, fall back to default 3
- Out of range: clamp silently (1 minimum, 20 maximum)
- No panics possible in this path

## Key Test Scenarios

1. Default k=3 when env var unset
2. k=5 when UNIMATRIX_BRIEFING_K=5
3. k clamped to 1 when UNIMATRIX_BRIEFING_K=0
4. k clamped to 20 when UNIMATRIX_BRIEFING_K=100
5. k falls back to 3 when UNIMATRIX_BRIEFING_K=abc
6. Existing briefing tests pass unchanged (they pass semantic_k=3 directly)
