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
5. **Test manual service publishing**: `avahi-publish-service "TestMatter" "_matterc._udp" 5540 "D=3840" "CM=1"` - if this works but the app doesn't, the issue is in the D-Bus registration code

### mDNS Development Notes

#### Known Issues (Critical Investigation Required)

- [ ] **ALL mDNS registration fails** - Neither the original `rs-matter::AvahiMdnsResponder` nor the custom `FilteredAvahiMdnsResponder` successfully registers services that appear in `avahi-browse`.
- [ ] **avahi-publish-service also fails to appear** - Even the command-line tool reports "Established" but services don't show up in `avahi-browse`.
- [ ] **Thread mesh address still advertised** - The `BBEC6C9F718D4F5E` service (from another device) is visible but locally registered services are not.

#### Investigation Report (2025-12-06)

**Summary**: mDNS service registration appears to succeed at the D-Bus level but services never become visible via `avahi-browse`. This affects both the application AND manual `avahi-publish-service` commands.

**Environment**:
- NixOS with Avahi daemon running
- `services.avahi.publish.userServices = true` is set
- `services.avahi.allowInterfaces = ["enp14s0"]` is set
- Avahi API version: 516

**Symptoms**:

1. **D-Bus registration succeeds**:
   - `entry_group_new()` returns valid path like `/Client19/EntryGroup1`
   - `add_service()` completes without error
   - `add_service_subtype()` completes without error for all subtypes
   - `commit()` completes without error
   - `get_state()` returns 1 (registering), never transitions to 2 (established)

2. **avahi-publish-service reports success but service not visible**:
   ```bash
   $ avahi-publish-service "TestMatter" "_matterc._udp" 5540 "D=3840" &
   Established under name 'TestMatter'

   $ avahi-browse -t _matterc._udp
   # Only shows BBEC6C9F718D4F5E from another device, NOT TestMatter
   ```

3. **External device services ARE visible**:
   - `BBEC6C9F718D4F5E` (_matterc._udp) - from Home Assistant / Thread network
   - Many `_matter._tcp` services from commissioned devices
   - These remain visible even when local app is not running

**What works**:
- Avahi daemon is running (`systemctl status avahi-daemon`)
- D-Bus communication to Avahi succeeds (API calls return valid responses)
- External mDNS services are discovered and displayed

**What doesn't work**:
- Local service registration doesn't appear in browse results
- Entry group state stays at 1 (registering), never reaches 2 (established)
- This affects BOTH programmatic registration AND avahi-publish-service

**Possible causes to investigate**:

1. **mDNS port conflict**: Another process may be binding to 224.0.0.251:5353
   - Check with: `ss -ulnp | grep 5353`
   - Steam's steamwebhelper was previously identified as running a competing mDNS stack

2. **Avahi reflector interference**: `enable-reflector=yes` might cause issues
   - The Thread border router reflects mDNS from the Thread network
   - This might be interfering with local registration

3. **Network namespace issues**: Avahi might be running in a different network context

4. **Cache/state corruption**: Avahi daemon may need restart
   - Try: `sudo systemctl restart avahi-daemon`

5. **Firewall rules**: mDNS multicast might be blocked
   - Check: `sudo iptables -L -n | grep -i 5353`
   - Check: `sudo nft list ruleset | grep -i 5353`

**Next steps**:
1. Restart Avahi daemon: `sudo systemctl restart avahi-daemon`
2. Check for competing mDNS stacks: `ss -ulnp | grep 5353`
3. Try disabling Avahi reflector temporarily
4. Check Avahi logs: `journalctl -u avahi-daemon -f`
5. Test on a fresh system without Thread border router

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
