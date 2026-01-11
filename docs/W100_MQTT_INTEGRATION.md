# Aqara W100 Integration via MQTT

This document describes the integration of the Aqara W100 Climate Sensor into the Virtual Matter Bridge via MQTT, translating zigbee2mqtt data into Matter devices for Home Assistant.

## Goal

**Instead of**: Home Assistant ← MQTT ← zigbee2mqtt ← W100
**We want**: Home Assistant ← Matter ← **Virtual Matter Bridge** ← MQTT ← zigbee2mqtt ← W100

The Virtual Matter Bridge connects to the MQTT broker, subscribes to zigbee2mqtt topics for the W100, and exposes it as Matter device(s) to Home Assistant.

---

## Infrastructure

| Component | Location | Details |
|-----------|----------|---------|
| HomeAssistant Yellow | 10.0.0.2 | Running NixOS (not HA OS) |
| Mosquitto MQTT Broker | 10.0.0.2:1883 | Docker container, no authentication |
| zigbee2mqtt | NixOS service | Manages Zigbee devices, publishes to MQTT |
| Home Assistant | 10.0.0.2:8123 | NixOS service |
| Virtual Matter Bridge | TBD | Will run on same host |

### Relevant NixOS Config Files

- Host config: `/home/tim/Coding/nixos/hosts/homeassistant-yellow.nix`
- HA config: `/home/tim/Coding/nixos/services/homeassistant.nix`
- Config root: `/home/tim/Coding/nixos/`

---

## Device: Aqara W100 Climate Sensor

- **Zigbee2mqtt name**: `Tim-Thermometer`
- **Model**: TH-S04D / X0028HPT89
- **Connection**: Zigbee (via zigbee2mqtt)
- **Native Matter**: Yes (Thread), but with limited functionality

### Physical Features

**Display layout (top to bottom):**
```
┌─────────────────────────────┐
│  [battery]  [signal]        │  ← Status icons
│                             │
│        22.5°C               │  ← Actual temperature (large)
│                             │
│         54%                 │  ← Actual humidity
│                             │
│  Secondary  25.5°  75%      │  ← External values line
└─────────────────────────────┘
```

- **Top row**: Battery icon + signal/connectivity icon
- **Main area**: Actual temperature reading (large, e.g., "22.5°C")
- **Middle**: Actual humidity reading (e.g., "54%")
- **Bottom row** (when `sensor: "external"`):
  - Left: "Secondary" label (fixed, auto-shown)
  - Middle: `external_temperature` value with °C
  - Right: `external_humidity` value with %

**Buttons** (right side, top to bottom): `+`, unlabeled center, `-`

### Why Zigbee Instead of Native Thread/Matter

The W100 natively supports Matter-over-Thread, but the **middle display line cannot be addressed** via Thread. Using Zigbee through zigbee2mqtt exposes `external_temperature` and `sensor` select, allowing control of the middle display.

---

## W100 zigbee2mqtt MQTT Interface

### State Topic (Device → Bridge)

**Topic**: `zigbee2mqtt/Tim-Thermometer`

**Actual payload observed** (2026-01-11):
```json
{
  "data": null,
  "display_off": false,
  "external_humidity": null,
  "external_temperature": null,
  "high_humidity": 99.99,
  "high_temperature": 60,
  "humi_period": 30,
  "humi_report_mode": "threshold",
  "humi_threshold": 3,
  "humidity": 55.27,
  "identify": null,
  "linkquality": 136,
  "low_humidity": 0,
  "low_temperature": -20,
  "mode": null,
  "period": 1,
  "sampling": "standard",
  "sensor": "external",
  "temp_period": 30,
  "temp_report_mode": "threshold",
  "temp_threshold": 0.5,
  "temperature": 21.98,
  "update": {"installed_version": -1, "latest_version": -1, "state": null}
}
```

| Field | Type | Description |
|-------|------|-------------|
| `temperature` | float | Current temperature reading (°C) |
| `humidity` | float | Current humidity reading (%) |
| `sensor` | string | Display mode: `"internal"` or `"external"` |
| `external_temperature` | float/null | Value shown on middle line (when sensor=external) |
| `external_humidity` | float/null | External humidity value |
| `linkquality` | int | Zigbee signal quality (0-255) |
| `display_off` | bool | Auto display off enabled |
| `mode` | string/null | Thermostat mode (ON/OFF) |
| `high_temperature` | float | High temp alert threshold |
| `low_temperature` | float | Low temp alert threshold |
| `high_humidity` | float | High humidity alert threshold |
| `low_humidity` | float | Low humidity alert threshold |
| `sampling` | string | Sampling mode: "low", "standard", "high", "custom" |
| `period` | float | Sampling period in seconds |

