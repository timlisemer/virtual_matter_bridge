# Shelly 2PM Gen4 Integration via MQTT

This document describes the integration of the Shelly 2PM Gen4 Zigbee2MQTT device `Büro Licht & PC Schalter` into the Virtual Matter Bridge.

**Instead of**: Home Assistant <- MQTT <- Zigbee2MQTT <- Shelly 2PM

**We want**: Home Assistant <- Matter <- **Virtual Matter Bridge** <- MQTT <- Zigbee2MQTT <- Shelly 2PM

The bridge subscribes to one Zigbee2MQTT Shelly state topic and exposes each relay channel as its own Matter bridged device.

## Device Mapping

The Zigbee2MQTT friendly name and physical MQTT command topic remain unchanged:

```text
zigbee2mqtt/Büro Licht & PC Schalter
zigbee2mqtt/Büro Licht & PC Schalter/set
zigbee2mqtt/Büro Licht & PC Schalter/get
```

Matter endpoint mapping:

| Matter Device | Matter Endpoint | Matter Type | MQTT Channel |
| --- | --- | --- | --- |
| `Shelly 2PM Gen4 - Switch 1` | `Büro Licht` | Light / On-Off Light | L2 / `state_l2` |
| `Shelly 2PM Gen4 - Switch 2` | `Tim-PC` | Normal switch / On-Off Plug-in Unit | L1 / `state_l1` |

The Matter device names are split for controller display. MQTT command routing still follows the physical channel mapping already used by the bridge.

## MQTT State Payload

The bridge parses the full retained/live Shelly payload shape used by Zigbee2MQTT:

```json
{
  "ac_frequency_l1": 49.99,
  "ac_frequency_l2": 49.99,
  "current_l1": 1.23,
  "current_l2": 0,
  "dhcp_enabled": true,
  "energy_l1": 215.24,
  "energy_l2": 0.12,
  "ip_address": "10.0.0.98",
  "linkquality": 148,
  "power_apparent_l1": 272,
  "power_apparent_l2": 0,
  "power_factor_l1": 0.01,
  "power_factor_l2": 0,
  "power_l1": 269,
  "power_l2": 0,
  "power_reactive_l1": 0,
  "power_reactive_l2": 0,
  "produced_energy_l1": 0,
  "produced_energy_l2": 0,
  "state_l1": "ON",
  "state_l2": "OFF",
  "voltage_l1": 231.67,
  "voltage_l2": 229.76,
  "wifi_config": {
    "enabled": false,
    "ssid": "<redacted>"
  },
  "wifi_status": "got ip"
}
```

## Matter Telemetry

Each Shelly switch endpoint also advertises:

| MQTT Data | Matter Representation |
| --- | --- |
| `power_l*`, `power_apparent_l*`, `power_reactive_l*`, `voltage_l*`, `current_l*`, `ac_frequency_l*`, `power_factor_l*` | Electrical Power Measurement cluster `0x0090` |
| `energy_l*`, `produced_energy_l*` | Electrical Energy Measurement cluster `0x0091` |
| `dhcp_enabled`, `ip_address`, `linkquality`, `wifi_config.enabled`, `wifi_config.ssid`, `wifi_status` | Shelly diagnostics cluster `0xFC00` |

Diagnostics are read-only. The bridge does not publish MQTT writes for `wifi_config`.

## Command Payloads

Matter commands are translated to Zigbee2MQTT set payloads. Each command targets exactly one physical channel.

Turn `Tim-PC` / L1 on or off:

```json
{"state_l1":"ON"}
{"state_l1":"OFF"}
```

Turn `Büro Licht` / L2 on or off:

```json
{"state_l2":"ON"}
{"state_l2":"OFF"}
```

## Data Flow

```text
Zigbee2MQTT state topic
zigbee2mqtt/Büro Licht & PC Schalter
payload has state_l1, state_l2, telemetry, diagnostics
|
+--> L1/state_l1 + L1 telemetry --> Shelly 2PM Gen4 - Switch 2 / Tim-PC
|
+--> L2/state_l2 + L2 telemetry --> Shelly 2PM Gen4 - Switch 1 / Büro Licht
|
+--> shared diagnostics ---------> both Shelly Matter devices
```

Matter command path:

```text
Matter switch/light command
|
v
Shelly channel EndpointHandler
|
v
Local state update + queued MQTT command
|
v
MqttIntegration command publisher
|
+--> Tim-PC publishes {"state_l1":"ON"|"OFF"}
|
+--> Büro Licht publishes {"state_l2":"ON"|"OFF"}
|
v
zigbee2mqtt/Büro Licht & PC Schalter/set
```

## Terminal Verification

Inspect retained Shelly state:

```bash
nix-shell -p mosquitto jq --run "mosquitto_sub -h 10.0.0.2 -t 'zigbee2mqtt/Büro Licht & PC Schalter' -C 1 -W 10 | jq ."
```

Monitor live Shelly state:

```bash
nix-shell -p mosquitto jq --run "mosquitto_sub -h 10.0.0.2 -t 'zigbee2mqtt/Büro Licht & PC Schalter' | jq ."
```

## Matter Verification

After starting the bridge and refreshing or re-pairing it in Home Assistant:

1. Confirm `Büro Licht & PC Schalter` no longer appears as a Shelly bridged Matter device.
2. Confirm `Shelly 2PM Gen4 - Switch 1` appears with light endpoint `Büro Licht`.
3. Confirm `Shelly 2PM Gen4 - Switch 2` appears with switch endpoint `Tim-PC`.
4. Toggle `Tim-PC` from Matter/Home Assistant and verify `state_l1` changes on MQTT.
5. Toggle `Büro Licht` from Matter/Home Assistant and verify `state_l2` changes on MQTT.
6. Confirm each Matter device exposes electrical telemetry, and confirm shared diagnostics are visible or inspectable through cluster `0xFC00`.

## Implementation Files

| File | Purpose |
| --- | --- |
| `src/input/mqtt/shelly_2pm.rs` | Shelly state parsing, channel handlers, telemetry extraction, and command payload generation |
| `src/input/mqtt/integration.rs` | MQTT subscription, state routing, telemetry/diagnostic updates, initial state request, and command publishing |
| `src/matter/clusters/electrical_power_measurement.rs` | Electrical Power Measurement cluster |
| `src/matter/clusters/electrical_energy_measurement.rs` | Electrical Energy Measurement cluster |
| `src/matter/clusters/shelly_diagnostics.rs` | Shelly diagnostics cluster |
| `src/main.rs` | Shelly Matter device registration and endpoint typing |
