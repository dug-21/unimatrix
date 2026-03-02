//! Graceful degradation event queue.
//!
//! When the UDS transport is unavailable, hook processes enqueue events
//! to JSONL files on disk. The queue supports file rotation, age-based
//! pruning, and replay through a transport.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::wire::HookRequest;

/// Maximum events per queue file before rotation.
const MAX_EVENTS_PER_FILE: usize = 1000;

/// Maximum number of queue files to keep on disk.
const MAX_QUEUE_FILES: usize = 10;

/// Pruning threshold: files older than 7 days are deleted.
const PRUNE_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// File name prefix for queue files.
const FILE_PREFIX: &str = "pending-";

/// File extension for queue files.
const FILE_EXTENSION: &str = ".jsonl";

/// JSONL event queue for graceful degradation.
///
/// When the server is unreachable, hook processes write events to
/// `pending-{timestamp}.jsonl` files. When the server becomes available,
/// events are replayed (oldest first) and processed files are deleted.
///
/// Disk usage is bounded by `MAX_QUEUE_FILES * MAX_EVENTS_PER_FILE` events
/// (approximately 5-10 MB maximum).
pub struct EventQueue {
    queue_dir: PathBuf,
}

impl EventQueue {
    /// Create a new `EventQueue` targeting the given directory.
    pub fn new(queue_dir: PathBuf) -> Self {
        Self { queue_dir }
    }

    /// Enqueue a request to the JSONL queue.
    ///
    /// Appends a single JSON line to the current queue file. Rotates to a
    /// new file when the current one reaches `MAX_EVENTS_PER_FILE` lines.
    /// Enforces `MAX_QUEUE_FILES` limit by deleting the oldest file if needed.
    pub fn enqueue(&self, request: &HookRequest) -> io::Result<()> {
        fs::create_dir_all(&self.queue_dir)?;

        // Best-effort pruning of old files
        let _ = self.prune();

        // Enforce file count limit
        self.enforce_file_limit()?;

        // Find a target file (existing with capacity, or create new)
        let target_file = self.find_or_create_target()?;

        // Serialize event as single JSON line
        let json_line = serde_json::to_string(request).map_err(|e| {
            io::Error::new(io::ErrorKind::InvalidData, format!("serialize failed: {e}"))
        })?;

        // Append to file
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&target_file)?;

