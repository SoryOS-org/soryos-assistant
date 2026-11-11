# Audio Pipeline Fixes

## Issues Identified

### 0. **CRITICAL: VAD Auto-Stopping PTT Recording** 🚨
**Problem**: PTT recording was being auto-stopped by VAD after 600ms of silence, **before the user finished speaking**. This caused:
1. Only capturing 20ms of audio (too short, filtered)
2. Capturing incomplete speech like "open" instead of "open google chrome"
3. 5-10 second lag between PTT press and processing
4. Transcriptions showing `[BLANK_AUDIO]` or being filtered as noise

**Root Cause**: The VAD silence detection logic (line 527-531) was running for **both** WakeWord AND PTT modes. In PTT mode, the user controls when to stop recording by releasing the key - VAD should NOT auto-stop.

**Evidence from Terminal**:
```
293|🎤 PTT ACTIVATED via frontend - Start speaking now!
... (user speaks "open google chrome")
302|🛑 PTT recording stopped  ← VAD auto-stopped after 600ms silence
303|🔄 Processing audio buffer...
304|🔇 Audio too short: 20ms (min: 500ms) - ignoring  ← Only captured 20ms!
```

**Fix**: Modified VAD logic to **only** auto-stop for `WakeWordRecording` state, NOT for `PttRecording`:

```rust
// BEFORE (BROKEN):
if !is_voice && silence_duration >= 600ms {
    stop_recording(); // Auto-stops for BOTH wake word AND PTT
}

// AFTER (FIXED):
if !is_voice {
    let current_state = state.lock().unwrap().clone();
    if matches!(current_state, AudioState::WakeWordRecording) {
        // Only auto-stop for wake word mode
        if silence_duration >= 600ms {
            stop_recording();
        }
    }
    // For PTT mode, do NOT auto-stop - user controls when to stop
}
```

### 1. **CRITICAL: Infinite Processing Loop (BLOCKING BUG)** 🚨
**Problem**: The audio processing loop was stuck infinitely printing `🔄 Processing audio buffer...` hundreds of times per second, making the app completely unusable. PTT input ("open google chrome") was never processed.

**Root Cause**: The `audio_processing_loop` checked if state was `Processing` every 100ms, but:
1. It called `process_speech_buffer()` which is **slow** (Whisper inference takes seconds)
2. The state was only reset to `Idle` **at the end** of `process_speech_buffer`
3. By the time processing finished, the loop had already re-entered hundreds of times
4. Each re-entry called Whisper again, creating a cascading blocking loop

**Evidence from Terminal**:
```
452|🔄 Processing audio buffer...
453|DEBUG: Waiting for wake word detection (async)...
454|🔄 Processing audio buffer...
455|🔄 Processing audio buffer...
... (repeated 250+ times)
549|PTT triggered, starting recording...
550|⚠️ Cannot start PTT recording - already in state: Processing
```

**Fix**: 
1. **Immediately** change state to `Idle` when entering the processing block (line 223)
2. This prevents re-entry before Whisper finishes
3. Added `pipeline.set_state()` method for atomic state changes

```rust
// BEFORE (BROKEN):
if matches!(current_state, AudioState::Processing) {
    println!("🔄 Processing audio buffer...");
    pipeline.process_speech_buffer(...).await;
    // State only reset here, after Whisper finishes
}

// AFTER (FIXED):
if matches!(current_state, AudioState::Processing) {
    pipeline.set_state(AudioState::Idle); // IMMEDIATE reset
    println!("🔄 Processing audio buffer...");
    pipeline.process_speech_buffer(...).await;
}
```

### 1. **Excessive Whisper Logging**
**Problem**: Whisper was outputting hundreds of lines of verbose decoding information to the terminal, making debugging impossible and cluttering the user experience.

**Example**:
```
whisper_full_with_state: id =  16, decoder = 0, token =     82, p =  0.605, ts =        [?],  0.024, result_len =   15 's'
whisper_full_with_state: id =  17, decoder = 0, token =    394, p =  0.972, ts =        [?],  0.042, result_len =   15 'igh'
... (hundreds more lines)
```

