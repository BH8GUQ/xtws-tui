//! QSO state machine and message parsing logic.
//!
//! This module tracks the active contact, decides whether to transmit or receive on
//! the next time slot, and turns decoded FT8 text into state transitions.

use mfsk_core::msg::decoded::Decoded;
use std::collections::{HashSet, VecDeque};

/// A next-step instruction for the runtime loop.
#[derive(Debug, Clone, PartialEq)]
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
    auto_reply_new_grid: bool,
    auto_reply_new_dxcc: bool,
    reply_b4: bool,
    idle_slots_before_cq: u32,

    state: QsoState,
    target: Option<Target>,
    tx_parity: Option<bool>,
    manual_cq_active: bool,
    retries: u32,
    idle_slots: u32,
    worked: HashSet<String>,
    worked_grids: HashSet<String>,
    worked_dxcc: HashSet<String>,
    start_slot: u64,
    completed: VecDeque<CompletedQso>,
}

impl QsoStateMachine {
    /// Creates a new machine with the local station identity and tuning parameters.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        my_call: String,
        my_grid: String,
        max_retries: u32,
        auto_cq_attempts: u32,
        auto_reply_cq: bool,
        auto_reply_new_grid: bool,
        auto_reply_new_dxcc: bool,
        reply_b4: bool,
        idle_slots_before_cq: u32,
        worked: HashSet<String>,
        worked_grids: HashSet<String>,
        worked_dxcc: HashSet<String>,
    ) -> Self {
        Self {
            my_call,
            my_grid,
            max_retries,
            auto_cq_attempts,
            auto_reply_cq,
            auto_reply_new_grid,
            auto_reply_new_dxcc,
            reply_b4,
            idle_slots_before_cq,
            state: QsoState::Idle,
            target: None,
            tx_parity: None,
            manual_cq_active: false,
            retries: 0,
            idle_slots: 0,
            worked,
            worked_grids,
            worked_dxcc,
            start_slot: 0,
            completed: VecDeque::new(),
        }
    }

    /// Removes and returns the next completed QSO record, if one exists.
    pub fn take_completed(&mut self) -> Option<CompletedQso> {
        self.completed.pop_front()
    }

    /// Records a grid square as worked so future CQ evaluation sees it as old.
    pub fn insert_worked_grid(&mut self, grid: String) {
        if !grid.is_empty() {
            self.worked_grids.insert(grid);
        }
    }

    /// Records a DXCC entity as worked so future CQ evaluation sees it as old.
    pub fn insert_worked_dxcc(&mut self, dxcc: String) {
        if !dxcc.is_empty() {
            self.worked_dxcc.insert(dxcc);
        }
    }

    /// Returns the current target callsign while the station is actively in a QSO.
    pub fn target_call(&self) -> Option<&str> {
        self.target.as_ref().map(|t| t.call.as_str())
    }

    /// Returns the active working target only after the station has already sent a reply.
    pub fn working_target_call(&self) -> Option<&str> {
        match self.state {
            QsoState::Idle | QsoState::ReplySent | QsoState::CqSent => None,
            QsoState::ReportSent
            | QsoState::Rr73Sent
            | QsoState::RReportSent
            | QsoState::Send73 => self.target.as_ref().map(|t| t.call.as_str()),
        }
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
        self.manual_cq_active = false;
        self.retries = 0;
        self.start_slot = rx_slot;
    }

    /// Starts or resumes a manual CQ burst on the configured TX parity.
    pub fn start_manual_cq(&mut self, tx_slot_parity: bool) {
        self.state = QsoState::CqSent;
        self.target = None;
        self.tx_parity = Some(tx_slot_parity);
        self.manual_cq_active = true;
        self.retries = 0;
        self.idle_slots = 0;
        self.start_slot = 0;
    }

    pub fn stop_manual_cq(&mut self) {
        self.state = QsoState::Idle;
        self.target = None;
        self.tx_parity = None;
        self.manual_cq_active = false;
        self.retries = 0;
        self.idle_slots = 0;
        self.start_slot = 0;
    }

    pub fn is_manual_cq_active(&self) -> bool {
        self.manual_cq_active
    }

    fn reset(&mut self) {
        self.state = QsoState::Idle;
        self.target = None;
        self.tx_parity = None;
        self.manual_cq_active = false;
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

        // next_action early-returns on the non-TX parity, so an exhausted CQ never
        // gets cleared there. Drop it here before matching so a fresh CQ that lands
        // in this frame can still trigger auto-reply on the current slot.
        if self.state == QsoState::CqSent && self.retries >= self.retry_limit() {
            self.reset();
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
                    self.manual_cq_active = true;
                    self.target = None;
                    self.retries = 0;
                    self.idle_slots = 0;
                    self.start_slot = rx_slot;
                    return;
                }
                if !self.manual_cq_active
                    && (self.auto_reply_cq || self.auto_reply_new_grid || self.auto_reply_new_dxcc)
                {
                    if let Some((call, grid)) = self.best_cq_candidate(results) {
                        self.target = Some(Target {
                            call,
                            grid: Some(grid),
                            their_snr: None,
                            my_snr: None,
                        });
                        self.state = QsoState::ReplySent;
                        self.tx_parity = Some(!rx_parity);
                        self.manual_cq_active = false;
                        self.retries = 0;
                        self.start_slot = rx_slot;
                        return;
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
                        self.manual_cq_active = false;
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

    /// Picks the highest-priority CQ to auto-reply to.
    ///
    /// Priority: new DXCC (0) > new grid (1) > new call (2). A lower number wins.
    /// Returns `None` when no candidate satisfies any enabled rule.
    fn best_cq_candidate(&self, results: &[Decoded]) -> Option<(String, String)> {
        let mut best: Option<(u8, String, String)> = None;
        for d in results {
            let Some((call, grid)) = parse_cq(&d.text) else {
                continue;
            };
            if call == self.my_call {
                continue;
            }
            if !self.reply_b4 && self.worked.contains(&call) {
                continue;
            }

            let is_new_dxcc = crate::domain::dxcc::entity_for_call(&call)
                .map(|e| !self.worked_dxcc.contains(&e))
                .unwrap_or(false);
            let is_new_grid = is_maidenhead_grid(&grid) && !self.worked_grids.contains(&grid);

            let priority = if is_new_dxcc && self.auto_reply_new_dxcc {
                0u8
            } else if is_new_grid && self.auto_reply_new_grid {
                1u8
            } else if self.auto_reply_cq {
                2u8
            } else {
                continue;
            };

            let better = match &best {
                None => true,
                Some((p, _, _)) => priority < *p,
            };
            if better {
                best = Some((priority, call, grid));
            }
        }
        best.map(|(_, call, grid)| (call, grid))
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

/// Returns whether the token looks like a valid Maidenhead grid square.
///
/// Mirrors the UI helper in `presentation::ui::feed` so the domain layer stays
/// independent of the presentation layer.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make(cq: bool, new_grid: bool, new_dxcc: bool) -> QsoStateMachine {
        QsoStateMachine::new(
            "JA1ABC".to_string(),
            "PM95".to_string(),
            3,
            1,
            cq,
            new_grid,
            new_dxcc,
            false,
            0,
            HashSet::new(),
            HashSet::new(),
            HashSet::new(),
        )
    }

    #[test]
    fn manual_cq_schedules_tx_for_configured_slot_parity() {
        let mut qso = make(false, false, false);
        qso.start_manual_cq(true);
        assert_eq!(qso.state_name(), "CQ");
        assert!(
            matches!(qso.next_action(true), Action::Transmit { text } if text == "CQ JA1ABC PM95")
        );
        assert_eq!(qso.next_action(false), Action::Receive);
    }

    #[test]
    fn manual_cq_ignores_opposite_parity() {
        let mut qso = make(false, false, false);
        qso.start_manual_cq(false);
        assert_eq!(
            qso.next_action(false),
            Action::Transmit {
                text: "CQ JA1ABC PM95".to_string()
            }
        );
        assert_eq!(qso.next_action(true), Action::Receive);
    }

    #[test]
    fn working_tag_starts_only_after_reply_has_been_sent() {
        let mut qso = make(false, false, false);
        qso.target = Some(Target {
            call: "ID1OHJ".to_string(),
            grid: Some("OL39".to_string()),
            their_snr: None,
            my_snr: None,
        });
        qso.state = QsoState::ReplySent;
        assert_eq!(qso.working_target_call(), None);
        qso.state = QsoState::ReportSent;
        assert_eq!(qso.working_target_call(), Some("ID1OHJ"));
    }
}
