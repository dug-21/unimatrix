# Test Plan — C8 protocol restart re-declaration line

Source: AC-09, FR-24. Risk: — (doc verification). Files: `.claude/protocols/uni/uni-design-protocol.md`, `uni-delivery-protocol.md`, `uni-bugfix-protocol.md`. Verification method: **grep** (no runtime test).

## Verification

### AC-09 — line present in all three protocols (FR-24)
```bash
grep -l 'context_cycle(type:"start"' \
  .claude/protocols/uni/uni-design-protocol.md \
  .claude/protocols/uni/uni-delivery-protocol.md \
  .claude/protocols/uni/uni-bugfix-protocol.md
```
- **Assert**: all three files are returned. The line states: on re-entering a broken session, the leader's first action is to re-issue `context_cycle(type:"start", topic:"{feature-id}")` (idempotent server-side — `AlreadyMatches` — and recreates the client tracker).

### Semantic check (manual)
- The line conveys: idempotent server-side; recreates the client tracker (`cycles/{session_key}.json` via the cycle_start interception seam). It is the recovery for workflow 3 (broken session, fresh restart — new session_id, tracker miss → re-declare).

## Coverage requirement
The grep matches in all three protocol files; the re-declaration is the documented recovery path that re-establishes both the server registry `Declared` feature and the client tracker.
