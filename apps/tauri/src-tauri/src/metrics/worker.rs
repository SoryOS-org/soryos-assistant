use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use async_channel::Receiver;
use tracing::{debug, error};

use super::recorder::{JsonlRecorder, MetricsRecorder};
use super::MetricsEvent;

pub(crate) enum MetricsCommand {
    Event(MetricsEvent),
    #[allow(dead_code)]
    // Reserved for forcing recorder flushes when running targeted diagnostics.
    Flush,
    Shutdown,
}

pub(crate) struct MetricsWorker {
    join: Mutex<Option<thread::JoinHandle<()>>>,
}

impl Drop for MetricsWorker {
    fn drop(&mut self) {
        if let Some(handle) = self.join.lock().unwrap().take() {
            if let Err(err) = handle.join() {
                debug!("Metrics worker thread terminated with error: {err:?}");
            }
        }
    }
}

pub(crate) fn spawn_writer(
    mut recorder: JsonlRecorder,
    receiver: Receiver<MetricsCommand>,
    write_errors: Arc<AtomicU64>,
) -> MetricsWorker {
    let join = thread::spawn(move || {
        while let Ok(command) = receiver.recv_blocking() {
            match command {
                MetricsCommand::Event(event) => {
                    if let Err(err) = recorder.write_event(&event) {
                        write_errors.fetch_add(1, Ordering::Relaxed);
                        error!("Failed to persist metrics event: {err:?}");
                    }
                }
                MetricsCommand::Flush => {
                    if let Err(err) = recorder.flush() {
                        write_errors.fetch_add(1, Ordering::Relaxed);
                        error!("Failed to flush metrics recorder: {err:?}");
                    }
                }
                MetricsCommand::Shutdown => {
                    if let Err(err) = recorder.flush() {
                        write_errors.fetch_add(1, Ordering::Relaxed);
                        error!("Failed to flush metrics recorder during shutdown: {err:?}");
                    }
                    break;
                }
            }
        }
    });

    MetricsWorker {
        join: Mutex::new(Some(join)),
    }
}
