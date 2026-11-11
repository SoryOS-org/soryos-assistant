# Clippy v2 - Project History

## Project Evolution

This document tracks how the Clippy v2 project evolved from initial concept to current implementation.

## October 2025 Refresh

- Reworked wake-word VAD buffering: frames now accumulate between callbacks and wake-word sessions honour a 3s grace window before silence evaluation.
- Added configurable `vad.start_grace_ms` and made logging purely config-driven via telemetry level setting.
- Archived legacy architecture docs under `docs/archive/` and introduced an up-to-date `ARCHITECTURE.md`.
- Documented the `sprite://ui` persona event channel alongside the backend/frontend wiring.
- Removed the stale `hybrid_test.rs` harness while keeping this history document as the nostalgia log.
- Vendored `whisper-rs` locally with an explicit `-O2/-DNDEBUG` patch to eliminate the release-build latency gap (see `docs/issues/whisper_release_patch.md`).

## Phase 1: Foundation (Completed ✅)

### **Initial Vision (`agent.md`)**
- **Goal**: Bundle Whisper STT inside Tauri desktop app
- **Approach**: Direct Rust integration with Whisper
- **Status**: ✅ COMPLETED - Basic Whisper integration working

### **Architecture Evolution (`AgentV2.md`)**
- **Problem**: Direct Rust integration was complex and limiting
- **Solution**: Hybrid architecture with Python sidecar
- **Key Decision**: Use Python for voice processing, Rust for orchestration
- **Benefits**: 
  - Easier voice processing in Python
  - Better wake-word detection with Porcupine
  - More flexible configuration
  - Swappable STT providers

### **Implementation Journey (`PYTHON_SIDECAR_IMPLEMENTATION.md`)**
- **Challenge**: Python sidecar communication with Rust
- **Solution**: stdin/stdout protocol with JSON messages
- **Result**: Seamless integration between Python and Rust

## Key Technical Decisions

### **1. Hybrid Architecture**
**Decision**: Python sidecar + Rust orchestration
**Reasoning**: 
- Python excels at audio processing and ML libraries
- Rust provides performance and system integration
- Clear separation of concerns

### **2. Configuration-Driven Design**
**Decision**: All settings in `stt.config.json`
**Reasoning**:
- Single source of truth for configuration
- Easy to modify without recompilation
- Supports live reloading

### **3. Wake Word Strategy**
**Decision**: Porcupine with configurable keywords
**Reasoning**:
- Industry-standard wake word detection
- Multiple keyword options
- Fallback to PTT mode when needed

### **4. Communication Protocol**
**Decision**: JSON over stdin/stdout
**Reasoning**:
- Simple and reliable
- Cross-platform compatibility
- Easy to debug and monitor

## Lessons Learned

### **What Worked Well:**
1. **Modular Design**: Clear separation between components
2. **Configuration Management**: Centralized config with live reloading
3. **Testing Strategy**: Comprehensive test coverage with mocks
4. **Documentation**: Detailed implementation guides

### **What We'd Do Differently:**
1. **Start with Hybrid**: Should have planned Python sidecar from the beginning
2. **Configuration First**: Design config schema before implementation
3. **Testing Earlier**: More integration testing during development

## Implementation History

### **Initial Approach**: Direct Rust Integration
- Started with direct Whisper integration in Rust
- Basic audio capture and transcription working
- Simple frontend integration

### **Architecture Pivot**: Hybrid Design
- Realized limitations of direct Rust approach for voice processing
- Designed Python sidecar for voice processing
- Implemented stdin/stdout communication protocol
- Added comprehensive testing and configuration management

## Current State (Phase 1 Complete)

### **Achievements:**
- ✅ Hybrid voice processing pipeline
- ✅ Wake-word detection with Porcupine
- ✅ Voice Activity Detection (VAD)
- ✅ Push-to-Talk (PTT) mode
- ✅ Configuration-driven design
- ✅ Comprehensive testing
- ✅ Live configuration reloading

### **Next Phase Goals:**
- Link up LLM in the worker to make it intelligent
- Make sprite animations responsive after validation
- Improve error handling and recovery

## Technical Debt & Future Improvements

### **Known Issues:**
1. **Error Recovery**: Better handling of sidecar failures
2. **Performance**: Optimize audio processing pipeline
3. **Configuration**: More granular settings
4. **Testing**: More edge case coverage

### **Cross-Platform Considerations:**
1. **Python Executable**: PyInstaller builds may need platform-specific adjustments
2. **Audio Libraries**: pyaudio and webrtcvad have platform-specific requirements
3. **Wake Word Detection**: Porcupine access key management across platforms
4. **Build Process**: Different build requirements for Windows, macOS, and Linux

## Archive of Old Plans

### **Original Agent Plan (`agent.md`)**
- Direct Rust integration approach
- Simpler architecture
- Limited flexibility
- **Status**: Superseded by hybrid approach

### **Gemini Evolution Plan (`GEMINI.md`)**
- Strategic roadmap for V2
- Performance and reliability focus
- **Status**: Partially implemented, ongoing

### **Testing Guide (`CONCRETE_TESTING_GUIDE.md`)**
- Comprehensive testing procedures
- Debug and troubleshooting guides
- **Status**: Integrated into DEVELOPMENT.md

---

*This history document will be updated as the project continues to evolve.*
