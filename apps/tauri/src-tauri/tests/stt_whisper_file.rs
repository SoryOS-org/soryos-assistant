//! Integration test for the STT inference path without a microphone.
//!
//! It loads the real Whisper model shipped in `resources/whisper`, reads a
//! WAV file (by default the Piper TTS output), resamples it to 16 kHz mono
//! f32 exactly like the audio pipeline does, and runs the same greedy
//! transcription used by `processing.rs::process_speech_buffer`.
//!
//! The expected transcript is asserted to be non-empty and to contain the
//! keyword "testing" — proving the full model-load + inference path works.

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

const TARGET_SAMPLE_RATE: u32 = 16_000;

#[test]
fn transcribes_wav_file() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let model_path = format!("{manifest_dir}/resources/whisper/ggml-base.en.bin");
    let wav_path = std::env::var("STT_TEST_WAV")
        .unwrap_or_else(|_| "/tmp/opencode/tts-step1-en.wav".to_string());

    // --- 1. Load the model (same params as the app: CPU, no GPU) ---
    let mut ctx_params = WhisperContextParameters::default();
    ctx_params.use_gpu(false);
    let ctx = WhisperContext::new_with_params(&model_path, ctx_params)
        .expect("failed to load the Whisper model");

    // --- 2. Decode the WAV (manual PCM16 WAV parsing — no extra deps) ---
    let bytes = std::fs::read(&wav_path)
        .unwrap_or_else(|e| panic!("failed to read {wav_path}: {e}"));
    // Find the "data" chunk and the fmt chunk for the sample rate.
    let mut pos = 12; // skip RIFF/WAVE header
    let mut raw_rate = 0u32;
    let mut data: Option<&[u8]> = None;
    while pos + 8 <= bytes.len() {
        let chunk_id = &bytes[pos..pos + 4];
        let chunk_len =
            u32::from_le_bytes([bytes[pos+4], bytes[pos+5], bytes[pos+6], bytes[pos+7]]) as usize;
        if chunk_id == b"fmt " {
            raw_rate = u32::from_le_bytes([
                bytes[pos+12], bytes[pos+13], bytes[pos+14], bytes[pos+15],
            ]);
        } else if chunk_id == b"data" {
            data = Some(&bytes[pos + 8..pos + 8 + chunk_len]);
        }
        pos += 8 + chunk_len + (chunk_len % 2);
    }
    let data = data.expect("no data chunk found in WAV");
    let raw: Vec<f32> = data
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0)
        .collect();
    let raw_rate = raw_rate.max(1);
    println!("loaded {wav_path}: {} samples @ {raw_rate} Hz", raw.len());

    // --- 3. Resample to 16 kHz mono (linear interpolation, same idea as the
    //        pipeline's LinearResampler) ---
    let samples: Vec<f32> = if raw_rate == TARGET_SAMPLE_RATE {
        raw
    } else {
        let ratio = raw_rate as f64 / TARGET_SAMPLE_RATE as f64;
        let out_len = (raw.len() as f64 / ratio) as usize;
        (0..out_len)
            .map(|i| {
                let pos = i as f64 * ratio;
                let i0 = pos.floor() as usize;
                let i1 = (i0 + 1).min(raw.len() - 1);
                let frac = (pos - i0 as f64) as f32;
                raw[i0] * (1.0 - frac) + raw[i1] * frac
            })
            .collect()
    };
    println!("resampled to {} samples @ {TARGET_SAMPLE_RATE} Hz", samples.len());

    // --- 4. Transcribe (same params as processing.rs) ---
    let mut state = ctx.create_state().expect("failed to create whisper state");
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    let physical = num_cpus::get_physical().max(1) as i32;
    params.set_n_threads(physical.min(num_cpus::get() as i32));
    params.set_language(Some("en"));
    params.set_translate(false);
    params.set_single_segment(true);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);

    state
        .full(params, &samples)
        .expect("whisper inference failed");

    let mut transcript = String::new();
    for i in 0..state.full_n_segments() {
        if let Some(seg) = state.get_segment(i) {
            let text = seg.to_str_lossy().unwrap_or_default();
            transcript.push_str(text.trim());
            transcript.push(' ');
        }
    }
    let transcript = transcript.trim().to_string();
    println!("TRANSCRIPT: \"{transcript}\"");

    assert!(
        !transcript.is_empty(),
        "whisper produced an empty transcript"
    );
    assert!(
        transcript.to_lowercase().contains("testing"),
        "transcript should contain 'testing', got: {transcript}"
    );
}
