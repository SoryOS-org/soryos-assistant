use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering},
    Arc, Mutex,
};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use tokio::sync::mpsc;
use tracing::{debug, info};
use whisper_rs::{WhisperContext, WhisperContextParameters};

use crate::metrics::{MetricsHandle, TurnTrigger};
use crate::ui::UiEventSender;

mod capture;
mod processing;
mod resampler;
mod types;
mod vad;

pub use types::{AudioPipelineConfig, AudioState};

const RESAMPLE_TARGET_SAMPLE_RATE: u32 = 16000;
pub const DEFAULT_MIN_TRANSCRIPT_CHARS: usize = 3;
const VAD_FRAME_SIZE_MS: usize = 30; // WebRTC VAD supports 10, 20, or 30 ms frames
const VAD_FRAME_SIZE_SAMPLES: usize =
    (RESAMPLE_TARGET_SAMPLE_RATE as usize * VAD_FRAME_SIZE_MS) / 1000;

#[cfg(feature = "vad-debug")]
const VAD_DEBUG: bool = true;
#[cfg(not(feature = "vad-debug"))]
const VAD_DEBUG: bool = false;

const MIN_ENERGY_THRESHOLD: f32 = 5e-5;

pub struct AudioPipeline {
    config: AudioPipelineConfig,
    whisper_ctx: Arc<Mutex<WhisperContext>>,
    whisper_state: Arc<Mutex<Option<whisper_rs::WhisperState>>>,
    vad: Arc<vad::VadWrapper>,
    audio_buffer: Arc<Mutex<VecDeque<f32>>>,
    speech_buffer: Arc<Mutex<Vec<f32>>>,
    recording_start_time: Arc<AtomicU64>,
    is_recording: Arc<AtomicBool>,
    capture_active: Arc<AtomicBool>,
    last_voice_time: Arc<AtomicU64>,
    silence_start_time: Arc<AtomicU64>,
    has_detected_voice: Arc<AtomicBool>,
    speech_activation_accumulator: Arc<AtomicU64>,
    shutdown: Arc<AtomicBool>,
    state: Arc<AtomicU8>,
    state_events_tx: mpsc::UnboundedSender<AudioState>,
    state_events_rx: Arc<Mutex<Option<mpsc::UnboundedReceiver<AudioState>>>>,
    ui_events: Option<UiEventSender>,
    metrics: Option<MetricsHandle>,
    current_trace: Arc<Mutex<Option<String>>>,
    discard_if_silent: Arc<AtomicBool>,
}

impl Clone for AudioPipeline {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            whisper_ctx: self.whisper_ctx.clone(),
            whisper_state: self.whisper_state.clone(),
            vad: self.vad.clone(),
            audio_buffer: self.audio_buffer.clone(),
            speech_buffer: self.speech_buffer.clone(),
            recording_start_time: self.recording_start_time.clone(),
            is_recording: self.is_recording.clone(),
            capture_active: self.capture_active.clone(),
            last_voice_time: self.last_voice_time.clone(),
            silence_start_time: self.silence_start_time.clone(),
            has_detected_voice: self.has_detected_voice.clone(),
            speech_activation_accumulator: self.speech_activation_accumulator.clone(),
            shutdown: self.shutdown.clone(),
            state: self.state.clone(),
            state_events_tx: self.state_events_tx.clone(),
            state_events_rx: self.state_events_rx.clone(),
            ui_events: self.ui_events.clone(),
            metrics: self.metrics.clone(),
            current_trace: self.current_trace.clone(),
            discard_if_silent: self.discard_if_silent.clone(),
        }
    }
}

impl AudioPipeline {
    pub fn new(
        mut config: AudioPipelineConfig,
        ui_events: Option<UiEventSender>,
        metrics: Option<MetricsHandle>,
    ) -> Result<Self> {
        let resolved_model_path = if config.model_path.exists() {
            config.model_path.clone()
        } else {
            return Err(anyhow!(
                "Whisper model not found at {}; run preload before starting STT",
                config.model_path.display()
            ));
        };

        let model_path_str = resolved_model_path
            .to_str()
            .ok_or_else(|| anyhow!("Model path contains invalid UTF-8"))?
            .to_string();

        config.model_path = resolved_model_path;

        // Ensure whisper.cpp logs are routed into tracing before the context loads the model.
        // Otherwise the native loader prints directly to stdout.
        whisper_rs::install_logging_hooks();

        let mut params = WhisperContextParameters::default();
        params.use_gpu(false);

        let whisper_ctx = WhisperContext::new_with_params(&model_path_str, params)
            .with_context(|| format!("Failed to load Whisper model from {model_path_str}"))?;

        // Log system info for debugging dev vs prod performance differences
        extern "C" {
            fn whisper_print_system_info() -> *const std::os::raw::c_char;
        }

        fn ggml_sysinfo() -> String {
            unsafe {
                std::ffi::CStr::from_ptr(whisper_print_system_info())
                    .to_string_lossy()
                    .into_owned()
            }
        }

        debug!("GGML_SYSINFO: {}", ggml_sysinfo());

        let vad = Arc::new(vad::VadWrapper::new(config.vad_aggressiveness));
        let audio_buffer = Arc::new(Mutex::new(VecDeque::with_capacity(
            (RESAMPLE_TARGET_SAMPLE_RATE as usize) * 30,
        )));

        let speech_buffer = Arc::new(Mutex::new(Vec::new()));
        let recording_start_time = Arc::new(AtomicU64::new(0));
        let is_recording = Arc::new(AtomicBool::new(false));
        let capture_active = Arc::new(AtomicBool::new(false));
        let last_voice_time = Arc::new(AtomicU64::new(0));
        let silence_start_time = Arc::new(AtomicU64::new(0));
        let has_detected_voice = Arc::new(AtomicBool::new(false));
        let speech_activation_accumulator = Arc::new(AtomicU64::new(0));
        let shutdown = Arc::new(AtomicBool::new(false));
        let state = Arc::new(AtomicU8::new(AudioState::Idle as u8));
        let discard_if_silent = Arc::new(AtomicBool::new(false));

        let (state_events_tx, state_events_rx) = mpsc::unbounded_channel();
        let state_events_rx = Arc::new(Mutex::new(Some(state_events_rx)));

        // Emit initial state so anyone subscribing sees Idle immediately.
        let _ = state_events_tx.send(AudioState::Idle);

        info!("Whisper model loaded: {model_path_str}");
        debug!(
            "VAD initialized with aggressiveness: {}",
            config.vad_aggressiveness
        );

        Ok(Self {
            config,
            whisper_ctx: Arc::new(Mutex::new(whisper_ctx)),
            whisper_state: Arc::new(Mutex::new(None)),
            vad,
            audio_buffer,
            speech_buffer,
            recording_start_time,
            is_recording,
            capture_active,
            last_voice_time,
            silence_start_time,
            has_detected_voice,
            speech_activation_accumulator,
            shutdown,
            state,
            state_events_tx,
            state_events_rx,
            ui_events,
            metrics,
            current_trace: Arc::new(Mutex::new(None)),
            discard_if_silent,
        })
    }

