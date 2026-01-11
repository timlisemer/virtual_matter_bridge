# Virtual Matter Bridge

A Rust application that creates virtual Matter devices from unstructured data sources, enabling integration with Matter-compatible smart home ecosystems like Apple Home, Google Home, Amazon Alexa, and Home Assistant.

## Overview

This project implements a general-purpose virtual Matter bridge that can:

- Accept unstructured data from various sources (RTSP streams, HTTP endpoints, MQTT, etc.)
- Process and transform it as needed
- Export it as Matter devices to any Matter controller

### Current Implementation

The bridge currently exposes:

- **Virtual Matter Bridge** (Endpoint 1): Bridge master on/off control
- **Door Sensor** (Endpoint 3+): Contact sensor (bridged)
- **Motion Sensor** (Endpoint 4+): Occupancy sensor (bridged)
- **Power Strip** (Endpoint 5+): On/Off plug-in unit (bridged)
- **Light** (Endpoint 7+): On/Off light (bridged)
- **Video Doorbell** (Endpoint 9+): Video doorbell with camera (bridged, stub)

Note: Endpoint numbers are dynamic based on device configuration. Video doorbell camera handlers are stub implementations awaiting Matter 1.5 controller support.

## Architecture

```
┌─────────────────┐     ┌──────────────────────────────┐     ┌───────────────────┐
│  Data Sources   │     │     Virtual Matter Bridge    │     │ Matter Controller │
│                 │     │                              │     │  (Apple/Google/   │
│ • RTSP Cameras  │────▶│  ┌────────────────────────┐  │────▶│   Amazon/HA)      │
│ • HTTP APIs     │     │  │   Endpoint 0 (Root)    │  │     └───────────────────┘
│ • MQTT Topics   │     │  └────────────────────────┘  │
│ • Files         │     │  ┌────────────────────────┐  │
│ • Commands      │     │  │ EP1 (Bridge Control)   │  │
│ • Simulation    │     │  │ • Master on/off        │  │
└─────────────────┘     │  └────────────────────────┘  │
                        │  ┌────────────────────────┐  │
                        │  │ EP2 (Aggregator)       │  │
                        │  │ • Bridge root          │  │
                        │  └────────────────────────┘  │
                        │  ┌────────────────────────┐  │
                        │  │ EP3+ (Bridged Devices) │  │
                        │  │ • Door (BooleanState)  │  │
                        │  │ • Motion (Occupancy)   │  │
                        │  │ • Power Strip (OnOff)  │  │
                        │  │ • Light (OnOff)        │  │
                        │  │ • Doorbell (Camera)    │  │
                        │  └────────────────────────┘  │
                        └──────────────────────────────┘
```

## Matter Clusters Implemented

| Cluster                     | ID       | Status         | Description                                                           |
| --------------------------- | -------- | -------------- | --------------------------------------------------------------------- |
| OnOff                       | `0x0006` | ✅ Implemented | On/Off control for switches and lights                                |
| BooleanState                | `0x0045` | ✅ Implemented | Binary sensor state (contact sensors)                                 |
| OccupancySensing            | `0x0406` | ✅ Implemented | Occupancy/motion detection                                            |
| Camera AV Stream Management | `0x0551` | ✅ Stub        | Video/audio stream allocation (for future video doorbell sub-devices) |
| WebRTC Transport Provider   | `0x0553` | ✅ Stub        | WebRTC session management (for future video doorbell sub-devices)     |

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
- [x] **Cluster Handlers**
  - OnOff (0x0006) - functional (switches and lights)
  - BooleanState (0x0045) - functional (contact sensors)
  - OccupancySensing (0x0406) - functional (occupancy sensors)
  - Camera AV Stream Management (0x0551) - stub (for future video doorbell sub-devices)
  - WebRTC Transport Provider (0x0553) - stub (for future video doorbell sub-devices)
- [x] **Endpoint Architecture**
  - `src/matter/endpoints/` folder structure with sensors, controls, shared helpers
  - `BinarySensorHelper` for read-only binary state with version tracking
  - `SwitchHelper` for read-write on/off controls with version tracking
  - `ClusterNotifier` for live Matter subscription updates
  - Type aliases: `ContactSensor`, `OccupancySensor`, `Switch`

### Current Status

Home Assistant now shows entities for:

- **Virtual Matter Bridge** (EP1, bridge master on/off control)
- **Door sensor** (contact sensor, bridged)
- **Motion sensor** (occupancy sensor, bridged)
- **Power Strip** (on/off plug-in unit, bridged)
- **Light** (on/off light, bridged)
- **Video Doorbell** (video doorbell, bridged, stub)

Camera clusters (AV Stream, WebRTC) are stub implementations awaiting Matter 1.5 controller support.

