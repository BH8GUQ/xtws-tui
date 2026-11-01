//! Runtime log writer for RX/TX events and in-memory trace history.
//!
//! This module persists each decoded or transmitted slot into a day-based log and
//! keeps a bounded in-memory buffer for UI inspection and debugging.

use mfsk_core::msg::decoded::Decoded;
use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub struct RunLog {
    dir: PathBuf,
    keep_days: u32,
    mem_lines: usize,
    memory: VecDeque<String>,
    current_day: u32,
}

impl RunLog {
    pub fn new(dir: PathBuf, keep_days: u32, mem_lines: usize) -> std::io::Result<Self> {
        fs::create_dir_all(&dir)?;
        let today = day_number(SystemTime::now());
        let log = Self {
            dir,
            keep_days,
            mem_lines,
            memory: VecDeque::with_capacity(mem_lines),
            current_day: today,
        };
        log.purge_old()?;
        Ok(log)
    }

    pub fn append_decode(
        &mut self,
        slot_index: u64,
        parity: bool,
        d: &Decoded,
    ) -> std::io::Result<()> {
        let body = format!(
            "RX {:7.1}Hz dt={:+.2}s SNR={:+.0}dB {}",
            d.freq_hz, d.dt_sec, d.snr_db, d.text
        );
        self.push(slot_index, parity, &body)
    }

    pub fn append_tx(
        &mut self,
        slot_index: u64,
        parity: bool,
        freq_hz: f32,
        text: &str,
    ) -> std::io::Result<()> {
        self.push(
            slot_index,
            parity,
            &format!("TX {:7.1}Hz {}", freq_hz, text),
        )
    }

    pub fn append_event(
        &mut self,
        slot_index: u64,
        kind: &str,
        detail: &str,
    ) -> std::io::Result<()> {
        self.push(slot_index, false, &format!("EVT {} {}", kind, detail))
    }

    pub fn memory_snapshot(&self) -> Vec<String> {
        self.memory.iter().cloned().collect()
    }

    pub fn memory_len(&self) -> usize {
        self.memory.len()
    }

    fn push(&mut self, slot_index: u64, parity: bool, body: &str) -> std::io::Result<()> {
        let now = SystemTime::now();
        let today = day_number(now);
        if today != self.current_day {
            self.current_day = today;
            let _ = self.purge_old();
        }
        let ts = slot_time_utc(slot_index);
        let slot_bit = if parity { 0 } else { 1 };
        let line = format!("{} {} {}", ts, slot_bit, body);
        let path = self.dir.join(format!("run-{}.log", day_str(today)));
        let mut f = OpenOptions::new().create(true).append(true).open(&path)?;
        writeln!(f, "{}", line)?;
        f.flush()?;
        if self.memory.len() >= self.mem_lines {
            self.memory.pop_front();
        }
        self.memory.push_back(line);
        Ok(())
    }

    fn purge_old(&self) -> std::io::Result<()> {
        let today = day_number(SystemTime::now());
        let cutoff = today.saturating_sub(self.keep_days);
        let entries = match fs::read_dir(&self.dir) {
            Ok(e) => e,
            Err(_) => return Ok(()),
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(day) = parse_day_from_path(&path) {
                if day < cutoff {
                    let _ = fs::remove_file(&path);
                }
            }
        }
        Ok(())
    }
}

fn parse_day_from_path(path: &Path) -> Option<u32> {
    let name = path.file_name()?.to_str()?;
    let stem = name.strip_prefix("run-")?.strip_suffix(".log")?;
    let compact = stem.replace('-', "");
    let y: u32 = compact.get(0..4)?.parse().ok()?;
    let m: u32 = compact.get(4..6)?.parse().ok()?;
    let d: u32 = compact.get(6..8)?.parse().ok()?;
    let days = days_from_civil(y as i64, m, d);
    Some(days as u32)
}

fn day_number(now: SystemTime) -> u32 {
    let since = now.duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO);
    (since.as_secs() / 86_400) as u32
}

fn day_str(day: u32) -> String {
    let (y, m, d) = civil_from_days(day as i64);
    format!("{:04}-{:02}-{:02}", y, m, d)
}

fn slot_time_utc(slot_index: u64) -> String {
    let secs = slot_index * 15;
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days as i64);
    let hh = rem / 3600;
    let mm = (rem % 3600) / 60;
    let ss = rem % 60;
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
        y, m, d, hh, mm, ss
    )
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let mp = if m > 2 { m - 3 } else { m + 9 } as u64;
    let doy = (153 * mp + 2) / 5 + d as u64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe as i64 - 719_468
}
