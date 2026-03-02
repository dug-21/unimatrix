# Pseudocode: event-queue

## Purpose

Implement local event queue for graceful degradation when the server is unavailable. Fire-and-forget events are written to JSONL files for future replay. Lives in `unimatrix-engine/src/event_queue.rs`.

## File: crates/unimatrix-engine/src/event_queue.rs

### Constants

```
MAX_EVENTS_PER_FILE: usize = 1000
MAX_QUEUE_FILES: usize = 10
PRUNE_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60)  // 7 days
QUEUE_DIR_NAME: &str = "event-queue"
FILE_PREFIX: &str = "pending-"
FILE_EXTENSION: &str = ".jsonl"
```

### EventQueue

```
struct EventQueue {
    queue_dir: PathBuf,
}

impl EventQueue {
    fn new(queue_dir: PathBuf) -> Self:
        Self { queue_dir }

    fn enqueue(&self, request: &HookRequest) -> io::Result<()>:
        // Ensure queue directory exists
        fs::create_dir_all(&self.queue_dir)?

        // Prune old files first (best-effort)
        let _ = self.prune()

        // Enforce file count limit
        self.enforce_file_limit()?

        // Find the most recent pending file, or create a new one
        let target_file = self.find_or_create_target()?

        // Serialize event as single JSON line
        let json_line = serde_json::to_string(request)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?

        // Append to file
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&target_file)?

        writeln!(file, "{json_line}")?
        file.flush()?

        Ok(())

    fn prune(&self) -> io::Result<()>:
        if !self.queue_dir.exists():
            return Ok(())

        let now = SystemTime::now()
        let entries = fs::read_dir(&self.queue_dir)?

        for entry in entries:
            let entry = entry?
            let path = entry.path()

            // Only process pending-*.jsonl files
            if !self.is_queue_file(&path):
                continue

            // Check file modification time
            let metadata = fs::metadata(&path)?
            let modified = metadata.modified()?
            if let Ok(age) = now.duration_since(modified):
                if age > PRUNE_AGE:
                    tracing::debug!(path = %path.display(), "pruning old queue file")
                    fs::remove_file(&path)?

        Ok(())

    fn replay(&self, transport: &mut dyn Transport) -> io::Result<usize>:
        if !self.queue_dir.exists():
            return Ok(0)

        let mut replayed = 0
        let mut files = self.list_queue_files()?

        // Sort by filename (timestamp-based, oldest first)
        files.sort()

        for file_path in &files:
            let content = fs::read_to_string(file_path)?

            for line in content.lines():
                let line = line.trim()
                if line.is_empty():
                    continue

                // Parse event (skip malformed lines -- R-15)
                let request: HookRequest = match serde_json::from_str(line):
                    Ok(req) => req,
                    Err(e) =>
                        tracing::warn!(
                            error = %e,
                            file = %file_path.display(),
                            "skipping malformed queue line"
                        )
                        continue

                // Best-effort send (skip failures, continue with remaining)
                match transport.fire_and_forget(&request):
                    Ok(()) => replayed += 1,
                    Err(e) =>
                        tracing::warn!(
                            error = %e,
                            "failed to replay queued event, skipping"
                        )
                        continue

            // Delete file after successful processing
            fs::remove_file(file_path)?

        Ok(replayed)
}
```

### Internal Helpers

```
impl EventQueue {
    fn find_or_create_target(&self) -> io::Result<PathBuf>:
        // List existing queue files
        let files = self.list_queue_files()?

        if let Some(latest) = files.last():
            // Check line count of the latest file
            let content = fs::read_to_string(latest)?
            let line_count = content.lines().filter(|l| !l.trim().is_empty()).count()

            if line_count < MAX_EVENTS_PER_FILE:
                return Ok(latest.clone())

        // Create new file with current timestamp
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()

        let filename = format!("{FILE_PREFIX}{timestamp}{FILE_EXTENSION}")
        Ok(self.queue_dir.join(filename))

    fn enforce_file_limit(&self) -> io::Result<()>:
        let mut files = self.list_queue_files()?
        files.sort()  // Oldest first by timestamp in filename

        // While we have too many files, delete the oldest
        while files.len() >= MAX_QUEUE_FILES:
            let oldest = files.remove(0)
            tracing::debug!(path = %oldest.display(), "removing oldest queue file (limit reached)")
            fs::remove_file(&oldest)?

        Ok(())

    fn list_queue_files(&self) -> io::Result<Vec<PathBuf>>:
        if !self.queue_dir.exists():
            return Ok(vec![])

        let mut result = vec![]
        for entry in fs::read_dir(&self.queue_dir)?:
            let entry = entry?
            let path = entry.path()
            if self.is_queue_file(&path):
                result.push(path)

        result.sort()  // Sort by filename (timestamp order)
        Ok(result)

    fn is_queue_file(&self, path: &Path) -> bool:
        let name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
        name.starts_with(FILE_PREFIX) && name.ends_with(FILE_EXTENSION)
}
```

## Design Notes

1. **JSONL format**: One serialized `HookRequest` per line. This is both the queue format and the unit of replay. Each line is independently parseable, so a crash mid-write only corrupts the last line (R-15).

2. **File rotation**: New file created when current file reaches 1000 lines. Filename encodes timestamp for ordering and pruning.

3. **File limit enforcement**: Before writing, check if file count >= MAX_QUEUE_FILES. If so, delete the oldest file. This ensures bounded disk usage (~10,000 events, ~5-10 MB max).

4. **Pruning**: Files older than 7 days are deleted on each enqueue call. Uses file modification time, not filename timestamp, to avoid clock skew issues (though both should be checked per R-17 edge case).

5. **Replay is best-effort**: Skip malformed lines, skip failed sends. Delete file after processing all lines (even if some were skipped). This is acceptable because queue events are telemetry, not critical data.

6. **No file locking**: JSONL appends are atomic up to PIPE_BUF (4096 bytes on Linux) for single-line writes. col-006 events are well under 4096 bytes. Concurrent hook processes appending to the same file will not interleave within a single line.

7. **Queue replay not invoked in col-006**: The `replay` method exists and is tested, but col-006 does not call it during server startup. col-010 will add the replay trigger.

## Error Handling

- `enqueue`: Directory creation failure -> io::Error. Serialization failure -> io::Error. File write failure -> io::Error. Prune failure -> swallowed (best-effort).
- `replay`: File read failure -> io::Error per file. Parse failure -> skip line (warning). Send failure -> skip event (warning).
- `prune`: Metadata read failure -> skip file. Remove failure -> io::Error.

## Key Test Scenarios

1. Enqueue single event creates queue file with correct name pattern
2. Enqueue 1001 events creates two files (first with 1000, second with 1)
3. Enqueue with 10 existing files deletes the oldest before creating new
4. Prune removes files older than 7 days
5. Prune does not remove files younger than 7 days
6. Replay processes events oldest-first, returns count
7. Replay skips malformed lines without failing
8. Replay deletes files after processing
9. Replay with empty queue returns 0
10. Queue directory created on first enqueue
11. Concurrent enqueue from two threads does not corrupt JSONL
12. File with partial last line (simulated crash) -- replay skips partial line
