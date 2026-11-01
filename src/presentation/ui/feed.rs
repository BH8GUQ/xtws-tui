//! Feed formatting and work-state analysis for the TUI.
//!
//! This module turns decoded FT8 text into display strings, call tags, and status
//! metadata such as NewCall, NewGrid, Working, and Worked.

use mfsk_core::msg::decoded::Decoded;
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct UiFeed {
    pub freq_mhz: f64,
    pub mode: String,
}

#[derive(Clone, Copy)]
pub enum ColorKey {
    NewCall,
    NewGrid,
    NewDxcc,
    Working,
    Worked,
}

#[derive(Clone)]
pub struct DecodeTag {
    pub label: String,
    pub color_key: ColorKey,
}

impl UiFeed {
    pub fn new(freq_mhz: f64, mode: String) -> Self {
        Self { freq_mhz, mode }
    }

    /// Returns the amateur band label for the configured frequency.
    pub fn band(&self) -> String {
        band_from_freq_mhz(self.freq_mhz)
    }

    /// Builds the header shown at the top of the TUI.
    pub fn header(&self, now: SystemTime) -> String {
        let secs = now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let rem = secs % 86_400;
        format!(
            "{:02}:{:02}:{:02} UTC | {} {} | q quit  ↑↓ select  ←→ pane  Enter reply",
            rem / 3600,
            (rem % 3600) / 60,
            rem % 60,
            self.band(),
            self.mode
        )
    }
}

/// Formats the separator line used between decoded slots in the UI feed.
pub fn format_divider_line(slot_index: u64, freq_mhz: f64, mode: &str) -> String {
    let secs = slot_index * 15;
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days as i64);
    format!(
        "--- {:04}-{:02}-{:02} - {:02}:{:02}:{:02} UTC - {} - {} ---",
        y,
        m,
        d,
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60,
        band_from_freq_mhz(freq_mhz),
        mode
    )
}

/// Formats a decoded FT8/FT4 message for display in the receive pane.
pub fn format_decode_line(slot_index: u64, d: &Decoded) -> String {
    format!(
        "{} {:.0} {:.1} {:.0} ~ {}",
        format_slot_hms(slot_index),
        d.snr_db,
        d.dt_sec,
        d.freq_hz,
        d.text
    )
}

/// Formats the transmit activity line which appears in the right pane.
pub fn format_tx_line(slot_index: u64, freq_hz: f32, text: &str) -> String {
    format!(
        "{} TX {:.1}Hz ~ {}",
        format_slot_hms(slot_index),
        freq_hz,
        text
    )
}

/// Converts a slot index into the HHMMSS timestamp used by the UI.
pub fn format_slot_hms(slot_index: u64) -> String {
    let secs = slot_index * 15;
    let rem = secs % 86_400;
    format!("{:02}{:02}{:02}", rem / 3600, (rem % 3600) / 60, rem % 60)
}

/// Returns true when a decoded transmission is directed at the current station.
pub fn is_message_addressed_to_me(text: &str, my_call: &str) -> bool {
    let p: Vec<&str> = text.split_whitespace().collect();
    if p.len() < 2 {
        return false;
    }
    if p[0] == "CQ" {
        return false;
    }
    p[1] == my_call
}

/// Extracts the remote call sign from a decoded line so the user can reply to it.
pub fn extract_reply_call_from_line(line: &str, my_call: &str) -> Option<String> {
    let after = line.split('~').nth(1)?;
    let p: Vec<&str> = after.split_whitespace().collect();
    if p.len() < 2 {
        return None;
    }
    if p[0] == my_call {
        return None;
    }
    if p[1] == my_call {
        return None;
    }
    Some(p[1].to_string())
}

