## ADR-004: Project Isolation via Path Hash

### Context

Each Unimatrix server instance serves one project. Project data must be isolated -- no cross-project leakage. Options for data directory naming:

1. **Hash of canonical path**: `SHA-256(canonical_project_root)[..16]` -> `~/.unimatrix/a1b2c3d4e5f6g7h8/`
2. **Project name**: `~/.unimatrix/my-project/` -- user-readable but collision-prone and path-insensitive
3. **UUID**: Random per-project -- no collisions but no determinism (can't re-detect)
4. **Symlink**: `~/.unimatrix/projects/my-project -> /actual/data/dir` -- complex lifecycle

### Decision

Use `SHA-256(canonical_project_root_path)[..16]` (first 16 hex characters of the hex digest).

Project root detection: walk up from cwd looking for `.git/` directory. If not found, use cwd itself. Canonicalize the detected path (resolve symlinks) before hashing.

Data directory: `~/.unimatrix/{hash}/` containing `unimatrix.redb` and `vector/`.

16 hex chars = 64 bits of hash = 2^64 possible values. Collision probability for 1M projects is ~0.000003%. This is safe.

### Consequences

- **Easier:** Deterministic -- same project always maps to same directory. Server can be restarted, machine rebooted, and data persists.
- **Easier:** Filesystem-safe -- hex chars only, fixed length, no spaces or special characters.
- **Easier:** Isolation guaranteed -- different project paths produce different hashes (barring the negligible collision probability).
- **Harder:** Not human-readable. `~/.unimatrix/a1b2c3d4e5f6g7h8/` tells you nothing about which project it belongs to. Future: a metadata file or registry can map hashes to project names (dsn-001).
- **Harder:** Moving a project directory changes its hash, creating a "new" project. Mitigated by: this is expected behavior -- the project's identity is its path. Future: dsn-001 can add explicit project registration that survives moves.
