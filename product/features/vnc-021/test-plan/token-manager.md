# Test Plan: Token Manager (`src/http/token.rs`)

Covers: C4 — Token file lifecycle (generate, load, validate format)
Risks: R-05 (file permissions), R-15 (format validation)

## Unit Tests

All tests target `load_or_generate_token(data_dir: &Path)`.

### T-TM-01: test_generate_token_creates_file_with_correct_length
- **Risk**: R-05
- **Arrange**: Create empty temp directory as data_dir
- **Act**: Call `load_or_generate_token(&temp_dir)`
- **Assert**: File `{temp_dir}/token` exists; contents are exactly 64 hex characters; decoded bytes are 32 bytes

### T-TM-02: test_generate_token_file_permissions_0600
- **Risk**: R-05
- **Arrange**: Create empty temp directory
- **Act**: Call `load_or_generate_token(&temp_dir)`
- **Assert**: `std::fs::metadata("{temp_dir}/token").permissions().mode() & 0o777 == 0o600`

### T-TM-03: test_generate_token_returns_raw_bytes
- **Risk**: R-05
- **Arrange**: Create empty temp directory
- **Act**: Call `load_or_generate_token(&temp_dir)`, capture returned `Vec<u8>`
- **Assert**: Returned bytes length is 32; hex-encoding the returned bytes matches file contents

### T-TM-04: test_load_existing_token_returns_same_bytes
- **Risk**: R-05
- **Arrange**: Write a known 64-char hex string to `{temp_dir}/token` with mode 0600
- **Act**: Call `load_or_generate_token(&temp_dir)`
- **Assert**: Returned bytes match the hex-decoded contents of the pre-written file; no new file created

### T-TM-05: test_reject_token_file_trailing_newline
- **Risk**: R-15
- **Arrange**: Write `"aa" * 32 + "\n"` (65 chars) to token file
- **Act**: Call `load_or_generate_token(&temp_dir)`
- **Assert**: Returns `Err` with descriptive error message mentioning format/length

### T-TM-06: test_reject_token_file_odd_length
- **Risk**: R-15
- **Arrange**: Write 63 hex characters to token file
- **Act**: Call `load_or_generate_token(&temp_dir)`
- **Assert**: Returns `Err` with descriptive error

### T-TM-07: test_reject_token_file_non_hex_characters
- **Risk**: R-15
- **Arrange**: Write 64 characters including 'g', 'z', '!' to token file
- **Act**: Call `load_or_generate_token(&temp_dir)`
- **Assert**: Returns `Err` with descriptive error

### T-TM-08: test_accept_token_file_exactly_64_hex_chars
- **Risk**: R-15
- **Arrange**: Write exactly 64 lowercase hex characters to token file
- **Act**: Call `load_or_generate_token(&temp_dir)`
- **Assert**: Returns `Ok` with correct 32-byte Vec

## Required Edge-Case Tests

### T-TM-09: test_generate_token_is_cryptographically_random
- **Arrange**: Create empty temp directory
- **Act**: Call `load_or_generate_token` twice in separate temp dirs
- **Assert**: The two generated tokens are different (probabilistic but overwhelming)

### T-TM-10: test_token_file_on_readonly_parent_dir
- **Arrange**: Create temp directory, make it read-only
- **Act**: Call `load_or_generate_token(&readonly_dir)`
- **Assert**: Returns `Err` (cannot write token file); does not panic

### T-TM-11: test_token_file_uppercase_hex_accepted
- **Risk**: R-15
- **Arrange**: Write 64 uppercase hex characters (e.g., "AA" * 32) to token file
- **Act**: Call `load_or_generate_token(&temp_dir)`
- **Assert**: Either accepted (case-insensitive) or rejected with clear error. Behavior must be defined and consistent with auth comparison path.

## AC Mapping

| AC-ID | Test(s) |
|-------|---------|
| AC-02 | T-TM-01, T-TM-02, T-TM-03 |
| AC-03 | T-TM-04 |
