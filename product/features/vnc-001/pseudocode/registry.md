# Pseudocode: registry.rs (C5 — Agent Registry)

## Purpose

Manages agent identity, trust levels, and capabilities using the AGENT_REGISTRY redb table. Provides the query interface that vnc-002's enforcement points call.

## Types

```
struct AgentRegistry {
    store: Arc<Store>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentRecord {
    agent_id: String,
    trust_level: TrustLevel,
    capabilities: Vec<Capability>,
    allowed_topics: Option<Vec<String>>,
    allowed_categories: Option<Vec<String>>,
    enrolled_at: u64,
    last_seen_at: u64,
    active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum TrustLevel { System, Privileged, Internal, Restricted }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum Capability { Read, Write, Search, Admin }
```

## Serialization

AgentRecord uses bincode v2 serde path (same as EntryRecord in unimatrix-store):
- `bincode::serde::encode_to_vec(record, bincode::config::standard())`
- `bincode::serde::decode_from_slice::<AgentRecord, _>(bytes, bincode::config::standard())`

## Functions

### AgentRegistry::new(store: Arc<Store>) -> Result<Self, ServerError>

```
RETURN AgentRegistry { store }
```

Construction is cheap. Bootstrap is a separate step so callers can control when it runs.

### AgentRegistry::bootstrap_defaults(&self) -> Result<(), ServerError>

```
txn = self.store.db.begin_write()?
{
    table = txn.open_table(AGENT_REGISTRY)?

    // Only bootstrap if "system" agent doesn't exist yet
    IF table.get("system")? is None:
        system_agent = AgentRecord {
            agent_id: "system",
            trust_level: System,
            capabilities: [Read, Write, Search, Admin],
            allowed_topics: None,
            allowed_categories: None,
            enrolled_at: current_unix_seconds(),
            last_seen_at: current_unix_seconds(),
            active: true,
        }
        bytes = serialize(system_agent)?
        table.insert("system", bytes)?

    IF table.get("human")? is None:
        human_agent = AgentRecord {
            agent_id: "human",
            trust_level: Privileged,
            capabilities: [Read, Write, Search, Admin],
            allowed_topics: None,
            allowed_categories: None,
            enrolled_at: current_unix_seconds(),
            last_seen_at: current_unix_seconds(),
            active: true,
        }
        bytes = serialize(human_agent)?
        table.insert("human", bytes)?
}
txn.commit()?
```

Key: checks for existing agents BEFORE inserting. Idempotent -- safe to call on every startup.

### AgentRegistry::resolve_or_enroll(&self, agent_id: &str) -> Result<AgentRecord, ServerError>

```
// First: try read-only lookup
read_txn = self.store.db.begin_read()?
{
    table = read_txn.open_table(AGENT_REGISTRY)?
    IF let Some(guard) = table.get(agent_id)?:
        record = deserialize(guard.value())?
        RETURN Ok(record)
}
// drop read_txn

// Not found: auto-enroll as Restricted
write_txn = self.store.db.begin_write()?
{
    table = write_txn.open_table(AGENT_REGISTRY)?

    // Double-check (another thread may have enrolled between read and write)
    IF let Some(guard) = table.get(agent_id)?:
        record = deserialize(guard.value())?
        write_txn.commit()?  // no-op write, just release
        RETURN Ok(record)

    new_agent = AgentRecord {
        agent_id: agent_id.to_string(),
        trust_level: Restricted,
        capabilities: [Read, Search],
        allowed_topics: None,
        allowed_categories: None,
        enrolled_at: current_unix_seconds(),
        last_seen_at: current_unix_seconds(),
        active: true,
    }
    bytes = serialize(new_agent)?
    table.insert(agent_id, bytes)?
}
write_txn.commit()?
RETURN Ok(new_agent)
```

Key: read-first optimization avoids write transactions for known agents. Double-check pattern prevents duplicate enrollment under concurrency.

### AgentRegistry::has_capability(&self, agent_id: &str, cap: Capability) -> Result<bool, ServerError>

```
read_txn = self.store.db.begin_read()?
table = read_txn.open_table(AGENT_REGISTRY)?
guard = table.get(agent_id)?
    .ok_or(ServerError::Registry(format!("agent '{agent_id}' not found")))?
record = deserialize(guard.value())?
RETURN Ok(record.capabilities.contains(&cap))
```

### AgentRegistry::require_capability(&self, agent_id: &str, cap: Capability) -> Result<(), ServerError>

```
IF NOT self.has_capability(agent_id, cap)?:
    RETURN Err(ServerError::CapabilityDenied { agent_id: agent_id.to_string(), capability: cap })
Ok(())
```

### AgentRegistry::update_last_seen(&self, agent_id: &str) -> Result<(), ServerError>

```
write_txn = self.store.db.begin_write()?
{
    table = write_txn.open_table(AGENT_REGISTRY)?
    guard = table.get(agent_id)?
    IF let Some(guard) = guard:
        record = deserialize(guard.value())?
        drop(guard)  // release borrow before insert
        updated = AgentRecord { last_seen_at: current_unix_seconds(), ..record }
        bytes = serialize(updated)?
        table.insert(agent_id, bytes)?
}
write_txn.commit()?
Ok(())
```

## Helper: current_unix_seconds() -> u64

```
std::time::SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap_or_default()
    .as_secs()
```

## Helper: serialize/deserialize AgentRecord

Private functions mirroring the pattern from unimatrix-store's schema.rs:
```
fn serialize_agent(record: &AgentRecord) -> Result<Vec<u8>, ServerError>
fn deserialize_agent(bytes: &[u8]) -> Result<AgentRecord, ServerError>
```

Both use `bincode::serde::encode_to_vec` / `decode_from_slice` with `bincode::config::standard()`.

## Error Handling

- All redb operations mapped to `ServerError::Registry(msg)` via `.map_err(|e| ServerError::Registry(e.to_string()))`
- Deserialization failures also map to `ServerError::Registry`
- `require_capability` returns `ServerError::CapabilityDenied` (specific error variant)

## Key Test Scenarios

1. bootstrap_defaults creates "human" and "system" on fresh database
2. bootstrap_defaults is idempotent (second call does not overwrite)
3. resolve_or_enroll returns existing agent unchanged
4. resolve_or_enroll auto-enrolls unknown agent as Restricted with [Read, Search]
5. has_capability returns true for capabilities the agent has
6. has_capability returns false for capabilities the agent lacks
7. require_capability returns Ok for permitted capability
8. require_capability returns CapabilityDenied for denied capability
9. update_last_seen updates timestamp without changing other fields
10. AgentRecord serialization round-trips correctly through bincode
