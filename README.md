# Virtual Matter Bridge

A Rust bridge that exposes MQTT devices and simulated sensors to Matter controllers.
It uses upstream `project-chip/rs-matter`, pinned to a Matter 1.5.1 revision. The
application connects to the Open Home Foundation matter.js-based Matter Server
through its compatible WebSocket API.

## Devices

| Source | Matter representation |
| --- | --- |
| Aqara W100 via Zigbee2MQTT | Temperature, humidity, battery, and three named Generic Switch buttons |
| Shelly 2PM Gen4 via Zigbee2MQTT | Four named relay devices, electrical measurements, and diagnostics |
| Simulation | Contact, occupancy, and on/off endpoints |

The code includes the four W100 sensors and two Shelly 2PM units on the live
Zigbee2MQTT network. It exposes `Büro Licht`, `Tim-PC`, `Tim-Schlafzimmer TV`, and
`Tim-Schlafzimmer Steckdose`. The W100 button labels use the Matter Fixed Label
cluster with `ha_entitylabel`, which Home Assistant reads for this test identity.

Endpoint numbers are assigned from the device configuration. Use device and
endpoint names when inspecting the bridge in a controller.

Simulate a doorbell press by publishing the text `press`, without retention, to
`virtual-matter-bridge/doorbell/press`. The bridge sends a Matter Generic Switch
event from the `Doorbell Press` endpoint. Cached button actions are never replayed.

Camera AV Stream Management, WebRTC, and RTSP code remains experimental. The
camera handlers are not connected to the Matter endpoint router and parts of the
media transport use placeholder values. Upgrading the controller or SDK does not
enable camera streaming. The bridge has not been certified for Matter 1.5.

## Development

Install Rust 1.90 or later and `just`. The bridge uses Rust 2024 and let chains;
a nightly compiler is not required. On Linux, the Rust dependencies require a
C compiler and the usual development libraries, including OpenSSL and pkg-config.

```sh
just
just check
just build
just run
```

`just check` checks formatting, compiles all targets, runs tests, and runs Clippy
with warnings treated as errors. `just build` builds all targets and runs tests.
`just run --release` starts the main bridge with Cargo's release profile.

For logs, use `RUST_LOG=debug just run` or `RUST_LOG=trace just run`.
`just test` runs tests and `just clean` removes build artifacts.

The upstream SDK revision and `Cargo.lock` are committed so a fresh checkout uses
the same dependency graph. Update the SDK pin deliberately and run `just check`.

## Configuration

Copy `.env.example` to `.env` and adjust it. Process environment variables take
precedence over the values loaded by the application from `.env`.

| Variable | Default | Purpose |
| --- | --- | --- |
| `MATTER_INTERFACE` | Automatic | LAN interface used for Matter and mDNS |
| `DEVICE_NAME` | `Virtual Matter Bridge` | Bridge name |
| `MATTER_DISCRIMINATOR` | `3840` | Commissioning discriminator |
| `MATTER_PASSCODE` | `20202021` | Commissioning passcode |
| `MATTER_SERVER_URL` | Unset | Optional automatic commissioning endpoint, such as `ws://10.0.0.2:5580/ws` |
| `MQTT_BROKER_HOST` | `10.0.0.2` | MQTT broker |
| `MQTT_BROKER_PORT` | `1883` | MQTT port |
| `ZIGBEE2MQTT_WS_URL` | Unset | Read-only frontend feed for cached W100 sensor readings, for example `ws://10.0.0.2:8084/api` |
| `MQTT_CLIENT_ID` | `virtual-matter-bridge` | MQTT client ID |
| `MQTT_USERNAME`, `MQTT_PASSWORD` | Unset | MQTT credentials |
| `RTSP_URL`, `RTSP_USERNAME`, `RTSP_PASSWORD` | Development values | Experimental camera input |
| `RUST_LOG` | `info` | Log level |
| `DEV_AUTO_RESET` | Disabled | Reset bridge persistence on startup for development |

The MQTT device friendly names and Matter endpoint layout are defined in
`src/main.rs`. Sensor availability follows source readiness; unavailable sources
must not be represented as valid zero measurements.

## Matter networking

The bridge uses its built-in mDNS responder and selects one LAN interface.
Allow UDP ports 5353 and 5540, IPv6 multicast, and direct IPv6 communication with
the controller. Set `MATTER_INTERFACE` if automatic selection picks the wrong
interface. Avoid running another Matter device process on the same port.

A stateful firewall can block Matter replies. On NixOS, use the appropriate
reverse-path policy for your network; this project has previously used
`networking.firewall.checkReversePath = false` on its development host. Do not
change the Thread border router or disable host-wide discovery services merely
to upgrade the controller.

## Commissioning and persistence

Start the bridge, then commission it from Home Assistant or from another terminal:

```sh
just commission
just status
just remove 123
```

The development CLI reads `MATTER_SERVER_URL` from the environment or `.env` and
otherwise uses `ws://localhost:5580/ws`. It commissions over the network, without
requiring a Bluetooth adapter.

If `MATTER_SERVER_URL` is set for the main bridge, it can automatically commission
itself. Fabric state is stored under `~/.config/virtual-matter-bridge` in the user's
home directory. Keep that directory and service user stable across restarts.
Changes to the endpoint schema can reset bridge persistence and remove old
controller nodes; do not enable `DEV_AUTO_RESET` during a server migration.

The Matter Server upgrade uses the controller's existing data directory and
migrates it on first start. Keep both the controller data and bridge persistence.
A controller upgrade does not require deleting and pairing the bridge again.
The bridge uses development attestation credentials. For matter.js Matter Server,
enable Test DCL (`ENABLE_TEST_NET_DCL=true`) before commissioning the bridge.

## Integration guides

- [Aqara W100](docs/W100_MQTT_INTEGRATION.md)
- [Shelly 2PM Gen4](docs/SHELLY_2PM_MQTT_INTEGRATION.md)

## Layout

- `src/input/`: MQTT, simulation, and experimental camera sources.
- `src/matter/`: Stack, endpoint routing, cluster handlers, and readiness tracking.
- `src/commissioning.rs`: Shared Matter Server WebSocket client helpers.
- `src/bin/dev-commission.rs`: Commissioning and node inspection CLI.
- `src/bin/mqtt-test.rs`: MQTT diagnostics CLI.

[Upstream SDK](https://github.com/project-chip/rs-matter) ·
[Matter Server](https://github.com/matter-js/matterjs-server)
