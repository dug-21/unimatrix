# ADR-001: Quarantined Entry Base Score

## Status

Accepted

## Context

crt-002 defines `base_score(status)` with exhaustive match: Active = 0.5, Proposed = 0.5, Deprecated = 0.2. Adding `Status::Quarantined` requires a base_score value. The base_score represents a quality proxy from lifecycle status and feeds into the additive weighted confidence formula at weight 0.20.

## Decision

`Status::Quarantined` receives `base_score = 0.1`.

## Rationale

- **Lower than Deprecated (0.2)**: Deprecated means "no longer relevant but historically valid." Quarantined means "potentially harmful, under active suspicion." Quarantine is a stronger negative signal than deprecation.
- **Not zero**: A zero base_score would zero the base component entirely (0.20 * 0.0 = 0.0). While quarantined entries are excluded from retrieval, their confidence value still appears in `context_get` and `context_status`. A small positive value provides more information than zero.
- **Recoverable**: When an entry is restored (quarantine lifted), confidence is recomputed with Active's base_score (0.5), naturally recovering.

## Consequences

- `confidence.rs::base_score()` must add `Status::Quarantined => 0.1`
- Tests for base_score must cover the new variant
- A quarantined entry's confidence will drop significantly but not to zero
