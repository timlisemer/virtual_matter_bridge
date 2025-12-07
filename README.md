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

| Cluster                     | ID       | Status         | Description                                      |
| --------------------------- | -------- | -------------- | ------------------------------------------------ |
| Camera AV Stream Management | `0x0551` | ✅ Implemented | Video/audio stream allocation, codec negotiation |
| WebRTC Transport Provider   | `0x0553` | ✅ Implemented | WebRTC session management, SDP/ICE handling      |
| Chime                       | `0x0556` | ✅ Implemented | Doorbell sounds configuration and playback       |

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
- [x] **Matter Stack Integration - Commissioning**
  - Basic Matter stack initialization with `rs-matter`
  - UDP transport on port 5540 (IPv6 dual-stack)
  - Commissioning window (PASE) with QR code and manual pairing code
  - mDNS advertisement via direct multicast (`mdns-sd` crate)
  - PASE handshake (successful key exchange)
  - Certificate chain requests (DAC, PAI)
  - Attestation and CSR generation
  - NOC (Node Operational Certificate) installation
  - Fabric creation and operational discovery (\_matter.\_tcp)

### In Progress

- [ ] **Matter Stack Integration - Data Model**
  - [ ] Connect clusters to `rs-matter` data model
  - [ ] Implement cluster attribute read/write handlers
  - [ ] Implement cluster command handlers
  - [ ] Device attestation with production credentials
  - [ ] Fabric management persistence (currently in-memory)
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

| Variable               | Default                                                      | Description                                                 |
| ---------------------- | ------------------------------------------------------------ | ----------------------------------------------------------- |
| `MATTER_INTERFACE`     | Auto-detected                                                | Network interface for Matter/mDNS (e.g., `eth0`, `enp14s0`) |
| `RTSP_URL`             | `rtsp://username:password@10.0.0.38:554/h264Preview_01_main` | Camera RTSP stream URL                                      |
| `RTSP_USERNAME`        | -                                                            | RTSP authentication username                                |
| `RTSP_PASSWORD`        | -                                                            | RTSP authentication password                                |
| `DEVICE_NAME`          | `Virtual Doorbell`                                           | Matter device name                                          |
| `MATTER_DISCRIMINATOR` | `3840`                                                       | Matter pairing discriminator                                |
| `MATTER_PASSCODE`      | `20202021`                                                   | Matter pairing passcode                                     |

### Network Interface Auto-Detection

If `MATTER_INTERFACE` is not set, the application automatically detects the first suitable network interface by looking for:

1. Non-loopback interfaces that are running
2. Interfaces with an IPv4 address
3. Preferring common interface name patterns (`eth*`, `enp*`, `eno*`, `ens*`, `wlan*`, `wlp*`)

The detected interface is logged at startup. If auto-detection fails, set `MATTER_INTERFACE` explicitly.

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
3. **Check interface**: Set `MATTER_INTERFACE` environment variable if auto-detection picks the wrong interface

### Debugging Commissioning

**Verify mDNS queries and responses:**

```bash
sudo tcpdump -i enp14s0 -n port 5353 -vv
```

When the phone scans for Matter devices, you should see:

- Phone queries `_matterc._udp.local.` (PTR query)
- PC responds with service info including `SRV tim-pc.local.:5540` and TXT records

**Verify Matter UDP traffic on port 5540:**

```bash
sudo tcpdump -i enp14s0 -n udp port 5540
```

When commissioning starts, you should see PASE packets from the phone to your PC on port 5540.

### Known Issues

**mDNS IPv6 AAAA record errors during deregistration:**

When the commissioning window closes, the `mdns-sd` crate logs many errors about not finding valid addresses for AAAA records on certain IPv6 interfaces. This is cosmetic and does not affect functionality:

```
[ERROR mdns_sd::service_daemon] Cannot find valid addrs for TYPE_AAAA response on intf Interface { name: "enp14s0", addr: V6(Ifv6Addr { ip: fdb3:10a8:8234:0:..., ... }) }
```

These errors occur because the mDNS library tries to announce on all IPv6 addresses but some (ULA addresses from Thread mesh, etc.) are filtered out during registration. The service deregistration still completes successfully.

**mDNS unregister channel closed error:**

```
[ERROR mdns_sd::service_daemon] unregister: failed to send response: sending on a closed channel
```

This occurs because the mDNS daemon's internal communication channel is closed before the unregister response can be sent. This is a timing issue during cleanup and does not affect the commissioning process.

### Previous Issues (Resolved)

**UDP packets not received (RESOLVED):**

Previously, the phone's UDP packets to port 5540 were not being received by the application due to Linux's reverse path filtering.

**Solution for NixOS:**

