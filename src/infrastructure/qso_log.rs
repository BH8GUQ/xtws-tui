//! ADIF-style QSO log writer for completed contacts.
//!
//! Contacts are stored per day in .adi files, with timestamp conversion and cleanup
//! logic provided here so higher-level code can focus on QSO flow.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub struct QsoRecord {
    pub call: String,
    pub gridsquare: Option<String>,
    pub mode: String,
    pub freq_mhz: f64,
    pub qso_date: String,
    pub time_on: String,
    pub rst_sent: String,
    pub rst_rcvd: String,
    pub station_callsign: String,
}

pub struct QsoLog {
    dir: PathBuf,
    keep_days: u32,
    last_purge_day: u32,
}

impl QsoLog {
    pub fn new(dir: PathBuf, keep_days: u32) -> std::io::Result<Self> {
        fs::create_dir_all(&dir)?;
        let today = day_number(SystemTime::now());
        let mut log = Self {
            dir,
            keep_days,
            last_purge_day: 0,
        };
        log.purge_old(today)?;
        log.last_purge_day = today;
        Ok(log)
    }

    pub fn append(&mut self, rec: &QsoRecord) -> std::io::Result<()> {
        let today = day_number(SystemTime::now());
        if today != self.last_purge_day {
            self.purge_old(today)?;
            self.last_purge_day = today;
        }
        let file_date = if rec.qso_date.len() == 8 {
            format!(
                "{}-{}-{}",
                &rec.qso_date[0..4],
                &rec.qso_date[4..6],
                &rec.qso_date[6..8]
            )
        } else {
            day_str(today)
        };
        let path = self.dir.join(format!("qso-{}.adi", file_date));
        let mut f = OpenOptions::new().create(true).append(true).open(&path)?;
        if f.metadata()?.len() == 0 {
            writeln!(f, "<adif_ver:5>3.1.4 <programid:8>jtrs-tui <eoh>")?;
        }
        let mut line = String::new();
        push_field(&mut line, "call", &rec.call);
        if let Some(ref g) = rec.gridsquare {
            push_field(&mut line, "gridsquare", g);
        }
        push_field(&mut line, "mode", &rec.mode);
        push_field(&mut line, "freq", &format!("{:.6}", rec.freq_mhz));
        push_field(&mut line, "qso_date", &rec.qso_date);
        push_field(&mut line, "time_on", &rec.time_on);
        push_field(&mut line, "rst_sent", &rec.rst_sent);
        push_field(&mut line, "rst_rcvd", &rec.rst_rcvd);
        push_field(&mut line, "station_callsign", &rec.station_callsign);
        line.push_str("<eor>");
        writeln!(f, "{}", line)?;
        Ok(())
    }

    fn purge_old(&self, today: u32) -> std::io::Result<()> {
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

pub fn slot_to_adif_dt(slot: u64) -> (String, String) {
    let secs = slot * 15;
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days as i64);
    let date = format!("{:04}{:02}{:02}", y, m, d);
    let time = format!(
        "{:02}{:02}{:02}",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    );
    (date, time)
}

fn push_field(out: &mut String, name: &str, value: &str) {
    out.push_str(&format!("<{}:{}>{} ", name, value.len(), value));
}

fn parse_day_from_path(path: &Path) -> Option<u32> {
    let name = path.file_name()?.to_str()?;
    let stem = name.strip_prefix("qso-")?.strip_suffix(".adi")?;
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