### Action Topic (Button Events)

**Topic**: `zigbee2mqtt/Tim-Thermometer/action`

Actions are also included in the main state topic as `"action": "<value>"`.

| Payload | Button | Description |
|---------|--------|-------------|
| `single_plus` | + button | Single press |
| `single_minus` | - button | Single press |
| `single_center` | Center button | Single press |
| `double_plus` | + button | Double press |
| `double_minus` | - button | Double press |
| `double_center` | Center button | Double press |
| `hold_plus` | + button | Hold started |
| `hold_minus` | - button | Hold started |
| `hold_center` | Center button | Hold started |
| `release_plus` | + button | Hold released |
| `release_minus` | - button | Hold released |
| `release_center` | Center button | Hold released |

### Set Topic (Bridge → Device)

**Topic**: `zigbee2mqtt/Tim-Thermometer/set`

To display external temperature on middle line:
```json
{
  "sensor": "external",
  "external_temperature": 22.0
}
```

To switch back to internal sensor:
```json
{
  "sensor": "internal"
}
```

---

## Desired Functionality

The W100 will be placed in "Büro" (office) room.

### Middle Display
- Show target temperature of `climate.tim_buro_schlafzimmer_heizung` (Better Thermostat)
- Requires periodic updates when climate target changes

### Plus Button (`single_plus`)
- Increase `climate.tim_buro_schlafzimmer_heizung` target temperature by 1°C

### Minus Button (`single_minus`)
- Decrease `climate.tim_buro_schlafzimmer_heizung` target temperature by 1°C

### Center Button (`single_center`)
- Toggle `switch.audio_receiver` AND `switch.subwoofer`
- Replaces the existing webhook trigger in `audio_receiver_control.yaml`

---

## Implementation Scopes

### Scope 1: MQTT Terminal Testing

**Goal**: Verify the W100 is connected to the MQTT broker and publishing data.

**Steps**:
1. Use `nix-shell -p mosquitto` to get MQTT tools
2. Subscribe to zigbee2mqtt topics to see W100 data
3. Test button presses and observe action payloads
4. Test publishing to set topic to control display

**Commands**:
```bash
# Subscribe to all W100 topics
nix-shell -p mosquitto --run "mosquitto_sub -h 10.0.0.2 -t 'zigbee2mqtt/Tim-Thermometer/#' -v"

# Test setting external display
nix-shell -p mosquitto --run "mosquitto_pub -h 10.0.0.2 -t 'zigbee2mqtt/Tim-Thermometer/set' -m '{\"sensor\":\"external\",\"external_temperature\":22.5}'"
```

**Success criteria**: See temperature/humidity JSON on state topic, button actions on action topic.

---

### Scope 2: MQTT Communication in Rust

**Goal**: Create Rust MQTT client that can subscribe and publish to W100 topics.

**Steps**:
1. Add `rumqttc` dependency to Cargo.toml
2. Create `src/input/mqtt/mod.rs` module
3. Implement async MQTT client wrapper
4. Subscribe to W100 state and action topics
5. Parse incoming JSON payloads
6. Implement publish function for set topic

**Deliverable**: Rust code that logs W100 state changes and button presses.

---

### Scope 3: Rust Test Implementation

**Goal**: Test-implement button handling and display control in Rust (no Matter yet).

**Steps**:
1. Create W100 device abstraction (`W100Device` struct)
2. Parse button actions (`single_plus`, `single_minus`, `single_center`)
3. Implement callback system for button events
4. Implement `set_external_temperature(f32)` method
5. Test bidirectional communication

**Deliverable**: Standalone Rust program that reacts to W100 buttons and can control display.

---

### Scope 4: Matter Translation

**Goal**: Expose W100 as Matter device(s) for Home Assistant.

**Steps**:
1. Research rs-matter cluster support (TemperatureMeasurement, Switch, etc.)
2. Map W100 features to Matter endpoints/clusters
3. Integrate MQTT input with existing Virtual Matter Bridge architecture
4. Create `EndpointHandler` implementations for W100 features
5. Register W100 as virtual device(s) in Matter stack

**Deliverable**: W100 visible as Matter device in Home Assistant.

---

### Scope 5: Bidirectional Communication

