//! Application entry and main coordinator for the FT8/FT4 terminal monitor.
//!
//! This module wires together configuration loading, audio capture/playback,
//! decoding, QSO flow control, log persistence, and the TUI renderer.

use mfsk_core::decoder::Decoder;
use mfsk_core::ft8::Ft8;
use mfsk_core::msg::wsjt77::pack77;
use std::collections::{HashSet, VecDeque};
use std::error::Error;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

mod domain;
mod infrastructure;
mod presentation;

/// Re-export of the audio subsystem used by the runtime and by tests.
pub mod audio {
    pub use crate::infrastructure::audio::*;
}
/// Re-export of the configuration model consumed by the application.
pub mod config {
    pub use crate::domain::config::*;
}
/// Re-export of the FT8/FT4 decode helpers.
pub mod decode {
    pub use crate::infrastructure::decode::*;
}
/// Re-export of the contact-state logic that drives the QSO flow.
pub mod qso {
    pub use crate::domain::qso::*;
}
/// Re-export of the ADIF QSO log persistence layer.
pub mod qso_log {
    pub use crate::infrastructure::qso_log::*;
}
/// Re-export of the runtime log storage used for received and transmitted messages.
pub mod run_log {
    pub use crate::infrastructure::run_log::*;
}
/// Re-export of the signal synthesis utilities used for FT8 message generation.
pub mod signal {
    pub use crate::infrastructure::signal::*;
}
/// Re-export of the FT8 slot timing helpers and parity calculations.
pub mod slot {
    pub use crate::infrastructure::slot::*;
}
/// Re-export of the UI modules that provide the terminal display and event feed.
pub mod ui {
    pub use crate::presentation::ui::*;
}
/// Re-export of the worked-callsign and worked-grid persistence helpers.
pub mod worked {
    pub use crate::domain::worked::*;
}

/// Program entry point.
pub fn main() {
    if let Err(e) = run_application() {
        eprintln!("fatal: {e}");
        std::process::exit(1);
    }
}