---

## Development Roadmap

### Phase 1: Bridge Architecture Foundation (Completed)

**Goal:** Establish bridge architecture with multiple device types

- [x] Create bridge endpoint structure with Aggregator (EP2) and bridged devices
- [x] Add OnOff cluster for switches and lights
- [x] Implement Contact Sensor and Occupancy Sensor as bridged devices

### Phase 2: Endpoint Architecture (Completed)

**Goal:** Create clean architecture for sensors and controls

- [x] Create `src/matter/endpoints/` folder structure with sensors, controls, and shared helpers
- [x] Implement `SwitchHelper` for on/off controls (mirrors `BinarySensorHelper` pattern)
- [x] Move shared utilities (notifier, traits) to `endpoints_helpers/`
- [x] Create `Switch` type alias for reusable on/off controls
- [x] Implement `ContactSensor` and `OccupancySensor` using `BinarySensorHelper`

### Phase 3: Multi-Device Bridge Architecture

**Goal:** Refactor to support multiple device types

- [ ] Create device abstraction layer (trait for generic Matter device)
- [ ] Implement configuration system for devices (YAML/TOML config file)
- [ ] Create endpoint manager (dynamic endpoint allocation)
- [ ] Implement proper Matter bridge topology (`DEV_TYPE_AGGREGATOR` + `DEV_TYPE_BRIDGED_NODE`)

### Phase 4: On/Off Switch Device Type

**Goal:** Add support for simple On/Off switches

- [x] Create Switch control type using `SwitchHelper` (matches sensor pattern)
- [ ] Create OnOff cluster handler for Switch controls
- [ ] Create data source abstraction (trait for boolean data sources)
- [ ] Implement data source backends: HTTP endpoint, MQTT, file, command execution
- [ ] Add configuration for switch devices (source URL, polling interval, read-only mode)

### Phase 5: Video Doorbell Sub-Device (Deferred: Matter 1.5 camera/video doorbell support not yet in controllers/rs-matter)

**Goal:** Add video doorbell as a bridged sub-device type

- [ ] ~~Implement actual RTSP client (`retina` crate for H.264/AAC)~~ (Deferred until Matter 1.5 camera/video doorbell support lands)
- [ ] ~~Implement WebRTC peer connections (`webrtc` crate)~~ (Deferred until Matter 1.5 camera/video doorbell support lands)
- [ ] ~~Bridge RTSP to WebRTC (RTP packetization, timestamp sync)~~ (Deferred until Matter 1.5 camera/video doorbell support lands)
- [ ] ~~Implement doorbell press events (Matter event notifications, external trigger API)~~ (Deferred until Matter 1.5 camera/video doorbell support lands)

### Phase 6: Additional Device Types

**Goal:** Expand supported device types

- [x] Contact sensor (BooleanState cluster 0x0045)
- [x] Occupancy sensor (OccupancySensing cluster 0x0406)
- [ ] Temperature sensor
- [ ] Humidity sensor
- [ ] Dimmable light/switch (LevelControl cluster)
- [ ] Thermostat (if needed)

### Phase 7: Production Readiness

**Goal:** Make the bridge production-ready

- [ ] Device attestation (replace test credentials with production DAC)
- [ ] Persistent storage (fabric credentials, device configuration)
- [ ] Error handling and recovery (reconnection logic, graceful degradation)
- [ ] Logging and monitoring (structured logging, health endpoints)

## Configuration

Configuration is loaded from environment variables with sensible defaults:

| Variable               | Default                                             | Description                                                 |
| ---------------------- | --------------------------------------------------- | ----------------------------------------------------------- |
| `MATTER_INTERFACE`     | Auto-detected                                       | Network interface for Matter/mDNS (e.g., `eth0`, `enp14s0`) |
| `DEVICE_NAME`          | `Virtual Matter Bridge`                             | Matter device name                                          |
| `MATTER_DISCRIMINATOR` | `3840`                                              | Matter pairing discriminator                                |
| `MATTER_PASSCODE`      | `20202021`                                          | Matter pairing passcode                                     |
| `RTSP_URL`             | `rtsp://user:pass@192.168.1.100:554/stream`         | Camera RTSP stream URL                                      |
| `RTSP_USERNAME`        | -                                                   | RTSP authentication username                                |
| `RTSP_PASSWORD`        | -                                                   | RTSP authentication password                                |
| `MQTT_BROKER_HOST`     | `localhost`                                         | MQTT broker hostname/IP                                     |
| `MQTT_BROKER_PORT`     | `1883`                                              | MQTT broker port                                            |
| `MQTT_CLIENT_ID`       | `virtual-matter-bridge`                             | MQTT client identifier                                      |
| `MQTT_USERNAME`        | -                                                   | MQTT authentication username (optional)                     |
| `MQTT_PASSWORD`        | -                                                   | MQTT authentication password (optional)                     |
| `RUST_LOG`             | `info`                                              | Logging level (error, warn, info, debug, trace)             |

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

