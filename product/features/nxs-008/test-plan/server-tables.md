# Test Plan: server-tables (Wave 3)

## Risk Coverage

| Risk | Tests |
|------|-------|
| RISK-08 (JSON Array Deser) | RT-47, RT-48, RT-49 |
| RISK-13 (Audit write_in_txn) | RT-61, RT-62, RT-63 |
| RISK-14 (Agent Capability JSON) | RT-64, RT-65, RT-66 |

## Unit Tests

### UT-srv-01: TryFrom<u8> for TrustLevel (RT-42 cont.)
```
Assert: TryFrom(0)=System, TryFrom(1)=Privileged, TryFrom(2)=Internal, TryFrom(3)=Restricted
Assert: TryFrom(4) returns Err
```

### UT-srv-02: TryFrom<u8> for Capability (RT-42 cont.)
```
Assert: TryFrom(0)=Read, TryFrom(1)=Write, TryFrom(2)=Search, TryFrom(3)=Admin, TryFrom(4)=SessionWrite
Assert: TryFrom(5) returns Err
```

### UT-srv-03: TryFrom<u8> for Outcome (RT-42 cont.)
```
Assert: TryFrom(0)=Success, TryFrom(1)=Denied, TryFrom(2)=Error, TryFrom(3)=NotImplemented
Assert: TryFrom(4) returns Err
```

## Integration Tests — Agent Registry

### IT-srv-tab-01: Agent enroll with all capabilities (RT-64)
```
Setup: Initialize registry
Action: Enroll agent with capabilities=[Read, Write, Search, Admin, SessionWrite]
Action: resolve_or_enroll(agent_id)
Assert: All 5 capabilities present (JSON integer array round-trip)
```

### IT-srv-tab-02: Capability JSON format (RT-47)
```
Setup: Enroll agent with capabilities=[Read, Write]
Action: Direct SQL: SELECT capabilities FROM agent_registry WHERE agent_id=?
Assert: Column value is "[0,1]" (JSON integer array)
```

### IT-srv-tab-03: allowed_topics NULL vs empty (RT-48)
```
Setup:
  - Agent A: allowed_topics=None (all topics allowed)
  - Agent B: allowed_topics=Some(vec![]) (no topics allowed)
  - Agent C: allowed_topics=Some(vec!["topic1"])
Action: Read back all agents
Assert:
  - A: allowed_topics=None (SQL NULL)
  - B: allowed_topics=Some(vec![]) (SQL "[]")
  - C: allowed_topics=Some(vec!["topic1"]) (SQL '["topic1"]')
```

### IT-srv-tab-04: Protected agents cannot be modified (RT-65)
```
Setup: Bootstrap defaults (creates "system" and "human")
Action: Attempt to modify "system" agent
Assert: Error returned (ProtectedAgent)
```

### IT-srv-tab-05: Self-lockout prevention (RT-66)
```
Setup: Enroll agent with Admin capability
Action: Attempt to remove Admin from own agent
Assert: Error returned (SelfLockout)
```

### IT-srv-tab-06: bootstrap_defaults idempotent
```
Action: Call bootstrap_defaults twice
Assert: No duplicate agents, no errors
```

### IT-srv-tab-07: resolve_or_enroll updates last_seen_at
```
Setup: Enroll agent at time T
Action: resolve_or_enroll at time T+100
Assert: last_seen_at updated to T+100
```

## Integration Tests — Audit Log

### IT-srv-tab-08: audit event round-trip (RT-49)
```
Setup: Log audit event with target_ids=[1, 2, 3]
Action: Read back from audit_log
Assert: target_ids==[1,2,3], empty target_ids stored as "[]" not NULL
```

### IT-srv-tab-09: write_in_txn transaction participation (RT-61)
```
Setup: Begin write transaction
Action: Write audit event via write_in_txn, then ROLLBACK
Assert: Audit event NOT in database (rolled back with transaction)

Setup: Begin write transaction
Action: Write audit event via write_in_txn, then COMMIT
Assert: Audit event IS in database
```

### IT-srv-tab-10: write_count_since indexed query (RT-62)
```
Setup: Log 3 audit events for agent "a1" at times 100, 200, 300
Action: write_count_since("a1", 150)
Assert: Returns 2 (events at 200 and 300)
```

### IT-srv-tab-11: Monotonic event_id (RT-63)
```
Setup: Log 5 audit events sequentially
Action: Read all events
Assert: event_ids are strictly increasing (1, 2, 3, 4, 5)
```

### IT-srv-tab-12: Empty target_ids (RT-49)
```
Setup: Log audit event with target_ids=vec![]
Action: Read back
Assert: target_ids==vec![] (not None, not error)
Direct SQL: SELECT target_ids returns "[]" not NULL
```

## Schema Verification

### SV-srv-01: agent_registry table structure (AC-09)
```
Action: PRAGMA table_info(agent_registry)
Assert: 8 columns (agent_id, trust_level, capabilities, allowed_topics,
  allowed_categories, enrolled_at, last_seen_at, active), no data BLOB
```

### SV-srv-02: audit_log table structure (AC-10)
```
Action: PRAGMA table_info(audit_log)
Assert: 8 columns (event_id, timestamp, session_id, agent_id, operation,
  target_ids, outcome, detail), no data BLOB
```

### SV-srv-03: Audit log indexes
```
Action: Check sqlite_master
Assert: idx_audit_log_agent, idx_audit_log_timestamp present
```