/// Runs the FT8 application loop: load config, initialize logs/audio, start the TUI,
/// then keep processing slot-based decode and transmit cycles until shutdown.
fn run_application() -> Result<(), Box<dyn Error>> {
    let config_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("config")
        .join("config.toml");
    let config = config::Config::load(
        config_path
            .to_str()
            .ok_or_else(|| format!("invalid config path: {}", config_path.display()))?,
    )?;
    let audio_io = audio::AudioIo::new().map_err(|e| e.to_string())?;

    let mut run_log_store = run_log::RunLog::new(
        PathBuf::from(&config.log.run.dir),
        config.log.run.keep_days,
        config.log.run.mem_lines,
    )?;
    let mut qso_log_store =
        qso_log::QsoLog::new(PathBuf::from(&config.log.qso.dir), config.log.qso.keep_days)?;
    let mut worked_calls_log =
        worked::WorkedLog::load(PathBuf::from(&config.log.worked.calls_path))?;
    let mut worked_grids_log =
        worked::WorkedLog::load(PathBuf::from(&config.log.worked.grids_path))?;

    let mode_upper = config.device.radio.protocol.to_uppercase();
    let mode_for_ui = mode_upper.clone();
    let mode_for_adif = mode_upper.clone();
    let freq_mhz = config.user.freq_mhz;
    let my_call = config.user.call.clone();
    let tx_audio_hz = config.build.ft8.tx_freq;
    let hide_worked = config.ui.hide_worked;
    let feed = ui::feed::UiFeed::new(freq_mhz, mode_upper);
    let state = Arc::new(Mutex::new(ui::tui::UiState::new(config.user.call.clone())));
    let shutdown = Arc::new(AtomicBool::new(false));

    let ui_state = Arc::clone(&state);
    let ui_config = config.ui.clone();
    let ui_shutdown = Arc::clone(&shutdown);
    let ui_thread = thread::spawn(move || {
        let _ = ui::run_tui(ui_state, feed, ui_config, ui_shutdown);
    });

    let mut worked_set: HashSet<String> = worked_calls_log.set().clone();
    let mut worked_grids: HashSet<String> = worked_grids_log.set().clone();
    let mut worked_dxcc: HashSet<String> = HashSet::new();
    for call in &worked_set {
        if let Some(d) = ui::feed::dxcc_prefix_for_call(call) {
            worked_dxcc.insert(d);
        }
    }

    let mut qso = qso::QsoStateMachine::new(
        config.user.call.clone(),
        config.user.grid.clone(),
        config.qso.max_retries,
        config.qso.auto_cq_attempts,
        config.qso.auto_reply_cq,
        config.qso.reply_b4,
        config.qso.idle_slots_before_cq,
        worked_set.clone(),
    );

    if let Some(ref manual) = config.test.manual_tx {
        let parts: Vec<&str> = manual.split_whitespace().collect();
        let msg77 = pack77(parts[0], parts[1], parts[2])
            .ok_or_else(|| format!("pack77 failed for '{}'", manual))?;
        let pcm = signal::synthesize_ft8_audio(&msg77, &config.build.ft8);
        let now = SystemTime::now();
        let index = slot::slot_index_for_time(now, slot::FT8_SLOT);
        let parity = slot::slot_parity_for_time(now, slot::FT8_SLOT);
        audio_io
            .play_blocking(
                &pcm,
                config.build.ft8.sample_rate,
                config.device.audio.output.as_deref(),
                Duration::from_secs(20),
                Some(Arc::clone(&shutdown)),
            )
            .map_err(|e| e.to_string())?;
        run_log_store.append_tx(index, parity, tx_audio_hz, manual)?;
    }

    let capture = audio_io
        .start_capture(
            config.build.ft8.sample_rate,
            config.device.audio.input.as_deref(),
        )
        .map_err(|e| e.to_string())?;

    let mut decoder = Decoder::<Ft8>::with_defaults();
    let target_rate = config.build.ft8.sample_rate;
    let expected_len = (slot::FT8_SLOT.as_secs_f64() * target_rate as f64) as usize;
    let mem_lines = config.log.run.mem_lines;

    let mut left_lines: VecDeque<(String, Option<ui::feed::DecodeTag>, Option<u64>)> =
        VecDeque::new();
    let mut right_lines: VecDeque<(String, Option<u64>)> = VecDeque::new();
    let mut last_left_slot: Option<u64> = None;
    let mut last_right_slot: Option<u64> = None;

    if !wait_until_next_slot_boundary(slot::FT8_SLOT, &shutdown) {
        let _ = ui_thread.join();
        return Ok(());
    }
    let _ = capture.take();

    loop {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }

        if !wait_until_next_slot_boundary(slot::FT8_SLOT, &shutdown) {
            break;
        }

        let now = SystemTime::now();
        let index = slot::slot_index_for_time(now, slot::FT8_SLOT);
        let parity = slot::slot_parity_for_time(now, slot::FT8_SLOT);
        let prev_index = index.saturating_sub(1);
        let prev_parity = !parity;

        let raw = capture.take();
        let rate = capture.rate();
        let pcm = if rate != target_rate {
            resample_pcm_linear(raw, rate, target_rate)
        } else {
            raw
        };

        if pcm.len() >= expected_len / 2 {
            let results = decode::decode_ft8_slot(&mut decoder, &pcm);
            qso.on_decode(&results, prev_parity, prev_index);

            if !results.is_empty() {
                let mut divider_added = false;

                let to_me: Vec<_> = results
                    .iter()
                    .filter(|d| ui::feed::is_message_addressed_to_me(&d.text, &my_call))
                    .collect();
                if !to_me.is_empty() && last_right_slot != Some(prev_index) {
                    right_lines.push_back((
                        ui::feed::format_divider_line(prev_index, freq_mhz, &mode_for_ui),
                        None,
                    ));
                    while right_lines.len() > mem_lines {
                        right_lines.pop_front();
                    }
                    last_right_slot = Some(prev_index);
                }

                for d in &results {
                    if let Err(e) = run_log_store.append_decode(prev_index, prev_parity, d) {
                        eprintln!("run log write failed: {e}");
                    }

                    let line_str = ui::feed::format_decode_line(prev_index, d);
                    let tag = ui::feed::tag_line_for_work_status(
                        &line_str,
                        &my_call,
                        &worked_set,
                        &worked_grids,
                        &worked_dxcc,
                        qso.target_call(),
                    );

                    let is_worked = tag
                        .as_ref()
                        .map(|t| matches!(t.color_key, ui::feed::ColorKey::Worked))
                        .unwrap_or(false);

                    if !(hide_worked && is_worked) {
                        if !divider_added && last_left_slot != Some(prev_index) {
                            left_lines.push_back((
                                ui::feed::format_divider_line(prev_index, freq_mhz, &mode_for_ui),
                                None,
                                None,
                            ));
                            while left_lines.len() > mem_lines {
                                left_lines.pop_front();
                            }
                            last_left_slot = Some(prev_index);
                            divider_added = true;
                        }
                        left_lines.push_back((line_str, tag, Some(prev_index)));
                        while left_lines.len() > mem_lines {
                            left_lines.pop_front();
                        }
                    }

                    if ui::feed::is_message_addressed_to_me(&d.text, &my_call) {
                        right_lines.push_back((
                            ui::feed::format_decode_line(prev_index, d),
                            Some(prev_index),
                        ));
                        while right_lines.len() > mem_lines {
                            right_lines.pop_front();
                        }
                    }
                }

                let mut s = state.lock().unwrap();
                s.set_left_lines(left_lines.iter().cloned().collect());
                s.set_right_lines(right_lines.iter().cloned().collect());
            }
        }

        if shutdown.load(Ordering::Relaxed) {
            break;
        }

        let pending = {
            let mut s = state.lock().unwrap();
            let pending_reply = s.pending.take();
            let pending_cq = s.pending_cq;
            s.pending_cq = false;
            (pending_reply, pending_cq)
        };
        if let (Some((call, rx_slot)), _) = pending {
            let rx_parity = slot::slot_parity_for_index(rx_slot);
            qso.start_manual_qso(call, rx_parity, rx_slot);
        }
        if let (_, true) = pending {
            qso.start_manual_cq(slot::slot_parity_for_time(SystemTime::now(), slot::FT8_SLOT));
        }

        if let qso::Action::Transmit { text } = qso.next_action(parity) {
            let _ = capture.take();
            record_transmit_line(
                index,
                parity,
                tx_audio_hz,
                &text,
                &mut run_log_store,
                &mut right_lines,
                &mut last_right_slot,
                &state,
                freq_mhz,
                &mode_for_ui,
                mem_lines,
            );
            wait_for_ui_render(&state, Duration::from_millis(500), &shutdown);
            play_transmit_message(&audio_io, &config, &text, &shutdown);
            let _ = capture.take();
        }

        while let Some(done) = qso.take_completed() {
            let (date, time) = qso_log::slot_to_adif_dt(done.start_slot);
            let rec = qso_log::QsoRecord {
                call: done.call.clone(),
                gridsquare: done.grid.clone(),
                mode: mode_for_adif.clone(),
                freq_mhz,
                qso_date: date,
                time_on: time,
                rst_sent: qso::format_snr(done.their_snr.unwrap_or(-10)),
                rst_rcvd: qso::format_snr(done.my_snr.unwrap_or(-10)),
                station_callsign: my_call.clone(),
            };
            if let Err(e) = qso_log_store.append(&rec) {
                eprintln!("qso log write failed: {e}");
            }
            if let Err(e) = worked_calls_log.insert(&done.call) {
                eprintln!("worked calls log write failed: {e}");
            }
            worked_set.insert(done.call.clone());
            if let Some(d) = ui::feed::dxcc_prefix_for_call(&done.call) {
                worked_dxcc.insert(d);
            }
            if let Some(ref g) = done.grid {
                worked_grids.insert(g.clone());
                if let Err(e) = worked_grids_log.insert(g) {
                    eprintln!("worked grids log write failed: {e}");
                }
            }
        }
    }

    drop(capture);
    let _ = ui_thread.join();
    Ok(())
}

