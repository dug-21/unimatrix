# Pseudocode: project.rs (C4 — Project Manager)

## Purpose

Detects the project root, computes a deterministic project hash, and ensures the data directory exists. Standalone component with no server dependencies.

## Types

```
struct ProjectPaths {
    project_root: PathBuf,
    project_hash: String,       // 16 hex chars
    data_dir: PathBuf,          // ~/.unimatrix/{hash}/
    db_path: PathBuf,           // ~/.unimatrix/{hash}/unimatrix.redb
    vector_dir: PathBuf,        // ~/.unimatrix/{hash}/vector/
}
```

## Functions

### detect_project_root(override_dir: Option<&Path>) -> io::Result<PathBuf>

```
IF override_dir is Some(dir):
    canonicalize(dir)  // resolve symlinks
    RETURN canonicalized path

current = std::env::current_dir()?
LOOP:
    IF current.join(".git").is_dir():
        RETURN current.canonicalize()
    IF current.parent() is None:
        BREAK  // reached filesystem root
    current = current.parent()

// No .git found — use original cwd
RETURN std::env::current_dir()?.canonicalize()
```

### compute_project_hash(project_root: &Path) -> String

```
path_string = project_root.to_string_lossy()
hasher = Sha256::new()
hasher.update(path_string.as_bytes())
digest = hasher.finalize()
hex_string = hex::encode(digest)   // use format!("{:x}", digest) from sha2
RETURN hex_string[..16]            // first 16 hex chars
```

Note: Uses `sha2::Sha256` from the `sha2` crate. The `Digest` trait provides `.finalize()` returning `GenericArray`. Format as lowercase hex via `format!("{:x}", digest)`.

### ensure_data_directory(override_dir: Option<&Path>) -> io::Result<ProjectPaths>

```
project_root = detect_project_root(override_dir)?
project_hash = compute_project_hash(&project_root)

home = dirs::home_dir()
    .ok_or(io::Error::new(NotFound, "home directory not found"))?

data_dir = home.join(".unimatrix").join(&project_hash)
db_path = data_dir.join("unimatrix.redb")
vector_dir = data_dir.join("vector")

// Create directories recursively (idempotent)
fs::create_dir_all(&data_dir)?
fs::create_dir_all(&vector_dir)?

RETURN ProjectPaths { project_root, project_hash, data_dir, db_path, vector_dir }
```

## Error Handling

- `detect_project_root`: Returns `io::Error` if cwd is unresolvable or canonicalization fails
- `compute_project_hash`: Infallible (SHA-256 works on any byte input, to_string_lossy is infallible)
- `ensure_data_directory`: Returns `io::Error` for directory creation failure or missing home dir

## Key Test Scenarios

1. Detect root from nested dir with `.git/` at level 2 -- should find correct root
2. Detect root from dir with no `.git/` ancestors -- should return cwd
3. Override dir takes precedence over detection
4. Hash is deterministic: same path produces same 16-char hex string
5. Hash is exactly 16 characters of lowercase hex
6. Different paths produce different hashes
7. `ensure_data_directory` creates all dirs including parent `~/.unimatrix/`
8. `ensure_data_directory` is idempotent (calling twice is safe)
