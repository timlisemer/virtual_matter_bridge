# Repository guidance

## Commands

```sh
just check
just build
just run
just test
```

Use `just check` for validation. It checks formatting, compiles all targets, runs
tests, and runs Clippy with warnings treated as errors. Use `just build` to build
all targets and run tests. `just run --release` runs the main binary in release
mode. Do not contact a live MQTT broker or commission/remove Matter nodes during
routine validation.

## Architecture

- The main Tokio runtime handles MQTT and input sources.
- A dedicated Matter thread runs the Embassy event loop, UDP transport, and subscriptions.
- `VirtualDevice` and `EndpointConfig` define parent and child endpoint layouts.
- `EndpointHandler` connects input state and Matter commands.
- `SourceReadiness` and `SourceSnapshot` track whether input values are available.
- W100 buttons use Generic Switch events through the upstream event emitter.
- Shelly channels remain separate bridged devices with their existing MQTT mapping.

The bridge uses a pinned upstream `project-chip/rs-matter` revision and a committed
Cargo lockfile. Keep the SDK pin and API usage consistent. Rust 1.90 or later is
required; the application does not require nightly Rust.

The controller is the Open Home Foundation matter.js-based Matter Server. Its
WebSocket API is compatible with the bridge's commissioning client. Preserve
fabric persistence and endpoint identity when changing dependencies.

Camera and WebRTC modules are experimental and are not wired into the Matter
endpoint router. Do not describe them as working camera streaming or interpret
a specification-version attribute as certification.

See README.md for configuration and docs/ for current MQTT integration details.