No known major issues at this time.

### Previous Issues (Resolved)

#### MRP Retransmission Error on Stale Subscriptions (RESOLVED)

After device restart, an error may appear in logs:

```
ERROR rs_matter::transport::mrp] Packet SID:XXXX,CTR:XXXXI|R,EID:1,PROTO:1,OP:5: Too many retransmissions. Giving up
WARN  rs_matter::dm] Got status response InvalidSubscription, aborting interaction
```

This occurs because the device attempts to send subscription updates to controllers using stale session information from before the restart. The error is harmless - the subscription system correctly detects the invalid state and aborts. Controllers re-establish subscriptions after CASE session recovery completes.

#### Session Recovery After Device Restart (RESOLVED)

This section documents the investigation into Matter session recovery behavior after device restart.

#### Problem Statement

After restarting the Matter device, Home Assistant (and other controllers) temporarily cannot communicate with the device. The device logs show:

```
>>RCV UDP [...] [SID:4,CTR:8a285e3][(encoded)]
      => No valid session found, dropping
```

The controller keeps trying to use the old encrypted session (SID:4), but the device lost all session keys on restart.

#### Investigation Summary

**Initial Hypothesis: ICD Check-In Protocol**

We initially investigated the ICD (Intermittently Connected Device) Management cluster with Check-In Protocol support, believing this would help controllers recover sessions after restart.

**Finding: ICD Check-In is for sleepy devices, not session recovery**

The ICD Check-In Protocol is designed for battery-powered devices that sleep and wake periodically. It is NOT the mechanism for session recovery after restart of always-on (hardwired) devices. Home Assistant does not use ICD Check-In for normal devices.

**Root Cause Analysis**

Through code analysis of rs-matter, we discovered:

1. **rs-matter silently drops packets for unknown sessions** - No response is sent to inform the controller the session is invalid. The packet is logged with a warning and discarded (`src/transport.rs:558-569`).

2. **No explicit session invalidation signal** - Per Matter spec, devices could send a Status Report with `SESSION_NOT_FOUND`, but rs-matter does not implement this.

3. **Controllers rely on MRP timeout** - The Message Reliability Protocol (MRP) has exponential backoff with 10 retries. Controllers must exhaust all retries before concluding the session is dead.

4. **mDNS has no restart indicator** - Operational mDNS records (`_matter._tcp`) contain only a dummy TXT record. There is no Session Active Counter or similar field that changes on restart to signal controllers.

**MRP Retry Timing (from rs-matter source)**

```rust
const MRP_BASE_RETRY_INTERVAL_MS: u16 = 300;  // 300ms base
const MRP_MAX_TRANSMISSIONS: u16 = 10;         // 10 retries max
const MRP_BACKOFF_BASE: (u64, u64) = (16, 10); // 1.6x exponential backoff
```

With exponential backoff and jitter, MRP exhausts all retries in approximately **15-30 seconds**.

#### Actual Behavior (Verified)

Testing confirmed that session recovery **works correctly** - it just requires patience:

| Event                          | Timestamp           | Notes                                       |
| ------------------------------ | ------------------- | ------------------------------------------- |
| Device restart                 | 22:46:30            | Matter stack initializes                    |
| Controller retries old session | 22:46:30 - 22:47:17 | "No valid session found, dropping" messages |
| MRP gives up                   | ~22:48:57           | "Too many retransmissions. Giving up"       |
| InvalidSubscription response   | 22:48:57            | Stale subscription correctly aborted        |
| **CASE re-established**        | ~22:49:09           | New session created                         |
| **Device available in HA**     | 22:49:09            | OnOff commands work                         |

**Timeline: ~2.5 minutes from restart to full recovery**

The recovery time includes:

- MRP retry exhaustion (~30 seconds per stale session)
- Controller's internal cooldown before CASE retry
- CASE handshake completion
- Subscription re-establishment

#### Conclusions

1. **Session recovery works as designed** - No code changes needed for basic functionality
2. **ICD Check-In is unnecessary** for always-on (hardwired) devices
3. **User experience improved** with logging to indicate recovery is in progress
4. **The MRP error is expected** - It indicates the system correctly handling stale state

#### Implementation

Based on these findings:

1. **Informative logging added** - Device logs when waiting for controllers and when recovery completes
2. **Session recovery is automatic** - Controllers handle it via MRP timeout + CASE re-establishment
3. **No ICD cluster needed** - This bridge is hardwired/always-on, not battery-powered

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

