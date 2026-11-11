# Concurrent Audio Processing Design

## Implementation Status

### ✅ Completed
- Python wake word detection with Porcupine
- Self-terminating Python sidecar (PID monitoring)
- Async Rust ↔ Python communication (tokio)
- Audio pipeline architecture with Whisper STT
- Unified audio state management inside `audio_pipeline.rs`
- Wake word trigger integration
- PTT integration
- Production logging
- Clean executable packaging (~9.8 MB)

### ⚠️ Known Issues
- VAD temporarily disabled (thread safety)
- Needs concurrent scenario testing

### 🔄 Next Steps
1. Fix VAD thread safety issues
2. Test wake word → STT flow
3. Test PTT → STT flow
4. Test concurrent wake word + PTT
5. Final integration testing

## Current State Analysis

### Files to Clean Up
- `mod.rs.backup` - Contains old STT implementation with Whisper/VAD
- `mod_old.rs` - Another backup with similar functionality
- Redundant code in current `mod.rs` (placeholder PTT implementation)

### Working Components
- ✅ Python sidecar: Wake word detection (async)
- ✅ Rust async communication: Non-blocking sidecar communication
- ✅ PTT hotkey registration: Working
- ✅ Event emission: `EVENT_STT_STARTED` working

### Missing Components
- ❌ Rust audio capture pipeline
- ❌ VAD processing
- ❌ Whisper STT processing
- ❌ Concurrent state management

## Concurrent Audio Processing Cases

### Case 1: Wake Word → STT Processing
**Scenario**: User says wake word, then speaks their command
```
Wake Word Detected → Start Audio Recording → VAD → Whisper → Emit Result
```

### Case 2: PTT → STT Processing  
**Scenario**: User holds PTT key and speaks
```
PTT Pressed → Start Audio Recording → PTT Released → VAD → Whisper → Emit Result
```

### Case 3: Wake Word + PTT Conflict
**Scenario**: Wake word detected while PTT is already active
```
PTT Active + Wake Word Detected → PTT takes precedence → Continue PTT flow
```

### Case 4: Multiple Wake Words
**Scenario**: Multiple wake words detected in quick succession
```
Wake Word 1 → Recording → Wake Word 2 → Ignore/Queue → Complete Recording 1
```

### Case 5: Wake Word in Speech
**Scenario**: User says "Hey Google, open Chrome" - wake word is part of speech
```
Wake Word Detected → Start Recording → Continue recording speech → Process full audio
```

### Case 6: PTT Interrupts Wake Word
**Scenario**: Wake word starts recording, user presses PTT
```
Wake Word Recording → PTT Pressed → Cancel Wake Word → Start PTT Recording
```

### Case 7: System Overload
**Scenario**: Multiple triggers happening simultaneously
```
Multiple Triggers → State Machine → Queue/Throttle → Process in Order
```

## State Machine Design

### Audio States
```rust
enum AudioState {
    Idle,                    // No audio processing
    WakeWordRecording,       // Recording after wake word
    PttRecording,           // Recording during PTT
    Processing,             // Whisper processing
    Error,                  // Error state
}
```

### Transition Rules
1. **Idle → WakeWordRecording**: Wake word detected
2. **Idle → PttRecording**: PTT pressed
3. **WakeWordRecording → PttRecording**: PTT pressed (PTT takes precedence)
4. **WakeWordRecording → Processing**: VAD detects end of speech
5. **PttRecording → Processing**: PTT released
6. **Processing → Idle**: Whisper processing complete
7. **Any → Error**: Error occurs
8. **Error → Idle**: Error resolved

### Concurrent Access Patterns
- **Wake Word**: Async signal from Python sidecar
- **PTT**: Synchronous hotkey events
- **Audio Capture**: Continuous background thread
- **VAD**: Real-time audio analysis
- **Whisper**: CPU-intensive processing

## Implementation Plan

### Phase 1: Code Cleanup ✅
1. ✅ Remove redundant backup files (`mod.rs.backup`, `mod_old.rs`)
2. ✅ Clean up placeholder implementations
3. ✅ Identify reusable components from old implementations

### Phase 2: Audio Pipeline Architecture ✅
1. ✅ Put audio state machine inside `audio_pipeline.rs`
2. ✅ Implement concurrent-safe audio capture (`audio_pipeline.rs`)
3. ⚠️ Add VAD processing pipeline (temporarily disabled due to thread safety)
4. ✅ Integrate Whisper STT processing (`audio_pipeline.rs`)

### Phase 3: Integration ✅
1. ✅ Connect wake word triggers to audio pipeline
2. ✅ Integrate PTT with audio pipeline
3. ✅ Implement state machine transitions
4. ⚠️ Add error handling and recovery (basic implementation)

### Phase 4: Testing 🔄
1. ✅ Test individual components (compilation working)
2. ✅ Test Python sidecar self-termination (working)
3. 🔄 Test concurrent scenarios (ready for testing)
4. ⏳ Test error conditions
5. ⏳ Performance testing

### Phase 5: Production Readiness 🔄
1. ✅ Clean up debug logging
2. ✅ Implement self-terminating sidecar
3. ✅ Production logging with error tracking
4. ⏳ Fix VAD thread safety
5. ⏳ Final integration testing

## Technical Requirements

### Thread Safety
- All shared state must be thread-safe
- Use Arc<Mutex<>> or Arc<RwLock<>> for shared data
- Minimize lock contention
- Use channels for async communication

### Performance
- Audio capture: Real-time (no blocking)
- VAD: Low latency detection
- Whisper: Background processing
- Memory: Efficient buffer management

### Error Handling
- Graceful degradation
- Recovery mechanisms
- User feedback
- Logging and debugging

## Files to Modify

### Core Files
- `apps/tauri/src-tauri/src/stt/mod.rs` - Main STT service
- `apps/tauri/src-tauri/src/stt/hybrid_service.rs` - Wake word integration
- `apps/tauri/src-tauri/src/stt/audio_pipeline.rs` - New audio processing
- `apps/tauri/src-tauri/src/stt/audio_pipeline.rs` - Centralized state + audio processing

### Backup Files to Remove
- `apps/tauri/src-tauri/src/stt/mod.rs.backup`
- `apps/tauri/src-tauri/src/stt/mod_old.rs`

### Components to Extract
- Whisper processing from backup files
- VAD implementation from backup files
- Audio capture logic from backup files
