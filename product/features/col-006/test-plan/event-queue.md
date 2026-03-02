# Test Plan: event-queue

## Risks Covered

| Risk | Severity | Test Coverage |
|------|----------|--------------|
| R-15 | Low | Queue file corruption recovery (malformed lines) |
| R-16 | Medium | Queue size limits enforcement (1000/file, 10 files) |
| R-17 | Medium | Pruning age correctness (7-day boundary) |

## Unit Tests

Location: `crates/unimatrix-engine/src/event_queue.rs` (within `#[cfg(test)]` module)

### Basic Operations

1. **test_enqueue_creates_directory**: `EventQueue::new(tempdir/event-queue)`. Directory does not exist. `enqueue(request)` succeeds. Directory and file exist.

2. **test_enqueue_creates_file_with_correct_name**: After enqueue, a file matching `pending-{timestamp}.jsonl` exists in the queue directory.

3. **test_enqueue_writes_valid_jsonl**: After enqueue, read the file. Parse the single line as JSON. Assert it matches the original `HookRequest`.

4. **test_enqueue_multiple_events_same_file**: Enqueue 3 events. All 3 are in the same file (< 1000 threshold). File has 3 lines.

5. **test_enqueue_appends_to_existing**: Enqueue once, read file (1 line). Enqueue again, read file (2 lines).

### File Rotation (R-16)

6. **test_rotation_at_1000_events**: Write 1000 events. Verify 1 file with 1000 lines. Write event 1001. Verify 2 files: first with 1000, second with 1.

7. **test_rotation_creates_new_timestamp**: Write 1000 events (file 1), then 1 more (file 2). File 2 has a different (later) timestamp in its name.

### File Count Limit (R-16)

8. **test_file_limit_at_10**: Create 10 queue files manually. Enqueue a new event. Verify oldest file was deleted. Total file count is still 10.

9. **test_file_limit_deletes_oldest**: Create files with timestamps t1 < t2 < ... < t10. Trigger limit enforcement. Verify t1 was deleted, t2-t10 remain.

10. **test_max_capacity_bounded**: With 10 files of 1000 events each, total capacity is ~10,000 events.

### Pruning (R-17)

11. **test_prune_removes_old_files**: Create a queue file. Set its modification time to 8 days ago. Call `prune()`. File is removed.

12. **test_prune_keeps_recent_files**: Create a queue file. Set its modification time to 6 days ago. Call `prune()`. File still exists.

13. **test_prune_boundary_7_days**: Create a file with modification time exactly 7 days ago. Document whether pruned or kept (implementation-dependent -- assert one or the other consistently).

14. **test_prune_empty_directory**: Call `prune()` on empty queue directory. No error.

15. **test_prune_nonexistent_directory**: Call `prune()` when queue directory does not exist. Returns `Ok(())`.

### Replay

16. **test_replay_empty_queue**: `replay()` on empty queue -> returns `Ok(0)`.

17. **test_replay_processes_events**: Enqueue 3 events. Create a mock Transport that counts calls. `replay(transport)` returns 3. All 3 events were sent via `fire_and_forget`.

18. **test_replay_deletes_files**: After successful replay, queue files are deleted.

19. **test_replay_skips_malformed_lines** (R-15): Write a queue file with 3 valid lines and 1 malformed line. `replay()` processes 3 events, skips 1. No error returned.

20. **test_replay_oldest_first**: Create two queue files with different timestamps. Replay processes the older file's events before the newer file's events.

21. **test_replay_partial_last_line**: Write a file with 2 complete lines and 1 truncated line (simulate crash). Replay processes 2 events, skips the partial line.

### Concurrent Access

22. **test_enqueue_from_multiple_threads**: Spawn 5 threads, each enqueuing 10 events. After all complete, verify total event count is 50 (no lost events, no corruption).

## Edge Cases

- Queue file with only whitespace lines -> skipped during replay
- Queue file with BOM (byte order mark) -> JSON parser handles or skips
- Queue directory contains non-JSONL files -> ignored by `list_queue_files`
- Clock skew: file modification time in the future -> not pruned (correct behavior)
- Filesystem full -> `enqueue` returns `io::Error`

## Assertions

- File names match pattern `pending-{digits}.jsonl`
- Line counts in files match expected counts
- Replay return value matches number of successfully processed events
- File deletion after replay confirmed via `!path.exists()`
- Pruning only removes files older than 7 days
- File count never exceeds MAX_QUEUE_FILES after enforcement