/// Waits for the UI render generation to catch up with the newest state update.
fn wait_for_ui_render(
    state: &Arc<Mutex<ui::tui::UiState>>,
    timeout: Duration,
    shutdown: &Arc<AtomicBool>,
) -> bool {
    let target = {
        let s = state.lock().unwrap();
        s.render_gen
    };
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if shutdown.load(Ordering::Relaxed) {
            return false;
        }
        let done = {
            let s = state.lock().unwrap();
            s.rendered_gen >= target
        };
        if done {
            return true;
        }
        thread::sleep(Duration::from_millis(5));
    }
    false
}

/// Waits until the next UTC slot boundary, while allowing shutdown checks every 20ms.
fn wait_until_next_slot_boundary(slot: Duration, shutdown: &Arc<AtomicBool>) -> bool {
    let wait = slot::time_until_next_boundary(SystemTime::now(), slot);
    if wait.is_zero() {
        return !shutdown.load(Ordering::Relaxed);
    }
    let deadline = Instant::now() + wait;
    while Instant::now() < deadline {
        if shutdown.load(Ordering::Relaxed) {
            return false;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        thread::sleep(remaining.min(Duration::from_millis(20)));
    }
    !shutdown.load(Ordering::Relaxed)
}

/// Records a TX event in both the in-memory UI right pane and the on-disk run log.
fn record_transmit_line(
    index: u64,
    parity: bool,
    freq_hz: f32,
    text: &str,
    run_log: &mut run_log::RunLog,
    right_lines: &mut VecDeque<(String, Option<u64>)>,
    last_right_slot: &mut Option<u64>,
    state: &Arc<Mutex<ui::tui::UiState>>,
    freq_mhz: f64,
    mode: &str,
    mem_lines: usize,
) {
    if *last_right_slot != Some(index) {
        right_lines.push_back((ui::feed::format_divider_line(index, freq_mhz, mode), None));
        while right_lines.len() > mem_lines {
            right_lines.pop_front();
        }
        *last_right_slot = Some(index);
    }
    right_lines.push_back((ui::feed::format_tx_line(index, freq_hz, text), Some(index)));
    while right_lines.len() > mem_lines {
        right_lines.pop_front();
    }
    if let Err(e) = run_log.append_tx(index, parity, freq_hz, text) {
        eprintln!("run log write failed: {e}");
    }
    let mut s = state.lock().unwrap();
    s.set_right_lines(right_lines.iter().cloned().collect());
}

/// Synthesizes and plays a single outgoing FT8 message.
fn play_transmit_message(
    audio_io: &audio::AudioIo,
    config: &config::Config,
    text: &str,
    shutdown: &Arc<AtomicBool>,
) {
    let p: Vec<&str> = text.split_whitespace().collect();
    if p.len() != 3 {
        return;
    }
    let msg77 = match pack77(p[0], p[1], p[2]) {
        Some(m) => m,
        None => return,
    };
    let pcm = signal::synthesize_ft8_audio(&msg77, &config.build.ft8);
    let _ = audio_io.play_blocking(
        &pcm,
        config.build.ft8.sample_rate,
        config.device.audio.output.as_deref(),
        Duration::from_secs(20),
        Some(Arc::clone(shutdown)),
    );
}

/// Resamples PCM samples from one sample rate to another using linear interpolation.
fn resample_pcm_linear(input: Vec<i16>, from: u32, to: u32) -> Vec<i16> {
    if from == to || input.is_empty() {
        return input;
    }
    let ratio = from as f64 / to as f64;
    let out_len = (input.len() as f64 / ratio).round() as usize;
    (0..out_len)
        .map(|i| {
            let src = i as f64 * ratio;
            let idx = src.floor() as usize;
            let frac = src - idx as f64;
            let a = input.get(idx).copied().unwrap_or(0) as f64;
            let b = input.get(idx + 1).copied().unwrap_or(a as i16) as f64;
            (a + (b - a) * frac).round() as i16
        })
        .collect()
}
