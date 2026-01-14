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

## Phase 2: GenericSwitch Cluster (FOR BUTTON EVENTS) - ✅ INTEGRATION COMPLETE (2026-01-14)

```
╔══════════════════════════════════════════════════════════════════════════════════════════╗
║                                                                                          ║
║  ✅ INTEGRATION COMPLETE - UNTESTED                                                      ║
║                                                                                          ║
║  The rs-matter fork (github.com/timlisemer/rs-matter) provides native event support.    ║
║  virtual_matter_bridge now uses the fork's EventSource trait directly.                  ║
║                                                                                          ║
║  IMPLEMENTED:                                                                            ║
║  - GenericSwitch cluster handler using rs-matter native types                           ║
║  - GenericSwitchState implements rs_matter::dm::EventSource trait                       ║
║  - Event encoding via encode_initial_press(), encode_short_release(), etc.              ║
║  - AggregatedEventSource in DynamicHandler collects events from all switches            ║
║  - AsyncHandler trait implementation wires events to Matter subscription reports        ║
║  - W100 button integration: Plus, Minus, Center endpoints                               ║
║  - MQTT action → GenericSwitch event mapping in integration.rs                          ║
║                                                                                          ║
║  REMOVED (was temporary shim):                                                           ║
║  - src/matter/events/ directory (mod.rs, data.rs, path.rs)                              ║
║                                                                                          ║
║  TESTING REQUIRED:                                                                       ║
║  - Commission bridge to Home Assistant                                                   ║
║  - Verify 3 button entities appear (Plus, Minus, Center)                                ║
║  - Press W100 button and confirm event appears in HA                                    ║
║                                                                                          ║
╚══════════════════════════════════════════════════════════════════════════════════════════╝
```

This is a PLATFORM-WIDE improvement. The `GenericSwitchHandler` is REUSABLE for ANY device with buttons, not just W100.

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
/// Shared state for GenericSwitch that can be updated from external sources.
/// Implements EventSource to provide events to Matter subscription reports.
pub struct GenericSwitchState {
    current_position: AtomicU8,
    event_number: EventNumberGenerator,
    pending_events: Mutex<heapless::Vec<PendingEvent, MAX_PENDING_EVENTS>>,
    start_time: Instant,
    endpoint_id: AtomicU8,
}

impl GenericSwitchState {
    /// Record an InitialPress event (button pressed down).
    pub fn press(&self) {
        self.current_position.store(1, Ordering::SeqCst);
        let payload = encode_initial_press(1);
        let event = PendingEvent::with_payload(/* ... */);
        self.pending_events.lock().push(event).ok();
    }

    /// Record a ShortRelease event (button released).
    pub fn release(&self) {
        self.current_position.store(0, Ordering::SeqCst);
        let payload = encode_short_release(prev_position);
        let event = PendingEvent::with_payload(/* ... */);
        self.pending_events.lock().push(event).ok();
    }

    /// Record a double press (MultiPressComplete with count=2).
    pub fn double_press(&self) {
        let payload = encode_multi_press_complete(1, 2);
        let event = PendingEvent::with_payload(/* ... */);
        self.pending_events.lock().push(event).ok();
    }
}

/// Handler for GenericSwitch cluster.
/// REUSABLE for ANY device with buttons - not just W100.
pub struct GenericSwitchHandler {
    dataver: Dataver,
    state: Arc<GenericSwitchState>,
    num_positions: u8,
    multi_press_max: u8,
}

impl GenericSwitchHandler {
    pub fn new(dataver: Dataver, state: Arc<GenericSwitchState>) -> Self {
        Self { dataver, state, num_positions: 2, multi_press_max: 2 }
    }

