# token-manager (C4) -- `src/http/token.rs`

## Purpose

Manages the bearer token lifecycle: generate on first run, load on subsequent runs, validate format. The token is a 32-byte cryptographic random value stored as 64 hex characters in `{data_dir}/token` with mode 0600.

## Constants

```
TOKEN_FILE_NAME: &str = "token"
TOKEN_BYTE_LEN: usize = 32
TOKEN_HEX_LEN: usize = 64   // TOKEN_BYTE_LEN * 2
```

## Functions

### `load_or_generate_token(data_dir: &Path) -> Result<Vec<u8>, ServerError>`

This is the single public entry point. Returns raw token bytes (32 bytes), not hex.

```
fn load_or_generate_token(data_dir: &Path) -> Result<Vec<u8>, ServerError>:
    let token_path = data_dir.join(TOKEN_FILE_NAME)

    if token_path.exists():
        return load_existing_token(&token_path)
    else:
        return generate_new_token(&token_path)
```

### `generate_new_token(path: &Path) -> Result<Vec<u8>, ServerError>`

```
fn generate_new_token(path: &Path) -> Result<Vec<u8>, ServerError>:
    // Generate 32 cryptographic random bytes
    let mut token_bytes = [0u8; TOKEN_BYTE_LEN]
    OsRng.fill_bytes(&mut token_bytes)

    // Hex-encode for storage
    let hex_string = hex::encode(&token_bytes)  // 64 chars

    // Write to file with restricted permissions
    // CRITICAL: Set permissions BEFORE writing content to prevent race condition
    // Use OpenOptions to create file, then set permissions, then write
    let file = File::create(path)?
    set_permissions(path, Permissions::from_mode(0o600))?
    file.write_all(hex_string.as_bytes())?

    // Print token to stdout exactly once (FR-08)
    println!("[UNIMATRIX TOKEN] {hex_string}")

    return Ok(token_bytes.to_vec())
```

### `load_existing_token(path: &Path) -> Result<Vec<u8>, ServerError>`

```
fn load_existing_token(path: &Path) -> Result<Vec<u8>, ServerError>:
    let content = fs::read_to_string(path)?

    // Strip trailing whitespace (R-15 mitigation: trailing newline tolerance)
    let trimmed = content.trim_end()

    // Validate format: exactly 64 hex characters
    validate_token_format(trimmed)?

    // Decode hex to bytes
    let token_bytes = hex::decode(trimmed)
        .map_err(|_| ServerError::Config("token file contains non-hex characters"))?

    // NO stdout output on load (FR-09)
    // Log at debug level only
    tracing::debug!("loaded existing bearer token from {}", path.display())

    return Ok(token_bytes)
```

### `validate_token_format(hex_str: &str) -> Result<(), ServerError>`

```
fn validate_token_format(hex_str: &str) -> Result<(), ServerError>:
    if hex_str.len() != TOKEN_HEX_LEN:
        return Err(ServerError::Config(
            format!("token file must contain exactly {TOKEN_HEX_LEN} hex characters, found {}", hex_str.len())
        ))

    // Validate all characters are hex digits
    if !hex_str.chars().all(|c| c.is_ascii_hexdigit()):
        return Err(ServerError::Config("token file contains non-hex characters"))

    return Ok(())
```

## Error Handling

| Error Case | Error Type | Caller Action |
|-----------|-----------|--------------|
| Cannot create token file | `ServerError::Io` | Server refuses to start |
| Cannot set file permissions | `ServerError::Io` | Server refuses to start |
| Token file unreadable | `ServerError::Io` | Server refuses to start |
| Token file wrong length | `ServerError::Config` | Server refuses to start with descriptive message |
| Token file non-hex chars | `ServerError::Config` | Server refuses to start with descriptive message |
| Token file with BOM | Caught by hex validation | Server refuses to start |

## Key Test Scenarios

1. **Generate on first run**: Call with empty data_dir. Verify file created with 64 hex chars, mode 0600, stdout contains `[UNIMATRIX TOKEN]`.
2. **Load existing valid token**: Create file with 64 hex chars. Call load. Verify same bytes returned, no stdout output.
3. **Reject short token**: File with 63 hex chars. Verify `ServerError::Config` with length message.
4. **Reject non-hex**: File with 64 chars including 'g'. Verify rejection.
5. **Trailing newline tolerance**: File with 64 hex chars + `\n`. Verify trimmed and accepted (R-15).
6. **File permissions**: After generate, verify `stat` shows 0600.
7. **Idempotent load**: Generate, then load from same path. Verify identical bytes.
