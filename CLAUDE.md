# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

```bash
make check      # cargo check + fmt + clippy -D warnings
make build      # cargo build --release
make run        # cargo run
make run-debug  # RUST_LOG=debug cargo run
make run-trace  # RUST_LOG=trace cargo run
make test       # cargo test
```

## Architecture

Virtual Matter Bridge creates virtual Matter devices from unstructured data sources (RTSP cameras, HTTP, MQTT, simulation) for Matter-compatible ecosystems (Apple Home, Google Home, Amazon Alexa, Home Assistant).

### High-Level Structure

```
src/
├── main.rs              # Entry point, tokio runtime, Matter thread spawn
├── config.rs            # Configuration from environment variables
├── error.rs             # BridgeError enum with thiserror
├── input/               # Data source modules (camera, simulation, udp)
└── matter/              # Matter protocol implementation
    ├── stack.rs         # Main Matter stack init & run_matter_stack()
    ├── virtual_device.rs # VirtualDevice builder, EndpointConfig, EndpointKind
    ├── handler_bridge.rs # SensorBridge, SwitchBridge adapters
    ├── clusters/        # Matter cluster handlers (BooleanState, OccupancySensing, etc.)
    └── endpoints/       # Endpoint architecture
        ├── handler.rs   # EndpointHandler trait (bidirectional comm)
        ├── sensors/     # Read-only sensors (ContactSensor, OccupancySensor)
        └── controls/    # Read-write switches (Switch, LightSwitch, DeviceSwitch)
```

### Key Patterns

**Virtual Device Builder**: Fluent API for creating devices with endpoints
```rust
let door_sensor = VirtualDevice::new(VirtualDeviceType::ContactSensor, "Door")
    .with_endpoint(EndpointConfig::contact_sensor("Door Sensor", handler));
```

**EndpointHandler Trait**: Bidirectional communication between business logic and Matter
- `get_state()` → read current state
- `on_command(bool)` → receive Matter commands
- `set_state_pusher()` → register callback for state changes

**Bridge Adapters**: `SensorBridge`/`SwitchBridge` adapt EndpointHandler to Matter traits

**Version Tracking**: Atomic `u32` counter per endpoint for subscription updates

### Endpoint Layout

- EP0: Root device
- EP1: Bridge master on/off control
- EP2: Aggregator (bridge root) - lists parent devices
- EP3+: Virtual devices with child endpoints (sensors, switches, video doorbell)

### Threading Model

```
Main thread (tokio::main)
├── Tokio async runtime (sensor simulation, camera init)
└── Matter thread (dedicated, 550KB stack)
    └── embassy event loop (UDP I/O, MRP, subscriptions)
```

## Key Dependencies

- **rs-matter** (git main): Matter protocol implementation with built-in mDNS
- **embassy-***: Async primitives for Matter stack
- **tokio**: Async runtime for application layer
- Rust 2024 edition (nightly required)

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `MATTER_INTERFACE` | Auto-detected | Network interface for Matter/mDNS |
| `DEVICE_NAME` | `Virtual Matter Bridge` | Matter device name |
| `MATTER_DISCRIMINATOR` | `3840` | Matter pairing discriminator |
| `MATTER_PASSCODE` | `20202021` | Matter pairing passcode |
| `RUST_LOG` | `info` | Logging level |

## NixOS Configuration

```nix
{
  services.avahi.enable = false;  # Use rs-matter's built-in mDNS
  networking.firewall.allowedUDPPorts = [ 5353 5540 ];
  networking.firewall.checkReversePath = false;  # Required for Matter/IPv6
}
```

## Device Types

| Type | Cluster | Module |
|------|---------|--------|
| Contact Sensor | BooleanState (0x0045) | `clusters/boolean_state.rs` |
| Occupancy Sensor | OccupancySensing (0x0406) | `clusters/occupancy_sensing.rs` |
| On/Off Switch | OnOff (0x0006) | rs-matter built-in |
| On/Off Light | OnOff (0x0006) | rs-matter built-in |

## Stub Implementations

Camera clusters (CameraAvStreamManagement, WebRtcTransportProvider) and RTSP/WebRTC input are stubs awaiting Matter 1.5 controller support.
