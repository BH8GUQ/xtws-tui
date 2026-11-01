//! QSO state machine and message parsing logic.
//!
//! This module tracks the active contact, decides whether to transmit or receive on
//! the next time slot, and turns decoded FT8 text into state transitions.

use mfsk_core::msg::decoded::Decoded;
use std::collections::{HashSet, VecDeque};

/// A next-step instruction for the runtime loop.
#[derive(Debug, Clone)]
pub enum Action {
    /// Wait for the next receive window.
    Receive,
    /// Transmit the provided FT8 message text.
    Transmit { text: String },
}

#[derive(Debug, Clone, PartialEq)]
enum QsoState {
    Idle,
    CqSent,
    ReportSent,
    Rr73Sent,
    ReplySent,
    RReportSent,
    Send73,
}

#[derive(Debug, Clone)]
struct Target {
    call: String,
    grid: Option<String>,
    their_snr: Option<i32>,
    my_snr: Option<i32>,
}

/// Summary record emitted when a QSO reaches a completed state.
#[derive(Debug, Clone)]
pub struct CompletedQso {
    /// Remote callsign.
    pub call: String,
    /// Optional remote Maidenhead square.
    pub grid: Option<String>,
    /// Their received SNR.
    pub their_snr: Option<i32>,
    /// Our reported SNR.
    pub my_snr: Option<i32>,
    /// Slot index where the QSO started.
    pub start_slot: u64,
}

/// Maintains the interaction state for a single QSO and decides what to transmit next.
pub struct QsoStateMachine {
    my_call: String,
    my_grid: String,
    max_retries: u32,
    auto_cq_attempts: u32,
    auto_reply_cq: bool,
    reply_b4: bool,
    idle_slots_before_cq: u32,

    state: QsoState,
    target: Option<Target>,
    tx_parity: Option<bool>,
    retries: u32,
    idle_slots: u32,
    worked: HashSet<String>,
    start_slot: u64,
    completed: VecDeque<CompletedQso>,
}

impl QsoStateMachine {
    /// Creates a new machine with the local station identity and tuning parameters.
    pub fn new(
        my_call: String,
        my_grid: String,
        max_retries: u32,
        auto_cq_attempts: u32,
        auto_reply_cq: bool,
        reply_b4: bool,
        idle_slots_before_cq: u32,
        worked: HashSet<String>,
    ) -> Self {
        Self {
            my_call,
            my_grid,
            max_retries,
            auto_cq_attempts,
            auto_reply_cq,
            reply_b4,
            idle_slots_before_cq,
            state: QsoState::Idle,
            target: None,
            tx_parity: None,
            retries: 0,
            idle_slots: 0,
            worked,
            start_slot: 0,
            completed: VecDeque::new(),
        }
    }

    /// Removes and returns the next completed QSO record, if one exists.
    pub fn take_completed(&mut self) -> Option<CompletedQso> {
        self.completed.pop_front()
    }

    /// Returns the current target callsign, if a QSO is active.
    pub fn target_call(&self) -> Option<&str> {
        self.target.as_ref().map(|t| t.call.as_str())
    }

    /// Starts a manual QSO against the provided station on the given receive slot.
    pub fn start_manual_qso(&mut self, call: String, rx_parity: bool, rx_slot: u64) {
        if call == self.my_call {
            return;
        }
        self.target = Some(Target {
            call,
            grid: None,
            their_snr: None,
            my_snr: None,
        });
        self.state = QsoState::ReplySent;
        self.tx_parity = Some(!rx_parity);
        self.retries = 0;
        self.start_slot = rx_slot;
    }

    fn reset(&mut self) {
        self.state = QsoState::Idle;
        self.target = None;
        self.tx_parity = None;
        self.retries = 0;
    }

    fn finish(&mut self) {
        if let Some(t) = self.target.take() {
            self.worked.insert(t.call.clone());
            self.completed.push_back(CompletedQso {
                call: t.call,
                grid: t.grid,
                their_snr: t.their_snr,
                my_snr: t.my_snr,
                start_slot: self.start_slot,
            });
        }
        self.reset();
    }

    fn retry_limit(&self) -> u32 {
        match self.state {
            QsoState::CqSent => self.auto_cq_attempts,
            _ => self.max_retries,
        }
    }

    /// Determines whether the runtime should receive or transmit in the next slot.
    pub fn next_action(&mut self, slot_parity: bool) -> Action {
        if self.state == QsoState::Idle {
            return Action::Receive;
        }
        let tx_p = match self.tx_parity {
            Some(p) => p,
            None => return Action::Receive,
        };
        if slot_parity != tx_p {
            return Action::Receive;
        }
        if self.retries >= self.retry_limit() {
            self.reset();
            return Action::Receive;
        }

        let text = match self.state {
            QsoState::CqSent => format!("CQ {} {}", self.my_call, self.my_grid),
            QsoState::ReplySent => {
                let t = self.target.as_ref().unwrap();
                format!("{} {} {}", t.call, self.my_call, self.my_grid)
            }
            QsoState::ReportSent => {
                let t = self.target.as_ref().unwrap();
                let r = t.their_snr.unwrap_or(-10);
                format!("{} {} {}", t.call, self.my_call, format_snr(r))
            }
            QsoState::RReportSent => {
                let t = self.target.as_ref().unwrap();
                let r = t.their_snr.unwrap_or(-10);
                format!("{} {} R{}", t.call, self.my_call, format_snr(r))
            }
            QsoState::Rr73Sent => {
                let t = self.target.as_ref().unwrap();
                let text = format!("{} {} RR73", t.call, self.my_call);
                self.retries += 1;
                self.finish();
                return Action::Transmit { text };
            }
            QsoState::Send73 => {
                let t = self.target.as_ref().unwrap();
                let text = format!("{} {} 73", t.call, self.my_call);
                self.retries += 1;
                self.finish();
                return Action::Transmit { text };
            }
            QsoState::Idle => return Action::Receive,
        };
        self.retries += 1;
        Action::Transmit { text }
    }

