use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use anyhow::{Context, Result};
use rodio::{Decoder, OutputStream, Sink};
use tracing::{debug, warn};

pub fn play_file(path: &Path, volume: f32, device_hint: Option<&str>) -> Result<()> {
    if let Some(device) = device_hint {
        warn!(
            "Requested audio device '{}' is not yet customisable; using system default",
            device
        );
    }

    debug!(
        "Starting playback of {} at volume {}",
        path.display(),
        volume
    );

    let (stream, handle) =
        OutputStream::try_default().context("Failed to open default audio output stream")?;

    let file = File::open(path)
        .with_context(|| format!("Failed to open generated audio {}", path.display()))?;
    let decoder = Decoder::new(BufReader::new(file)).context("Failed to decode audio")?;
    let sink = Sink::try_new(&handle).context("Failed to create audio sink")?;
    let clamped_volume = volume.max(0.0);
    if (clamped_volume - volume).abs() > f32::EPSILON {
        warn!(
            "Requested volume {} below zero; clamping to {}",
            volume, clamped_volume
        );
    }
    sink.set_volume(clamped_volume);
    sink.append(decoder);
    sink.sleep_until_end();
    debug!("Playback completed for {}", path.display());
    drop(sink);
    drop(handle);
    drop(stream);
    Ok(())
}
