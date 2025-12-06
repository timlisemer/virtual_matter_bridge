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
  - [x] mDNS advertisement via direct multicast (`mdns-sd` crate)
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

This application uses direct mDNS multicast via the `mdns-sd` crate. Disable Avahi to avoid conflicts:

```nix
{
  # Disable Avahi - we use direct mDNS instead
  services.avahi.enable = false;

  # Open firewall for Matter
  networking.firewall.allowedUDPPorts = [ 5353 5540 ];
}
```

### Verifying mDNS Registration

To verify the Matter service is being advertised correctly, use Python zeroconf:

```bash
nix-shell -p python3Packages.zeroconf --run 'python3 -c "
from zeroconf import Zeroconf, ServiceBrowser
import time

class Listener:
    def add_service(self, zc, type_, name):
        print(\"Found:\", name)
        info = zc.get_service_info(type_, name)
        if info:
            print(\"  Addresses:\", info.parsed_addresses())
            print(\"  Port:\", info.port)
            for k, v in info.properties.items():
                kstr = k.decode() if isinstance(k, bytes) else k
                vstr = v.decode() if isinstance(v, bytes) else str(v)
                print(\"  {}={}\".format(kstr, vstr))
    def remove_service(self, zc, type_, name):
        pass
    def update_service(self, zc, type_, name):
        pass

zc = Zeroconf()
browser = ServiceBrowser(zc, \"_matterc._udp.local.\", Listener())
print(\"Browsing for 5 seconds...\")
time.sleep(5)
zc.close()
print(\"Done.\")
"'
```

Expected output when the application is running:

```
Browsing for 5 seconds...
Found: 5B9408616867442C._matterc._udp.local.
  Addresses: ['10.0.0.3']
  Port: 5540
  D=3840
  CM=1
  VP=65521+32769
  SAI=300
  SII=5000
  DN=MyTest
Done.
```

### Troubleshooting mDNS

If the device is not discoverable:

1. **Check for port conflicts**: `ss -ulnp | grep 5353` - ensure no other process is binding to port 5353
2. **Verify firewall**: Ensure UDP ports 5353 (mDNS) and 5540 (Matter) are open
3. **Check interface name**: The application is configured to use `enp14s0` - update `src/matter/stack.rs` if your interface differs

### mDNS Implementation Notes

This application uses **direct mDNS multicast** via the `mdns-sd` crate. This approach was chosen because:

1. **Interface filtering**: The `mdns-sd` crate allows explicit interface binding, ensuring only the correct LAN addresses are advertised (not Docker bridges or Thread mesh addresses).

2. **No daemon dependency**: Works without requiring any system mDNS daemon.

The `DirectMdnsResponder` in `src/matter/mdns.rs`:
- Binds exclusively to the configured network interface (e.g., `enp14s0`)
- Filters out link-local IPv6 addresses (fe80::/10)
- Filters out Thread mesh addresses (fd00::/8 ULAs)
- Registers Matter commissioning services with proper TXT records

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
- **mdns-sd** - Direct mDNS multicast for service discovery
- **webrtc** - WebRTC stack for video streaming
- **retina** - RTSP client for camera connections
- **tokio** - Async runtime
- **embassy-\*** - Async primitives for embedded/no_std compatibility
- **nix** - Unix/Linux system interfaces

## Requirements

- Rust 2024 edition (nightly)
- Linux (NixOS recommended)
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
