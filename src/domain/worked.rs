//! Persistent tracking of worked callsigns and Maidenhead grids.
//!
//! The set is stored in text files so it survives restarts while still being fast to
//! query during QSO state evaluation and UI labeling.

use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

pub struct WorkedLog {
    path: PathBuf,
    set: HashSet<String>,
}

impl WorkedLog {
    pub fn load(path: PathBuf) -> std::io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut set = HashSet::new();
        if path.exists() {
            let f = fs::File::open(&path)?;
            for line in BufReader::new(f).lines().flatten() {
                let t = line.trim();
                if !t.is_empty() && !t.starts_with('#') {
                    set.insert(t.to_string());
                }
            }
        }
        Ok(Self { path, set })
    }

    pub fn contains(&self, call: &str) -> bool {
        self.set.contains(call)
    }

    pub fn insert(&mut self, call: &str) -> std::io::Result<()> {
        if call.is_empty() {
            return Ok(());
        }
        if !self.set.insert(call.to_string()) {
            return Ok(());
        }
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(f, "{}", call)?;
        Ok(())
    }

    pub fn set(&self) -> &HashSet<String> {
        &self.set
    }
}