Add to your host configuration (e.g., `hosts/your-host.nix`):

```nix
{
  # Disable reverse path filtering for Matter/IPv6 traffic
  networking.firewall.checkReversePath = false;

  # Or use "loose" mode for less permissive filtering:
  # networking.firewall.checkReversePath = "loose";

  # Ensure Matter port is open
  networking.firewall.allowedUDPPorts = [ 5353 5540 ];
}
```

Then rebuild: `sudo nixos-rebuild switch`

### mDNS Implementation Notes

This application uses **direct mDNS multicast** via the `mdns-sd` crate. This approach was chosen because:

1. **Interface filtering**: The `mdns-sd` crate allows explicit interface binding, ensuring only the correct LAN addresses are advertised (not Docker bridges or Thread mesh addresses).

2. **No daemon dependency**: Works without requiring any system mDNS daemon.

The `DirectMdnsResponder` in `src/matter/mdns.rs`:

- Binds exclusively to the auto-detected or configured network interface
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

# Run with debug logging (shows UDP packet flow)
make run-debug

# Run with trace logging (shows full packet dumps)
make run-trace
```

### Environment Configuration

Copy `.env.example` to `.env` and adjust values:

```bash
cp .env.example .env
# Edit .env with your settings
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

### Commissioning Flow (Working)

The following commissioning steps complete successfully:

1. **mDNS Discovery**: Device advertises `_matterc._udp` service with discriminator and vendor info
2. **PASE Handshake**: Secure channel established using the pairing code
3. **Certificate Chain**: Device provides DAC (Device Attestation Certificate) and PAI certificates
4. **Attestation**: Device proves authenticity via attestation challenge
5. **CSR Generation**: Device generates certificate signing request
6. **NOC Installation**: Controller installs Node Operational Certificate
7. **Fabric Join**: Device joins the controller's fabric
8. **Operational Discovery**: Device re-advertises as `_matter._tcp` with fabric/node IDs

After commissioning, the device transitions from commissionable (`_matterc._udp`) to operational (`_matter._tcp`) mDNS service.

**Note**: Currently uses test device credentials from rs-matter. Production deployments should use proper device attestation certificates. Fabric credentials are stored in memory and lost on restart.

## Known Limitations

The following features are currently stub implementations or use placeholder values:

### Matter Device Model

- **Device Type**: Uses `DEV_TYPE_ON_OFF_LIGHT` as placeholder (not video doorbell device type)
- **Device Type ID**: `0x0012` is a placeholder value (needs actual Matter 1.5 spec value)
- **Clusters**: Not connected to rs-matter data model - no actual attribute/command handlers

### RTSP Streaming

- **RTSP Client**: Stub that returns fake 1920x1080@30fps stream info
- **Frame Generation**: Produces placeholder frames (zeros) instead of actual video data

### WebRTC

- **WebRTC Bridge**: Frame forwarding is a no-op (counts frames but doesn't transmit)
- **SDP Generation**: Uses placeholder values for ICE credentials and DTLS fingerprints
- **Peer Connections**: Not implemented

### Testing/Debug Code

- **Doorbell Simulation**: Automatically triggers doorbell press every 30 seconds for testing
- **Fabric Persistence**: Credentials stored in memory only (lost on restart)

## Open TODOs and Placeholders (code-level)

- Matter stack (`src/matter/stack.rs`): still uses test device credentials and includes a TODO to build proper static device info from `MatterConfig`.
- Matter thread setup (`src/main.rs`): cluster handlers are seeded with placeholder dataver randomness (`Dataver::new(0)`); switch to real rs-matter datavers.
- Video doorbell device type (`src/device/video_doorbell.rs`): device type ID is a placeholder; replace with the official Matter 1.5 video doorbell ID.
- RTSP client (`src/rtsp/client.rs`): connection/streaming are mocked; implement retina-based RTSP connect/stream (RTP/RTCP, depacketize H.264/AAC, invoke callbacks with real frames).
- RTSP ↔ WebRTC bridge (`src/rtsp/webrtc_bridge.rs`): TODO to set up WebRTC peer connection, media tracks, and actually forward video/audio frames instead of counting bytes.
- WebRTC cluster SDP (`src/clusters/webrtc_transport_provider.rs`): ICE ufrag/pwd and DTLS fingerprint are placeholder strings in offers/answers; replace with real credentials/certs from the WebRTC stack.

## License

MIT

## References

- [Matter 1.5 Specification](https://csa-iot.org/developer-resource/specifications-download-request/)
- [rs-matter](https://github.com/project-chip/rs-matter)
- [connectedhomeip Camera Example](https://github.com/project-chip/connectedhomeip/tree/master/examples/camera-app/linux)