    /// Consumes decoded FT8 text and updates the internal state machine for the next interaction.
    pub fn on_decode(&mut self, results: &[Decoded], rx_parity: bool, rx_slot: u64) {
        if let Some(tx_p) = self.tx_parity {
            if rx_parity == tx_p {
                return;
            }
        }

        if results.is_empty() {
            self.idle_slots = self.idle_slots.saturating_add(1);
        } else {
            self.idle_slots = 0;
        }

        match self.state {
            QsoState::Idle => {
                if self.idle_slots_before_cq > 0 && self.idle_slots >= self.idle_slots_before_cq {
                    self.state = QsoState::CqSent;
                    self.tx_parity = Some(!rx_parity);
                    self.target = None;
                    self.retries = 0;
                    self.idle_slots = 0;
                    self.start_slot = rx_slot;
                    return;
                }
                if self.auto_reply_cq {
                    for d in results {
                        if let Some((call, grid)) = parse_cq(&d.text) {
                            if call == self.my_call {
                                continue;
                            }
                            if !self.reply_b4 && self.worked.contains(&call) {
                                continue;
                            }
                            self.target = Some(Target {
                                call,
                                grid: Some(grid),
                                their_snr: None,
                                my_snr: None,
                            });
                            self.state = QsoState::ReplySent;
                            self.tx_parity = Some(!rx_parity);
                            self.retries = 0;
                            self.start_slot = rx_slot;
                            return;
                        }
                    }
                }
            }
            QsoState::CqSent => {
                for d in results {
                    if let Some((call, grid)) = parse_me_other_grid(&d.text, &self.my_call) {
                        if !self.reply_b4 && self.worked.contains(&call) {
                            continue;
                        }
                        let their_snr = d.snr_db.round() as i32;
                        self.target = Some(Target {
                            call,
                            grid: Some(grid),
                            their_snr: Some(their_snr),
                            my_snr: None,
                        });
                        self.state = QsoState::ReportSent;
                        self.retries = 0;
                        return;
                    }
                }
            }
            QsoState::ReportSent => {
                for d in results {
                    if let Some((call, snr)) = parse_me_other_rreport(&d.text, &self.my_call) {
                        if let Some(ref mut t) = self.target {
                            if t.call == call {
                                t.my_snr = Some(snr);
                                self.state = QsoState::Rr73Sent;
                                self.retries = 0;
                                return;
                            }
                        }
                    }
                }
            }
            QsoState::ReplySent => {
                for d in results {
                    if let Some((call, snr)) = parse_me_other_report(&d.text, &self.my_call) {
                        if let Some(ref mut t) = self.target {
                            if t.call == call {
                                t.my_snr = Some(snr);
                                t.their_snr = Some(d.snr_db.round() as i32);
                                self.state = QsoState::RReportSent;
                                self.retries = 0;
                                return;
                            }
                        }
                    }
                }
            }
            QsoState::RReportSent => {
                for d in results {
                    if let Some(call) = parse_me_other_with(&d.text, &self.my_call, "RR73") {
                        if self
                            .target
                            .as_ref()
                            .map(|t| t.call == call)
                            .unwrap_or(false)
                        {
                            self.state = QsoState::Send73;
                            self.retries = 0;
                            return;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    pub fn state_name(&self) -> &'static str {
        match self.state {
            QsoState::Idle => "Idle",
            QsoState::CqSent => "CQ",
            QsoState::ReportSent => "Rpt",
            QsoState::Rr73Sent => "RR73",
            QsoState::ReplySent => "Reply",
            QsoState::RReportSent => "R-Rpt",
            QsoState::Send73 => "73",
        }
    }
}

pub fn format_snr(snr: i32) -> String {
    if snr >= 0 {
        format!("+{:02}", snr)
    } else {
        format!("-{:02}", -snr)
    }
}

fn parse_cq(text: &str) -> Option<(String, String)> {
    let p: Vec<&str> = text.split_whitespace().collect();
    if p.len() >= 3 && p[0] == "CQ" {
        Some((p[1].to_string(), p[2].to_string()))
    } else {
        None
    }
}

fn parse_me_other_grid(text: &str, my_call: &str) -> Option<(String, String)> {
    let p: Vec<&str> = text.split_whitespace().collect();
    if p.len() < 3 || p[0] != my_call || p[1] == my_call {
        return None;
    }
    Some((p[1].to_string(), p[2].to_string()))
}

fn parse_me_other_report(text: &str, my_call: &str) -> Option<(String, i32)> {
    let p: Vec<&str> = text.split_whitespace().collect();
    if p.len() < 3 || p[0] != my_call {
        return None;
    }
    if p[2].starts_with('R') {
        return None;
    }
    let snr = p[2].parse::<i32>().ok()?;
    Some((p[1].to_string(), snr))
}

fn parse_me_other_rreport(text: &str, my_call: &str) -> Option<(String, i32)> {
    let p: Vec<&str> = text.split_whitespace().collect();
    if p.len() < 3 || p[0] != my_call {
        return None;
    }
    let r = p[2].strip_prefix('R')?;
    let snr = r.parse::<i32>().ok()?;
    Some((p[1].to_string(), snr))
}

fn parse_me_other_with(text: &str, my_call: &str, token: &str) -> Option<String> {
    let p: Vec<&str> = text.split_whitespace().collect();
    if p.len() < 3 || p[0] != my_call || p[2] != token {
        return None;
    }
    Some(p[1].to_string())
}
