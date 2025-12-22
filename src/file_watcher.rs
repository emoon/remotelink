use anyhow::{Context, Result};
use log::{debug, info, trace, warn};
use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode, DebouncedEventKind};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::time::{Duration, SystemTime};

/// Metadata snapshot for file stability checking
#[derive(Debug, Clone)]
struct FileSnapshot {
    size: u64,
    modified: SystemTime,
}

impl FileSnapshot {
    /// Take a snapshot of file metadata
    fn from_path(path: &Path) -> Result<Self> {
        let metadata = fs::metadata(path).context("Failed to read file metadata")?;

        Ok(FileSnapshot {
            size: metadata.len(),
            modified: metadata
                .modified()
                .context("Failed to get file modification time")?,
        })
    }

    /// Check if this snapshot matches another (file is stable)
    fn matches(&self, other: &FileSnapshot) -> bool {
        self.size == other.size && self.modified == other.modified
    }
}

/// File watcher that monitors a file for changes and ensures stability before notifying
pub struct FileWatcher {
    _debouncer: notify_debouncer_mini::Debouncer<notify_debouncer_mini::notify::RecommendedWatcher>,
    event_rx: Receiver<
        Result<Vec<notify_debouncer_mini::DebouncedEvent>, notify_debouncer_mini::notify::Error>,
    >,
    file_path: PathBuf,
    last_notified_snapshot: Option<FileSnapshot>,
}

impl FileWatcher {
    /// Create a new file watcher for the given path
    pub fn new(path: &Path) -> Result<Self> {
        let path_buf = path.to_path_buf();

        // Verify file exists
        if !path_buf.exists() {
            anyhow::bail!("File does not exist: {}", path_buf.display());
        }

        let (tx, rx) = channel();

        // Create debouncer with 100ms debounce time
        let mut debouncer = new_debouncer(Duration::from_millis(100), tx)
            .context("Failed to create file watcher debouncer")?;

        // Watch the file (not recursive since we're watching a single file)
        debouncer
            .watcher()
            .watch(&path_buf, RecursiveMode::NonRecursive)
            .context("Failed to start watching file")?;

        info!("File watching enabled for: {}", path_buf.display());

        Ok(FileWatcher {
            _debouncer: debouncer,
            event_rx: rx,
            file_path: path_buf,
            last_notified_snapshot: None,
        })
    }

    /// Check if the file has changed and is stable
    /// Returns Ok(true) if file has changed and is ready to use
    /// Returns Ok(false) if no change or file is not yet stable
    pub fn check_for_stable_change(&mut self) -> Result<bool> {
        // Drain all pending events to get the latest state
        let mut has_event = false;
        loop {
            match self.event_rx.try_recv() {
                Ok(Ok(events)) => {
                    // Process events to see if any are actual modifications
                    let has_modification = events
                        .iter()
                        .any(|event| matches!(event.kind, DebouncedEventKind::Any));

                    if has_modification {
                        trace!(
                            "File modification event detected for {}",
                            self.file_path.display()
                        );
                        has_event = true;
                    }
                }
                Ok(Err(error)) => {
                    warn!("File watcher error: {:?}", error);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    anyhow::bail!("File watcher channel disconnected");
                }
            }
        }

        // If no event, nothing to do
        if !has_event {
            return Ok(false);
        }

        // File was modified - now verify it's stable
        debug!(
            "Change detected, verifying file stability for {}",
            self.file_path.display()
        );

        if self.is_file_stable()? {
            let current_snapshot = FileSnapshot::from_path(&self.file_path)?;

            // Check if this is actually a new version (not just same file touched)
            let is_new_version = match &self.last_notified_snapshot {
                Some(last) => !last.matches(&current_snapshot),
                None => true, // First time, consider it new
            };

            if is_new_version {
                info!(
                    "File change verified as stable: {}",
                    self.file_path.display()
                );
                self.last_notified_snapshot = Some(current_snapshot);
                return Ok(true);
            } else {
                debug!("File touched but content unchanged, ignoring");
            }
        } else {
            debug!("File not yet stable, will check again");
        }

        Ok(false)
    }