**Goal**: Enable Home Assistant to write values that appear on W100 display.

**Steps**:
1. Implement Matter → MQTT path for setpoint writes
2. Choose appropriate cluster (Thermostat setpoint or custom)
3. Handle HA writing target temperature → bridge publishes to MQTT → W100 display updates
4. Create HA automations in NixOS config to sync climate target with W100 display

**Deliverable**: W100 middle display shows value controlled from Home Assistant.

---

### Scope 6: Finalization

**Goal**: Complete remaining features and polish.

**Steps**:
1. Add battery level reporting (PowerSource cluster)
2. Handle MQTT reconnection gracefully
3. Add configuration options (device name, topics)
4. Update CLAUDE.md with new input type
5. Create NixOS service configuration for deployment
6. Test end-to-end on homeassistant-yellow

**Deliverable**: Production-ready W100 integration.

---

## Scope 1 Testing Results (2026-01-11)

### MQTT Broker Connectivity

**Status**: Working

- Broker: `10.0.0.2:1883` (no authentication)
- Connection: Successful from local machine via `nix-shell -p mosquitto`

### zigbee2mqtt Status

**Status**: Online

- Bridge state: `{"state":"online"}`
- W100 IEEE address: `0x54ef441001421fb0`
- Friendly name: `Tim-Thermometer`
- zigbee2mqtt version: 2.6.3

### W100 State Publishing

**Status**: Working (publishes on state change or command)

- Device is battery-powered and sleeps most of the time
- State is published when:
  - A set command is received
  - Temperature/humidity changes beyond threshold
  - Button is pressed
- Current readings observed:
  - Temperature: **21.98°C**
  - Humidity: **55.27%**
  - Link quality: **128-136**

### Set Command (External Display)

**Status**: Working

**Enable external display and set values:**
```bash
mosquitto_pub -h 10.0.0.2 -t 'zigbee2mqtt/Tim-Thermometer/set' \
  -m '{"sensor":"external","external_temperature":25.5,"external_humidity":75}'
```

**Display behavior:**
- "Secondary" label appears in bottom left (fixed label, auto-shown)
- `external_temperature` shown in bottom middle with °C
- `external_humidity` shown in bottom right with %
- Both values are optional (can set one or both)
- Supports negative temperature values (tested: -5.0)
- Supports whole numbers and decimals
- Updates immediately when new values are published

**Disable external display (return to internal):**
```bash
mosquitto_pub -h 10.0.0.2 -t 'zigbee2mqtt/Tim-Thermometer/set' \
  -m '{"sensor":"internal"}'
```

Result: The bottom-left "Secondary" line clears (becomes blank). Temperature, humidity, battery, and connectivity continue displaying normally.

### Button Actions

**Status**: Working

Button presses publish to **two locations**:
1. Main state topic with `"action"` field in JSON
2. Dedicated `/action` topic with just the action string

**Verified action payloads:**

| Action | Button | Type |
|--------|--------|------|
| `single_plus` | + button | Single press |
| `single_minus` | - button | Single press |
| `single_center` | Center button | Single press |
| `double_plus` | + button | Double press |
| `double_minus` | - button | Double press |
| `double_center` | Center button | Double press |
| `hold_plus` | + button | Hold start |
| `hold_minus` | - button | Hold start |
| `hold_center` | Center button | Hold start |
| `release_plus` | + button | Hold release |
| `release_minus` | - button | Hold release |
| `release_center` | Center button | Hold release |

**Example `/action` topic message:**
```
zigbee2mqtt/Tim-Thermometer/action single_center
```

**Example state topic with action:**
```json
{
  "action": "single_center",
  "temperature": 21.98,
  "humidity": 58.74,
  ...
}
```

### Scope 1 Summary

**All tests passed!** The W100 is fully functional via MQTT:
- State publishing works (temperature, humidity, settings)
- Button actions work (single, double, hold/release for all 3 buttons)
- External display works:
  - `sensor: "external"` enables the bottom "Secondary" line
  - `external_temperature` displays a temperature value (middle)
  - `external_humidity` displays a humidity value (right)
  - Both update immediately when published

**For the target use case**: Set `external_temperature` to the Better Thermostat target temp to show it on the W100 display.

Ready to proceed to **Scope 2: MQTT Communication in Rust**

---

## Scope 2 Implementation (2026-01-11)

### Files Created

