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

## Scope 4 Implementation (2026-01-11)

### Matter Device Structure

The W100 is exposed as a single parent device "Tim Thermometer" with two child endpoints:
- Temperature sensor (TemperatureMeasurement cluster 0x0402)
- Humidity sensor (RelativeHumidityMeasurement cluster 0x0405)

### Files Created/Modified

| File | Purpose |
|------|---------|
| `src/matter/clusters/temperature_measurement.rs` | TemperatureSensor + TemperatureMeasurementHandler |
| `src/matter/clusters/relative_humidity.rs` | HumiditySensor + RelativeHumidityHandler |
| `src/matter/virtual_device.rs` | Added `EndpointConfig::temperature_sensor()` and `humidity_sensor()` |
| `src/main.rs` | W100 device registration and MQTT integration wiring |

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│ main.rs                                                      │
├─────────────────────────────────────────────────────────────┤
│ Arc<TemperatureSensor> ←──shared──→ MqttIntegration        │
│ Arc<HumiditySensor>    ←──shared──→ MqttIntegration        │
│         ↓                                  ↓                │
│   Matter Stack                       MQTT Client            │
│         ↓                                  ↓                │
│ TemperatureMeasurementHandler    W100Device::process_*()   │
│ RelativeHumidityHandler          set_celsius() / set_percent()│
└─────────────────────────────────────────────────────────────┘
```

The sensors use atomic values (`AtomicI16`, `AtomicU16`) with version counters for thread-safe updates and Matter subscription notifications.

### Initial Issue: Hardcoded Values on Startup

**Symptom**: Home Assistant showed 20.0°C and 50.0% (initialization defaults) until W100 woke up.

**Resolution**: The MQTT → Matter data flow works correctly. The W100 is battery-powered and only publishes on:
1. Button press
2. Temperature/humidity change beyond threshold
3. Set command received

Once the W100 published (via button press or threshold change), values updated correctly:
```
[MQTT] Tim-Thermometer temperature updated: 21.5°C
[MQTT] Tim-Thermometer humidity updated: 47.5%
```
Home Assistant reflected these values immediately.

### Improvement: Request State on Startup

To avoid showing stale defaults on bridge startup, request W100 state immediately after subscribing by publishing to `zigbee2mqtt/Tim-Thermometer/get`.

**Implementation** (in `integration.rs` after subscribing):
```rust
// Request current state from all devices
for device in &self.w100_devices {
    let get_topic = format!("zigbee2mqtt/{}/get", device.friendly_name);
    client.publish(&get_topic, QoS::AtMostOnce, false, r#"{"state":""}"#.as_bytes()).await;
}
```

### Test Commands

```bash
# Monitor W100 topics
nix-shell -p mosquitto --run "mosquitto_sub -h 10.0.0.2 -t 'zigbee2mqtt/Tim-Thermometer/#' -v"

# Force state publish manually
nix-shell -p mosquitto --run "mosquitto_pub -h 10.0.0.2 -t 'zigbee2mqtt/Tim-Thermometer/get' -m '{\"state\":\"\"}'"
```

### Status

**Scope 4 complete** - Temperature and humidity sensors working via Matter. Remaining: add state request on startup for better UX.

---

## Scope 4.5: Device Identification & Button Events (2026-01-12)

### Reference Comparison

Comparing our implementation against a native Thread/Matter W100 ("Küche Thermometer"):

| Feature | Reference (Thread) | Current (Zigbee→Bridge) | Status |
|---------|-------------------|------------------------|--------|
| Device Name | "Aqara Climate Sensor W100 (8196)" | "Climate Sensor W100" | ✅ Working |
| Vendor | "by Aqara" | "by Aqara" | ✅ Working |
| Temperature | 20.5°C | 21.5°C | ✅ Working |
| Humidity | 45.31% | 50.86% | ✅ Working |
| Button (3) - Plus | Event entity | Logged only | ⛔ Blocked |
| Button (4) - Minus | Event entity | Logged only | ⛔ Blocked |
| Button (5) - Center | Event entity | Logged only | ⛔ Blocked |
| Battery | 100% | Not exposed | ❌ Missing |
| Battery Type | CR2450 | Not exposed | ❌ Missing |
| Battery Voltage | 3V | Not exposed | ❌ Missing |
| Firmware | 1.0.1.0 | Not exposed | ❌ Missing |
| Hardware | 0.0.1.2 | Not exposed | ❌ Missing |
| Serial Number | 54EF441001422B32 | Not exposed | ❌ Missing |
| Identify | 5 endpoints | Not implemented | ❌ Missing |

### Status Summary

1. **Device Name/Vendor**: ✅ **IMPLEMENTED** - `BridgedDeviceBasicInformation` cluster now exposes `VendorName`, `ProductName`, `NodeLabel`, and other attributes.

2. **Button Events**: ⛔ **BLOCKED** - W100 button actions ARE parsed from MQTT and logged, but **rs-matter does NOT support Matter events** (listed as "next steps" in rs-matter roadmap). Cannot expose as GenericSwitch without event support.

3. **Battery**: ❌ **NOT STARTED** - zigbee2mqtt publishes battery data, but no `PowerSource` cluster (0x002F) implementation exists yet.

### Implementation Status

#### Part A: Enhanced BridgedDeviceBasicInformation - ✅ COMPLETE

All attributes now implemented in `src/matter/clusters/bridged_device_basic_info.rs`:

| Attribute | ID | Type | Value for W100 | Status |
|-----------|-----|------|----------------|--------|
| VendorName | 0x0001 | string | "Aqara" | ✅ |
| ProductName | 0x0003 | string | "Climate Sensor W100" | ✅ |
| NodeLabel | 0x0005 | string | "Tim Thermometer" | ✅ |
| HardwareVersion | 0x0007 | u16 | From zigbee2mqtt | ✅ |
| SoftwareVersion | 0x0009 | u32 | From zigbee2mqtt | ✅ |
| SerialNumber | 0x000F | string | IEEE address | ✅ |
| Reachable | 0x0011 | bool | true | ✅ |

#### Part B: GenericSwitch Cluster for Buttons - ⛔ BLOCKED

**Blocker:** rs-matter does NOT support Matter events (required for GenericSwitch).

Button actions ARE parsed and logged, but cannot be exposed to Matter until rs-matter adds event support.

**Current Endpoint Structure:**
```
Tim Thermometer (Parent Device)
├── EP3: Temperature Sensor     ✅ Working
├── EP4: Humidity Sensor        ✅ Working
├── EP5: Button (Plus)          ⛔ Blocked (needs events)
├── EP6: Button (Minus)         ⛔ Blocked (needs events)
└── EP7: Button (Center)        ⛔ Blocked (needs events)
```

### Current Success Criteria

| Criteria | Status |
|----------|--------|
| Device info: "Climate Sensor W100" by "Aqara" | ✅ Working |
| Temperature sensor | ✅ Working |
| Humidity sensor | ✅ Working |
| Button events in Home Assistant | ⛔ Blocked |
| Button actions logged to console | ✅ Working |

---

## Phase 1: BridgedDeviceBasicInformation Enhancement - ✅ COMPLETE (2026-01-13)

This was a PLATFORM-WIDE improvement. The `BridgedDeviceInfo` struct is REUSABLE for ALL bridged devices, not just W100.

### Implementation Summary

**Files Modified:**

| File | Changes |
|------|---------|
| `src/matter/clusters/bridged_device_basic_info.rs` | Added `BridgedDeviceInfo` struct (lines 99-161), updated `BridgedHandler` to read all attributes (lines 163-289) |
| `src/matter/virtual_device.rs` | Added `device_info: Option<BridgedDeviceInfo>` field, added `with_device_info()` builder |
| `src/matter/stack.rs` | Passes `device_info` to `BridgedHandler` during endpoint creation |
| `src/main.rs` | W100 uses device info: vendor="Aqara", product="Climate Sensor W100" (lines 168-173) |

**Attributes Now Exposed:**

| Attribute | ID | Type | Status |
|-----------|-----|------|--------|
| VendorName | 0x0001 | string | ✅ Implemented |
| ProductName | 0x0003 | string | ✅ Implemented |
| NodeLabel | 0x0005 | string | ✅ Implemented |
| HardwareVersion | 0x0007 | u16 | ✅ Implemented |
| SoftwareVersion | 0x0009 | u32 | ✅ Implemented |
| SerialNumber | 0x000F | string | ✅ Implemented |
| Reachable | 0x0011 | bool | ✅ Implemented |

**Current W100 Setup (main.rs:168-181):**

```rust
VirtualDevice::new("Tim Thermometer")
    .with_device_info(
        BridgedDeviceInfo::new("Tim Thermometer")
            .with_vendor("Aqara")
            .with_product("Climate Sensor W100"),
    )
    .with_endpoint(EndpointConfig::temperature_sensor("Temperature", w100_temperature.clone()))
    .with_endpoint(EndpointConfig::humidity_sensor("Humidity", w100_humidity.clone()))
```

**Also Implemented:** State request on startup (`integration.rs:222-239`) - bridge now requests current state from W100 immediately after subscribing, avoiding stale default values.

---

## Phase 2: GenericSwitch Cluster (FOR BUTTON EVENTS) - ⛔ BLOCKED

```
╔══════════════════════════════════════════════════════════════════════════════════════════╗
║                                                                                          ║
║  ⛔ VERIFIED BLOCKER (2026-01-14)                                                        ║
║                                                                                          ║
║  rs-matter does NOT support Matter events yet.                                          ║
║                                                                                          ║
║  Verified via: https://github.com/project-chip/rs-matter README                         ║
║  Under "Next steps" it explicitly states: "Support for Events" as a future objective.   ║
║                                                                                          ║
║  GenericSwitch REQUIRES Matter events (InitialPress, ShortRelease, MultiPressComplete)  ║
║  to function. Without event support, button presses cannot be exposed to Home Assistant.║
║                                                                                          ║
║  CURRENT STATE:                                                                          ║
║  - Button actions ARE parsed from MQTT (integration.rs:112-114, 122-125)                ║
║  - Button actions ARE logged to console                                                 ║
║  - Button actions CANNOT be forwarded to Matter (no event support)                      ║
║                                                                                          ║
║  ALTERNATIVES:                                                                           ║
║  1. Skip buttons until rs-matter adds event support                                     ║
║  2. Hacky workaround: Expose buttons as OnOff switches that briefly toggle              ║
║  3. Use MQTT directly from Home Assistant for button automations (bypasses Matter)      ║
║                                                                                          ║
╚══════════════════════════════════════════════════════════════════════════════════════════╝
```

This would be a PLATFORM-WIDE improvement. The `GenericSwitchHandler` would be REUSABLE for ANY device with buttons, not just W100.

### GenericSwitch Cluster Details

| Property | Value |
|----------|-------|
| Cluster ID | 0x003B |
| Device Type ID | 0x000F (Generic Switch) |
| Device Type Revision | 2 |
| Feature | Momentary Switch (MS) - bit 2 (0x04) |

### Attributes

| Attribute | ID | Type | Description |
|-----------|-----|------|-------------|
| NumberOfPositions | 0x0000 | u8 | Always 2 for momentary (released/pressed) |
| CurrentPosition | 0x0001 | u8 | 0 = released, 1 = pressed |
| MultiPressMax | 0x0002 | u8 | Max multi-press count (2 for double-press) |

### Events (REQUIRE rs-matter SUPPORT - UNVERIFIED)

| Event | ID | Description |
|-------|-----|-------------|
| InitialPress | 0x01 | Button pressed down |
| ShortRelease | 0x03 | Button released after short press |
| MultiPressComplete | 0x06 | Multi-press sequence completed |

### GenericSwitchHandler Struct (REUSABLE)

```rust
/// Handler for GenericSwitch cluster.
/// REUSABLE for ANY device with buttons - not just W100.
pub struct GenericSwitchHandler {
    dataver: Dataver,
    /// Current position (0 = released, 1 = pressed)
    current_position: AtomicU8,
    /// Version counter for change detection
    version: AtomicU32,
    /// Number of positions (always 2 for momentary)
    num_positions: u8,
    /// Maximum multi-press count
    multi_press_max: u8,
}

impl GenericSwitchHandler {
    pub fn new(dataver: Dataver) -> Self {
        Self {
            dataver,
            current_position: AtomicU8::new(0),
            version: AtomicU32::new(0),
            num_positions: 2,
            multi_press_max: 2,
        }
    }

    /// Called when button is pressed.
    /// TODO: Emit InitialPress event when rs-matter supports events.
    pub fn on_press(&self) {
        self.current_position.store(1, Ordering::SeqCst);
        self.version.fetch_add(1, Ordering::SeqCst);
        self.dataver.changed();
    }

    /// Called when button is released.
    /// TODO: Emit ShortRelease/MultiPressComplete event when rs-matter supports events.
    pub fn on_release(&self, press_type: ButtonPress) {
        self.current_position.store(0, Ordering::SeqCst);
        self.version.fetch_add(1, Ordering::SeqCst);
        self.dataver.changed();
    }
}
```

### Button Mapping for W100

| W100 Action | Matter Event | Endpoint |
|-------------|--------------|----------|
| `single_plus` | ShortRelease | Button 3 (Plus) |
| `single_minus` | ShortRelease | Button 4 (Minus) |
| `single_center` | ShortRelease | Button 5 (Center) |
| `double_plus` | MultiPressComplete(2) | Button 3 |
| `double_minus` | MultiPressComplete(2) | Button 4 |
| `double_center` | MultiPressComplete(2) | Button 5 |
| `hold_plus` | InitialPress | Button 3 |
| `hold_minus` | InitialPress | Button 4 |
| `hold_center` | InitialPress | Button 5 |

### Files to Create/Modify

| File | Changes |
|------|---------|
| `src/matter/clusters/generic_switch.rs` | **NEW** - GenericSwitchHandler |
| `src/matter/clusters/mod.rs` | Export generic_switch module |
| `src/matter/device_types.rs` | Add `DEV_TYPE_GENERIC_SWITCH` (0x000F), add `GenericSwitch` to `VirtualDeviceType` |
| `src/matter/virtual_device.rs` | Add `EndpointKind::GenericSwitch`, add `generic_switch()` factory |
| `src/matter/stack.rs` | Wire `GenericSwitchHandler` in `DynamicHandler` |
| `src/input/mqtt/integration.rs` | Connect `W100Action` to `GenericSwitchHandler` methods |
| `src/main.rs` | Add 3 button endpoints to W100 device |

### Usage Example (W100 with Buttons)

```rust
let button_plus = Arc::new(GenericSwitchHandler::new(Dataver::new_rand(rand)));
let button_minus = Arc::new(GenericSwitchHandler::new(Dataver::new_rand(rand)));
let button_center = Arc::new(GenericSwitchHandler::new(Dataver::new_rand(rand)));

VirtualDevice::new(VirtualDeviceType::TemperatureSensor, "Tim Thermometer")
    .with_device_info(
        BridgedDeviceInfo::new("Tim Thermometer")
            .with_vendor("Aqara")
            .with_product("Climate Sensor W100")
    )
    .with_endpoint(EndpointConfig::temperature_sensor("Temperature", temp))
    .with_endpoint(EndpointConfig::humidity_sensor("Humidity", humidity))
    .with_endpoint(EndpointConfig::generic_switch("Button Plus", button_plus))
    .with_endpoint(EndpointConfig::generic_switch("Button Minus", button_minus))
    .with_endpoint(EndpointConfig::generic_switch("Button Center", button_center))
```

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
