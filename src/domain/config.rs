//! Configuration definitions and validation for the project.
//!
//! The TOML config is parsed and normalized here before the runtime consumes it,
//! including defaults, schema checks, and validation of frequency and audio values.

use serde::Deserialize;
use std::error::Error;
use std::fs;

/// Top-level configuration loaded from the project TOML file.
#[derive(Deserialize, Debug)]
pub struct Config {
    /// User identity and operating frequency.
    pub user: User,
    /// Device and radio settings.
    #[serde(default)]
    pub device: Device,
    /// Build-time FT8/FT4 transmit settings.
    #[serde(default)]
    pub build: Build,
    /// Decode range configuration for each mode.
    #[serde(default)]
    pub decode: Decode,
    /// Logging configuration.
    #[serde(default)]
    pub log: LogConfig,
    /// Terminal UI presentation and color settings.
    #[serde(default)]
    pub ui: UiConfig,
    /// Optional testing helpers.
    #[serde(default)]
    pub test: TestConfig,
    /// QSO automation tuning parameters.
    #[serde(default)]
    pub qso: QsoConfig,
}

/// Station identity for operation and logging.
#[derive(Deserialize, Debug)]
pub struct User {
    /// Callsign used for transmission and identification.
    pub call: String,
    /// Maidenhead grid square for the station.
    pub grid: String,
    /// Operating frequency in MHz.
    pub freq_mhz: f64,
}

/// Hardware and transport configuration for audio and radio modules.
#[derive(Deserialize, Debug, Default)]
pub struct Device {
    /// Audio input/output device selection.
    #[serde(default)]
    pub audio: AudioDevice,
    /// Radio protocol and mode settings.
    #[serde(default)]
    pub radio: Radio,
}

/// Named audio devices selected by the runtime.
#[derive(Deserialize, Debug, Default)]
pub struct AudioDevice {
    /// Optional input device name for the capture stream.
    pub input: Option<String>,
    /// Optional output device name for TX audio playback.
    pub output: Option<String>,
}

/// Radio protocol configuration used by the application loop.
#[derive(Deserialize, Debug, Default)]
pub struct Radio {
    /// Indicates whether the radio is full duplex.
    #[serde(default)]
    pub duplex: bool,
    /// Low-level protocol name, for example FT8 or FT4.
    #[serde(default = "default_protocol")]
    pub protocol: String,
}

/// Transmit build settings for the supported digital modes.
#[derive(Deserialize, Debug)]
pub struct Build {
    /// FT8-specific transmit and waveform settings.
    #[serde(default)]
    pub ft8: Ft8Build,
    /// FT4-specific transmit and waveform settings.
    #[serde(default)]
    pub ft4: Ft4Build,
}

#[derive(Deserialize, Debug)]
pub struct Ft8Build {
    #[serde(default = "default_sample_rate")]
    pub sample_rate: u32,
    #[serde(default = "default_tx_freq")]
    pub tx_freq: f32,
    #[serde(default = "default_tx_amp")]
    pub tx_amp: i16,
    #[serde(default = "default_ft8_start_offset")]
    pub start_offset: f32,
}

#[derive(Deserialize, Debug)]
pub struct Ft4Build {
    #[serde(default = "default_sample_rate")]
    pub sample_rate: u32,
    #[serde(default = "default_tx_freq")]
    pub tx_freq: f32,
    #[serde(default = "default_tx_amp")]
    pub tx_amp: i16,
    #[serde(default = "default_ft4_start_offset")]
    pub start_offset: f32,
}

#[derive(Deserialize, Debug)]
pub struct Decode {
    #[serde(default)]
    pub ft8: Ft8Decode,
    #[serde(default)]
    pub ft4: Ft4Decode,
}

#[derive(Deserialize, Debug)]
pub struct Ft8Decode {
    #[serde(default = "default_freq_min")]
    pub freq_min: f32,
    #[serde(default = "default_freq_max")]
    pub freq_max: f32,
}

#[derive(Deserialize, Debug)]
pub struct Ft4Decode {
    #[serde(default = "default_freq_min")]
    pub freq_min: f32,
    #[serde(default = "default_freq_max")]
    pub freq_max: f32,
}

