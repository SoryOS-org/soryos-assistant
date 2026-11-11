pub mod sprite_window;
pub mod state_router;

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime},
};

use async_channel::{bounded, Receiver, Sender};
use serde::Serialize;
use tauri::{async_runtime, AppHandle, Emitter};
use tokio::time::{interval, MissedTickBehavior};
use tracing::{debug, trace, warn};

pub const UI_EVENT_CHANNEL: &str = "sprite://ui";
const QUEUE_CAPACITY: usize = 128;
const VU_MIN_INTERVAL: Duration = Duration::from_millis(33);

#[derive(Debug, Copy, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum UiVisualState {
    Idle,
    Listening,
    Speaking,
    Thinking,
    Planning,
    Error,
}

#[derive(Debug, Copy, Clone, Serialize)]
pub struct UiStatePayload {
    pub state: UiVisualState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<u64>,
}

#[derive(Debug, Copy, Clone, Serialize)]
pub struct UiVuPayload {
    pub level: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", content = "payload")]
pub enum UiEvent {
    State(UiStatePayload),
    Vu(UiVuPayload),
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct UiSnapshot {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<UiStatePayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vu: Option<UiVuPayload>,
}

struct QueuedUiEvent {
    event: UiEvent,
    enqueued_at: Instant,
}

#[derive(Clone)]
pub struct UiEventSender {
    sender: Sender<QueuedUiEvent>,
}

impl UiEventSender {
    pub fn emit_state(&self, state: UiVisualState) {
        let payload = UiStatePayload {
            state,
            timestamp: Some(unix_timestamp_ms()),
        };
        self.enqueue(UiEvent::State(payload));
    }

    pub fn emit_vu_level(&self, level: f32) {
        let clamped = level.clamp(0.0, 1.0);
        let payload = UiVuPayload {
            level: clamped,
            timestamp: Some(unix_timestamp_ms()),
        };
        self.enqueue(UiEvent::Vu(payload));
    }

    fn enqueue(&self, event: UiEvent) {
        let queued = QueuedUiEvent {
            event,
            enqueued_at: Instant::now(),
        };

        if let Err(err) = self.sender.try_send(queued) {
            if err.is_full() {
                warn!("ui event queue is full; dropping latest event");
            } else if err.is_closed() {
                warn!("ui event queue closed; dropping event");
            }
        }
    }
}

struct UiEventInner {
    sender: Sender<QueuedUiEvent>,
    snapshot: Mutex<UiSnapshot>,
}

#[derive(Clone)]
pub struct UiEventService {
    inner: Arc<UiEventInner>,
}

impl UiEventService {
    pub fn start(app_handle: AppHandle) -> Self {
        let (sender, receiver) = bounded(QUEUE_CAPACITY);
        let inner = Arc::new(UiEventInner {
            sender,
            snapshot: Mutex::new(UiSnapshot::default()),
        });

        let dispatcher = UiEventDispatcher::new(app_handle, receiver, inner.clone());
        async_runtime::spawn(dispatcher.run());

        Self { inner }
    }

    pub fn sender(&self) -> UiEventSender {
        UiEventSender {
            sender: self.inner.sender.clone(),
        }
    }

    pub fn snapshot(&self) -> UiSnapshot {
        self.inner
            .snapshot
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }
}

struct UiEventDispatcher {
    app_handle: AppHandle,
    receiver: Receiver<QueuedUiEvent>,
    inner: Arc<UiEventInner>,
}

impl UiEventDispatcher {
    fn new(
        app_handle: AppHandle,
        receiver: Receiver<QueuedUiEvent>,
        inner: Arc<UiEventInner>,
    ) -> Self {
        Self {
            app_handle,
            receiver,
            inner,
        }
    }

    async fn run(self) {
        let mut vu_interval = interval(VU_MIN_INTERVAL);
        vu_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        let mut pending_vu: Option<QueuedUiEvent> = None;
        let mut last_state: Option<UiVisualState> = None;

        loop {
            tokio::select! {
                maybe_event = self.receiver.recv() => {
                    match maybe_event {
                        Ok(queued) => {
                            match &queued.event {
                                UiEvent::State(payload) => {
                                    if last_state == Some(payload.state) {
                                        debug!(
                                            state = ?payload.state,
                                            "ui dispatcher deduplicated consecutive state"
                                        );
                                        continue;
                                    }
                                    last_state = Some(payload.state);
                                    self.emit_event(queued).await;
                                }
                                UiEvent::Vu(_) => {
                                    pending_vu = Some(queued);
                                }
                            }
                        }
                        Err(_) => {
                            trace!("ui event receiver closed; stopping dispatcher");
                            break;
                        }
                    }
                }
                _ = vu_interval.tick() => {
                    if let Some(queued) = pending_vu.take() {
                        self.emit_event(queued).await;
                    }
                }
            }
        }
    }

    async fn emit_event(&self, queued: QueuedUiEvent) {
        let latency_ms = queued.enqueued_at.elapsed().as_millis();
        trace!("emitting ui event after {} ms in queue", latency_ms);

        if let Err(err) = self.app_handle.emit(UI_EVENT_CHANNEL, &queued.event) {
            warn!("failed to emit ui event: {err}");
        } else if let UiEvent::State(payload) = &queued.event {
            debug!(
                state = ?payload.state,
                timestamp = ?payload.timestamp,
                "ui dispatcher forwarded state"
            );
        }

        self.update_snapshot(&queued.event);
    }

    fn update_snapshot(&self, event: &UiEvent) {
        if let Ok(mut snapshot) = self.inner.snapshot.lock() {
            match event {
                UiEvent::State(payload) => {
                    snapshot.state = Some(*payload);
                }
                UiEvent::Vu(payload) => {
                    snapshot.vu = Some(*payload);
                }
            }
        }
    }
}

fn unix_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}