**Root Cause**: Whisper's `FullParams` was not configured to suppress logging.

**Fix**: Set print flags to false in `FullParams`:
```rust
params.set_print_progress(false);
params.set_print_realtime(false);
params.set_print_timestamps(false);
```

### 2. **Noise Transcription ("(sighs)")**
**Problem**: Whisper was transcribing background noise and silence as "(sighs) (sighs) (sighs)..." which was then sent to the worker API, wasting resources.

**Root Cause**: No filtering for noise artifacts that Whisper commonly outputs when processing silence or background noise.

**Fix**: Added comprehensive noise detection filters:
```rust
let transcript_lower = transcript.to_lowercase();
let is_noise = transcript_lower.contains("(sigh")
    || transcript_lower.contains("[ blank")
    || transcript_lower.contains("[blank")
    || transcript_lower == "blank"
    || transcript_lower.starts_with("(")
    || (transcript.chars().all(|c| c.is_whitespace() || c == '(' || c == ')'));
```

### 3. **Short Audio Processing**
**Problem**: Very short audio clips (< 500ms) were being processed, often containing only background noise or accidental key presses.

**Root Cause**: No minimum duration check before running Whisper inference.

**Fix**: Added minimum audio duration check:
```rust
let audio_duration_ms = (speech_buffer.len() as f32 / RESAMPLE_TARGET_SAMPLE_RATE as f32 * 1000.0) as u64;
const MIN_AUDIO_DURATION_MS: u64 = 500; // Minimum 500ms of audio

if audio_duration_ms < MIN_AUDIO_DURATION_MS {
    println!("🔇 Audio too short: {}ms (min: {}ms) - ignoring", ...);
    *self.state.lock().unwrap() = AudioState::Idle;
    return Ok(());
}
```

## Files Modified

- `apps/tauri/src-tauri/src/stt/hybrid_service.rs`
  - **Line 223: CRITICAL FIX** - Added immediate state reset to prevent infinite loop
  - Simplified audio processing loop logic
  
- `apps/tauri/src-tauri/src/stt/audio_pipeline.rs`
  - **Line 520-538: CRITICAL FIX** - VAD now only auto-stops for WakeWord mode, NOT PTT
  - **Line 424-426: Added `set_state()` method** for atomic state changes
  - Line 330-336: Added Whisper logging suppression (still shows some output, needs further work)
  - Line 324-335: Added minimum audio duration check
  - Line 359-378: Enhanced noise filtering
  - Removed redundant state resets (now handled by processing loop)

## Expected Behavior Now

1. **Clean Terminal Output**: Only relevant messages (wake word detection, PTT triggers, actual transcripts) will be shown
2. **No Noise Processing**: Background noise and silence will be filtered out before sending to worker
3. **Resource Efficiency**: Short/invalid audio clips won't trigger Whisper inference
4. **Better UX**: Users will only see meaningful transcriptions

## Testing Recommendations

1. **Test Wake Word**: Say wake word, then speak - verify transcript appears
2. **Test PTT**: Hold Ctrl+Space, speak, release - verify transcript appears
3. **Test Short Press**: Quick tap Ctrl+Space - verify NO transcript (too short)
4. **Test Silence**: Hold PTT but don't speak - verify NO transcript (filtered as noise)
5. **Test Background Noise**: Let app run idle - verify NO spurious transcripts

## What Still Needs Work (Future)

1. **Async State Management**: Current implementation uses blocking loops with `Mutex`, should be refactored to use proper async channels/queues
2. **Audio Level Threshold**: Could add RMS/peak detection to filter out very quiet audio before VAD
3. **VAD Tuning**: The `end_silence_ms` (600ms) might need adjustment based on user feedback
4. **Configurable Thresholds**: Move `MIN_AUDIO_DURATION_MS` and noise filters to config file