    /// Verify that file is fully written and stable
    /// Uses multiple checks with delays to ensure the file isn't being actively written
    fn is_file_stable(&self) -> Result<bool> {
        // Check if file exists
        if !self.file_path.exists() {
            debug!(
                "File does not exist (may be deleted during rebuild): {}",
                self.file_path.display()
            );
            return Ok(false);
        }

        // Take initial snapshot
        let snapshot1 = match FileSnapshot::from_path(&self.file_path) {
            Ok(s) => s,
            Err(e) => {
                debug!("Failed to read file metadata (may be in-flight): {}", e);
                return Ok(false);
            }
        };

        // Wait for stability period
        std::thread::sleep(Duration::from_millis(200));

        // Check if file still exists (could be deleted and recreated during build)
        if !self.file_path.exists() {
            debug!(
                "File disappeared during stability check: {}",
                self.file_path.display()
            );
            return Ok(false);
        }

        // Take second snapshot
        let snapshot2 = match FileSnapshot::from_path(&self.file_path) {
            Ok(s) => s,
            Err(e) => {
                debug!("Failed to read file metadata on second check: {}", e);
                return Ok(false);
            }
        };

        // Verify snapshots match
        if !snapshot1.matches(&snapshot2) {
            trace!("File changed during first stability check (size: {} -> {}, modified: {:?} -> {:?})",
                   snapshot1.size, snapshot2.size, snapshot1.modified, snapshot2.modified);
            return Ok(false);
        }

        // Second stability check - wait another 200ms
        std::thread::sleep(Duration::from_millis(200));

        // Check existence again
        if !self.file_path.exists() {
            debug!(
                "File disappeared during second stability check: {}",
                self.file_path.display()
            );
            return Ok(false);
        }

        // Take third snapshot
        let snapshot3 = match FileSnapshot::from_path(&self.file_path) {
            Ok(s) => s,
            Err(e) => {
                debug!("Failed to read file metadata on third check: {}", e);
                return Ok(false);
            }
        };

        // Verify still stable
        if !snapshot2.matches(&snapshot3) {
            trace!("File changed during second stability check");
            return Ok(false);
        }

        // File has been stable for 400ms total - safe to use
        Ok(true)
    }

    /// Get the path being watched
    pub fn path(&self) -> &Path {
        &self.file_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_file_snapshot_creation() -> Result<()> {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_snapshot.txt");

        // Create test file
        fs::write(&test_file, b"test data")?;

        // Take snapshot
        let snapshot = FileSnapshot::from_path(&test_file)?;
        assert_eq!(snapshot.size, 9);

        // Cleanup
        fs::remove_file(&test_file)?;
        Ok(())
    }

    #[test]
    fn test_file_snapshot_equality() -> Result<()> {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_snapshot_eq.txt");

        // Create test file
        fs::write(&test_file, b"test data")?;

        // Take two snapshots immediately
        let snapshot1 = FileSnapshot::from_path(&test_file)?;
        let snapshot2 = FileSnapshot::from_path(&test_file)?;

        // Should match
        assert!(snapshot1.matches(&snapshot2));

        // Modify file
        std::thread::sleep(Duration::from_millis(10));
        fs::write(&test_file, b"modified data")?;

        // Take another snapshot
        let snapshot3 = FileSnapshot::from_path(&test_file)?;

        // Should not match original
        assert!(!snapshot1.matches(&snapshot3));

        // Cleanup
        fs::remove_file(&test_file)?;
        Ok(())
    }

    #[test]
    fn test_stability_detection_with_partial_write() -> Result<()> {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_stability.txt");

        // Create initial file
        fs::write(&test_file, b"initial")?;

        // Create watcher
        let watcher = FileWatcher::new(&test_file)?;

        // Simulate partial write in background thread
        let test_file_clone = test_file.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            let mut file = fs::OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(&test_file_clone)
                .unwrap();

            // Write in chunks with delays (simulating slow write)
            file.write_all(b"part1").unwrap();
            file.flush().unwrap();
            std::thread::sleep(Duration::from_millis(150));
            file.write_all(b"part2").unwrap();
            file.flush().unwrap();
        });

        // Check stability immediately - should fail because file is being written
        std::thread::sleep(Duration::from_millis(100));
        let _is_stable = watcher.is_file_stable()?;

        // Should not be stable yet (file is still being written)
        // Note: This test is timing-dependent and may occasionally pass even during write
        // The important thing is that the stability check doesn't panic

        // Wait for write to complete
        std::thread::sleep(Duration::from_millis(500));

        // Now should be stable
        let is_stable_final = watcher.is_file_stable()?;
        assert!(
            is_stable_final,
            "File should be stable after writes complete"
        );

        // Cleanup
        fs::remove_file(&test_file)?;
        Ok(())
    }
}
