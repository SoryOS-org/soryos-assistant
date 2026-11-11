use std::sync::atomic::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use async_channel::Sender;
use tauri::{AppHandle, Emitter};
use tracing::{debug, info, warn};
use whisper_rs::{FullParams, SamplingStrategy};

use super::super::events::{SttFinalEvent, TranscriptionEvent};
use super::super::{threading, EVENT_STT_FINAL};
use super::types::AudioState;
use super::AudioPipeline;
use super::MIN_ENERGY_THRESHOLD;
use crate::metrics::monotonic_now_ms;

impl AudioPipeline {
    /// Returns true if a transcript was emitted.
    pub async fn process_speech_buffer(
        &self,
        transcript_tx: &Sender<TranscriptionEvent>,
        app_handle: &AppHandle,
    ) -> Result<bool> {
        let trace_id = self.current_trace_id();
        let speech_buffer = {
            let mut buffer = self.speech_buffer.lock().unwrap();
            if buffer.is_empty() {
                if let (Some(metrics), Some(trace_ref)) = (&self.metrics, trace_id.as_ref()) {
                    if self.discard_if_silent.swap(false, Ordering::Relaxed) {
                        metrics.discard_turn(trace_ref);
                    } else {
                        metrics.finalize_turn(trace_ref, false, Some("empty_audio_buffer"));
                    }
                }
                self.clear_current_trace();
                self.set_state(AudioState::Idle);
                return Ok(false);
            }
            std::mem::take(&mut *buffer)
        };
        self.discard_if_silent.store(false, Ordering::Relaxed);

        threading::boost_current_thread_priority();

        let detected_voice = self.has_detected_voice.load(Ordering::Relaxed);
        if !detected_voice {
            let energy: f32 = speech_buffer.iter().map(|sample| sample * sample).sum();
            let avg_energy = energy / (speech_buffer.len() as f32).max(1.0);
            if avg_energy < MIN_ENERGY_THRESHOLD {
                debug!(
                    "Skipping transcription - no voice detected (avg energy {:.6})",
                    avg_energy
                );
                if let (Some(metrics), Some(trace_ref)) = (&self.metrics, trace_id.as_ref()) {
                    if self.discard_if_silent.swap(false, Ordering::Relaxed) {
                        metrics.discard_turn(trace_ref);
                    } else {
                        metrics.finalize_turn(trace_ref, false, Some("no_voice_detected"));
                    }
                }
                self.clear_current_trace();
                self.set_state(AudioState::Idle);
                return Ok(false);
            }
        }

        let start_time = self.recording_start_time.load(Ordering::Relaxed);
        let ts_end_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let audio_duration_ms = ts_end_ms.saturating_sub(start_time);

        if audio_duration_ms < 50 {
            debug!("Audio duration too short: {}ms", audio_duration_ms);
            if let (Some(metrics), Some(trace_ref)) = (&self.metrics, trace_id.as_ref()) {
                if self.discard_if_silent.swap(false, Ordering::Relaxed) {
                    metrics.discard_turn(trace_ref);
                } else {
                    metrics.finalize_turn(trace_ref, false, Some("audio_too_short"));
                }
            }
            self.clear_current_trace();
            self.set_state(AudioState::Idle);
            return Ok(false);
        }

        // Past this point we have usable speech; clear the discard flag.
        self.discard_if_silent.store(false, Ordering::Relaxed);

        if let (Some(metrics), Some(trace_ref)) = (&self.metrics, trace_id.as_ref()) {
            metrics.clear_discardable(trace_ref);
            metrics.mark_stt_inference_start(trace_ref);
        }

        let whisper_ctx = self.whisper_ctx.lock().unwrap();

        // Get or create reusable WhisperState
        let mut whisper_state_guard = self.whisper_state.lock().unwrap();
        if whisper_state_guard.is_none() {
            let new_state = whisper_ctx
                .create_state()
                .context("Failed to create Whisper state")?;
            *whisper_state_guard = Some(new_state);
            debug!("Created new reusable WhisperState");
        }
        let state = whisper_state_guard.as_mut().unwrap();

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        let physical_cores = num_cpus::get_physical().max(1) as u32;
        let logical_cores = num_cpus::get().max(1) as u32;
        let recommended = std::cmp::max(1, physical_cores / 2);
        let configured = self.config.whisper_threads.unwrap_or(recommended);
        let configured = std::cmp::max(1, configured);
        let configured = std::cmp::min(configured, physical_cores);
        let configured = std::cmp::min(configured, logical_cores);
        let thread_count = configured as i32;
        debug!(
            "Configuring Whisper with {} CPU thread(s) (physical cores: {}, logical cores: {}, configured override: {:?})",
            thread_count,
            physical_cores,
            logical_cores,
            self.config.whisper_threads
        );
        params.set_n_threads(thread_count);
        params.set_language(Some(&self.config.language));
        params.set_translate(false);
        params.set_single_segment(true);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        debug!("Starting Whisper inference...");

        if let Err(err) = state.full(params, &speech_buffer) {
            let err_string = err.to_string();
            if let (Some(metrics), Some(trace_ref)) = (&self.metrics, trace_id.as_ref()) {
                metrics.mark_stt_done(
                    trace_ref,
                    0,
                    Some(audio_duration_ms),
                    false,
                    Some(err_string.as_str()),
                );
            }
            self.clear_current_trace();
            return Err(err).context("Whisper inference failed");
        }

        let mut transcript = String::new();
        for segment_index in 0..state.full_n_segments() {
            if let Some(segment) = state.get_segment(segment_index) {
                if let Ok(text) = segment.to_str_lossy() {
                    let segment = text.trim();
                    if !segment.is_empty() {
                        if !transcript.is_empty() {
                            transcript.push(' ');
                        }
                        transcript.push_str(segment);
                    }
                }
            }
        }

        let transcript = transcript.trim();
        let transcript_lower = transcript.to_lowercase();
        let is_noise = transcript_lower.contains("(sigh")
            || transcript_lower.contains("[ blank")
            || transcript_lower.contains("[blank")
            || transcript_lower == "blank"
            || transcript_lower.starts_with('(')
            || (transcript_lower.starts_with('(') && transcript_lower.ends_with(')'))
            || transcript
                .chars()
                .all(|c| c.is_whitespace() || c == '(' || c == ')');

        if transcript.is_empty() || is_noise || transcript.len() < self.config.min_transcript_chars
        {
            debug!(
                "Transcript filtered (noise/too short): \"{transcript}\" ({} chars, min: {})",
                transcript.len(),
                self.config.min_transcript_chars
            );
            if let (Some(metrics), Some(trace_ref)) = (&self.metrics, trace_id.as_ref()) {
                metrics.finalize_turn(trace_ref, false, Some("transcript_filtered"));
            }
            self.speech_activation_accumulator
                .store(0, Ordering::Relaxed);
            self.clear_current_trace();
            self.set_state(AudioState::Idle);
            return Ok(false);
        }

        info!("Transcript: \"{}\"", transcript);

        if let (Some(metrics), Some(trace_ref)) = (&self.metrics, trace_id.as_ref()) {
            metrics.mark_stt_done(
                trace_ref,
                transcript.len(),
                Some(audio_duration_ms),
                true,
                None,
            );
        }

        let final_event = SttFinalEvent {
            text: transcript.to_string(),
            ts_start_ms: start_time,
            ts_end_ms,
            lang: self.config.language.clone(),
            confidence: 1.0,
        };
        let _ = app_handle.emit(EVENT_STT_FINAL, final_event);

        let trace_for_event = trace_id
            .clone()
            .unwrap_or_else(|| format!("turn-local-{}", monotonic_now_ms()));
        let event = TranscriptionEvent {
            text: transcript.to_string(),
            trace_id: trace_for_event,
        };

        if let Err(err) = transcript_tx.try_send(event) {
            let err_string = err.to_string();
            warn!("Dropping transcript event: {err_string}");
            if let (Some(metrics), Some(trace_ref)) = (&self.metrics, trace_id.as_ref()) {
                metrics.finalize_turn(trace_ref, false, Some(err_string.as_str()));
            }
            self.clear_current_trace();
            self.speech_activation_accumulator
                .store(0, Ordering::Relaxed);
            self.set_state(AudioState::Idle);
            return Ok(false);
        }

        self.speech_activation_accumulator
            .store(0, Ordering::Relaxed);
        self.set_state(AudioState::Idle);
        self.clear_current_trace();
        Ok(true)
    }
}