        writeln!(file, "{json_line}")?;
        file.flush()?;
        Ok(())
    }

    /// Replay queued events through a transport.
    ///
    /// Reads queue files oldest-first, deserializes each line as a
    /// `HookRequest`, and sends via `fire_and_forget`. Malformed lines
    /// are skipped (not treated as errors). Files are deleted after
    /// processing. Returns the count of successfully replayed events.
    pub fn replay(
        &self,
        transport: &mut dyn crate::transport::Transport,
    ) -> io::Result<usize> {
        if !self.queue_dir.exists() {
            return Ok(0);
        }

        let files = self.list_queue_files()?;
        let mut replayed = 0;

        for file_path in &files {
            let content = fs::read_to_string(file_path)?;

            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                // Skip malformed lines (R-15)
                let request: HookRequest = match serde_json::from_str(line) {
                    Ok(req) => req,
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            file = %file_path.display(),
                            "skipping malformed queue line"
                        );
                        continue;
                    }
                };

                // Best-effort send (skip failures, continue with remaining)
                match transport.fire_and_forget(&request) {
                    Ok(()) => replayed += 1,
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            "failed to replay queued event, skipping"
                        );
                        continue;
                    }
                }
            }

            // Delete file after processing all lines
            fs::remove_file(file_path)?;
        }

        Ok(replayed)
    }

    /// Prune queue files older than `PRUNE_AGE` (7 days).
    ///
    /// Uses file modification time for age calculation.
    pub fn prune(&self) -> io::Result<()> {
        if !self.queue_dir.exists() {
            return Ok(());
        }

        let now = SystemTime::now();

        for entry in fs::read_dir(&self.queue_dir)? {
            let entry = entry?;
            let path = entry.path();

            if !self.is_queue_file(&path) {
                continue;
            }

            let metadata = match fs::metadata(&path) {
                Ok(m) => m,
                Err(_) => continue,
            };

            let modified = match metadata.modified() {
                Ok(m) => m,
                Err(_) => continue,
            };

            if let Ok(age) = now.duration_since(modified) {
                if age > PRUNE_AGE {
                    tracing::debug!(path = %path.display(), "pruning old queue file");
                    fs::remove_file(&path)?;
                }
            }
        }

        Ok(())
    }

    /// Check if there are queued events waiting for replay.
    pub fn has_pending(&self) -> bool {
        match self.list_queue_files() {
            Ok(files) => !files.is_empty(),
            Err(_) => false,
        }
    }

    // -- Internal helpers --

    /// Find the most recent queue file with capacity, or create a new one.
    fn find_or_create_target(&self) -> io::Result<PathBuf> {
        let files = self.list_queue_files()?;

        if let Some(latest) = files.last() {
            let content = fs::read_to_string(latest)?;
            let line_count = content.lines().filter(|l| !l.trim().is_empty()).count();

            if line_count < MAX_EVENTS_PER_FILE {
                return Ok(latest.clone());
            }
        }

        // Create new file with current timestamp (milliseconds for uniqueness)
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();

        let filename = format!("{FILE_PREFIX}{timestamp}{FILE_EXTENSION}");
        Ok(self.queue_dir.join(filename))
    }

    /// Enforce the maximum queue file count by deleting the oldest files.
    fn enforce_file_limit(&self) -> io::Result<()> {
        let mut files = self.list_queue_files()?;

        // Delete oldest files while at or over the limit
        while files.len() >= MAX_QUEUE_FILES {
            let oldest = files.remove(0);
            tracing::debug!(
                path = %oldest.display(),
                "removing oldest queue file (limit reached)"
            );
            fs::remove_file(&oldest)?;
        }

        Ok(())
    }

    /// List queue files sorted by filename (oldest first by timestamp).
    fn list_queue_files(&self) -> io::Result<Vec<PathBuf>> {
        if !self.queue_dir.exists() {
            return Ok(vec![]);
        }

        let mut result = vec![];
        for entry in fs::read_dir(&self.queue_dir)? {
            let entry = entry?;
            let path = entry.path();
            if self.is_queue_file(&path) {
                result.push(path);
            }
        }

        result.sort();
        Ok(result)
    }

    /// Check if a path matches the queue file naming pattern.
    fn is_queue_file(&self, path: &Path) -> bool {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        name.starts_with(FILE_PREFIX) && name.ends_with(FILE_EXTENSION)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    fn make_queue(dir: &std::path::Path) -> EventQueue {
        EventQueue::new(dir.to_path_buf())
    }

    #[test]
    fn event_queue_new() {
        let q = EventQueue::new(PathBuf::from("/tmp/test-queue"));
        assert!(!q.has_pending());
    }

    #[test]
    fn event_queue_enqueue_creates_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let q = make_queue(dir.path());

        q.enqueue(&HookRequest::Ping).unwrap();

        assert!(q.has_pending());
        let files = q.list_queue_files().unwrap();
        assert_eq!(files.len(), 1);
        let name = files[0].file_name().unwrap().to_str().unwrap();
        assert!(name.starts_with(FILE_PREFIX));
        assert!(name.ends_with(FILE_EXTENSION));
    }

    #[test]
    fn event_queue_enqueue_appends_to_existing() {
        let dir = tempfile::TempDir::new().unwrap();
        let q = make_queue(dir.path());

        q.enqueue(&HookRequest::Ping).unwrap();
        q.enqueue(&HookRequest::Ping).unwrap();

        let files = q.list_queue_files().unwrap();
        assert_eq!(files.len(), 1, "should reuse same file");

        let contents = fs::read_to_string(&files[0]).unwrap();
        let lines: Vec<&str> = contents.trim().lines().collect();
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn event_queue_creates_directory() {
        let dir = tempfile::TempDir::new().unwrap();
        let queue_dir = dir.path().join("sub").join("queue");
        let q = EventQueue::new(queue_dir.clone());

        q.enqueue(&HookRequest::Ping).unwrap();
        assert!(queue_dir.exists());
    }

    #[test]
    fn event_queue_has_pending_false_when_empty() {
        let dir = tempfile::TempDir::new().unwrap();
        let q = make_queue(dir.path());
        assert!(!q.has_pending());
    }

    #[test]
    fn event_queue_prune_empty_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let q = make_queue(dir.path());
        q.prune().unwrap();
    }

    #[test]
    fn event_queue_prune_nonexistent_dir() {
        let q = EventQueue::new(PathBuf::from("/tmp/nonexistent-eq-test-dir-99999"));
        q.prune().unwrap();
    }

    #[test]
    fn event_queue_file_rotation_at_limit() {
        let dir = tempfile::TempDir::new().unwrap();
        let q = make_queue(dir.path());

        // Create a file manually with MAX_EVENTS_PER_FILE lines
        fs::create_dir_all(dir.path()).unwrap();
        let full_file = dir.path().join("pending-0000000000001.jsonl");
        {
            let mut f = File::create(&full_file).unwrap();
            for _ in 0..MAX_EVENTS_PER_FILE {
                writeln!(f, r#"{{"type":"Ping"}}"#).unwrap();
            }
        }

        // Next enqueue should create a new file
        q.enqueue(&HookRequest::Ping).unwrap();

        let files = q.list_queue_files().unwrap();
        assert_eq!(files.len(), 2, "should have rotated to a new file");
    }

    #[test]
    fn event_queue_enforce_file_limit() {
        let dir = tempfile::TempDir::new().unwrap();
        let q = make_queue(dir.path());

        // Create MAX_QUEUE_FILES files, all with MAX_EVENTS_PER_FILE lines
        // so that none can be reused (forces new file creation).
        fs::create_dir_all(dir.path()).unwrap();
        for i in 0..MAX_QUEUE_FILES {
            let name = format!("pending-{i:013}.jsonl");
            let mut content = String::new();
            for _ in 0..MAX_EVENTS_PER_FILE {
                content.push_str("{\"type\":\"Ping\"}\n");
            }
            fs::write(dir.path().join(name), &content).unwrap();
        }

        assert_eq!(q.list_queue_files().unwrap().len(), MAX_QUEUE_FILES);

        // Enqueue should delete oldest to make room, then create new file
        q.enqueue(&HookRequest::Ping).unwrap();

        let files = q.list_queue_files().unwrap();
        assert_eq!(files.len(), MAX_QUEUE_FILES);
        // Oldest file (pending-0000000000000.jsonl) should be gone
        let names: Vec<String> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap().to_string())
            .collect();
        assert!(!names.contains(&"pending-0000000000000.jsonl".to_string()));
    }

    #[test]
    fn event_queue_replay_empty() {
        let dir = tempfile::TempDir::new().unwrap();
        let q = make_queue(dir.path());

        let mut transport = MockTransport::new();
        let count = q.replay(&mut transport).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn event_queue_replay_processes_and_deletes() {
        let dir = tempfile::TempDir::new().unwrap();
        let q = make_queue(dir.path());

        // Enqueue some events
        q.enqueue(&HookRequest::Ping).unwrap();
        q.enqueue(&HookRequest::Ping).unwrap();

        assert!(q.has_pending());

        let mut transport = MockTransport::new();
        let count = q.replay(&mut transport).unwrap();
        assert_eq!(count, 2);
        assert_eq!(transport.sent_count, 2);

        // Files should be deleted
        assert!(!q.has_pending());
    }

    #[test]
    fn event_queue_replay_skips_malformed_lines() {
        let dir = tempfile::TempDir::new().unwrap();
        fs::create_dir_all(dir.path()).unwrap();

        // Write a file with valid and invalid lines
        let file_path = dir.path().join("pending-0000000000001.jsonl");
        fs::write(
            &file_path,
            "{\"type\":\"Ping\"}\nnot-valid-json\n{\"type\":\"Ping\"}\n",
        )
        .unwrap();

        let q = make_queue(dir.path());
        let mut transport = MockTransport::new();
        let count = q.replay(&mut transport).unwrap();

        // Should replay 2 valid events, skip the malformed one
        assert_eq!(count, 2);
    }

    #[test]
    fn event_queue_replay_skips_empty_lines() {
        let dir = tempfile::TempDir::new().unwrap();
        fs::create_dir_all(dir.path()).unwrap();

        let file_path = dir.path().join("pending-0000000000001.jsonl");
        fs::write(
            &file_path,
            "{\"type\":\"Ping\"}\n\n  \n{\"type\":\"Ping\"}\n",
        )
        .unwrap();

        let q = make_queue(dir.path());
        let mut transport = MockTransport::new();
        let count = q.replay(&mut transport).unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn event_queue_list_ignores_non_queue_files() {
        let dir = tempfile::TempDir::new().unwrap();
        fs::create_dir_all(dir.path()).unwrap();

        // Create a queue file and a non-queue file
        fs::write(dir.path().join("pending-001.jsonl"), "").unwrap();
        fs::write(dir.path().join("other-file.txt"), "").unwrap();
        fs::write(dir.path().join("pending-002.jsonl"), "").unwrap();

        let q = make_queue(dir.path());
        let files = q.list_queue_files().unwrap();
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn event_queue_list_sorted_by_name() {
        let dir = tempfile::TempDir::new().unwrap();
        fs::create_dir_all(dir.path()).unwrap();

        fs::write(dir.path().join("pending-003.jsonl"), "").unwrap();
        fs::write(dir.path().join("pending-001.jsonl"), "").unwrap();
        fs::write(dir.path().join("pending-002.jsonl"), "").unwrap();

        let q = make_queue(dir.path());
        let files = q.list_queue_files().unwrap();
        assert_eq!(files.len(), 3);
        let names: Vec<String> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap().to_string())
            .collect();
        assert_eq!(names, vec![
            "pending-001.jsonl",
            "pending-002.jsonl",
            "pending-003.jsonl",
        ]);
    }

    #[test]
    fn event_queue_enqueue_serializes_session_register() {
        let dir = tempfile::TempDir::new().unwrap();
        let q = make_queue(dir.path());

        let req = HookRequest::SessionRegister {
            session_id: "sess-1".to_string(),
            cwd: "/workspace".to_string(),
            agent_role: Some("dev".to_string()),
            feature: None,
        };
        q.enqueue(&req).unwrap();

        let files = q.list_queue_files().unwrap();
        let content = fs::read_to_string(&files[0]).unwrap();
        assert!(content.contains("SessionRegister"));
        assert!(content.contains("sess-1"));
    }

    // -- Mock transport for replay tests --

    struct MockTransport {
        sent_count: usize,
    }

    impl MockTransport {
        fn new() -> Self {
            Self { sent_count: 0 }
        }
    }

    impl crate::transport::Transport for MockTransport {
        fn connect(&mut self) -> Result<(), crate::wire::TransportError> {
            Ok(())
        }

        fn request(
            &mut self,
            _req: &HookRequest,
            _timeout: std::time::Duration,
        ) -> Result<crate::wire::HookResponse, crate::wire::TransportError> {
            Ok(crate::wire::HookResponse::Ack)
        }

        fn fire_and_forget(
            &mut self,
            _req: &HookRequest,
        ) -> Result<(), crate::wire::TransportError> {
            self.sent_count += 1;
            Ok(())
        }

        fn disconnect(&mut self) {}

        fn is_connected(&self) -> bool {
            true
        }
    }
}