| File | Purpose |
|------|---------|
| `src/input/mqtt/mod.rs` | Module exports |
| `src/input/mqtt/client.rs` | `MqttClient` - async MQTT client wrapper using rumqttc |
| `src/input/mqtt/w100.rs` | `W100Device` - W100-specific state parsing and commands |

### Dependencies Added

```toml
rumqttc = "0.24"  # Async MQTT client
```

### Configuration Added

New `MqttConfig` in `src/config.rs`:

| Environment Variable | Default | Description |
|---------------------|---------|-------------|
| `MQTT_BROKER_HOST` | `10.0.0.2` | MQTT broker hostname/IP |
| `MQTT_BROKER_PORT` | `1883` | MQTT broker port |
| `MQTT_CLIENT_ID` | `virtual-matter-bridge` | Client ID |
| `MQTT_USERNAME` | None | Optional username |
| `MQTT_PASSWORD` | None | Optional password |

### Key Types

**`MqttClient`** - MQTT client wrapper:
- `new(config)` - Create from MqttConfig
- `subscribe(topic)` - Subscribe to topic
- `publish(topic, payload)` - Publish message
- `run(tx)` - Run event loop, forward messages to channel
- `client()` - Get AsyncClient clone for publishing from other tasks

**`W100Device`** - W100 device handler:
- `new(friendly_name)` - Create handler for device
- `with_mqtt_client(client)` - Set MQTT client for publishing
- `with_action_channel(tx)` - Set channel for button events
- `process_message(topic, payload)` - Process incoming MQTT message
- `get_temperature()` / `get_humidity()` - Read sensor values
- `set_external_temperature(f32)` - Set display value
- `set_external_humidity(f32)` - Set display humidity
- `set_external_values(temp, humidity)` - Set both
- `set_internal_mode()` - Switch back to internal display

**`W100Action`** - Button action enum:
- `SinglePlus`, `SingleMinus`, `SingleCenter`
- `DoublePlus`, `DoubleMinus`, `DoubleCenter`
- `HoldPlus`, `HoldMinus`, `HoldCenter`
- `ReleasePlus`, `ReleaseMinus`, `ReleaseCenter`

### Status

Code compiles. Scope 2 complete.

---

## Scope 3 Testing Results (2026-01-11)

### Test Binary Created

`src/bin/mqtt-test.rs` - standalone test for W100 MQTT communication.

Run with: `cargo run --bin mqtt-test`

### Test Results

**External Display**: Working
- Set `external_temperature` to 23.5°C via Rust
- W100 displayed "Secondary 23.5 75%"

**State Reading**: Working
- Read `temperature=22.5°C`, `humidity=53.0%`

**Button Detection**: Working
- `SinglePlus` → ">>> PLUS button pressed!"
- `SingleMinus` → ">>> MINUS button pressed!"
- `SingleCenter` → ">>> CENTER button pressed!"
- `DoubleMinus`, `DoubleCenter` → Also detected

### Status

Scope 3 complete. Ready for **Scope 4: Matter Translation**.

---

## Technical Implementation Details

### Phase 1: Add MQTT Input Source to Virtual Matter Bridge

Create new input module at `src/input/mqtt/`:

```
src/input/mqtt/
├── mod.rs           # Module exports
├── client.rs        # MQTT client wrapper (rumqttc crate)
├── z2m_device.rs    # zigbee2mqtt device abstraction
└── w100.rs          # W100-specific parsing and Matter mapping
```

#### MQTT Client Requirements

- Connect to `mqtt://10.0.0.2:1883` (no auth)
- Subscribe to topics:
  - `zigbee2mqtt/Tim-Thermometer` (state)
  - `zigbee2mqtt/Tim-Thermometer/action` (button events)
- Publish to:
  - `zigbee2mqtt/Tim-Thermometer/set` (control display)

#### Recommended Crate

- `rumqttc` - async MQTT client, tokio-compatible

### Phase 2: Map W100 to Matter Devices

The W100 should be exposed as multiple Matter endpoints:

| W100 Feature | Matter Device Type | Cluster | Direction |
|--------------|-------------------|---------|-----------|
| Temperature | Temperature Sensor (0x0302) | TemperatureMeasurement (0x0402) | Read |
| Humidity | Humidity Sensor (0x0307) | RelativeHumidityMeasurement (0x0405) | Read |
| Battery | (attribute on device) | PowerSource (0x002F) | Read |
| Plus Button | Generic Switch (0x000F) | Switch (0x003B) | Event |
| Minus Button | Generic Switch (0x000F) | Switch (0x003B) | Event |
| Center Button | Generic Switch (0x000F) | Switch (0x003B) | Event |
| External Display | Custom/Thermostat? | Thermostat (0x0201) | Write |

