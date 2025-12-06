# Virtual Matter Bridge

A Rust application that exposes RTSP camera streams as Matter 1.5 video doorbell devices, enabling integration with Matter-compatible smart home ecosystems like Apple Home, Google Home, and Amazon Alexa.

## Overview

This project implements a virtual Matter bridge that:
- Connects to RTSP camera streams (e.g., IP cameras, NVRs)
- Exposes them as Matter 1.5 Video Doorbell devices
- Provides WebRTC-based video streaming to Matter controllers
- Simulates doorbell press events with configurable chime sounds

## Architecture

```
┌─────────────────┐     ┌──────────────────────┐     ┌─────────────────┐
│   RTSP Camera   │────▶│  Virtual Matter      │────▶│ Matter Controller│
│  (IP Camera)    │     │      Bridge          │     │ (Apple/Google/  │
└─────────────────┘     │                      │     │  Amazon)        │
                        │  ┌────────────────┐  │     └─────────────────┘
                        │  │ Video Doorbell │  │
                        │  │    Device      │  │
                        │  └───────┬────────┘  │
                        │          │           │
                        │  ┌───────┴────────┐  │
                        │  │   Clusters:    │  │
                        │  │ • Camera AV    │  │
                        │  │ • WebRTC       │  │
                        │  │ • Chime        │  │
                        │  └────────────────┘  │
                        └──────────────────────┘
```

## Matter 1.5 Clusters Implemented

| Cluster | ID | Status | Description |
|---------|-----|--------|-------------|
| Camera AV Stream Management | `0x0551` | ✅ Implemented | Video/audio stream allocation, codec negotiation |
| WebRTC Transport Provider | `0x0553` | ✅ Implemented | WebRTC session management, SDP/ICE handling |
| Chime | `0x0556` | ✅ Implemented | Doorbell sounds configuration and playback |

## Project Status

### Completed

- [x] Project structure following Rust best practices
- [x] Configuration system with environment variable overrides
- [x] Error handling with `thiserror`
- [x] **Camera AV Stream Management Cluster**
  - Video stream allocation/deallocation
  - Audio stream allocation/deallocation
  - Codec support (H.264, HEVC, VVC, AV1, Opus, AAC)
  - Resolution and framerate configuration
  - Stream usage types (LiveView, Recording, Analysis)
- [x] **WebRTC Transport Provider Cluster**
  - Session creation and management
  - SDP offer/answer generation
  - ICE candidate handling
  - STUN/TURN server configuration
- [x] **Chime Cluster**
  - Configurable chime sounds
  - Enable/disable functionality
  - Play chime command
- [x] **RTSP Client** (stub implementation)
  - URL parsing and validation
  - Connection state management
  - Stream info retrieval
- [x] **RTSP to WebRTC Bridge** (stub implementation)
  - Session management
  - Frame forwarding infrastructure
  - Statistics tracking
- [x] **Video Doorbell Device**
  - Combines all clusters
  - Doorbell press simulation
  - Device initialization

### In Progress

- [ ] **Actual RTSP Streaming**
  - Integration with `retina` crate for H.264/AAC depacketization
  - RTP/RTCP handling
  - Frame extraction and buffering

### Planned

- [ ] **WebRTC Integration**
  - Peer connection establishment with `webrtc` crate
  - Media track creation
  - ICE connectivity checks
  - DTLS-SRTP encryption
- [ ] **H.264 to WebRTC Transcoding**
  - RTP packetization for WebRTC
  - Timestamp synchronization
  - Keyframe detection and handling
- [ ] **Matter Stack Integration**
  - Connect clusters to `rs-matter` data model
  - Device attestation
  - Commissioning flow (QR code, manual pairing)
  - Fabric management
- [ ] **mDNS Advertisement**
  - Service discovery for Matter controllers
- [ ] **Persistent Storage**
  - Fabric credentials
  - Device configuration
- [ ] **Two-Way Audio** (optional)
  - Microphone input from Matter controller
  - Audio forwarding to RTSP camera (if supported)
- [ ] **Motion Detection Events**
  - Zone management cluster
  - Event notifications to Matter controllers
- [ ] **Snapshot Support**
  - Still image capture
  - Snapshot stream management

## Configuration

Configuration is loaded from environment variables with sensible defaults:

| Variable | Default | Description |
|----------|---------|-------------|
| `RTSP_URL` | `rtsp://username:password@10.0.0.38:554/h264Preview_01_main` | Camera RTSP stream URL |
| `RTSP_USERNAME` | - | RTSP authentication username |
| `RTSP_PASSWORD` | - | RTSP authentication password |
| `DEVICE_NAME` | `Virtual Doorbell` | Matter device name |
| `MATTER_DISCRIMINATOR` | `3840` | Matter pairing discriminator |
| `MATTER_PASSCODE` | `20202021` | Matter pairing passcode |

## Building

```bash
# Check compilation
make check

# Build release binary
make build

# Run in development
make run
```

## Dependencies

Key dependencies:
- **rs-matter** - Rust Matter protocol implementation
- **webrtc** - WebRTC stack for video streaming
- **retina** - RTSP client for camera connections
- **tokio** - Async runtime

## Requirements

- Rust 2024 edition (nightly)
- RTSP camera with H.264 video stream
- Network connectivity to camera and Matter controller

## License

MIT

## References

- [Matter 1.5 Specification](https://csa-iot.org/developer-resource/specifications-download-request/)
- [rs-matter](https://github.com/project-chip/rs-matter)
- [connectedhomeip Camera Example](https://github.com/project-chip/connectedhomeip/tree/master/examples/camera-app/linux)
