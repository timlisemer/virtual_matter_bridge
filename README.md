# Virtual Matter Bridge

A Rust application that creates virtual Matter devices from unstructured data sources, enabling integration with Matter-compatible smart home ecosystems like Apple Home, Google Home, Amazon Alexa, and Home Assistant.

## Overview

This project implements a general-purpose virtual Matter bridge that can:

- Accept unstructured data from various sources (RTSP streams, HTTP endpoints, MQTT, etc.)
- Process and transform it as needed
- Export it as Matter devices to any Matter controller

### Current Focus

- **Video Doorbell**: RTSP camera streams exposed as Matter 1.5 video doorbell devices
- **On/Off Switches**: Boolean data sources exposed as Matter switches (planned)

## Architecture

```
┌─────────────────┐     ┌──────────────────────────────┐     ┌───────────────────┐
│  Data Sources   │     │     Virtual Matter Bridge    │     │ Matter Controller │
│                 │     │                              │     │  (Apple/Google/   │
│ • RTSP Cameras  │────▶│  ┌────────────────────────┐  │────▶│   Amazon/HA)      │
│ • HTTP APIs     │     │  │   Endpoint 0 (Root)    │  │     └───────────────────┘
│ • MQTT Topics   │     │  └────────────────────────┘  │
│ • Files         │     │  ┌────────────────────────┐  │
│ • Commands      │     │  │ Endpoint 1 (Aggregator)│  │
└─────────────────┘     │  └────────────────────────┘  │
                        │  ┌────────────────────────┐  │
                        │  │ Endpoint 2+ (Devices)  │  │
                        │  │ • Video Doorbell       │  │
                        │  │ • On/Off Switch        │  │
                        │  │ • Sensors (planned)    │  │
                        │  └────────────────────────┘  │
                        └──────────────────────────────┘
```

## Matter 1.5 Clusters Implemented

| Cluster                     | ID       | Status         | Description                                      |
| --------------------------- | -------- | -------------- | ------------------------------------------------ |
| Camera AV Stream Management | `0x0551` | ✅ Implemented | Video/audio stream allocation, codec negotiation |
| WebRTC Transport Provider   | `0x0553` | ✅ Implemented | WebRTC session management, SDP/ICE handling      |

## Project Status

### Completed

- [x] Project structure following Rust best practices
- [x] Configuration system with environment variable overrides
- [x] Error handling with `thiserror`
- [x] **Matter Stack Integration - Commissioning**
  - Basic Matter stack initialization with `rs-matter`
  - UDP transport on port 5540 (IPv6 dual-stack)
  - Commissioning window (PASE) with QR code and manual pairing code
  - mDNS advertisement via `BuiltinMdnsResponder`
  - PASE handshake (successful key exchange)
  - Certificate chain requests (DAC, PAI)
  - Attestation and CSR generation
  - NOC (Node Operational Certificate) installation
  - Fabric creation and operational discovery (`_matter._tcp`)
  - Multi-admin commissioning (phone + Home Assistant)
- [x] **Cluster Handlers (stub implementations)**
  - Camera AV Stream Management (0x0551)
  - WebRTC Transport Provider (0x0553)

### Current Issue

Home Assistant shows **"This device has no entities"** because:

- Current clusters (Camera AV, WebRTC) are protocol-level, not user-facing
- No standard clusters like OnOff that HA recognizes as entities
- Cluster handlers return placeholder values

---

## Development Roadmap

### Phase 1: Fix Current Video Doorbell (Make Entities Appear)

**Goal:** Make the existing video doorbell show entities in Home Assistant

- [x] Add OnOff cluster to video doorbell endpoint (exposes armed/disarmed state)
- [ ] Implement Software Diagnostics cluster (0x46) on endpoint 0
- [ ] ~~Fix device type registration (correct Matter 1.5 video doorbell ID)~~ - Skipped: Home Assistant does not support Matter 1.5 camera device types yet

### Phase 2: Multi-Device Bridge Architecture

**Goal:** Refactor to support multiple device types

- [ ] Create device abstraction layer (trait for generic Matter device)
- [ ] Implement configuration system for devices (YAML/TOML config file)
- [ ] Create endpoint manager (dynamic endpoint allocation)
- [ ] Implement proper Matter bridge topology (`DEV_TYPE_AGGREGATOR` + `DEV_TYPE_BRIDGED_NODE`)

### Phase 3: On/Off Switch Device Type

**Goal:** Add support for simple On/Off switches

- [ ] Create OnOffSwitch device type using rs-matter's `OnOffHandler`
- [ ] Create data source abstraction (trait for boolean data sources)
- [ ] Implement data source backends: HTTP endpoint, MQTT, file, command execution
- [ ] Add configuration for switch devices (source URL, polling interval, read-only mode)

### Phase 4: Complete Video Doorbell Implementation

**Goal:** Make video doorbell fully functional with real streaming

- [ ] Implement actual RTSP client (`retina` crate for H.264/AAC)
- [ ] Implement WebRTC peer connections (`webrtc` crate)
- [ ] Bridge RTSP to WebRTC (RTP packetization, timestamp sync)
- [ ] Implement doorbell press events (Matter event notifications, external trigger API)

### Phase 5: Additional Device Types

