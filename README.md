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

- [ ] **Matter Stack Integration**
  - [x] Basic Matter stack initialization with `rs-matter`
  - [x] UDP transport on port 5540
  - [x] Commissioning window (PASE) with QR code and manual pairing code
  - [x] mDNS advertisement via Avahi (custom responder for interface filtering)
  - [ ] Connect clusters to `rs-matter` data model
  - [ ] Device attestation with production credentials
  - [ ] Fabric management and persistence
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

## NixOS Configuration

This application requires specific Avahi settings on NixOS to enable mDNS service publishing:

```nix
{
  # Enable Avahi for mDNS
  services.avahi = {
    enable = true;

    # Allow user applications to publish mDNS services
    publish = {
      enable = true;
      userServices = true;  # Required for this app to register services
    };

    # Optional: Restrict Avahi to specific interfaces
    # This prevents Thread mesh addresses from being advertised
    allowInterfaces = [ "enp14s0" ];  # Replace with your LAN interface
  };

  # Open firewall for Matter
  networking.firewall.allowedUDPPorts = [ 5353 5540 ];
}
```

### Troubleshooting mDNS

If the device is not discoverable:

1. **Check Avahi is running**: `systemctl status avahi-daemon`
2. **Verify service registration**: `avahi-browse -a` while the app is running
3. **Check for Thread mesh interference**: If you have a Thread border router (e.g., Home Assistant Yellow), mDNS reflection can cause Thread mesh addresses to be advertised instead of LAN addresses. Use `allowInterfaces` to restrict Avahi to your LAN interface.
4. **Verify firewall**: Ensure UDP ports 5353 (mDNS) and 5540 (Matter) are open

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
- **rs-matter** - Rust Matter protocol implementation (git: main branch)
- **webrtc** - WebRTC stack for video streaming
- **retina** - RTSP client for camera connections
- **tokio** - Async runtime
- **embassy-\*** - Async primitives for embedded/no_std compatibility
- **zbus** - D-Bus client for Avahi communication
- **nix** - Unix/Linux system interfaces

## Requirements

- Rust 2024 edition (nightly)
- Linux with Avahi daemon (NixOS recommended)
- RTSP camera with H.264 video stream
- Network connectivity to camera and Matter controller

## Matter Commissioning

When the application starts, it displays:
- A QR code for mobile app pairing
- A setup code: `MT:-24J0AFN00KA064IJ3P0WISA0DK5N1K8SQ1RYCU1O0`
- A manual pairing code: `3497-0112-332`

The commissioning window is open for 15 minutes (900 seconds) after startup.

**Note**: Currently uses test device credentials from rs-matter. Production deployments should use proper device attestation certificates.

## License

MIT

## References

- [Matter 1.5 Specification](https://csa-iot.org/developer-resource/specifications-download-request/)
- [rs-matter](https://github.com/project-chip/rs-matter)
- [connectedhomeip Camera Example](https://github.com/project-chip/connectedhomeip/tree/master/examples/camera-app/linux)
