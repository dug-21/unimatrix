## ADR-002: Versioned Sentinel + Head-Check Fallback for Init Idempotency

### Context

`/unimatrix-init` must not append the CLAUDE.md block twice. SR-02 identifies that the idempotency check — searching for a sentinel string — depends on Claude reading the full CLAUDE.md, which may fail silently for large files or partial reads.

Two failure modes exist:
1. Large CLAUDE.md: Claude reads the beginning of a large file and misses the sentinel if it was appended at the end of a very long file
2. Misread: The model misidentifies the absence of the sentinel and proceeds to append

The SCOPE.md resolved decision specifies the sentinel as `<!-- unimatrix-init v1: DO NOT REMOVE THIS LINE -->` and notes that a version number enables future `--update` detection.

### Decision

Use a **paired versioned sentinel** (open + close comment) with a **head-check fallback** strategy:

**Open sentinel**: `<!-- unimatrix-init v1: DO NOT REMOVE THIS LINE -->`
**Close sentinel**: `<!-- end unimatrix-init v1 -->`

**Idempotency check algorithm** in SKILL.md:
1. Read CLAUDE.md (or confirm file does not exist)
2. Search for the string `unimatrix-init v1` anywhere in the file content
3. If found anywhere: report "already initialized (unimatrix-init v1 detected)" and STOP
4. If the file is large (> 200 lines), also explicitly check the last 30 lines as a secondary pass

**Head-check fallback**: The skill instruction tells Claude to check both the beginning AND end of CLAUDE.md for the sentinel when the file is long. This covers the case where prior initialization appended to a large file and the sentinel is at the end.

**Version number rationale**: The `v1` in the sentinel enables a future `/unimatrix-init --update` to:
- Find the block between `<!-- unimatrix-init v1: ... -->` and `<!-- end unimatrix-init v1 -->`
- Replace the entire block with an updated version
- Change the version to `v2` in the replacement

The paired sentinel (open + close) makes block replacement deterministic — the update path does not need to guess where the Unimatrix block ends.

### Consequences

- Double-initialization is prevented by the sentinel check for all normal cases
- Very large CLAUDE.md files (> 200 lines) get an explicit tail check, reducing the SR-02 risk
- The version number adds zero complexity to the current implementation and enables a future update path
- If a user manually deletes the sentinel comment, idempotency is lost — this is an acceptable edge case (the comment says "DO NOT REMOVE")
- The close sentinel (`<!-- end unimatrix-init v1 -->`) provides a clear block boundary for future `--update` and for human readers who want to understand the scope of the injected block