    pub fn state(&self) -> &Arc<GenericSwitchState> {
        &self.state
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
// Create shared state for each button (can be updated from MQTT handler)
let state_plus = Arc::new(GenericSwitchState::new());
let state_minus = Arc::new(GenericSwitchState::new());
let state_center = Arc::new(GenericSwitchState::new());

VirtualDevice::new(VirtualDeviceType::TemperatureSensor, "Tim Thermometer")
    .with_device_info(
        BridgedDeviceInfo::new("Tim Thermometer")
            .with_vendor("Aqara")
            .with_product("Climate Sensor W100")
    )
    .with_endpoint(EndpointConfig::temperature_sensor("Temperature", temp))
    .with_endpoint(EndpointConfig::humidity_sensor("Humidity", humidity))
    .with_endpoint(EndpointConfig::generic_switch("Button Plus", state_plus.clone()))
    .with_endpoint(EndpointConfig::generic_switch("Button Minus", state_minus.clone()))
    .with_endpoint(EndpointConfig::generic_switch("Button Center", state_center.clone()))

// In MQTT handler, call state methods to generate events:
// state_plus.single_press();     // single press
// state_minus.double_press();    // double press
// state_center.hold_start();     // hold started
// state_center.hold_release();   // hold released
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

## Phase 3: Matter Events Implementation (rs-matter Fork)

This section documents the complete technical approach to implementing Matter event support in rs-matter, enabling GenericSwitch button events to reach Home Assistant.

> **Status: IMPLEMENTED ✅** (January 2025)
>
> Fork: https://github.com/timlisemer/rs-matter
>
> All core event functionality is complete, including:
> - EventDataIB/EventReportIB TLV structures with ToTLV
> - ReportDataResponder event iteration
> - EventSource trait and PendingEvent
> - GenericSwitch cluster with all 6 event types
> - Long press detection (500ms configurable threshold)
> - Multi-press detection (300ms window, max 3 presses)
> - Event filtering with wildcard support
> - Event number persistence framework
>
> The documentation below serves as both historical reference and implementation guide.

### Problem Statement (SOLVED ✅)

rs-matter (as of January 2025) does **not support Matter events**. This is tracked in [rs-matter issue #36](https://github.com/project-chip/rs-matter/issues/36), open since March 2023.

**Original blocker** (now resolved): Button presses on W100 were:
1. Received via MQTT ✅
2. Parsed and mapped to GenericSwitch events ✅
3. Queued in `GenericSwitchState` ✅
4. ~~**Never sent to Matter controllers** ❌~~ → **Now sent via EventReports** ✅

The events previously sat in a queue with no code path to include them in Matter subscription reports. This has been fixed in the fork.

### Solution Overview

Fork rs-matter to `/home/tim/Coding/public_repos/rs-matter` and implement:

1. **EventReportIB TLV structures** - Protocol-level event encoding
2. **ReportDataResponder enhancement** - Include events in subscription reports
3. **Event source interface** - Allow handlers to expose pending events
4. **GenericSwitch integration** - Connect our existing event queue

---

### rs-matter Architecture Overview

#### Repository Structure

```
rs-matter/
├── rs-matter/src/
│   ├── im/                    # Interaction Model (protocol layer)
│   │   ├── attr.rs            # ReportDataReq, ReportDataResp, EventPath
│   │   └── mod.rs             # OpCodes, protocol constants
│   ├── dm/                    # Data Model (application layer)
│   │   ├── mod.rs             # DataModel, report_data(), ReportDataResponder
│   │   ├── subscriptions.rs   # Subscription management
│   │   └── types/
│   │       ├── handler.rs     # AsyncHandler trait
│   │       └── node.rs        # Node, Cluster, Endpoint metadata
│   └── tlv/                   # TLV encoding/decoding
│       ├── write.rs           # TLVWrite, WriteBuf
│       └── traits.rs          # FromTLV, ToTLV derive macros
```

#### Key Data Flow

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         SUBSCRIPTION FLOW                                │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  1. Controller sends SubscribeRequest                                    │
│     └─> dm.rs: subscribe() handles request                              │
│         └─> Creates subscription in DefaultSubscriptions                 │
│                                                                          │
│  2. Attribute changes trigger notification                               │
│     └─> subscriptions.notify_cluster_changed(fabric, node, endpoint)    │
│         └─> Marks subscription as "changed"                              │
│                                                                          │
│  3. Subscription timer fires or change detected                          │
│     └─> dm.rs: process_subscriptions() loops through subscriptions      │
│         └─> report_data() called for changed subscriptions              │
│             └─> ReportDataResponder::respond() builds TLV response      │
│                 └─> Iterates attr_requests(), encodes AttributeReports  │
│                 └─> ❌ NO EVENT ITERATION EXISTS                         │
│                                                                          │
│  4. ReportData message sent to controller                                │
│     └─> Contains AttributeReports array                                  │
│     └─> ❌ EventReports array is MISSING                                 │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

---

### rs-matter Code Analysis

#### File: `rs-matter/src/im/attr.rs`

This file defines the protocol structures for read/subscribe operations.

##### EventPath (lines 208-239) - ALREADY EXISTS

```rust
#[derive(Debug, Clone, Eq, PartialEq, Hash, FromTLV, ToTLV)]
#[tlvargs(lifetime = "'a")]
pub struct EventPath {
    #[tlvargs(tag = 0)]
    pub node: Option<u64>,
    #[tlvargs(tag = 1)]
    pub endpoint: Option<EndptId>,
    #[tlvargs(tag = 2)]
    pub cluster: Option<ClusterId>,
    #[tlvargs(tag = 3)]
    pub event: Option<u32>,
    #[tlvargs(tag = 4)]
    pub is_urgent: Option<bool>,
}
```

This is already implemented and usable - no changes needed.

##### ReportDataResp (lines 284-290) - ~~NEEDS MODIFICATION~~ IMPLEMENTED ✅

```rust
#[derive(FromTLV, ToTLV, Debug)]
#[tlvargs(lifetime = "'a")]
pub struct ReportDataResp<'a> {
    pub subscription_id: Option<u32>,
    pub attr_reports: Option<TLVArray<'a, AttrResp<'a>>>,
    pub event_reports: Option<bool>,  // ❌ STUB - just a bool!
    pub more_chunks: Option<bool>,
    pub suppress_response: Option<bool>,
}
```

**Problem**: `event_reports` is `Option<bool>` instead of `Option<TLVArray<EventReportIB>>`.

##### ReportDataRespTag (lines 296-306) - ~~EXISTS BUT UNDERSCORE PREFIX~~ FIXED ✅

```rust
#[repr(u8)]
pub enum ReportDataRespTag {
    SubscriptionId = 0,
    AttributeReports = 1,
    _EventReport = 2,      // Note underscore - treated as unused
    MoreChunkedMsgs = 3,
    SupressResponse = 4,
}
```

The `_EventReport` tag exists but is never used in the codebase.

##### ReportDataReq Methods (lines 250-275) - ~~EVENT METHODS EXIST BUT UNUSED~~ NOW USED ✅

```rust
impl<'a> ReportDataReq<'a> {
    // Used extensively:
    pub fn attr_requests(&self) -> Result<Option<TLVArray<'a, AttrPath>>, Error>
    pub fn dataver_filters(&self) -> Result<Option<TLVArray<'a, DataVersionFilter>>, Error>

    // EXIST but NEVER CALLED:
    pub fn event_requests(&self) -> Result<Option<TLVArray<'a, EventPath>>, Error>
    pub fn event_filters(&self) -> Result<Option<TLVArray<'a, EventFilter>>, Error>
}
```

The infrastructure to parse event requests from controllers exists, but nothing uses it.

---

#### File: `rs-matter/src/dm.rs`

This file contains the data model logic and the critical `ReportDataResponder`.

##### ReportDataResponder Structure (lines 813-818)

```rust
struct ReportDataResponder<'a, 'b, 'c, D, B> {
    req: &'a ReportDataReq<'a>,
    node: &'a Node<'a>,
    subscription_id: Option<u32>,
    invoker: HandlerInvoker<'b, 'c, D, B>,
}
```

##### ReportDataResponder::respond() - ~~THE CRITICAL METHOD~~ IMPLEMENTED ✅

Location: `rs-matter/src/dm.rs` lines ~847-1100

This is where attribute reports are serialized. The pattern we must replicate for events:

```rust
// Simplified flow of respond() method:

async fn respond(&mut self, exchange: &mut Exchange<'_>) -> Result<(), Error> {
    loop {
        // Get buffer for writing response
        let mut wb = exchange.writer()?;

        // Start ReportData structure
        wb.start_struct(&TLVTag::Anonymous)?;

        // Write subscription ID if present
        if let Some(id) = self.subscription_id {
            wb.u32(&TLVTag::Context(ReportDataRespTag::SubscriptionId as u8), id)?;
        }

        // Check if there are attribute requests
        let has_attr_requests = self.req.attr_requests()?.is_some();

        if has_attr_requests {
            // Start AttributeReports array (tag = 1)
            wb.start_array(&TLVTag::Context(ReportDataRespTag::AttributeReports as u8))?;
        }

        // ═══════════════════════════════════════════════════════════════
        // ATTRIBUTE ITERATION LOOP (lines ~920-1020)
        // This is the pattern we need to replicate for events
        // ═══════════════════════════════════════════════════════════════

        for attr_path in self.req.attr_requests()?.unwrap() {
            // Expand wildcards via node.read()
            for (endpoint, cluster, attr) in self.node.read(&attr_path)? {
                // Get handler for this endpoint/cluster
                let handler = self.invoker.handler(endpoint, cluster)?;

                // Read attribute value
                let value = handler.read(attr, &mut wb)?;

                // Write AttributeReportIB structure
                // ... TLV encoding of path + data + status
            }
        }

        if has_attr_requests {
            // End AttributeReports array
            wb.end_container()?;
        }

        // ═══════════════════════════════════════════════════════════════
        // ❌ EVENT ITERATION WOULD GO HERE - BUT DOESN'T EXIST
        // ═══════════════════════════════════════════════════════════════

        // Write more_chunks flag if needed
        if more_chunks {
            wb.bool(&TLVTag::Context(ReportDataRespTag::MoreChunkedMsgs as u8), true)?;
        }

        // Write suppress_response flag
        if suppress_response {
            wb.bool(&TLVTag::Context(ReportDataRespTag::SupressResponse as u8), true)?;
        }

        // End ReportData structure
        wb.end_container()?;

        // Send response
        exchange.send(OpCode::ReportData, wb)?;

        if !more_chunks {
            break;
        }

        // Wait for StatusResponse before next chunk
        exchange.recv_status()?;
    }

    Ok(())
}
```

---

#### File: `rs-matter/src/dm/subscriptions.rs`

##### DefaultSubscriptions (lines 20-50)

```rust
pub struct DefaultSubscriptions<const N: usize> {
    subscriptions: heapless::Vec<Subscription, N>,
}

struct Subscription {
    fabric_idx: u8,
    peer_node_id: u64,
    id: u32,
    min_interval_secs: u16,
    max_interval_secs: u16,
    last_report: Option<Instant>,
    changed: bool,  // Set by notify_cluster_changed()
}
```

##### notify_cluster_changed() (lines 138-149)

```rust
pub fn notify_cluster_changed(
    &mut self,
    fabric_idx: u8,
    peer_node_id: u64,
    endpoint_id: EndptId,
) {
    for sub in &mut self.subscriptions {
        if sub.fabric_idx == fabric_idx && sub.peer_node_id == peer_node_id {
            sub.changed = true;  // Marks subscription for report
        }
    }
}
```

This is called when attributes change. For events, we might need:
- An `notify_event()` method, OR
- Re-use `changed` flag (simpler - events just trigger a report cycle)

---

### Matter Event TLV Specification

Based on Matter Core Specification 1.4, Section 10.6.

#### EventReportIB Structure

```
EventReportIB ::= STRUCTURE {
    event_status [0, opt]: EventStatusIB,    // For error reporting
    event_data [1, opt]: EventDataIB,        // Actual event data
}
```

#### EventDataIB Structure

```
EventDataIB ::= STRUCTURE {
    path [0]: EventPathIB,
    event_number [1]: UNSIGNED INTEGER (64-bit),
    priority [2]: UNSIGNED INTEGER (8-bit),
    // Exactly ONE of the following timestamps:
    epoch_timestamp [3, opt]: UNSIGNED INTEGER (64-bit),      // Microseconds since Unix epoch
    system_timestamp [4, opt]: UNSIGNED INTEGER (64-bit),     // Milliseconds since boot
    delta_epoch_timestamp [5, opt]: UNSIGNED INTEGER (64-bit),
    delta_system_timestamp [6, opt]: UNSIGNED INTEGER (64-bit),
    data [7, opt]: ANY,  // Cluster-specific event payload
}
```

#### EventPathIB Structure (already in rs-matter)

```
EventPathIB ::= STRUCTURE {
    node [0, opt]: UNSIGNED INTEGER (64-bit),
    endpoint [1, opt]: UNSIGNED INTEGER (16-bit),
    cluster [2, opt]: UNSIGNED INTEGER (32-bit),
    event [3, opt]: UNSIGNED INTEGER (32-bit),
    is_urgent [4, opt]: BOOLEAN,
}
```

#### GenericSwitch Event Payloads

| Event ID | Name | Payload |
|----------|------|---------|
| 0x00 | SwitchLatched | `{ NewPosition [0]: u8 }` |
| 0x01 | InitialPress | `{ NewPosition [0]: u8 }` |
| 0x02 | LongPress | `{ NewPosition [0]: u8 }` |
| 0x03 | ShortRelease | `{ PreviousPosition [0]: u8 }` |
| 0x04 | LongRelease | `{ PreviousPosition [0]: u8 }` |
| 0x05 | MultiPressOngoing | `{ NewPosition [0]: u8, CurrentCount [1]: u8 }` |
| 0x06 | MultiPressComplete | `{ PreviousPosition [0]: u8, TotalCount [1]: u8 }` |

---

### Implementation Plan (ALL STEPS COMPLETE ✅)

#### Step 1: Clone rs-matter Fork ✅

```bash
cd /home/tim/Coding/public_repos/
git clone https://github.com/project-chip/rs-matter.git
cd rs-matter
git checkout -b event-support
```

Update `virtual_matter_bridge/Cargo.toml`:
```toml
rs-matter = { path = "../rs-matter/rs-matter", features = ["std", "os", "zbus", "async-io"] }
```

#### Step 2: Add EventReportIB Structures ✅

**File**: `rs-matter/rs-matter/src/im/event.rs` (created)

Add after `AttrResp` definition (~line 180):

```rust
/// Event status for error reporting in event responses.
#[derive(Debug, Clone, FromTLV, ToTLV)]
pub struct EventStatusIB {
    #[tlvargs(tag = 0)]
    pub path: EventPath,
    #[tlvargs(tag = 1)]
    pub status: StatusIB,
}

/// Single event report in a ReportData response.
#[derive(Debug, Clone)]
pub struct EventReportIB<'a> {
    pub event_status: Option<EventStatusIB>,
    pub event_data: Option<EventDataIB<'a>>,
}

/// Event data with path, number, priority, timestamp, and payload.
#[derive(Debug, Clone)]
pub struct EventDataIB<'a> {
    pub path: EventPath,
    pub event_number: u64,
    pub priority: u8,
    pub epoch_timestamp: Option<u64>,
    pub system_timestamp: Option<u64>,
    pub data: Option<&'a [u8]>,  // Pre-encoded TLV payload
}

/// Context tags for EventReportIB
pub mod EventReportIBTag {
    pub const EVENT_STATUS: u8 = 0;
    pub const EVENT_DATA: u8 = 1;
}

/// Context tags for EventDataIB
pub mod EventDataIBTag {
    pub const PATH: u8 = 0;
    pub const EVENT_NUMBER: u8 = 1;
    pub const PRIORITY: u8 = 2;
    pub const EPOCH_TIMESTAMP: u8 = 3;
    pub const SYSTEM_TIMESTAMP: u8 = 4;
    pub const DELTA_EPOCH_TIMESTAMP: u8 = 5;
    pub const DELTA_SYSTEM_TIMESTAMP: u8 = 6;
    pub const DATA: u8 = 7;
}
```

#### Step 3: Implement ToTLV for EventDataIB ✅

**File**: `rs-matter/rs-matter/src/im/event.rs`

```rust
impl<'a> ToTLV for EventDataIB<'a> {
    fn to_tlv<W: TLVWrite>(&self, tag: &TLVTag, tw: &mut W) -> Result<(), Error> {
        tw.start_struct(tag)?;

        // Path (tag 0) - required
        self.path.to_tlv(&TLVTag::Context(EventDataIBTag::PATH), tw)?;

        // Event number (tag 1) - required
        tw.u64(&TLVTag::Context(EventDataIBTag::EVENT_NUMBER), self.event_number)?;

        // Priority (tag 2) - required
        tw.u8(&TLVTag::Context(EventDataIBTag::PRIORITY), self.priority)?;

        // Timestamp - exactly one required
        if let Some(ts) = self.epoch_timestamp {
            tw.u64(&TLVTag::Context(EventDataIBTag::EPOCH_TIMESTAMP), ts)?;
        } else if let Some(ts) = self.system_timestamp {
            tw.u64(&TLVTag::Context(EventDataIBTag::SYSTEM_TIMESTAMP), ts)?;
        }

        // Data payload (tag 7) - optional, pre-encoded TLV
        if let Some(data) = self.data {
            tw.raw_value(&TLVTag::Context(EventDataIBTag::DATA), data)?;
        }

        tw.end_container()
    }
}

impl<'a> ToTLV for EventReportIB<'a> {
    fn to_tlv<W: TLVWrite>(&self, tag: &TLVTag, tw: &mut W) -> Result<(), Error> {
        tw.start_struct(tag)?;

        if let Some(status) = &self.event_status {
            status.to_tlv(&TLVTag::Context(EventReportIBTag::EVENT_STATUS), tw)?;
        }

        if let Some(data) = &self.event_data {
            data.to_tlv(&TLVTag::Context(EventReportIBTag::EVENT_DATA), tw)?;
        }

        tw.end_container()
    }
}
```

#### Step 4: Fix ReportDataRespTag ✅

**File**: `rs-matter/rs-matter/src/im/attr.rs` (~line 299)

```rust
pub enum ReportDataRespTag {
    SubscriptionId = 0,
    AttributeReports = 1,
    EventReports = 2,      // Remove underscore prefix
    MoreChunkedMsgs = 3,
    SupressResponse = 4,
}
```

#### Step 5: Add Event Source Trait ✅

**File**: `rs-matter/rs-matter/src/dm/types/event.rs` (created)

Add new trait for handlers that can produce events:

```rust
/// Trait for handlers that can produce Matter events.
///
/// Handlers implementing this trait can queue events that will be
/// included in subscription reports to controllers.
pub trait EventSource: Send + Sync {
    /// Returns pending events and clears the internal queue.
    ///
    /// Called during subscription report generation. Events returned
    /// here will be encoded as EventReportIB structures in the
    /// ReportData response.
    ///
    /// # Returns
    /// Vector of (event_id, event_number, priority, timestamp_ms, payload_tlv)
    fn take_pending_events(&self) -> Vec<PendingEvent>;

    /// Check if there are pending events without draining.
    fn has_pending_events(&self) -> bool;
}

/// A pending event ready to be reported.
#[derive(Debug, Clone)]
pub struct PendingEvent {
    /// Event ID within the cluster (e.g., 0x01 for InitialPress)
    pub event_id: u32,
    /// Monotonically increasing event number (never resets)
    pub event_number: u64,
    /// Priority: 0=Debug, 1=Info, 2=Critical
    pub priority: u8,
    /// System timestamp in milliseconds since boot
    pub system_timestamp_ms: u64,
    /// Pre-encoded TLV payload (cluster-specific event data)
    pub payload: Vec<u8>,
}
```

#### Step 6: Modify ReportDataResponder ✅

**File**: `rs-matter/rs-matter/src/dm.rs`

Add event iteration after attribute reports in `respond()` method.

Location: After the attribute reports `wb.end_container()` call (~line 1020), before `more_chunks` handling:

```rust
// ═══════════════════════════════════════════════════════════════════════
// EVENT REPORTS - New code to add
// ═══════════════════════════════════════════════════════════════════════

// Check if there are event requests in the subscription
let has_event_requests = self.req.event_requests()?.is_some();

if has_event_requests {
    // Collect pending events from all endpoints with EventSource handlers
    let mut all_events: Vec<(EndptId, ClusterId, PendingEvent)> = Vec::new();

    // Iterate through event requests to find matching endpoints/clusters
    if let Some(event_requests) = self.req.event_requests()? {
        for event_path in event_requests {
            // Expand wildcards similar to attr_requests
            let endpoint_id = event_path.endpoint.unwrap_or(0);  // TODO: wildcard expansion
            let cluster_id = event_path.cluster.unwrap_or(0);

            // Get handler and check if it implements EventSource
            if let Ok(handler) = self.invoker.handler(endpoint_id, cluster_id) {
                if let Some(event_source) = handler.as_event_source() {
                    for event in event_source.take_pending_events() {
                        // Filter by event ID if specified
                        if event_path.event.is_none() || event_path.event == Some(event.event_id) {
                            all_events.push((endpoint_id, cluster_id, event));
                        }
                    }
                }
            }
        }
    }

    // Only write EventReports array if we have events
    if !all_events.is_empty() {
        wb.start_array(&TLVTag::Context(ReportDataRespTag::EventReports as u8))?;

        for (endpoint_id, cluster_id, event) in all_events {
            // Build EventPath
            let path = EventPath {
                node: None,
                endpoint: Some(endpoint_id),
                cluster: Some(cluster_id),
                event: Some(event.event_id),
                is_urgent: None,
            };

            // Build EventDataIB
            let event_data = EventDataIB {
                path,
                event_number: event.event_number,
                priority: event.priority,
                epoch_timestamp: None,
                system_timestamp: Some(event.system_timestamp_ms),
                data: if event.payload.is_empty() { None } else { Some(&event.payload) },
            };

            // Build EventReportIB
            let report = EventReportIB {
                event_status: None,
                event_data: Some(event_data),
            };

            // Write to TLV
            report.to_tlv(&TLVTag::Anonymous, &mut wb)?;
        }

        wb.end_container()?;
    }
}
```

#### Step 7: Add as_event_source() to Handler ✅

**File**: `rs-matter/rs-matter/src/dm/types/handler.rs`

Add to the `AsyncHandler` trait or create a wrapper:

```rust
/// Extension trait for handlers that can also be event sources.
pub trait AsyncHandlerExt: AsyncHandler {
    /// Returns this handler as an EventSource if it implements the trait.
    fn as_event_source(&self) -> Option<&dyn EventSource> {
        None  // Default implementation returns None
    }
}
```

---

### virtual_matter_bridge Integration - ✅ COMPLETE (2026-01-14)

> **Status**: Integration complete, UNTESTED. The rs-matter fork provides native event support, and virtual_matter_bridge now uses it directly.

#### Step 8: Implement EventSource for GenericSwitchState ✅

**File**: `src/matter/clusters/generic_switch.rs`

The `GenericSwitchState` struct now implements `rs_matter::dm::EventSource` directly:

```rust
use rs_matter::dm::clusters::generic_switch::{
    encode_initial_press, encode_multi_press_complete, encode_short_release, events,
};
use rs_matter::dm::{EventNumberGenerator, EventSource, MAX_PENDING_EVENTS, PendingEvent};

impl EventSource for GenericSwitchState {
    fn take_pending_events(&self) -> heapless::Vec<PendingEvent, MAX_PENDING_EVENTS> {
        let mut events = self.pending_events.lock();
        core::mem::take(&mut *events)
    }

    fn has_pending_events(&self) -> bool {
        !self.pending_events.lock().is_empty()
    }
}
```

Event payloads are encoded using rs-matter's built-in functions:
- `encode_initial_press(position)` - for button press events
- `encode_short_release(position)` - for button release events
- `encode_multi_press_complete(position, count)` - for double-press events

#### Step 9: Wire EventSource in DynamicHandler ✅

**File**: `src/matter/stack.rs`

An `AggregatedEventSource` collects events from all GenericSwitch handlers:

```rust
pub struct AggregatedEventSource {
    sources: Vec<Arc<GenericSwitchState>>,
}

impl EventSource for AggregatedEventSource {
    fn take_pending_events(&self) -> heapless::Vec<PendingEvent, MAX_PENDING_EVENTS> {
        let mut events = heapless::Vec::new();
        for source in &self.sources {
            for event in source.take_pending_events() {
                let _ = events.push(event);
            }
        }
        events
    }

    fn has_pending_events(&self) -> bool {
        self.sources.iter().any(|s| s.has_pending_events())
    }
}
```

The `DynamicHandler` implements `AsyncHandler` with `as_event_source()`:

```rust
impl AsyncHandler for DynamicHandler {
    fn as_event_source(&self) -> Option<&dyn EventSource> {
        if self.event_sources.has_sources() {
            Some(&self.event_sources)
        } else {
            None
        }
    }
    // ... read, write, invoke implementations
}
```

---

### Testing Strategy

#### Test 1: TLV Encoding Verification

Create unit test that encodes an EventReportIB and verifies the TLV structure:

```rust
#[test]
fn test_event_report_ib_encoding() {
    let path = EventPath {
        node: None,
        endpoint: Some(5),
        cluster: Some(0x003B),  // GenericSwitch
        event: Some(0x03),      // ShortRelease
        is_urgent: None,
    };

    let event = EventDataIB {
        path,
        event_number: 42,
        priority: 1,  // Info
        epoch_timestamp: None,
        system_timestamp: Some(123456),
        data: Some(&[0x15, 0x24, 0x00, 0x01, 0x18]),  // {NewPosition: 1}
    };

    let report = EventReportIB {
        event_status: None,
        event_data: Some(event),
    };

    let mut buf = [0u8; 256];
    let mut tw = TLVWriter::new(&mut buf);
    report.to_tlv(&TLVTag::Anonymous, &mut tw).unwrap();

    // Verify TLV structure matches Matter spec
    // ...
}
```

#### Test 2: End-to-End with Home Assistant

1. Start bridge with W100 and GenericSwitch endpoints
2. Commission to Home Assistant
3. Verify button entities appear in HA
4. Press W100 button
5. Check HA event entity updates (or automation triggers)

Log output should show:
```
[Matter] GenericSwitch endpoint 5: InitialPress event queued (event_number=1)
[Matter] GenericSwitch endpoint 5: ShortRelease event queued (event_number=2)
[Matter] Subscription report: 2 events included
```

---

### Estimated Changes

| Component | File | Lines Changed |
|-----------|------|---------------|
| EventReportIB structs | rs-matter/src/im/attr.rs | +80 |
| ToTLV implementations | rs-matter/src/im/attr.rs | +60 |
| EventSource trait | rs-matter/src/dm/types/handler.rs | +40 |
| ReportDataResponder | rs-matter/src/dm.rs | +80 |
| GenericSwitchHandler | src/matter/clusters/generic_switch.rs | +50 |
| DynamicHandler | src/matter/stack.rs | +30 |
| **Total** | | **~340 lines** |

---

### Future Work (Nice-to-Have Enhancements)

> **Note**: Core GenericSwitch event support is complete in the fork. These are optional enhancements.

1. **EventSource registry**: Allow any cluster handler to register as event source
2. ~~**Event filtering**: Implement full `event_filters()` support per Matter spec~~ ✅ (implemented with wildcard support)
3. **Event persistence**: Queue events across subscription reconnects
4. ~~**Urgent events**: Implement `is_urgent` flag to bypass min interval~~ ✅ (implemented for Critical priority)
5. ~~**Event number persistence**: Save last event number across restarts~~ ✅ (framework implemented - `EventNumberGeneratorPersistent<P>` trait, needs platform storage integration)
6. **Epoch timestamps**: Infrastructure ready (`new_with_epoch()` constructor), needs platform time sync (NTP/RTC)
7. **Delta timestamps**: TLV tags defined, falls back to absolute timestamps - bandwidth optimization only
8. ~~**Long press detection**: Timing logic for `LongPress`/`LongRelease` events~~ ✅ (implemented with configurable 500ms threshold)
9. ~~**Multi-press detection**: Timing logic for `MultiPressOngoing`/`MultiPressComplete` events~~ ✅ (implemented with configurable 300ms window, max 3 presses)

### Remaining TODOs (Non-Blocking)

| Category | Item | Status |
|----------|------|--------|
| Platform Integration | Event number persistence storage | Framework ready, needs NVS/file impl |
| Platform Integration | Epoch timestamp time source | Infrastructure ready, needs NTP/RTC |
| Optimization | Delta timestamp encoding | Nice-to-have bandwidth optimization |
| Code Quality | `tlv_iter()` implementations | Uses `to_tlv()` directly - works fine |
| Configuration | `MAX_PENDING_EVENTS` / `MAX_EVENT_PAYLOAD_SIZE` | Hardcoded (16/64), sufficient for W100 |

---

### References

- [Matter Core Specification 1.4, Chapter 10 - Interaction Model](https://csa-iot.org/wp-content/uploads/2024/11/24-27349-006_Matter-1.4-Core-Specification.pdf)
- [Matter Application Cluster Spec - GenericSwitch Cluster](https://csa-iot.org/wp-content/uploads/2022/11/22-27350-001_Matter-1.0-Application-Cluster-Specification.pdf)
- [rs-matter GitHub - Issue #36 (Event Support)](https://github.com/project-chip/rs-matter/issues/36)
- [rs-matter fork with event support](https://github.com/timlisemer/rs-matter)

---

## References

- [zigbee2mqtt W100 device page](https://www.zigbee2mqtt.io/devices/TH-S04D.html)
- [GitHub issue: W100 external temperature workaround](https://github.com/Koenkk/zigbee2mqtt/issues/27262)
- [Home Assistant blueprint for W100](https://github.com/clementTal/homelab/blob/main/blueprint/aqara-w100.yaml)
- [Matter specification clusters](https://csa-iot.org/developer-resource/specifications-download-request/)
