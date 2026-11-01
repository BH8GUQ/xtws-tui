//! Audio device abstraction for capture, playback, and sample-rate adaptation.
//!
//! The module owns all cpal interaction, including input/output device selection,
//! stream negotiation, and PCM resampling needed by the FT8/FT4 pipeline.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, Device, Host, StreamConfig};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Errors raised while selecting devices or creating audio streams.
#[derive(Debug)]
pub enum AudioError {
    /// No output device could be found.
    NoOutputDevice,
    /// No input device could be found.
    NoInputDevice,
    /// A named device was not present in the system.
    DeviceNotFound(String),
    /// No usable output config was negotiated.
    NoOutputConfig,
    /// No usable input config was negotiated.
    NoInputConfig,
    /// A CPAL stream could not be built.
    BuildStream(String),
    /// Playback or capture stream startup failed.
    PlayStream(String),
    /// The audio operation exceeded the timeout window.
    Timeout,
}

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoOutputDevice => write!(f, "no output device available"),
            Self::NoInputDevice => write!(f, "no input device available"),
            Self::DeviceNotFound(n) => write!(f, "device '{}' not found", n),
            Self::NoOutputConfig => write!(f, "no usable output config"),
            Self::NoInputConfig => write!(f, "no usable input config"),
            Self::BuildStream(e) => write!(f, "failed to build stream: {}", e),
            Self::PlayStream(e) => write!(f, "failed to play stream: {}", e),
            Self::Timeout => write!(f, "audio operation timed out"),
        }
    }
}

impl std::error::Error for AudioError {}

/// Entry point for all cpal-based audio operations in the application.
pub struct AudioIo {
    host: Host,
}

impl AudioIo {
    /// Creates a new audio backend bound to the default host.
    pub fn new() -> Result<Self, AudioError> {
        Ok(Self {
            host: cpal::default_host(),
        })
    }