    fn publish_state(
        state: &Arc<AtomicU8>,
        tx: &mpsc::UnboundedSender<AudioState>,
        new_state: AudioState,
    ) {
        state.store(new_state as u8, Ordering::Relaxed);
        let _ = tx.send(new_state);
    }

    pub(crate) fn set_state(&self, new_state: AudioState) {
        Self::publish_state(&self.state, &self.state_events_tx, new_state);
    }

    pub fn take_state_events(&self) -> Option<mpsc::UnboundedReceiver<AudioState>> {
        self.state_events_rx.lock().unwrap().take()
    }

    pub fn current_state(&self) -> AudioState {
        AudioState::from_code(self.state.load(Ordering::Relaxed))
    }

    fn begin_turn(&self, trigger: TurnTrigger) -> String {
        let trace_id = if let Some(metrics) = &self.metrics {
            metrics.begin_turn(trigger)
        } else {
            format!("turn-local-{}", crate::metrics::monotonic_now_ms())
        };
        *self.current_trace.lock().unwrap() = Some(trace_id.clone());
        trace_id
    }

    fn current_trace_id(&self) -> Option<String> {
        self.current_trace.lock().unwrap().clone()
    }

    fn clear_current_trace(&self) {
        let _ = self.current_trace.lock().unwrap().take();
    }

    fn mark_vad_stop_if_needed(&self) {
        if let Some(metrics) = &self.metrics {
            if let Some(trace_id) = self.current_trace_id() {
                metrics.mark_vad_stop(&trace_id);
            }
        }
    }

    pub fn start_wake_word_recording(&self) -> Result<()> {
        match self.current_state() {
            AudioState::Idle => {
                self.begin_turn(TurnTrigger::Wakeword);
                debug!("Wake word recording started");
                self.begin_recording(AudioState::WakeWordRecording);
                Ok(())
            }
            AudioState::PttRecording => {
                info!("Wake word detected during PTT - ignoring");
                Ok(())
            }
            state => {
                debug!("Cannot start wake word recording - state: {state:?}");
                Ok(())
            }
        }
    }

    pub fn start_ptt_recording(&self) -> Result<()> {
        match self.current_state() {
            AudioState::Idle => {
                self.begin_turn(TurnTrigger::Ptt);
                debug!("PTT recording started");
                self.begin_recording(AudioState::PttRecording);
                Ok(())
            }
            AudioState::WakeWordRecording => {
                debug!("PTT takes precedence over wake word recording");
                self.begin_turn(TurnTrigger::Ptt);
                self.begin_recording(AudioState::PttRecording);
                Ok(())
            }
            state => {
                debug!("Cannot start PTT recording - state: {state:?}");
                Ok(())
            }
        }
    }

    fn begin_recording(&self, new_state: AudioState) {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        self.recording_start_time.store(now_ms, Ordering::Relaxed);
        self.last_voice_time.store(now_ms, Ordering::Relaxed);
        self.silence_start_time.store(0, Ordering::Relaxed);
        self.has_detected_voice.store(false, Ordering::Relaxed);
        self.speech_activation_accumulator
            .store(0, Ordering::Relaxed);
        self.is_recording.store(true, Ordering::Relaxed);
        self.speech_buffer.lock().unwrap().clear();
        self.audio_buffer.lock().unwrap().clear();
        self.discard_if_silent.store(false, Ordering::Relaxed);
        self.set_state(new_state);
    }

    pub fn stop_recording(&self) -> Result<()> {
        match self.current_state() {
            AudioState::WakeWordRecording | AudioState::PttRecording => {
                if self.is_recording.swap(false, Ordering::Relaxed) {
                    debug!("Stopping recording");
                    self.mark_vad_stop_if_needed();
                    Self::publish_state(&self.state, &self.state_events_tx, AudioState::Processing);
                }
                Ok(())
            }
            state => {
                debug!("No recording to stop - state: {state:?}");
                Ok(())
            }
        }
    }

    pub fn mark_discard_if_silent(&self) {
        self.discard_if_silent.store(true, Ordering::Relaxed);
        if let (Some(metrics), Some(trace_id)) = (&self.metrics, self.current_trace_id().as_ref()) {
            metrics.mark_discardable(trace_id);
        }
    }

    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }
}
