//! Signal synthesis helpers for FT8 and FT4 transmission audio.
//!
//! This module turns a packed 77-bit message into PCM samples that the audio layer
//! can play over the configured TX device.

use crate::config;
use mfsk_core::engine::tx::{message_to_tones, synthesize_i16};
use mfsk_core::ft4::Ft4;
use mfsk_core::ft8::Ft8;

/// Synthesizes a complete FT8 transmission frame with the configured TX offset and silence padding.
pub fn synthesize_ft8_audio(msg77: &[u8; 77], cfg: &config::Ft8Build) -> Vec<i16> {
    let tones = message_to_tones::<Ft8>(msg77);
    let frame = synthesize_i16::<Ft8>(&tones, cfg.sample_rate, cfg.tx_freq, cfg.tx_amp);

    let mut audio = vec![0i16; (15 * cfg.sample_rate) as usize];
    let start = (cfg.start_offset * cfg.sample_rate as f32) as usize;
    let end = (start + frame.len()).min(audio.len());
    audio[start..end].copy_from_slice(&frame[..end - start]);

    audio
}

/// Synthesizes a complete FT4 transmission frame, matching the shorter 7.5-second slot length.
pub fn synthesize_ft4_audio(msg77: &[u8; 77], cfg: &config::Ft4Build) -> Vec<i16> {
    let tones = message_to_tones::<Ft4>(msg77);
    let frame = synthesize_i16::<Ft4>(&tones, cfg.sample_rate, cfg.tx_freq, cfg.tx_amp);

    let mut audio = vec![0i16; (7.5 * cfg.sample_rate as f32) as usize];
    let start = (cfg.start_offset * cfg.sample_rate as f32) as usize;
    let end = (start + frame.len()).min(audio.len());
    audio[start..end].copy_from_slice(&frame[..end - start]);

    audio
}
