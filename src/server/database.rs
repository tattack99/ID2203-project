use log::*;
use omnipaxos_kv::common::kv::KVCommand;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;

/// Snapshot that gets persisted to disk.
/// Contains the full database state and the OmniPaxos decided index
/// so we know which log entries have already been applied.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DatabaseSnapshot {
    pub db: HashMap<String, String>,
    pub decided_idx: usize,
}

pub struct Database {
    db: HashMap<String, String>,
    snapshot_path: Option<String>,
}

impl Database {
    pub fn new() -> Self {
        Self {
            db: HashMap::new(),
            snapshot_path: None,
        }
    }

    /// Try to load a previously persisted snapshot from disk.
    /// Returns the Database with restored state and the decided_idx to resume from.
    pub fn recover(snapshot_path: &str) -> Option<(Self, usize)> {
        let path = Path::new(snapshot_path);
        if !path.exists() {
            info!("No snapshot found at {snapshot_path}, starting fresh.");
            return None;
        }
        match fs::read_to_string(path) {
            Ok(json) => match serde_json::from_str::<DatabaseSnapshot>(&json) {
                Ok(snap) => {
                    let entry_count = snap.db.len();
                    let decided_idx = snap.decided_idx;
                    info!(
                        "Recovered snapshot from {snapshot_path}: {entry_count} keys, decided_idx={decided_idx}"
                    );
                    let mut db = Database {
                        db: snap.db,
                        snapshot_path: Some(snapshot_path.to_string()),
                    };
                    // snapshot_path is set, so future saves will go here
                    Some((db, decided_idx))
                }
                Err(e) => {
                    warn!("Failed to parse snapshot at {snapshot_path}: {e}");
                    None
                }
            },
            Err(e) => {
                warn!("Failed to read snapshot at {snapshot_path}: {e}");
                None
            }
        }
    }

    /// Set the path where snapshots will be saved.
    pub fn set_snapshot_path(&mut self, path: String) {
        self.snapshot_path = Some(path);
    }

    /// Persist current database state + decided_idx to disk.
    /// Uses atomic write (write to temp file, then rename) to avoid corruption.
    pub fn save_snapshot(&self, decided_idx: usize) {
        let Some(ref path) = self.snapshot_path else {
            warn!("No snapshot path configured, skipping save.");
            return;
        };
        let snap = DatabaseSnapshot {
            db: self.db.clone(),
            decided_idx,
        };
        let json = match serde_json::to_string(&snap) {
            Ok(j) => j,
            Err(e) => {
                error!("Failed to serialize snapshot: {e}");
                return;
            }
        };
        // Atomic write: write to .tmp then rename
        let tmp_path = format!("{path}.tmp");
        match fs::File::create(&tmp_path) {
            Ok(mut f) => {
                if let Err(e) = f.write_all(json.as_bytes()) {
                    error!("Failed to write snapshot to {tmp_path}: {e}");
                    return;
                }
                if let Err(e) = f.flush() {
                    error!("Failed to flush snapshot {tmp_path}: {e}");
                    return;
                }
                // fsync for durability
                if let Err(e) = f.sync_all() {
                    error!("Failed to fsync snapshot {tmp_path}: {e}");
                    return;
                }
            }
            Err(e) => {
                error!("Failed to create snapshot file {tmp_path}: {e}");
                return;
            }
        }
        if let Err(e) = fs::rename(&tmp_path, path) {
            error!("Failed to rename snapshot {tmp_path} -> {path}: {e}");
            return;
        }
        debug!("Saved snapshot: decided_idx={decided_idx}, {} keys", self.db.len());
    }

    pub fn handle_command(&mut self, command: KVCommand) -> Option<Option<String>> {
        match command {
            KVCommand::Put(key, value) => {
                self.db.insert(key, value);
                None
            }
            KVCommand::Delete(key) => {
                self.db.remove(&key);
                None
            }
            KVCommand::Get(key) => Some(self.db.get(&key).map(|v| v.clone())),
        }
    }
}