#[derive(Deserialize, Debug)]
pub struct LogConfig {
    #[serde(default)]
    pub run: RunLogConfig,
    #[serde(default)]
    pub qso: QsoLogConfig,
    #[serde(default)]
    pub worked: WorkedLogConfig,
}

#[derive(Deserialize, Debug)]
pub struct RunLogConfig {
    #[serde(default = "default_run_log_dir")]
    pub dir: String,
    #[serde(default = "default_run_keep_days")]
    pub keep_days: u32,
    #[serde(default = "default_mem_lines")]
    pub mem_lines: usize,
}

#[derive(Deserialize, Debug)]
pub struct QsoLogConfig {
    #[serde(default = "default_qso_log_dir")]
    pub dir: String,
    #[serde(default = "default_qso_keep_days")]
    pub keep_days: u32,
}

#[derive(Deserialize, Debug)]
pub struct WorkedLogConfig {
    #[serde(default = "default_worked_calls_path", alias = "path")]
    pub calls_path: String,
    #[serde(default = "default_worked_grids_path")]
    pub grids_path: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct UiConfig {
    #[serde(default = "default_ui_refresh_ms")]
    pub refresh_ms: u32,
    #[serde(default = "default_ui_left_percent")]
    pub left_percent: u16,
    #[serde(default = "default_ui_show_rx")]
    pub show_rx: bool,
    #[serde(default = "default_ui_show_tx")]
    pub show_tx: bool,
    #[serde(default = "default_ui_show_evt")]
    pub show_evt: bool,
    #[serde(default = "default_ui_hide_worked")]
    pub hide_worked: bool,
    #[serde(default = "default_ui_color_new_call")]
    pub color_new_call: String,
    #[serde(default = "default_ui_color_new_grid")]
    pub color_new_grid: String,
    #[serde(default = "default_ui_color_new_dxcc")]
    pub color_new_dxcc: String,
    #[serde(default = "default_ui_color_working")]
    pub color_working: String,
    #[serde(default = "default_ui_color_worked")]
    pub color_worked: String,
}

#[derive(Deserialize, Debug, Default)]
pub struct TestConfig {
    pub manual_tx: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct QsoConfig {
    #[serde(default)]
    pub auto_reply_cq: bool,
    #[serde(default)]
    pub reply_b4: bool,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_idle_slots_before_cq")]
    pub idle_slots_before_cq: u32,
    #[serde(default = "default_auto_cq_attempts")]
    pub auto_cq_attempts: u32,
}

impl Default for Build {
    fn default() -> Self {
        Self {
            ft8: Ft8Build::default(),
            ft4: Ft4Build::default(),
        }
    }
}

impl Default for Ft8Build {
    fn default() -> Self {
        Self {
            sample_rate: default_sample_rate(),
            tx_freq: default_tx_freq(),
            tx_amp: default_tx_amp(),
            start_offset: default_ft8_start_offset(),
        }
    }
}

impl Default for Ft4Build {
    fn default() -> Self {
        Self {
            sample_rate: default_sample_rate(),
            tx_freq: default_tx_freq(),
            tx_amp: default_tx_amp(),
            start_offset: default_ft4_start_offset(),
        }
    }
}

impl Default for Decode {
    fn default() -> Self {
        Self {
            ft8: Ft8Decode::default(),
            ft4: Ft4Decode::default(),
        }
    }
}

impl Default for Ft8Decode {
    fn default() -> Self {
        Self {
            freq_min: default_freq_min(),
            freq_max: default_freq_max(),
        }
    }
}

impl Default for Ft4Decode {
    fn default() -> Self {
        Self {
            freq_min: default_freq_min(),
            freq_max: default_freq_max(),
        }
    }
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            run: RunLogConfig::default(),
            qso: QsoLogConfig::default(),
            worked: WorkedLogConfig::default(),
        }
    }
}

impl Default for RunLogConfig {
    fn default() -> Self {
        Self {
            dir: default_run_log_dir(),
            keep_days: default_run_keep_days(),
            mem_lines: default_mem_lines(),
        }
    }
}