### Development Workflow

The bridge includes tooling to streamline the development cycle when making changes that affect the device structure.

#### Schema Auto-Reset

The bridge automatically detects when the device endpoint structure changes (device types, labels, endpoint configurations) and resets the persistence file accordingly:

```bash
# Normal development - auto-resets only if structure changed
make dev

# Force reset every time (useful for rapid iteration)
make dev-reset
```

When schema changes are detected, you'll see:
```
Schema hash changed (0x... -> 0x...), resetting persistence
```

#### Automated Commissioning

Instead of scanning QR codes with your phone, you can commission directly to python-matter-server (used by Home Assistant):

```bash
# Commission to local python-matter-server
make commission

# Commission to remote server
MATTER_SERVER_URL=ws://homeassistant.local:5580/ws make commission

# Check commissioned nodes
make status

# Remove a node
make remove NODE_ID=123
```

**Prerequisites:**
- python-matter-server must be running and accessible
- NixOS: `services.matter-server.enable = true`
- Firewall: Allow port 5580 from your dev machine

#### Typical Development Cycle

```bash
# Terminal 1: Run the bridge (auto-resets if structure changed)
make dev

# Terminal 2: Commission once after reset
MATTER_SERVER_URL=ws://your-ha-server:5580/ws make commission

# Device appears in Home Assistant automatically
```

#### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `MATTER_SERVER_URL` | `ws://localhost:5580/ws` | python-matter-server WebSocket URL |
| `MATTER_DISCRIMINATOR` | `3840` | Pairing discriminator (must match bridge config) |
| `MATTER_PASSCODE` | `20202021` | Pairing passcode (must match bridge config) |
| `DEV_AUTO_RESET` | `false` | Force persistence reset on every startup |

#### Finding Your Python Matter Server URL

The `MATTER_SERVER_URL` depends on where python-matter-server is running:

**Home Assistant OS / Supervised:**
- Default: `ws://homeassistant.local:5580/ws`
- Or use the IP: `ws://<your-ha-ip>:5580/ws`

**Docker (standalone):**
- Use the IP/hostname of the machine running the container
- Example: `ws://192.168.1.100:5580/ws`

**Verify connectivity:**
```bash
curl -i http://<your-server>:5580/ws
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

- **Test Credentials**: Uses test device credentials from rs-matter (not production DAC)
- **Fabric Persistence**: Credentials stored in file, but commissioning state may need reset after major changes

### Video Doorbell Sub-Device (Future Feature)

The following are stub implementations preserved for future video doorbell sub-device support:

- **RTSP Client**: Stub that returns fake 1920x1080@30fps stream info
- **Frame Generation**: Produces placeholder frames (zeros) instead of actual video data
- **WebRTC Bridge**: Frame forwarding is a no-op (counts frames but doesn't transmit)
- **SDP Generation**: Uses placeholder values for ICE credentials and DTLS fingerprints
- **Peer Connections**: Not implemented

These stubs will be completed when Matter 1.5 camera/video doorbell support lands in controllers and rs-matter.

## Open TODOs and Placeholders (code-level)

### Matter Stack

- **Test credentials** (`src/matter/stack.rs`): Uses test device credentials; TODO to build proper static device info from `MatterConfig`.
- **Persistence path** (`src/matter/stack.rs:117`): Hardcoded to `.config/virtual-matter-bridge`; should respect `XDG_CONFIG_HOME` or be configurable.
- **Network change detection** (`src/matter/netif.rs:242-246`): `wait_changed()` just waits forever; no actual network change detection implemented.

### Video Doorbell Sub-Device (Future Feature)

The following TODOs are for future video doorbell sub-device support:

- **RTSP client** (`src/input/camera/`): Connection/streaming are mocked; implement retina-based RTSP connect/stream.
- **WebRTC bridge** (`src/input/camera/webrtc_bridge.rs`): Set up WebRTC peer connection and actually forward video/audio frames.
- **SDP placeholders** (`src/matter/clusters/webrtc_transport_provider.rs`): ICE ufrag/pwd and DTLS fingerprint are placeholder strings.
- **Camera cluster** (`src/matter/clusters/camera_av_stream_mgmt.rs`): Video parameters hardcoded; snapshot commands return errors.

### Error Handling

- **Panic on missing interface** (`src/matter/netif.rs:54-59`): Panics if no suitable network interface found; could return `Result` for graceful handling.

## License

MIT

## References

- [Matter 1.5 Specification](https://csa-iot.org/developer-resource/specifications-download-request/)
- [rs-matter](https://github.com/project-chip/rs-matter)
- [connectedhomeip Camera Example](https://github.com/project-chip/connectedhomeip/tree/master/examples/camera-app/linux)
