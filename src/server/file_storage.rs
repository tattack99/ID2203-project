use omnipaxos::storage::{Entry, Storage};
use omnipaxos::util::LogEntry;

use omnipaxos_kv::common::kv::Command;

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
struct Persisted {
    log: Vec<LogEntry<Command>>,
    decided_idx: u64,
}

pub struct FileStorage {
    path: PathBuf,
    log: Vec<LogEntry<Command>>,
    decided_idx: u64,
}

impl FileStorage {
    pub fn new(path: PathBuf) -> Self {
        if path.exists() {
            let mut file = File::open(&path).unwrap();
            let mut buf = Vec::new();
            file.read_to_end(&mut buf).unwrap();

            let persisted: Persisted = bincode::deserialize(&buf).unwrap();

            Self {
                path,
                log: persisted.log,
                decided_idx: persisted.decided_idx,
            }
        } else {
            Self {
                path,
                log: Vec::new(),
                decided_idx: 0,
            }
        }
    }

    fn persist(&self) {
        let data = Persisted {
            log: self.log.clone(),
            decided_idx: self.decided_idx,
        };

        let encoded = bincode::serialize(&data).unwrap();

        let mut file = File::create(&self.path).unwrap();
        file.write_all(&encoded).unwrap();
        file.sync_all().unwrap();
    }
}

impl Storage<Command> for FileStorage {
    fn append_entry(&mut self, entry: LogEntry<Command>) {
        self.log.push(entry);
        self.persist();
    }

    fn append_entries(&mut self, entries: Vec<LogEntry<Command>>) {
        self.log.extend(entries);
        self.persist();
    }

    fn set_decided_idx(&mut self, idx: u64) {
        self.decided_idx = idx;
        self.persist();
    }

    fn get_decided_idx(&self) -> u64 {
        self.decided_idx
    }

    fn get_log_len(&self) -> u64 {
        self.log.len() as u64
    }

    fn get_entries(&self, from: u64, to: u64) -> Vec<LogEntry<Command>> {
        self.log[from as usize..to as usize].to_vec()
    }
}