impl Default for QsoLogConfig {
    fn default() -> Self {
        Self {
            dir: default_qso_log_dir(),
            keep_days: default_qso_keep_days(),
        }
    }
}

impl Default for WorkedLogConfig {
    fn default() -> Self {
        Self {
            calls_path: default_worked_calls_path(),
            grids_path: default_worked_grids_path(),
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            refresh_ms: default_ui_refresh_ms(),
            left_percent: default_ui_left_percent(),
            show_rx: default_ui_show_rx(),
            show_tx: default_ui_show_tx(),
            show_evt: default_ui_show_evt(),
            hide_worked: default_ui_hide_worked(),
            color_new_call: default_ui_color_new_call(),
            color_new_grid: default_ui_color_new_grid(),
            color_new_dxcc: default_ui_color_new_dxcc(),
            color_working: default_ui_color_working(),
            color_worked: default_ui_color_worked(),
        }
    }
}

impl Default for QsoConfig {
    fn default() -> Self {
        Self {
            auto_reply_cq: false,
            reply_b4: false,
            max_retries: default_max_retries(),
            idle_slots_before_cq: default_idle_slots_before_cq(),
            auto_cq_attempts: default_auto_cq_attempts(),
        }
    }
}

fn default_sample_rate() -> u32 {
    12_000
}
fn default_tx_freq() -> f32 {
    1500.0
}
fn default_tx_amp() -> i16 {
    20_000
}
fn default_ft8_start_offset() -> f32 {
    0.5
}
fn default_ft4_start_offset() -> f32 {
    0.5
}
fn default_freq_min() -> f32 {
    100.0
}
fn default_freq_max() -> f32 {
    3000.0
}
fn default_protocol() -> String {
    "ft8".into()
}
fn default_run_log_dir() -> String {
    "logs/run".into()
}
fn default_run_keep_days() -> u32 {
    30
}
fn default_mem_lines() -> usize {
    1_000
}
fn default_qso_log_dir() -> String {
    "logs/qso".into()
}
fn default_qso_keep_days() -> u32 {
    3650
}
fn default_worked_calls_path() -> String {
    "logs/worked_calls.txt".into()
}
fn default_worked_grids_path() -> String {
    "logs/worked_grids.txt".into()
}
fn default_ui_refresh_ms() -> u32 {
    100
}
fn default_ui_left_percent() -> u16 {
    55
}
fn default_ui_show_rx() -> bool {
    true
}
fn default_ui_show_tx() -> bool {
    true
}
fn default_ui_show_evt() -> bool {
    false
}
fn default_ui_hide_worked() -> bool {
    false
}
fn default_ui_color_new_call() -> String {
    "green".into()
}
fn default_ui_color_new_grid() -> String {
    "purple".into()
}
fn default_ui_color_new_dxcc() -> String {
    "red".into()
}
fn default_ui_color_working() -> String {
    "orange".into()
}
fn default_ui_color_worked() -> String {
    "gray".into()
}
fn default_max_retries() -> u32 {
    5
}
fn default_idle_slots_before_cq() -> u32 {
    0
}
fn default_auto_cq_attempts() -> u32 {
    10
}

fn parse_color(name: &str) -> Result<(), Box<dyn Error>> {
    match name {
        "black" | "red" | "green" | "yellow" | "blue" | "magenta" | "cyan" | "gray"
        | "darkgray" | "lightred" | "lightgreen" | "lightyellow" | "lightblue" | "lightmagenta"
        | "lightcyan" | "white" | "reset" | "orange" | "purple" => Ok(()),
        _ => Err(format!("invalid color name: {}", name).into()),
    }
}

