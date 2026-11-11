# Architecture Decision: Wake Word + STT/VAD Split

## Decision Made
**Python handles wake word detection only, Rust handles STT/VAD/audio processing**

## Rationale

### Why This Split?
1. **Python Wake Word**: Porcupine is the best wake word detection library, specialized for this task
2. **Rust STT/VAD**: Better performance for real-time audio processing, already working in PTT
3. **Isolation**: Wake word failures don't break core audio functionality
4. **Performance**: Rust is faster for continuous audio processing

### Architecture
```
┌─────────────────┐    ┌──────────────────┐
│   Rust App      │    │  Python Sidecar  │
│                 │    │                  │
│ • Audio Capture │    │ • Wake Word      │
│ • VAD           │◄──►│   Detection      │
│ • Whisper STT   │    │ • Signal Rust    │
│ • PTT           │    │                  │
│ • App Logic     │    │                  │
└─────────────────┘    └──────────────────┘
```

## Implementation Plan

### Phase 1: Async Integration (Current Priority)
- [x] Make Python wake word detection async/non-blocking
- [x] Convert Rust sidecar communication to async tokio I/O
- [x] Ensure Rust can handle wake word signals without blocking
- [x] Test async communication between Rust and Python

### Phase 2: Audio Pipeline Architecture ✅
- [x] Create audio pipeline module (`audio_pipeline.rs`)
- [x] Implement audio capture and buffering
- [x] Integrate Whisper STT processing
- [x] Centralize audio state handling inside `audio_pipeline.rs`
- [ ] Fix VAD thread safety issues (temporarily disabled)

### Phase 3: Integration ✅
- [x] Connect wake word triggers to audio pipeline
- [x] Integrate PTT with audio pipeline
- [x] Ensure concurrent state management
- [x] Implement self-terminating Python sidecar

### Phase 4: Testing & Refinement 🔄
- [ ] Test wake word → Rust audio processing flow
- [ ] Test PTT independently
- [ ] Test concurrent wake word + PTT scenarios
- [ ] Fix remaining issues (VAD, state management)

## Current Status
- ✅ Python sidecar: Wake word detection working (demo mode + self-termination)
- ✅ Rust PTT: Working with audio pipeline integration
- ✅ Rust Audio Pipeline: Audio capture and Whisper STT implemented
- ✅ State Management: Concurrent-safe audio state management
- ✅ Integration: Wake word triggers connected to audio pipeline
- ⚠️ Rust VAD: Temporarily disabled due to thread safety issues
- 🔄 Testing: Ready for concurrent audio testing

## Technical Requirements

### Python Sidecar (Minimal) ✅
- ✅ Wake word detection only (Porcupine)
- ✅ Async signal to Rust via stdout
- ✅ Self-termination via parent PID monitoring
- ✅ Production-ready logging
- ✅ No audio processing beyond wake word
- ✅ No blocking operations

### Rust Integration (Async) ✅
- ✅ Non-blocking wake word signal handling (tokio)
- ✅ Audio pipeline with Whisper STT
- ✅ PTT compatibility maintained
- ✅ Async event handling
- ✅ Concurrent state management
- ⚠️ VAD temporarily disabled (thread safety)

## Files to Modify
- `apps/tauri/src-tauri/src/stt/hybrid_service.rs` - Async wake word handling
- `apps/tauri/src-tauri/src/stt/mod.rs` - Restore audio processing
- `apps/voice-service/voice_processor.py` - Ensure async operation
- `apps/voice-service/wake_word_detector.py` - Non-blocking detection

## Success Criteria
- Wake word triggers Rust audio processing
- PTT works independently
- No blocking operations
- Reliable async communication
- Fallback handling for failures
