# Pseudocode: registry component

## Purpose

Add `EnrollResult`, `PROTECTED_AGENTS` constant, and `enroll_agent()` method to `AgentRegistry`. This is the core business logic for enrollment.

## New Types

```
/// Result of an enrollment operation.
pub struct EnrollResult {
    /// Whether this was a create (true) or update (false).
    pub created: bool,
    /// The final agent record after enrollment.
    pub agent: AgentRecord,
}
```

## New Constants

```
/// Agent IDs that cannot be modified via enrollment (ADR-002).
const PROTECTED_AGENTS: &[&str] = &["system", "human"];
```

This is a private constant. Protection is identity-based, not trust-level-based (ADR-002).

## Method: enroll_agent

```
impl AgentRegistry:
    pub fn enroll_agent(
        &self,
        caller_id: &str,
        target_id: &str,
        trust_level: TrustLevel,
        capabilities: Vec<Capability>,
    ) -> Result<EnrollResult, ServerError>:

        // 1. Protected agent check (ADR-002)
        if PROTECTED_AGENTS.contains(&target_id):
            return Err(ServerError::ProtectedAgent { agent_id: target_id.to_string() })

        // 2. Self-lockout prevention
        //    If caller is modifying themselves, ensure Admin stays in capabilities
        if caller_id == target_id:
            if !capabilities.contains(&Capability::Admin):
                return Err(ServerError::SelfLockout)

        // 3. Read-first: check if target already exists
        let existing: Option<AgentRecord> = {
            let read_txn = self.store.begin_read()?
            let table = read_txn.open_table(AGENT_REGISTRY)?
            match table.get(target_id)?:
                Some(guard) => Some(deserialize_agent(guard.value())?)
                None => None
        }

        let now = current_unix_seconds()

        // 4. Build the agent record
        let (created, record) = match existing:
            Some(existing_record) =>
                // UPDATE: preserve enrolled_at, update trust + caps + last_seen
                let updated = AgentRecord {
                    agent_id: target_id.to_string(),
                    trust_level,
                    capabilities,
                    allowed_topics: existing_record.allowed_topics,
                    allowed_categories: existing_record.allowed_categories,
                    enrolled_at: existing_record.enrolled_at,  // PRESERVE
                    last_seen_at: now,
                    active: existing_record.active,  // PRESERVE
                }
                (false, updated)

            None =>
                // CREATE: new agent with defaults
                let new_agent = AgentRecord {
                    agent_id: target_id.to_string(),
                    trust_level,
                    capabilities,
                    allowed_topics: None,
                    allowed_categories: None,
                    enrolled_at: now,
                    last_seen_at: now,
                    active: true,
                }
                (true, new_agent)

        // 5. Write to AGENT_REGISTRY
        let txn = self.store.begin_write()?
        {
            let mut table = txn.open_table(AGENT_REGISTRY)?
            let bytes = serialize_agent(&record)?
            table.insert(target_id, bytes.as_slice())?
        }
        txn.commit()?

        Ok(EnrollResult { created, agent: record })
```

## Error Handling

- Protected agent check: returns `ServerError::ProtectedAgent` -- terminates immediately before any DB access
- Self-lockout check: returns `ServerError::SelfLockout` -- terminates before any DB access
- Store read/write errors: propagated as `ServerError::Registry` via `.map_err(|e| ServerError::Registry(e.to_string()))?`
- Serialization errors: propagated via `serialize_agent` (existing helper)

## Key Test Scenarios

### Create path
- Target does not exist -> EnrollResult { created: true, agent with specified trust/caps }
- New agent has enrolled_at set to current time
- New agent has active = true

### Update path
- Target exists -> EnrollResult { created: false, agent with updated trust/caps }
- enrolled_at is preserved from original record
- active is preserved from original record
- allowed_topics/allowed_categories are preserved

### Protection
- target = "system" -> Err(ProtectedAgent)
- target = "human" -> Err(ProtectedAgent)
- target = "SYSTEM" -> succeeds (case-sensitive, "SYSTEM" is not "system")

### Self-lockout
- caller = target, capabilities does NOT include Admin -> Err(SelfLockout)
- caller = target, capabilities includes Admin -> Ok (allowed)
- caller != target -> no self-lockout check applies

### Sequential updates
- Enroll target twice with different trust levels -> second call's trust level wins