/// Tags a decoded line with work-state metadata like NewCall, NewGrid, and Working.
pub fn tag_line_for_work_status(
    line: &str,
    my_call: &str,
    worked: &HashSet<String>,
    worked_grids: &HashSet<String>,
    worked_dxcc: &HashSet<String>,
    working: Option<&str>,
) -> Option<DecodeTag> {
    let after = line.split('~').nth(1)?;
    let p: Vec<&str> = after.split_whitespace().collect();
    if p.len() < 2 {
        return None;
    }
    if p[1] == my_call {
        return None;
    }
    let call = p[1].to_string();
    let is_cq = p[0] == "CQ";
    let grid = p
        .get(2)
        .filter(|s| is_maidenhead_grid(s))
        .map(|s| s.to_string());
    let dxcc = dxcc_prefix_for_call(&call);

    if !is_cq {
        if let Some(w) = working {
            if w == call {
                return Some(DecodeTag {
                    label: "Working".into(),
                    color_key: ColorKey::Working,
                });
            }
        }
    }

    if let Some(ref d) = dxcc {
        if !worked_dxcc.contains(d) {
            return Some(DecodeTag {
                label: "NEW dx".into(),
                color_key: ColorKey::NewDxcc,
            });
        }
    }
    if let Some(ref g) = grid {
        if !worked_grids.contains(g) {
            return Some(DecodeTag {
                label: "NEW grid".into(),
                color_key: ColorKey::NewGrid,
            });
        }
    }
    if !worked.contains(&call) {
        return Some(DecodeTag {
            label: "NEW call".into(),
            color_key: ColorKey::NewCall,
        });
    }
    Some(DecodeTag {
        label: String::new(),
        color_key: ColorKey::Worked,
    })
}

/// Returns whether the token looks like a valid Maidenhead grid square.
fn is_maidenhead_grid(s: &str) -> bool {
    match s {
        "RR73" | "RRR" | "73" => return false,
        _ => {}
    }
    if s.starts_with('R') && s.len() <= 4 {
        return false;
    }
    if (s.starts_with('+') || s.starts_with('-')) && s.len() <= 3 {
        return false;
    }
    let b = s.as_bytes();
    b.len() >= 4
        && (b'A'..=b'R').contains(&b[0])
        && (b'A'..=b'R').contains(&b[1])
        && b[2].is_ascii_digit()
        && b[3].is_ascii_digit()
}

/// Extracts the DXCC-style prefix from a callsign for worked-country tracking.
pub fn dxcc_prefix_for_call(call: &str) -> Option<String> {
    let base = call.split('/').next()?;
    let mut prefix = String::new();
    for ch in base.chars() {
        if ch.is_ascii_alphabetic() || ch.is_ascii_digit() {
            prefix.push(ch);
        }
        if ch.is_ascii_digit() {
            break;
        }
    }
    if prefix.is_empty() {
        None
    } else {
        Some(prefix)
    }
}

/// Maps a frequency in MHz to the corresponding amateur radio band label.
pub fn band_from_freq_mhz(freq_mhz: f64) -> String {
    if (1.8..=2.0).contains(&freq_mhz) {
        "160m".into()
    } else if (3.5..=4.0).contains(&freq_mhz) {
        "80m".into()
    } else if (5.3..=5.4).contains(&freq_mhz) {
        "60m".into()
    } else if (7.0..=7.3).contains(&freq_mhz) {
        "40m".into()
    } else if (10.1..=10.15).contains(&freq_mhz) {
        "30m".into()
    } else if (14.0..=14.35).contains(&freq_mhz) {
        "20m".into()
    } else if (18.068..=18.168).contains(&freq_mhz) {
        "17m".into()
    } else if (21.0..=21.45).contains(&freq_mhz) {
        "15m".into()
    } else if (24.89..=24.99).contains(&freq_mhz) {
        "12m".into()
    } else if (28.0..=29.7).contains(&freq_mhz) {
        "10m".into()
    } else if (50.0..=54.0).contains(&freq_mhz) {
        "6m".into()
    } else if (144.0..=148.0).contains(&freq_mhz) {
        "2m".into()
    } else if (430.0..=440.0).contains(&freq_mhz) {
        "70cm".into()
    } else {
        format!("{:.3} MHz", freq_mhz)
    }
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
