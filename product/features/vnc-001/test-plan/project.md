# Test Plan: project.rs

## Risks Covered
- R-02: Project root detection failure (High)
- R-03: Project hash non-determinism (High)
- R-15: Data directory permission issues (High)

## Unit Tests

### detect_project_root

```
test_detect_root_from_nested_dir
  Arrange: create temp dir with .git/ at depth 2, set cwd to depth 4
  Act: detect_project_root(None)
  Assert: returns path to depth 2 (where .git/ is)

test_detect_root_from_git_parent
  Arrange: create temp dir with .git/ at root
  Act: detect_project_root(None) from that dir
  Assert: returns that directory

test_detect_root_no_git_fallback
  Arrange: create temp dir with NO .git/ anywhere
  Act: detect_project_root(None) from that dir
  Assert: returns cwd

test_detect_root_override
  Arrange: create temp dir
  Act: detect_project_root(Some(temp_dir))
  Assert: returns canonicalized temp_dir, ignoring cwd

test_detect_root_canonicalizes_path
  Arrange: create temp dir, create symlink to it
  Act: detect_project_root(Some(symlink_path))
  Assert: returns canonical (real) path, not symlink
```

### compute_project_hash

```
test_hash_deterministic
  Act: compute_project_hash(path) twice with same path
  Assert: both results identical

test_hash_is_16_hex_chars
  Act: compute_project_hash(any_path)
  Assert: result.len() == 16, all chars in [0-9a-f]

test_hash_different_paths_different_hashes
  Act: hash("/path/a"), hash("/path/b")
  Assert: results are different

test_hash_uses_sha256
  Act: compute manually with sha2::Sha256, compare with function output
  Assert: match
```

### ensure_data_directory

```
test_ensure_creates_dirs
  Arrange: set override to temp dir, ensure HOME is set to a temp dir
  Act: ensure_data_directory(Some(temp_dir))
  Assert: data_dir exists, vector_dir exists, db_path parent exists

test_ensure_idempotent
  Act: call ensure_data_directory twice with same input
  Assert: second call succeeds, no error

test_ensure_returns_correct_paths
  Act: ensure_data_directory(Some(temp_dir))
  Assert: db_path ends with "unimatrix.redb", vector_dir ends with "vector"

test_ensure_creates_parent_dirs
  Arrange: HOME dir exists but ~/.unimatrix/ does NOT
  Act: ensure_data_directory(...)
  Assert: ~/.unimatrix/{hash}/ created recursively
```

## Edge Cases (from Risk Strategy)

```
test_hash_unicode_path
  Act: compute_project_hash(Path::new("/tmp/test-unicode"))
  Assert: returns valid 16-char hex (SHA-256 handles UTF-8)

test_hash_long_path
  Act: compute_project_hash with 1000+ char path
  Assert: returns valid 16-char hex
```
