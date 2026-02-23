# Test Plan: content-hash

## Scope

Verify compute_content_hash() produces correct SHA-256 hex output for all input combinations.

## Unit Tests (in crates/unimatrix-store/src/hash.rs)

### test_content_hash_known_value
- `compute_content_hash("Test", "Content")` must equal SHA-256 hex of `"Test: Content"`.
- Independently compute SHA-256 of the byte string `b"Test: Content"` and assert equality.
- Expected: 64-character lowercase hex string.

### test_content_hash_empty_title
- `compute_content_hash("", "Content")` must equal SHA-256 hex of `"Content"`.
- Verify the separator is NOT included when title is empty.

### test_content_hash_empty_content
- `compute_content_hash("Title", "")` must equal SHA-256 hex of `"Title"`.
- Verify the separator is NOT included when content is empty.

### test_content_hash_both_empty
- `compute_content_hash("", "")` must equal SHA-256 hex of `""`.
- Assert output is the well-known SHA-256 of empty string: `"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"`.

### test_content_hash_unicode
- `compute_content_hash("Unicode Title", "CJK content")` with Unicode characters.
- Assert output is 64 characters, all lowercase hex.
- Assert determinism: calling twice returns same result.

### test_content_hash_determinism
- Call `compute_content_hash("Same", "Input")` 100 times.
- Assert all results are identical.

### test_content_hash_format
- Assert output is exactly 64 characters long.
- Assert all characters are in `[0-9a-f]`.

### test_content_hash_matches_prepare_text
- Import `unimatrix_embed::prepare_text`.
- For inputs ("Title", "Content"):
  - `prepare_text("Title", "Content", ": ")` produces a string.
  - SHA-256 of that string must equal `compute_content_hash("Title", "Content")`.
- This validates cross-crate alignment (IR-04).

## Risk Coverage

| Risk | Covered By |
|------|-----------|
| R-02 | test_content_hash_known_value, test_content_hash_empty_title, test_content_hash_empty_content, test_content_hash_both_empty, test_content_hash_matches_prepare_text |
| EC-06 | test_content_hash_unicode |

## AC Coverage

| AC | Covered By |
|----|-----------|
| AC-18 | test_content_hash_known_value, test_content_hash_matches_prepare_text |
