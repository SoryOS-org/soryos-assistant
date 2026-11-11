use anyhow::{anyhow, Result};
use std::sync::Mutex;
use webrtc_vad::{Vad, VadMode};

/// Thread-safe wrapper for WebRTC VAD.
/// The underlying Vad is not Send+Sync, so we wrap it in a Mutex and mark our wrapper as Send+Sync.
pub(super) struct VadWrapper {
    vad: Mutex<Vad>,
}

impl VadWrapper {
    pub(super) fn new(aggressiveness: u8) -> Self {
        let vad_mode = match aggressiveness {
            0 => VadMode::Quality,
            1 => VadMode::LowBitrate,
            2 => VadMode::Aggressive,
            3 => VadMode::VeryAggressive,
            _ => VadMode::Aggressive,
        };

        let vad = Vad::new_with_rate_and_mode(webrtc_vad::SampleRate::Rate16kHz, vad_mode);
        Self {
            vad: Mutex::new(vad),
        }
    }

    pub(super) fn is_voice_segment(&self, samples: &[i16]) -> Result<bool> {
        let mut vad = self.vad.lock().unwrap();
        vad.is_voice_segment(samples)
            .map_err(|_| anyhow!("VAD analysis failed"))
    }
}

// Explicitly mark as Send+Sync since we're using Mutex for interior mutability.
unsafe impl Send for VadWrapper {}
unsafe impl Sync for VadWrapper {}