**Goal:** Expand supported device types

- [ ] Sensor devices (temperature, humidity, contact, occupancy)
- [ ] Dimmable light/switch (LevelControl cluster)
- [ ] Thermostat (if needed)

### Phase 6: Production Readiness

**Goal:** Make the bridge production-ready

- [ ] Device attestation (replace test credentials with production DAC)
- [ ] Persistent storage (fabric credentials, device configuration)
- [ ] Error handling and recovery (reconnection logic, graceful degradation)
- [ ] Logging and monitoring (structured logging, health endpoints)

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

This application uses rs-matter's built-in mDNS responder. Disable Avahi to avoid conflicts:

```nix
{
  # Disable Avahi - we use rs-matter's built-in mDNS
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

**Software Diagnostics Cluster Not Implemented (cluster 0x46):**

Home Assistant queries for cluster 70 (0x46 = Software Diagnostics) on endpoint 0, which is not implemented:

```
Error processing attribute read: AttrStatus { path: AttrPath { endpoint: Some(0), cluster: Some(70), attr: Some(6) }, status: UnsupportedCluster }
```

This is non-fatal - HA continues to work but won't display software version info.

**TODO:** Implement Software Diagnostics cluster on endpoint 0.

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

This application uses rs-matter's built-in `BuiltinMdnsResponder` for mDNS service advertisement. Key features:

1. **Interface filtering**: The `FilteredNetifs` implementation in `src/matter/netif.rs` ensures only the correct LAN addresses are advertised (not Docker bridges or Thread mesh addresses).

2. **Subtype support**: Correctly handles mDNS subtype PTR queries (e.g., `_S3840._sub._matterc._udp`) for discriminator-based discovery.

3. **IPv6 source address binding**: The Matter UDP socket binds to the specific IPv6 address advertised in mDNS, ensuring response packets have the correct source address for multi-admin commissioning.

The network interface implementation:

- Filters to a single auto-detected or configured network interface
- Filters out link-local IPv6 addresses (fe80::/10)
- Filters out Thread mesh addresses (fd00::/8 ULAs)

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

- **rs-matter** - Rust Matter protocol implementation (git: main branch, includes mDNS)
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

### Matter Stack

- **Test credentials** (`src/matter/stack.rs`): Uses test device credentials; TODO to build proper static device info from `MatterConfig`.
- **Dataver placeholder** (`src/main.rs`): Cluster handlers use `Dataver::new(0)` placeholder randomness; switch to real rs-matter datavers.
- **Device type ID** (`src/device/video_doorbell.rs`): Device type ID `0x0012` is a placeholder; replace with official Matter 1.5 video doorbell ID.
- **Persistence path** (`src/matter/stack.rs:117`): Hardcoded to `.config/virtual-matter-bridge`; should respect `XDG_CONFIG_HOME` or be configurable.
- **Network change detection** (`src/matter/netif.rs:242-246`): `wait_changed()` just waits forever; no actual network change detection implemented.

### RTSP Streaming

- **RTSP client** (`src/rtsp/client.rs`): Connection/streaming are mocked; implement retina-based RTSP connect/stream (RTP/RTCP, depacketize H.264/AAC, invoke callbacks with real frames).
- **Stream info hardcoded** (`src/rtsp/client.rs:96-102`): Returns fake 1920x1080@30fps regardless of actual camera capabilities.

### WebRTC

- **WebRTC bridge** (`src/rtsp/webrtc_bridge.rs`): TODO to set up WebRTC peer connection, media tracks, and actually forward video/audio frames instead of counting bytes.
- **SDP placeholders** (`src/clusters/webrtc_transport_provider.rs`): ICE ufrag/pwd and DTLS fingerprint are placeholder strings; replace with real credentials/certs from WebRTC stack.
- **Unused session_id** (`src/clusters/webrtc_transport_provider.rs:246-250`): `_session_id` parameter not used in SDP generation; should generate session-specific SDP.

### Camera Cluster

- **Video parameters hardcoded** (`src/clusters/camera_av_stream_mgmt.rs`): Resolutions, bitrates, sample rates hardcoded; should match actual camera capabilities.
- **Snapshot commands** (`src/matter/clusters/camera_av_stream_mgmt.rs:997-1004`): `SnapshotStreamAllocate`/`Deallocate` return `InvalidAction` error; not implemented.
- **Silent no-op commands**: `SetStreamPriorities`, `SetViewport` accept but do nothing.

### Doorbell

- **Doorbell event notification** (`src/main.rs:78`, `src/device/video_doorbell.rs:154`): Sets atomic flag but doesn't send Matter event notification to controllers.
- **Empty DoorbellConfig** (`src/config.rs:41-44`): Struct has no fields; either add doorbell-specific config or remove.

### Error Handling

- **Panic on missing interface** (`src/matter/netif.rs:54-59`): Panics if no suitable network interface found; could return `Result` for graceful handling.

## License

MIT

## References

- [Matter 1.5 Specification](https://csa-iot.org/developer-resource/specifications-download-request/)
- [rs-matter](https://github.com/project-chip/rs-matter)
- [connectedhomeip Camera Example](https://github.com/project-chip/connectedhomeip/tree/master/examples/camera-app/linux)
