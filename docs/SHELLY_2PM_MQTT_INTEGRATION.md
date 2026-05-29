# Shelly 2PM Gen4 Integration via MQTT

This document describes the integration of the Shelly 2PM Gen4 device `Büro Licht & PC Schalter` into the Virtual Matter Bridge via Zigbee2MQTT and MQTT.

**Instead of**: Home Assistant <- MQTT <- Zigbee2MQTT <- Shelly 2PM

**We want**: Home Assistant <- Matter <- **Virtual Matter Bridge** <- MQTT <- Zigbee2MQTT <- Shelly 2PM

The bridge subscribes to the Shelly Zigbee2MQTT state topic and exposes the two controllable relay channels as two Matter child endpoints under one bridged device.

## Device: Shelly 2PM Gen4

Observed device:

- Matter parent device: `Büro Licht & PC Schalter`
- Zigbee2MQTT friendly name: `Büro Licht & PC Schalter`
- L1 / `state_l1`: `Tim PC Switch`
- L2 / `state_l2`: `Büro Light`

Matter endpoint mapping:

| Shelly Channel | Zigbee2MQTT Key | Matter Endpoint Label | Matter Type |
| --- | --- | --- | --- |
| L1 | `state_l1` | `Tim PC Switch` | Normal switch / On-Off Plug-in Unit |
| L2 | `state_l2` | `Büro Light` | Light / On-Off Light |

L1 is intentionally exposed with `EndpointConfig::switch`, so controllers should treat it as a normal switch rather than as a light. L2 is intentionally exposed with `EndpointConfig::light_switch`, so controllers should treat it as a light.

## MQTT Interface

State topic:

```text
zigbee2mqtt/Büro Licht & PC Schalter
```

Set topic:

```text
zigbee2mqtt/Büro Licht & PC Schalter/set
```

Get topic:

```text
zigbee2mqtt/Büro Licht & PC Schalter/get
```

On MQTT connection and reconnection, the bridge subscribes to the state topic and requests current state with:

```json
{"state":""}
```

## Observed State Payload

Retained state observed from the MQTT broker:

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
    "ssid": "BND_Observavations_Van_3"
  },
  "wifi_status": "got ip"
}
```

The bridge currently consumes only:

- `state_l1`
- `state_l2`

Power, current, voltage, energy, Wi-Fi status, and linkquality are documented here but are not exposed to Matter in the first implementation.

## Command Payloads

Matter commands are translated to Zigbee2MQTT set payloads.

Turn L1 / Tim PC Switch on:

```json
{"state_l1":"ON"}
```

Turn L1 / Tim PC Switch off:

```json
{"state_l1":"OFF"}
```

Turn L2 / Büro Light on:

```json
{"state_l2":"ON"}
```

Turn L2 / Büro Light off:

```json
{"state_l2":"OFF"}
```

Each command payload targets exactly one channel.

## Data Flow

```text
Zigbee2MQTT state topic
zigbee2mqtt/Büro Licht & PC Schalter
payload has state_l1 and state_l2
|
+--> state_l1 --> Tim PC Switch shared state --> Matter normal switch
|
+--> state_l2 --> Büro Light shared state ----> Matter light
```

Matter command path:

```text
Matter switch/light command
|
v
Shelly channel EndpointHandler
|
v
Local state update + Matter pusher notification + queued MQTT command
|
v
MqttIntegration command publisher
|
+--> l1 publishes {"state_l1":"ON"|"OFF"}
|
+--> l2 publishes {"state_l2":"ON"|"OFF"}
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

Request current state:

```bash
nix-shell -p mosquitto --run "mosquitto_pub -h 10.0.0.2 -t 'zigbee2mqtt/Büro Licht & PC Schalter/get' -m '{\"state\":\"\"}'"
```

Manually command L1 / Tim PC Switch:

```bash
nix-shell -p mosquitto --run "mosquitto_pub -h 10.0.0.2 -t 'zigbee2mqtt/Büro Licht & PC Schalter/set' -m '{\"state_l1\":\"ON\"}'"
nix-shell -p mosquitto --run "mosquitto_pub -h 10.0.0.2 -t 'zigbee2mqtt/Büro Licht & PC Schalter/set' -m '{\"state_l1\":\"OFF\"}'"
```

Manually command L2 / Büro Light:

```bash
nix-shell -p mosquitto --run "mosquitto_pub -h 10.0.0.2 -t 'zigbee2mqtt/Büro Licht & PC Schalter/set' -m '{\"state_l2\":\"ON\"}'"
nix-shell -p mosquitto --run "mosquitto_pub -h 10.0.0.2 -t 'zigbee2mqtt/Büro Licht & PC Schalter/set' -m '{\"state_l2\":\"OFF\"}'"
```

## Matter Verification

After starting the bridge and refreshing or pairing it in Home Assistant:

1. Confirm `Büro Licht & PC Schalter` appears as one bridged device.
2. Confirm `Tim PC Switch` appears as a normal switch, not as a light.
3. Confirm `Büro Light` appears as a light.
4. Toggle `Tim PC Switch` from Matter/Home Assistant and verify `state_l1` changes on MQTT.
5. Toggle `Büro Light` from Matter/Home Assistant and verify `state_l2` changes on MQTT.
6. Toggle either channel outside Matter and confirm Matter/Home Assistant reflects the updated state.

## Implementation Files

| File | Purpose |
| --- | --- |
| `src/input/mqtt/shelly_2pm.rs` | Shelly state parsing, channel handlers, and command payload generation |
| `src/input/mqtt/integration.rs` | MQTT subscription, state routing, initial state request, and command publishing |
| `src/main.rs` | Shelly Matter device registration and endpoint typing |