**Note**: The buttons emit Matter switch events that Home Assistant automations can react to. The "external display" feature requires the bridge to receive commands FROM Home Assistant and publish to MQTT.

### Phase 3: Bidirectional Communication

```
[Temperature/Humidity/Battery]
W100 → zigbee2mqtt → MQTT → Bridge → Matter → Home Assistant
                                                    ↓
                                              (HA reads sensors)

[Button Press]
W100 → zigbee2mqtt → MQTT → Bridge → Matter Event → Home Assistant
                                                          ↓
                                                    (HA automation triggers)

[Set External Display]
Home Assistant → Matter Write → Bridge → MQTT → zigbee2mqtt → W100
       ↑
(HA automation writes to thermostat setpoint)
```

### Phase 4: Home Assistant Configuration

After the bridge exposes the W100 as Matter devices, configure automations in NixOS:

```yaml
# W100 button actions (in Home Assistant automations)
- id: w100_plus_button
  trigger:
    - platform: device
      domain: matter
      device_id: <w100_plus_button_device_id>
      type: single_press
  action:
    - service: climate.set_temperature
      target:
        entity_id: climate.tim_buro_schlafzimmer_heizung
      data:
        temperature: "{{ state_attr('climate.tim_buro_schlafzimmer_heizung', 'temperature') + 1 }}"
```

---

## New Dependencies

Add to `Cargo.toml`:

```toml
[dependencies]
rumqttc = "0.24"  # Async MQTT client
```

---

## Configuration

New environment variables for the bridge:

| Variable | Default | Description |
|----------|---------|-------------|
| `MQTT_BROKER_HOST` | `10.0.0.2` | MQTT broker hostname/IP |
| `MQTT_BROKER_PORT` | `1883` | MQTT broker port |
| `MQTT_CLIENT_ID` | `virtual-matter-bridge` | Client ID for MQTT connection |

---

## New Clusters Required

The current bridge implementation has:
- BooleanState (contact sensors)
- OccupancySensing (motion sensors)
- OnOff (switches)

New clusters needed for W100:
- **TemperatureMeasurement** (0x0402) - temperature sensor readings
- **RelativeHumidityMeasurement** (0x0405) - humidity readings
- **Switch** (0x003B) - button press events (GenericSwitch device type)
- **PowerSource** (0x002F) - battery level (optional)
- **Thermostat** (0x0201) - for external display setpoint (bidirectional)

---

## Testing Plan

### Local Testing (before deployment)

1. Use `nix-shell -p mosquitto` to get MQTT client tools
2. Subscribe to W100 topics:
   ```bash
   mosquitto_sub -h 10.0.0.2 -t 'zigbee2mqtt/Tim-Thermometer/#' -v
   ```
3. Verify button presses appear as `action` payloads
4. Test publishing to set topic:
   ```bash
   mosquitto_pub -h 10.0.0.2 -t 'zigbee2mqtt/Tim-Thermometer/set' \
     -m '{"sensor":"external","external_temperature":22.5}'
   ```

### Integration Testing

1. Run Virtual Matter Bridge locally connected to remote MQTT broker
2. Commission bridge to Home Assistant (via python-matter-server)
3. Verify W100 appears as Matter devices in HA
4. Test button events trigger automations
5. Test writing to thermostat updates W100 display

---

## Open Questions

1. **Matter Generic Switch**: Does rs-matter support GenericSwitch device type and Switch cluster for button events?

2. **Thermostat cluster**: Is Thermostat (0x0201) the right cluster for bidirectional display control, or should we use a simpler approach?

3. **Multiple endpoints**: Should the W100 be one parent device with multiple child endpoints, or separate devices?

4. **Reconnection**: How should the bridge handle MQTT disconnections and zigbee2mqtt restarts?

---

## References

- [zigbee2mqtt W100 device page](https://www.zigbee2mqtt.io/devices/TH-S04D.html)
- [GitHub issue: W100 external temperature workaround](https://github.com/Koenkk/zigbee2mqtt/issues/27262)
- [Home Assistant blueprint for W100](https://github.com/clementTal/homelab/blob/main/blueprint/aqara-w100.yaml)
- [Matter specification clusters](https://csa-iot.org/developer-resource/specifications-download-request/)
