use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SizedSample};
use tracing::{debug, error, info, trace};

use super::resampler::LinearResampler;
use super::{AudioPipeline, AudioState};
use super::{
    MIN_ENERGY_THRESHOLD, RESAMPLE_TARGET_SAMPLE_RATE, VAD_DEBUG, VAD_FRAME_SIZE_MS,
    VAD_FRAME_SIZE_SAMPLES,
};

impl AudioPipeline {
    pub async fn start_audio_capture(&self) -> Result<()> {
        if self.capture_active.swap(true, Ordering::Relaxed) {
            // Already running
            return Ok(());
        }

        let stream = match (|| -> Result<cpal::Stream> {
            let host = cpal::default_host();
            let device = host
                .default_input_device()
                .ok_or_else(|| anyhow!("No default input audio device available"))?;

            let supported_config = device
                .default_input_config()
                .context("Failed to query default input audio configuration")?;

            let sample_format = supported_config.sample_format();
            let stream_config: cpal::StreamConfig = supported_config.clone().into();

            let stream = self.build_input_stream(&device, &stream_config, sample_format)?;
            stream
                .play()
                .context("Failed to start audio capture stream")?;

            Ok(stream)
        })() {
            Ok(stream) => stream,
            Err(err) => {
                self.capture_active.store(false, Ordering::Relaxed);
                return Err(err);
            }
        };

        while !self.shutdown.load(Ordering::Relaxed) && self.capture_active.load(Ordering::Relaxed)
        {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        self.capture_active.store(false, Ordering::Relaxed);
        drop(stream);

        Ok(())
    }

    pub fn stop_audio_capture(&self) {
        self.capture_active.store(false, Ordering::Relaxed);
    }

    fn build_input_stream(
        &self,
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        sample_format: cpal::SampleFormat,
    ) -> Result<cpal::Stream> {
        let err_fn = |err| error!("Audio capture error: {err}");

        match sample_format {
            cpal::SampleFormat::F32 => self.build_stream::<f32>(device, config, err_fn),
            cpal::SampleFormat::I16 => self.build_stream::<i16>(device, config, err_fn),
            cpal::SampleFormat::U16 => self.build_stream::<u16>(device, config, err_fn),
            other => Err(anyhow!("Unsupported audio sample format: {other:?}")),
        }
    }

    fn build_stream<T>(
        &self,
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        err_fn: impl Fn(cpal::StreamError) + Send + 'static,
    ) -> Result<cpal::Stream>
    where
        T: Sample + SizedSample,
        f32: FromSample<T>,
    {
        let channels = usize::from(config.channels.max(1));
        let sample_rate = config.sample_rate.0;

        let resampler = Arc::new(LinearResampler::new(
            sample_rate,
            RESAMPLE_TARGET_SAMPLE_RATE,
        ));
        let audio_buffer = self.audio_buffer.clone();
        let speech_buffer = self.speech_buffer.clone();
        let is_recording = self.is_recording.clone();
        let shutdown = self.shutdown.clone();
        let vad = self.vad.clone();
        let last_voice_time = self.last_voice_time.clone();
        let silence_start_time = self.silence_start_time.clone();
        let has_detected_voice = self.has_detected_voice.clone();
        let speech_activation_accumulator = self.speech_activation_accumulator.clone();
        let state = self.state.clone();
        let state_events = self.state_events_tx.clone();
        let start_grace_ms = self.config.start_grace_ms;
        let recording_start_time = self.recording_start_time.clone();
        let max_segment_ms = self.config.max_segment_ms;
        let ui_events = self.ui_events.clone();
        let activation_ms = self.config.activation_ms;
        let post_speech_silence_ms = self.config.post_speech_silence_ms;
        let frame_duration_ms = VAD_FRAME_SIZE_MS as u64;
        let metrics = self.metrics.clone();
        let turn_trace = self.current_trace.clone();
        let discard_if_silent = self.discard_if_silent.clone();

        let stream = device.build_input_stream(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                let mark_vad_stop = || {
                    if let Some(metrics) = &metrics {
                        if let Some(trace_id) = turn_trace.lock().unwrap().clone() {
                            metrics.mark_vad_stop(&trace_id);
                        }
                    }
                };

                if shutdown.load(Ordering::Relaxed) {
                    return;
                }

                if data.is_empty() {
                    return;
                }

                let channel_samples: Vec<f32> = if channels > 1 {
                    data.chunks(channels)
                        .map(|frame| f32::from_sample(frame[0]))
                        .collect()
                } else {
                    data.iter()
                        .map(|sample| f32::from_sample(*sample))
                        .collect()
                };

                let resampled = if resampler.needs_resample() {
                    resampler.process(&channel_samples)
                } else {
                    channel_samples
                };

                if is_recording.load(Ordering::Relaxed) {
                    let mut vad_frames: Vec<Vec<f32>> = Vec::new();
                    {
                        let mut buffer = audio_buffer.lock().unwrap();
                        buffer.extend(resampled.iter());
                        while buffer.len() >= VAD_FRAME_SIZE_SAMPLES {
                            vad_frames
                                .push(buffer.drain(..VAD_FRAME_SIZE_SAMPLES).collect::<Vec<f32>>());
                        }
                    }

                    if max_segment_ms > 0 {
                        let start = recording_start_time.load(Ordering::Relaxed);
                        if start > 0 {
                            let now_ms = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_millis() as u64;
                            if now_ms.saturating_sub(start) >= max_segment_ms {
                                if is_recording.swap(false, Ordering::Relaxed) {
                                    info!("Max segment length reached, stopping recording");
                                    mark_vad_stop();
                                    AudioPipeline::publish_state(
                                        &state,
                                        &state_events,
                                        AudioState::Processing,
                                    );
                                }
                                return;
                            }
                        }
                    }

                    {
                        let mut buffer = speech_buffer.lock().unwrap();
                        buffer.extend(resampled.iter());
                    }

                    let release_ms = post_speech_silence_ms;

                    for frame in vad_frames {
                        let i16_samples: Vec<i16> = frame
                            .iter()
                            .map(|&s| (s * 32767.0).clamp(-32768.0, 32767.0) as i16)
                            .collect();

                        let energy: f32 = frame
                            .iter()
                            .map(|&sample| {
                                let sample = sample.clamp(-1.0, 1.0);
                                sample * sample
                            })
                            .sum::<f32>()
                            / (VAD_FRAME_SIZE_SAMPLES as f32);
                        if let Some(sender) = &ui_events {
                            let vu_level = energy.sqrt().clamp(0.0, 1.0);
                            sender.emit_vu_level(vu_level);
                        }

                        let vad_decision = vad.is_voice_segment(&i16_samples).unwrap_or(false);
                        let detected_voice = has_detected_voice.load(Ordering::Relaxed);
                        let energy_trigger = energy >= MIN_ENERGY_THRESHOLD;

                        if VAD_DEBUG {
                            trace!(
                                "Frame: vad={} energy={:.6} detected_before={}",
                                vad_decision,
                                energy,
                                detected_voice
                            );
                        }

                        let is_voice = if vad_decision {
                            true
                        } else if !detected_voice && energy_trigger {
                            debug!(
                                "Energy-triggered voice detection before VAD lock (energy {:.6})",
                                energy
                            );
                            true
                        } else {
                            false
                        };

                        let now_ms = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64;

                        if is_voice {
                            let detection_ready = if detected_voice {
                                speech_activation_accumulator
                                    .store(activation_ms, Ordering::Relaxed);
                                true
                            } else {
                                let previous = speech_activation_accumulator
                                    .fetch_add(frame_duration_ms, Ordering::Relaxed);
                                let total = previous + frame_duration_ms;
                                if total >= activation_ms {
                                    has_detected_voice.store(true, Ordering::Relaxed);
                                    discard_if_silent.store(false, Ordering::Relaxed);
                                    speech_activation_accumulator
                                        .store(activation_ms, Ordering::Relaxed);
                                    debug!("Speech activation threshold reached after {}ms", total);
                                    true
                                } else {
                                    false
                                }
                            };

                            if !detection_ready {
                                continue;
                            }

                            last_voice_time.store(now_ms, Ordering::Relaxed);
                            silence_start_time.store(0, Ordering::Relaxed);
                        } else {
                            if !detected_voice {
                                speech_activation_accumulator.store(0, Ordering::Relaxed);
                            }

                            let active_state = AudioState::from_code(state.load(Ordering::Relaxed));

                            if matches!(active_state, AudioState::WakeWordRecording) {
                                if detected_voice {
                                    let previous = silence_start_time.load(Ordering::Relaxed);
                                    if previous == 0 {
                                        debug!(
                                            "Silence window started (energy {:.6}, vad={})",
                                            energy, vad_decision
                                        );
                                        silence_start_time.store(now_ms, Ordering::Relaxed);
                                    }
                                    let elapsed_since_voice = now_ms
                                        .saturating_sub(last_voice_time.load(Ordering::Relaxed));

                                    if elapsed_since_voice >= release_ms {
                                        debug!(
                                            "Silence threshold reached after {}ms (threshold {}ms)",
                                            elapsed_since_voice, release_ms
                                        );
                                        if is_recording.swap(false, Ordering::Relaxed) {
                                            info!("Silence detected, stopping wake word recording");
                                            mark_vad_stop();
                                            AudioPipeline::publish_state(
                                                &state,
                                                &state_events,
                                                AudioState::Processing,
                                            );
                                        }
                                    }
                                } else {
                                    silence_start_time.store(0, Ordering::Relaxed);
                                    let start = recording_start_time.load(Ordering::Relaxed);
                                    if start > 0 {
                                        let elapsed_since_start = now_ms.saturating_sub(start);
                                        if elapsed_since_start >= start_grace_ms {
                                            debug!(
                                                "No speech detected within {}ms grace window",
                                                elapsed_since_start
                                            );
                                            if is_recording.swap(false, Ordering::Relaxed) {
                                                info!("Grace window elapsed without speech");
                                                mark_vad_stop();
                                                AudioPipeline::publish_state(
                                                    &state,
                                                    &state_events,
                                                    AudioState::Processing,
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    audio_buffer.lock().unwrap().clear();
                }
            },
            err_fn,
            None,
        )?;

        Ok(stream)
    }
}