impl Config {
    pub fn load(path: &str) -> Result<Self, Box<dyn Error>> {
        let text = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&text)?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), Box<dyn Error>> {
        if self.user.call.is_empty() {
            return Err("user.call must not be empty".into());
        }
        if self.user.grid.is_empty() {
            return Err("user.grid must not be empty".into());
        }
        if self.user.freq_mhz <= 0.0 {
            return Err("user.freq_mhz must be > 0".into());
        }
        if let Some(ref s) = self.device.audio.input {
            if s.is_empty() {
                return Err("device.audio.input must not be empty".into());
            }
        }
        if let Some(ref s) = self.device.audio.output {
            if s.is_empty() {
                return Err("device.audio.output must not be empty".into());
            }
        }
        validate_build(
            "build.ft8",
            self.build.ft8.sample_rate,
            self.build.ft8.tx_freq,
            self.build.ft8.tx_amp,
            self.build.ft8.start_offset,
            15.0,
        )?;
        validate_build(
            "build.ft4",
            self.build.ft4.sample_rate,
            self.build.ft4.tx_freq,
            self.build.ft4.tx_amp,
            self.build.ft4.start_offset,
            7.5,
        )?;
        validate_decode(
            "decode.ft8",
            self.decode.ft8.freq_min,
            self.decode.ft8.freq_max,
        )?;
        validate_decode(
            "decode.ft4",
            self.decode.ft4.freq_min,
            self.decode.ft4.freq_max,
        )?;
        if self.log.run.dir.is_empty() {
            return Err("log.run.dir must not be empty".into());
        }
        if self.log.run.keep_days == 0 {
            return Err("log.run.keep_days must be > 0".into());
        }
        if self.log.run.mem_lines == 0 {
            return Err("log.run.mem_lines must be > 0".into());
        }
        if self.log.qso.dir.is_empty() {
            return Err("log.qso.dir must not be empty".into());
        }
        if self.log.qso.keep_days == 0 {
            return Err("log.qso.keep_days must be > 0".into());
        }
        if self.log.worked.calls_path.is_empty() {
            return Err("log.worked.calls_path must not be empty".into());
        }
        if self.log.worked.grids_path.is_empty() {
            return Err("log.worked.grids_path must not be empty".into());
        }
        if self.ui.refresh_ms == 0 {
            return Err("ui.refresh_ms must be > 0".into());
        }
        if self.ui.left_percent == 0 || self.ui.left_percent >= 100 {
            return Err("ui.left_percent must be in 1..99".into());
        }
        parse_color(&self.ui.color_new_call)?;
        parse_color(&self.ui.color_new_grid)?;
        parse_color(&self.ui.color_new_dxcc)?;
        parse_color(&self.ui.color_working)?;
        parse_color(&self.ui.color_worked)?;
        if let Some(ref s) = self.test.manual_tx {
            let parts: Vec<&str> = s.split_whitespace().collect();
            if parts.len() != 3 || parts[0] != "CQ" {
                return Err("test.manual_tx must be 'CQ CALL GRID'".into());
            }
        }
        if self.qso.max_retries == 0 {
            return Err("qso.max_retries must be > 0".into());
        }
        if self.qso.auto_cq_attempts == 0 {
            return Err("qso.auto_cq_attempts must be > 0".into());
        }
        Ok(())
    }
}

fn validate_build(
    name: &str,
    sample_rate: u32,
    tx_freq: f32,
    tx_amp: i16,
    start_offset: f32,
    slot_secs: f64,
) -> Result<(), Box<dyn Error>> {
    if sample_rate == 0 {
        return Err(format!("{}.sample_rate must be > 0", name).into());
    }
    if tx_freq <= 0.0 {
        return Err(format!("{}.tx_freq must be > 0", name).into());
    }
    if tx_amp <= 0 {
        return Err(format!("{}.tx_amp must be > 0", name).into());
    }
    if start_offset < 0.0 || start_offset as f64 >= slot_secs {
        return Err(format!(
            "{}.start_offset {} outside [0, {})",
            name, start_offset, slot_secs
        )
        .into());
    }
    Ok(())
}

fn validate_decode(name: &str, freq_min: f32, freq_max: f32) -> Result<(), Box<dyn Error>> {
    if freq_min < 0.0 || freq_max <= freq_min {
        return Err(format!(
            "{}: require 0 <= freq_min < freq_max, got [{}, {}]",
            name, freq_min, freq_max
        )
        .into());
    }
    Ok(())
}
