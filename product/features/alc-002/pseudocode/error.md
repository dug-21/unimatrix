# Pseudocode: error component

## Purpose

Add two new `ServerError` variants and their corresponding MCP error codes for the enrollment tool's security boundaries.

## New Error Codes

```
// error.rs -- add after existing error code constants

/// MCP error code: protected bootstrap agent cannot be modified.
pub const ERROR_PROTECTED_AGENT: ErrorCode = ErrorCode(-32008);

/// MCP error code: caller cannot remove own Admin capability.
pub const ERROR_SELF_LOCKOUT: ErrorCode = ErrorCode(-32009);
```

NOTE: The brief says 32004/32005 but those are already used by ERROR_EMBED_NOT_READY and ERROR_NOT_IMPLEMENTED. Use -32008 and -32009 instead.

## New ServerError Variants

```
// Add to the ServerError enum:

/// Attempt to modify a protected bootstrap agent.
ProtectedAgent { agent_id: String },

/// Caller attempted to remove own Admin capability.
SelfLockout,
```

## Display Implementation

```
// Add to Display impl match arms:

ServerError::ProtectedAgent { agent_id } =>
    write!(f, "agent '{}' is a protected bootstrap agent and cannot be modified via enrollment", agent_id)

ServerError::SelfLockout =>
    write!(f, "cannot remove Admin capability from the calling agent")
```

## ErrorData Conversion

```
// Add to From<ServerError> for ErrorData match arms:

ServerError::ProtectedAgent { agent_id } => ErrorData::new(
    ERROR_PROTECTED_AGENT,
    format!("Agent '{}' is a protected bootstrap agent and cannot be modified via enrollment.", agent_id),
    None,
),

ServerError::SelfLockout => ErrorData::new(
    ERROR_SELF_LOCKOUT,
    "Cannot remove Admin capability from the calling agent. This would cause lockout.".to_string(),
    None,
),
```

## Error Handling

Both variants are terminal errors -- the operation is rejected and no state changes occur. The caller receives a clear MCP error with a unique error code.

## Key Test Scenarios

- ProtectedAgent variant Display does not leak Rust type names
- SelfLockout variant Display does not leak Rust type names
- ProtectedAgent maps to ERROR_PROTECTED_AGENT (-32008) ErrorCode
- SelfLockout maps to ERROR_SELF_LOCKOUT (-32009) ErrorCode
- ProtectedAgent ErrorData message contains the agent_id
- SelfLockout ErrorData message mentions "Admin" and "lockout"