    /// Plays a PCM buffer using the configured output device and waits until it completes.
    pub fn play_blocking(
        &self,
        samples: &[i16],
        target_rate: u32,
        device_name: Option<&str>,
        timeout: Duration,
        cancel: Option<Arc<AtomicBool>>,
    ) -> Result<(), AudioError> {
        let device = self.select_output_device(device_name)?;
        let config = self.negotiate_output_config(&device, target_rate)?;

        let resampled = if config.sample_rate != target_rate {
            resample_linear(samples, target_rate, config.sample_rate)
        } else {
            samples.to_vec()
        };

        let data = Arc::new(Mutex::new(resampled));
        let pos = Arc::new(Mutex::new(0usize));
        let done = Arc::new(Mutex::new(false));

        let data_cb = Arc::clone(&data);
        let pos_cb = Arc::clone(&pos);
        let done_cb = Arc::clone(&done);

        let stream = device
            .build_output_stream(
                config,
                move |output: &mut [i16], _: &cpal::OutputCallbackInfo| {
                    let mut p = pos_cb.lock().unwrap();
                    let d = data_cb.lock().unwrap();
                    for s in output.iter_mut() {
                        if *p < d.len() {
                            *s = d[*p];
                            *p += 1;
                        } else {
                            *s = 0;
                            *done_cb.lock().unwrap() = true;
                        }
                    }
                },
                |err| eprintln!("output stream error: {}", err),
                None,
            )
            .map_err(|e| AudioError::BuildStream(e.to_string()))?;

        stream
            .play()
            .map_err(|e| AudioError::PlayStream(e.to_string()))?;

        let start = Instant::now();
        while !*done.lock().unwrap() {
            if let Some(ref c) = cancel {
                if c.load(Ordering::Relaxed) {
                    return Ok(());
                }
            }
            if start.elapsed() > timeout {
                return Err(AudioError::Timeout);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Ok(())
    }

    /// Starts a capture stream and returns a handle that can be polled for incoming samples.
    pub fn start_capture(
        &self,
        target_rate: u32,
        device_name: Option<&str>,
    ) -> Result<CaptureStream, AudioError> {
        let device = self.select_input_device(device_name)?;
        let config = self.negotiate_input_config(&device, target_rate)?;
        let rate = config.sample_rate;

        let buffer = Arc::new(Mutex::new(Vec::<i16>::new()));
        let running = Arc::new(AtomicBool::new(true));

        let buffer_cb = Arc::clone(&buffer);
        let running_cb = Arc::clone(&running);

        let stream = device
            .build_input_stream(
                config,
                move |input: &[i16], _: &cpal::InputCallbackInfo| {
                    if !running_cb.load(Ordering::Relaxed) {
                        return;
                    }
                    let mut b = buffer_cb.lock().unwrap();
                    b.extend_from_slice(input);
                    let max = 60 * 48_000usize;
                    if b.len() > max {
                        let excess = b.len() - max;
                        b.drain(..excess);
                    }
                },
                |err| eprintln!("input stream error: {}", err),
                None,
            )
            .map_err(|e| AudioError::BuildStream(e.to_string()))?;

        stream
            .play()
            .map_err(|e| AudioError::PlayStream(e.to_string()))?;

        Ok(CaptureStream {
            _stream: stream,
            buffer,
            rate,
            running,
        })
    }

    fn select_output_device(&self, name: Option<&str>) -> Result<Device, AudioError> {
        if let Some(n) = name {
            if let Some(d) = self
                .host
                .output_devices()
                .map_err(|_| AudioError::NoOutputDevice)?
                .find(|d| device_name_matches(d, n))
            {
                return Ok(d);
            }
            eprintln!(
                "warning: output device '{}' not found, falling back to default",
                n
            );
        }
        self.host
            .default_output_device()
            .ok_or(AudioError::NoOutputDevice)
    }

    fn select_input_device(&self, name: Option<&str>) -> Result<Device, AudioError> {
        if let Some(n) = name {
            if let Some(d) = self
                .host
                .input_devices()
                .map_err(|_| AudioError::NoInputDevice)?
                .find(|d| device_name_matches(d, n))
            {
                return Ok(d);
            }
            eprintln!(
                "warning: input device '{}' not found, falling back to default",
                n
            );
        }
        self.host
            .default_input_device()
            .ok_or(AudioError::NoInputDevice)
    }

    fn negotiate_output_config(
        &self,
        device: &Device,
        target_rate: u32,
    ) -> Result<StreamConfig, AudioError> {
        if let Ok(supported) = device.supported_output_configs() {
            for cfg in supported {
                if cfg.min_sample_rate() <= target_rate && target_rate <= cfg.max_sample_rate() {
                    return Ok(StreamConfig {
                        channels: cfg.channels(),
                        sample_rate: target_rate,
                        buffer_size: BufferSize::Fixed(2048),
                    });
                }
            }
        }
        let default = device
            .default_output_config()
            .map_err(|_| AudioError::NoOutputConfig)?;
        Ok(StreamConfig {
            channels: default.channels(),
            sample_rate: default.sample_rate(),
            buffer_size: BufferSize::Fixed(2048),
        })
    }

    fn negotiate_input_config(
        &self,
        device: &Device,
        target_rate: u32,
    ) -> Result<StreamConfig, AudioError> {
        if let Ok(supported) = device.supported_input_configs() {
            for cfg in supported {
                if cfg.min_sample_rate() <= target_rate && target_rate <= cfg.max_sample_rate() {
                    return Ok(StreamConfig {
                        channels: cfg.channels(),
                        sample_rate: target_rate,
                        buffer_size: BufferSize::Fixed(2048),
                    });
                }
            }
        }
        let default = device
            .default_input_config()
            .map_err(|_| AudioError::NoInputConfig)?;
        Ok(StreamConfig {
            channels: default.channels(),
            sample_rate: default.sample_rate(),
            buffer_size: BufferSize::Fixed(2048),
        })
    }

    pub fn list_input_devices(&self) -> Result<Vec<String>, AudioError> {
        Ok(self
            .host
            .input_devices()
            .map_err(|_| AudioError::NoInputDevice)?
            .filter_map(|d| d.description().ok().map(|desc| desc.name().to_string()))
            .collect())
    }

    pub fn list_output_devices(&self) -> Result<Vec<String>, AudioError> {
        Ok(self
            .host
            .output_devices()
            .map_err(|_| AudioError::NoOutputDevice)?
            .filter_map(|d| d.description().ok().map(|desc| desc.name().to_string()))
            .collect())
    }
}

pub struct CaptureStream {
    _stream: cpal::Stream,
    buffer: Arc<Mutex<Vec<i16>>>,
    rate: u32,
    running: Arc<AtomicBool>,
}

impl CaptureStream {
    pub fn take(&self) -> Vec<i16> {
        let mut b = self.buffer.lock().unwrap();
        std::mem::take(&mut *b)
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    pub fn buffered(&self) -> usize {
        self.buffer.lock().unwrap().len()
    }
}

impl Drop for CaptureStream {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }
}

fn device_name_matches(device: &Device, name: &str) -> bool {
    device
        .description()
        .map(|desc| {
            let s = desc.name();
            s == name || s.contains(name)
        })
        .unwrap_or(false)
}

fn resample_linear(input: &[i16], from_rate: u32, to_rate: u32) -> Vec<i16> {
    if from_rate == to_rate || input.is_empty() {
        return input.to_vec();
    }
    let ratio = from_rate as f64 / to_rate as f64;
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