//! FT8/FT4 decoding helpers built on top of mfsk_core.
//!
//! These functions wrap the decoder API and return a flat list of decoded rows for
//! the state machine and UI to consume.

use mfsk_core::decoder::{Decoder, SlotInput};
use mfsk_core::ft4::Ft4;
use mfsk_core::ft8::Ft8;
use mfsk_core::msg::decoded::Decoded;

/// Decodes a single FT8 slot from PCM audio and returns the candidate messages.
pub fn decode_ft8_slot(decoder: &mut Decoder<Ft8>, audio: &[i16]) -> Vec<Decoded> {
    let slot = decoder.decode(&SlotInput::i16(audio));
    slot.rows.into_iter().map(|row| row.decoded).collect()
}

/// Decodes a single FT4 slot from PCM audio and returns the candidate messages.
pub fn decode_ft4_slot(decoder: &mut Decoder<Ft4>, audio: &[i16]) -> Vec<Decoded> {
    let slot = decoder.decode(&SlotInput::i16(audio));
    slot.rows.into_iter().map(|row| row.decoded).collect()
}
