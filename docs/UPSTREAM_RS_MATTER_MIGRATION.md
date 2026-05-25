# Upstream rs-matter Migration Goal

This repository was reset to `0c39a04fa45c6192487427459b8abfb6a98e1c5e` and migrated forward against current upstream `rs-matter`.

## Current Status

`virtual_matter_bridge` currently depends on upstream `rs-matter` from GitHub `main`:

```toml
rs-matter = { git = "https://github.com/project-chip/rs-matter.git", branch = "main", features = ["std", "os", "zbus", "async-io"] }
```

The `Cargo.lock` commit this repo last knew about is:

```text
project-chip/rs-matter@93d9274be10f646bf7d1ec0ed666e3bece857d81
```

That commit is the baseline for "old upstream rs-matter" in this repo. The bridge has been updated for newer upstream `rs-matter` API and behavior after that commit.

The repo now passes the agent-framework check MCP on `/home/tim/Coding/public_repos/virtual_matter_bridge`, including `cargo check` and `cargo clippy -- -D warnings`. The migration addressed the previous `rs-matter` API drift:

- removed or moved imports such as `dev_att::DataType`, `DevAttDataFetcher`, `clusters::on_off`, `DefaultSubscriptions`, `Psm`, `NO_NETWORKS`, `Context`, `sys_epoch`, and `utils::rand`
- changed endpoint/root handler builder APIs such as `endpoints::with_eth` and `endpoints::with_sys`
- new trait requirements such as `Handler::bump_dataver`
- new `DynBase` requirements for types such as `FilteredNetifs` and `DynamicPartsMatcher`
- upstream event storage, metadata, and `DataModel::emit_event` wiring for GenericSwitch events

These items are historical migration context, not current blockers.

## Why This Goal Exists

The W100 integration is the main reason this matters.

The bridge should expose an Aqara W100 climate sensor from zigbee2mqtt/MQTT into Matter. Temperature and humidity are attribute-style data and fit naturally into Matter sensor clusters. The W100 buttons are different: they are momentary actions that need Matter events so Home Assistant can use them as automation triggers.

Target W100 flow:

```text
W100 -> zigbee2mqtt -> MQTT -> Virtual Matter Bridge -> Matter -> Home Assistant
```

The important W100 button use cases are:

- plus button: increase the office thermostat target temperature
- minus button: decrease the office thermostat target temperature
- center button: toggle the audio receiver and subwoofer

## What Was Previously Tried

Before upstream `rs-matter` had usable event support, this project tried to unblock W100 buttons by maintaining a forked `rs-matter` with event support work. The bridge-side idea was to queue GenericSwitch events for the W100 buttons and report those events to Matter controllers.

That attempt was useful for learning the shape of the problem, but it is no longer the direction for this repository.

## Why The Fork Attempt Is Abandoned

Upstream `rs-matter` has caught up on the important protocol machinery:

- Interaction Model event structures
- event storage and event number handling
- event reporting through Read/Subscribe
- event filtering and path validation
- event subscription wakeups
- generated event metadata/builders from IDL
- `EventEmitter` APIs for emitting events from data model code

The bridge now uses upstream `rs-matter` instead of carrying a fork. W100 buttons are modeled on top of upstream GenericSwitch event emission.

## Migration Outcome

The migration made `virtual_matter_bridge` compile and run against current upstream `rs-matter`.

Completed work areas:

1. Update imports and stack setup to current upstream `rs-matter`.
2. Update custom handlers for new trait requirements such as `bump_dataver`.
3. Replace old event assumptions with upstream `EventEmitter` / `Events` usage.
4. Keep or rebuild the bridge's GenericSwitch cluster metadata as needed for W100 button endpoints.
5. Preserve the W100 MQTT behavior: temperature, humidity, state request on startup, and button action parsing.
6. Verify button actions can be delivered as Matter GenericSwitch-style events to Home Assistant.

The migration treats `project-chip/rs-matter@93d9274be10f646bf7d1ec0ed666e3bece857d81` as the last known upstream baseline and current upstream `main` as the target.
