# Aqara W100 via MQTT

The bridge exposes an Aqara W100 paired to Zigbee2MQTT as a Matter device named
`Büro Thermometer`, with temperature, humidity, and three Generic Switch endpoints.
The Zigbee2MQTT friendly name is `Büro-Thermometer`.

## MQTT topics

| Topic | Direction | Payload |
| --- | --- | --- |
| `zigbee2mqtt/Büro-Thermometer` | Device to bridge | JSON state, optionally including `action` |
| `zigbee2mqtt/Büro-Thermometer/action` | Device to bridge | Button action string |
| `zigbee2mqtt/Büro-Thermometer/get` | Bridge to device | State request at startup |
| `zigbee2mqtt/Büro-Thermometer/set` | Bridge to device | Display configuration |

Example state:

```json
{"temperature": 22.5, "humidity": 45, "sensor": "internal", "linkquality": 150}
```

Measurements are reported through Temperature Measurement (`0x0402`) and Relative
Humidity Measurement (`0x0405`). No initial measurement is invented before the
source supplies a value. MQTT disconnection marks source endpoints unavailable.

## Buttons

Each button has a Generic Switch cluster (`0x003b`). The bridge queues actions and
emits Matter events through the upstream SDK.

| Zigbee2MQTT action | Endpoint | Meaning |
| --- | --- | --- |
| `single_plus`, `double_plus`, `hold_plus`, `release_plus` | Button Plus | Single, double, hold, release |
| `single_minus`, `double_minus`, `hold_minus`, `release_minus` | Button Minus | Single, double, hold, release |
| `single_center`, `double_center`, `hold_center`, `release_center` | Button Center | Single, double, hold, release |

The center aliases `single`, `double`, `hold`, and `release` are also parsed.
Retained action messages must not replay a button press after a reconnect.

After commissioning the bridge, inspect the three event entities in Home
Assistant and use their actual entity IDs in automations. The repository's
Home Assistant configuration uses plus/minus for thermostat changes and center
for the audio receiver. Device entity IDs are assigned by Home Assistant.

## Display control

The MQTT handler can update the W100 external display. For a direct diagnostic:

```sh
mosquitto_pub -h 10.0.0.2 -t 'zigbee2mqtt/Büro-Thermometer/set' \
  -m '{"sensor":"external","external_temperature":23.5,"external_humidity":50}'
```

These display fields are MQTT controls; they are not additional writable Matter
attributes in the current temperature and humidity endpoints.

## Verification

1. Run `just check` for local parsing, event, and readiness tests.
2. Start the bridge with `just run` and inspect its MQTT connection logs.
3. Confirm temperature and humidity match Zigbee2MQTT.
4. Press each physical button and verify the corresponding Home Assistant event.
5. Restart the bridge without deleting persistence and confirm it reconnects.
6. Disconnect the MQTT source and confirm availability changes propagate.

Implementation lives in `src/input/mqtt/w100.rs`, `src/input/mqtt/integration.rs`,
`src/matter/clusters/generic_switch.rs`, and the device registration in `src/main.rs`.
